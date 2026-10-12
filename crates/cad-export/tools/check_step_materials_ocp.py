"""Independent STEP units and XCAF material assignment acceptance.

OCCT STEPCAFControl_Reader::ReadMaterials normalizes imported density to g/cm3;
XCAFDoc_MaterialTool::GetDensityForShape returns g/mm3. We check both against
independently decoded STEP SI/derived units and native kg/m3 source values.
Sources: https://github.com/Open-Cascade-SAS/OCCT/blob/master/src/DataExchange/TKDESTEP/STEPCAFControl/STEPCAFControl_Reader.cxx
https://github.com/Open-Cascade-SAS/OCCT/blob/master/src/DataExchange/TKXCAF/XCAFDoc/XCAFDoc_MaterialTool.cxx
"""
import argparse
import importlib.metadata
import json
import math
import pathlib
import platform
import sys
import tempfile
import OCP
from OCP.IFSelect import IFSelect_RetDone
from OCP.STEPControl import STEPControl_Reader
from OCP.STEPCAFControl import STEPCAFControl_Reader
from OCP.StepBasic import (StepBasic_SiUnit, StepBasic_SiPrefix, StepBasic_SiUnitName,
                           StepBasic_ConversionBasedUnit)
from OCP.StepRepr import (StepRepr_PropertyDefinitionRepresentation,
                          StepRepr_DescriptiveRepresentationItem, StepRepr_MeasureRepresentationItem)
from OCP.TCollection import TCollection_ExtendedString
from OCP.TDF import TDF_Label, TDF_AttributeIterator
from OCP.TDocStd import TDocStd_Document
from OCP.XCAFDoc import XCAFDoc, XCAFDoc_DocumentTool, XCAFDoc_Material, XCAFDoc_MaterialTool
from check_assembly_ocp import LabelSequence, label_entry, label_name, require
from check_step_metadata_ocp import command
from check_constrained_fixture_ocp import body_properties

ROOT = pathlib.Path(__file__).resolve().parents[3]
RECIPE = ROOT/'examples/step_materials.star'
PREFIX_POWERS = dict(zip(list(StepBasic_SiPrefix.__members__.values()),
                        [18,15,12,9,6,3,2,1,-1,-2,-3,-6,-9,-12,-15,-18]))


def close(actual, expected):
    require(math.isfinite(actual) and math.isclose(actual, expected, rel_tol=1e-10, abs_tol=1e-12),
            f'density or mass mismatch: {actual} != {expected}')


def named_unit(unit, depth=0):
    require(depth < 8, 'cyclic or excessive unit conversion')
    if isinstance(unit, StepBasic_SiUnit):
        scale = 10.**PREFIX_POWERS[unit.Prefix()] if unit.HasPrefix() else 1.
        if unit.Name() == StepBasic_SiUnitName.StepBasic_sunGram:
            return scale * .001, (1.,0.)  # kg
        require(unit.Name() == StepBasic_SiUnitName.StepBasic_sunMetre, 'unsupported density base unit')
        return scale, (0.,1.)  # m
    require(isinstance(unit, StepBasic_ConversionBasedUnit), 'unsupported density conversion unit')
    conversion = unit.ConversionFactor()
    factor, dimensions = named_unit(conversion.UnitComponent().NamedUnit(), depth+1)
    declared = unit.Dimensions()
    require(declared is not None and (declared.MassExponent(), declared.LengthExponent()) == dimensions
            and all(getattr(declared, name)() == 0. for name in [
                "TimeExponent", "ElectricCurrentExponent", "ThermodynamicTemperatureExponent",
                "AmountOfSubstanceExponent", "LuminousIntensityExponent"]),
            "conversion unit dimensions contradict its base unit")
    value = conversion.ValueComponent()
    require(math.isfinite(value) and value > 0, 'invalid conversion factor')
    return factor * value, dimensions


