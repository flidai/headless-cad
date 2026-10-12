"""Independent decoded STEP product fields, shared hierarchy and metadata-only edits."""
import argparse
import importlib.metadata
import platform
import OCP
import json
import pathlib
import subprocess
import sys
import tempfile

from OCP.IFSelect import IFSelect_RetDone
from OCP.STEPControl import STEPControl_Reader
from OCP.StepBasic import StepBasic_Product, StepBasic_ProductDefinitionFormation
from check_constrained_fixture_ocp import body_properties, require

ROOT = pathlib.Path(__file__).resolve().parents[3]
RECIPE = ROOT / 'examples/step_metadata.star'


def command(argv):
    return json.loads(subprocess.check_output([str(x) for x in argv]))


def ordered(records):
    return sorted(records, key=lambda r: (r['name'], r['number'], r['revision'], r['description']))


def metadata(path):
    reader = STEPControl_Reader()
    require(reader.ReadFile(str(path)) == IFSelect_RetDone, 'STEP reader failed')
    model = reader.StepModel()
    products, revisions = {}, {}
    # Keep wrappers alive so OfProduct resolves to the same bound OCCT handle.
    # OCP 8's InterfaceModel.Number currently returns zero for these handles;
    # the authoritative one-based Value index supplies the entity number instead.
    entities = [(i, model.Value(i)) for i in range(1, model.NbEntities() + 1)]
    product_indices = {entity: i for i, entity in entities if isinstance(entity, StepBasic_Product)}
    for i, entity in entities:
        if isinstance(entity, StepBasic_Product):
            products[i] = dict(name=entity.Name().ToCString(),
                               number=entity.Id().ToCString(),
                               description=entity.Description().ToCString())
        if isinstance(entity, StepBasic_ProductDefinitionFormation):
            identity = product_indices.get(entity.OfProduct())
            require(identity is not None, 'formation references product outside STEP model')
            require(identity not in revisions, 'duplicate product formation')
            revisions[identity] = entity.Id().ToCString()
    require(set(products) == set(revisions), 'unbound product formation')
    # Names are namespace-local: a part and an assembly may share a name. Bind
    # formations by actual STEP entity identity, then compare a counted multiset.
    return ordered([dict(value, revision=revisions[identity]) for identity, value in products.items()])


def check_metadata(step_path, assembly_json):
    """Compare decoded STEP entities with declared native definition metadata."""
    assembly = json.loads(pathlib.Path(assembly_json).read_bytes())
    expected = []
    for name, definition in assembly['definitions'].items():
        fields = definition.get('metadata', {})
        expected.append(dict(name=name, number=fields.get('part_number', 'part:'+name),
                             description=fields.get('description', ''),
                             revision=fields.get('revision', '1')))
    for name in assembly['hierarchy']:
        expected.append(dict(name=name, number='assembly:'+name, description='', revision='1'))
    decoded = metadata(step_path)
    require(decoded == ordered(expected), f'decoded STEP fields differ from native metadata: {decoded!r}')
    return dict(valid=True, products_checked=len(decoded),
                part_definitions_checked=len(assembly['definitions']), decoded_products=decoded)


def check(cad, root):
    cases = []
    for edit in [0, 1]:
        out = root / str(edit)
        build = command([cad, 'build', RECIPE, '--param', f'revision_edit={edit}', '--out-dir', out])
        decoded = check_metadata(out / 'step_metadata.step', out / 'step_metadata.assembly.json')['decoded_products']
        expected = [
            dict(name='revision_a', number="SHARED-α'42", description="Bearing's café — α", revision='A' if edit == 0 else 'A2'),
            dict(name='revision_b', number="SHARED-α'42", description="Bearing's café — β", revision='B'),
            dict(name='plain', number='part:plain', description='', revision='1'),
            dict(name='plain', number='assembly:plain', description='', revision='1'),
            dict(name='step_metadata', number='assembly:step_metadata', description='', revision='1'),
        ]
        require(decoded == ordered(expected), f'decoded product metadata mismatch: {decoded!r}')
        imported = command([sys.executable, pathlib.Path(__file__).with_name('check_assembly_ocp.py'), out / 'step_metadata.step', '--instances', '6', '--definitions', '3', '--solids', '6', '--assembly-json', out / 'step_metadata.assembly.json'])
        require(imported['hierarchy_checked'] and imported['placements_checked'], 'assembly identity/placement check missing')
        require(build['exchange_policy_status'] == 'passed', 'metadata requirements not evaluated')
        require(all(r['status'] == 'passed' and r['source_count'] == 2 for r in build['exchange_requirements'].values()), 'metadata policy counts incorrect')
        require(command([cad, 'verify', out])['valid'], 'generation integrity failed')
        native = command([cad, 'properties', RECIPE, '--output', 'step_metadata', '--param', f'revision_edit={edit}'])
        cases.append(dict(parameters={'revision_edit':edit}, decoded_products=decoded, imported=imported, step_properties=body_properties(out/'step_metadata.step'), native_properties=native, body_hash=build['outputs']['body']['content_hash'], assembly_hash=build['outputs']['step_metadata']['content_hash']))
    require(cases[0]['body_hash'] == cases[1]['body_hash'], 'metadata edit rebuilt different body')
    require(cases[0]['assembly_hash'] != cases[1]['assembly_hash'], 'metadata edit missing assembly identity')
    require(cases[0]['step_properties'] == cases[1]['step_properties'], 'metadata edit changed imported STEP moments')
    for field in ['volume', 'volume_centroid', 'central_volume_inertia']:
        require(cases[0]['native_properties'][field] == cases[1]['native_properties'][field], 'metadata edit changed native '+field)
    require((root/'0/step_metadata.stl').read_bytes() == (root/'1/step_metadata.stl').read_bytes(), 'metadata edit changed placed STL geometry')
    return dict(valid=True, cases=cases, metadata_only_geometry_unchanged=True,
                oracle=dict(python_version=platform.python_version(),
                            cadquery_ocp_version=importlib.metadata.version('cadquery-ocp'),
                            ocp_version=str(OCP.__version__)))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--cad', type=pathlib.Path, required=True)
    args = parser.parse_args()
    with tempfile.TemporaryDirectory(prefix='cad-step-metadata-') as directory:
        result = check(args.cad.resolve(), pathlib.Path(directory))
    print(json.dumps(result, indent=2, ensure_ascii=False))


if __name__ == '__main__':
    main()