def density_units(measure):
    unit = measure.UnitComponent().DerivedUnit()
    require(unit is not None and unit.NbElements() > 0, 'density needs explicit derived units')
    factor, mass, length = 1., 0., 0.
    for i in range(1,unit.NbElements()+1):
        element = unit.ElementsValue(i)
        exponent = element.Exponent()
        require(math.isfinite(exponent), 'nonfinite unit exponent')
        scale, dimensions = named_unit(element.Unit())
        factor *= scale ** exponent
        mass += dimensions[0]*exponent
        length += dimensions[1]*exponent
    require(mass == 1. and length == -3., 'density dimensions must be mass / length^3')
    require(math.isfinite(factor) and factor > 0, 'invalid density unit factor')
    return factor


def step_materials(path):
    reader = STEPControl_Reader()
    require(reader.ReadFile(str(path)) == IFSelect_RetDone, 'STEP read failed')
    model = reader.StepModel()
    result = {}
    for i in range(1,model.NbEntities()+1):
        entity = model.Value(i)
        if not isinstance(entity, StepRepr_PropertyDefinitionRepresentation):
            continue
        property_definition = entity.Definition().PropertyDefinition()
        if property_definition is None:
            continue
        product = property_definition.Definition().ProductDefinition()
        if product is None:
            continue
        name = product.Formation().OfProduct().Name().ToCString()
        representation = entity.UsedRepresentation()
        for j in range(1,representation.NbItems()+1):
            item = representation.ItemsValue(j)
            if isinstance(item, StepRepr_DescriptiveRepresentationItem):
                record = result.setdefault(name,{})
                require('name' not in record, 'duplicate material name')
                record['name'] = item.Name().ToCString()
            if isinstance(item, StepRepr_MeasureRepresentationItem):
                record = result.setdefault(name,{})
                require('density_kg_per_m3' not in record, 'duplicate material density')
                measure = item.Measure()
                record['density_kg_per_m3'] = measure.ValueComponent()*density_units(measure)
    return result


def check_materials(step_path, assembly_json):
    source = json.loads(pathlib.Path(assembly_json).read_bytes())
    expected = {name: d.get('metadata',{}).get('material') for name,d in source['definitions'].items()}
    decoded = step_materials(step_path)
    require(set(decoded) == {name for name,value in expected.items() if value is not None}, 'STEP material membership mismatch')
    for name,record in decoded.items():
        require(record['name'] == expected[name]['name'], 'decoded STEP material name mismatch')
        close(record['density_kg_per_m3'],expected[name]['density_kg_per_m3'])
    document = TDocStd_Document(TCollection_ExtendedString('cad-material-oracle'))
    reader = STEPCAFControl_Reader()
    reader.SetNameMode(True)
    reader.SetMatMode(True)
    require(reader.ReadFile(str(step_path)) == IFSelect_RetDone and reader.Transfer(document), 'XCAF transfer failed')
    tool = XCAFDoc_DocumentTool.ShapeTool_s(document.Main())
    roots = LabelSequence()
    tool.GetFreeShapes(roots)
    definitions, occurrences, identity_by_part = {}, {}, {}

    def material(label):
        def attribute(label, guid):
            iterator = TDF_AttributeIterator(label)
            while iterator.More():
                value = iterator.Value()
                if value.ID().IsSame(guid):
                    return value
                iterator.Next()
            return None
        node = attribute(label, XCAFDoc.MaterialRefGUID_s())
        if node is None:
            close(XCAFDoc_MaterialTool.GetDensityForShape_s(label),0.)
            return None
        require(node.HasFather(), 'unbound material reference')
        attr = attribute(node.Father().Label(), XCAFDoc_Material.GetID_s())
        require(isinstance(attr, XCAFDoc_Material), 'material attribute missing')
        density = attr.GetDensity()*1000.  # g/cm3 to kg/m3, as specified by OCCT importer.
        close(XCAFDoc_MaterialTool.GetDensityForShape_s(label)*1e6,density)  # g/mm3 to kg/m3.
        return dict(name=attr.GetName().ToCString(),density_kg_per_m3=density,
                    imported_density_g_per_cm3=attr.GetDensity())

    def walk(label,path,active):
        identity = label_entry(label)
        require(identity not in active, 'cyclic assembly')
        if tool.IsAssembly_s(label):
            children = LabelSequence()
            require(tool.GetComponents_s(label,children), 'missing assembly children')
            for i in range(1,children.Length()+1):
                child = children.Value(i)
                reference = TDF_Label()
                require(tool.GetReferredShape_s(child,reference), 'missing definition reference')
                walk(reference,path+[label_name(child)],active|{identity})
        else:
            name = label_name(label)
            require(name in expected, 'unknown imported part definition')
            require(identity_by_part.setdefault(name,identity) == identity, 'repeated part definition not shared')
            require(identity not in definitions or definitions[identity]['part'] == name, 'distinct material definitions merged')
            value = material(label)
            wanted = expected[name]
            require((value is None) == (wanted is None), 'missing material was fabricated or assigned material was lost')
            if wanted is not None:
                require(value['name'] == wanted['name'], 'XCAF material name mismatch')
                close(value['density_kg_per_m3'],wanted['density_kg_per_m3'])
            definitions[identity] = dict(part=name,material=value)
            key = tuple(path)
            require(key not in occurrences,'duplicate occurrence')
            occurrences[key] = name
    require(roots.Length()==1,'expected one assembly root')
    walk(roots.Value(1),[],set())
    wanted_occurrences = {tuple(source['instance_paths'].get(i['id'],[i['id']])):i['part'] for i in source['instances']}
    require(occurrences == wanted_occurrences,'material-to-occurrence mapping differs')
    require(set(identity_by_part)==set(expected),'part definition membership differs')
    return dict(valid=True,part_definitions_checked=len(definitions),occurrences_checked=len(occurrences),
                materials_checked=len(decoded),missing_material_definitions=sum(v is None for v in expected.values()),
                decoded_step_materials=decoded,xcaf_definitions=list(definitions.values()))


def check(cad,root):
    cases=[]
    for density in [7800,7900]:
        out=root/str(density)
        built=command([cad,'build',RECIPE,'--param',f'steel_density={density}','--out-dir',out])
        material=check_materials(out/'step_materials.step',out/'step_materials.assembly.json')
        imported=command([sys.executable,pathlib.Path(__file__).with_name('check_assembly_ocp.py'),out/'step_materials.step','--instances','6','--definitions','3','--solids','6','--assembly-json',out/'step_materials.assembly.json'])
        require(material['materials_checked']==2 and material['missing_material_definitions']==1,'fixture material counts')
        weighted=command([cad,'properties',RECIPE,'--output','weighted','--param',f'steel_density={density}'])
        close(weighted['mass_kg'],math.pi*3**2*8*(density+2700)*1e-9)
        require(built['exchange_policy_status']=='passed','material requirements did not pass')
        require(command([cad,'verify',out])['valid'],'generation integrity failed')
        cases.append(dict(density_kg_per_m3=density,materials=material,assembly_importer=imported,
                          step_properties=body_properties(out/'step_materials.step'),body_hash=built['outputs']['body']['content_hash'],
                          assembly_hash=built['outputs']['step_materials']['content_hash'],weighted_mass_kg=weighted['mass_kg']))
    require(cases[0]['body_hash']==cases[1]['body_hash'],'density edit changed native geometry')
    require(cases[0]['assembly_hash']!=cases[1]['assembly_hash'],'density edit did not change engineering identity')
    require(cases[0]['step_properties']==cases[1]['step_properties'],'density edit changed imported geometry')
    require((root/'7800/step_materials.stl').read_bytes()==(root/'7900/step_materials.stl').read_bytes(),'density edit changed placed mesh')
    close(cases[1]['weighted_mass_kg']-cases[0]['weighted_mass_kg'],math.pi*3**2*8*100*1e-9)
    return dict(valid=True,cases=cases,density_units_independently_checked=True,
                oracle=dict(python_version=platform.python_version(),cadquery_ocp_version=importlib.metadata.version('cadquery-ocp'),ocp_version=str(OCP.__version__)))


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--cad',type=pathlib.Path,required=True)
    args=parser.parse_args()
    with tempfile.TemporaryDirectory(prefix='cad-step-materials-') as directory:
        result=check(args.cad.resolve(),pathlib.Path(directory))
    print(json.dumps(result,indent=2,ensure_ascii=False,allow_nan=False))


if __name__=='__main__':main()
