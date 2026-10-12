"""Acceptance against a real collected engineering bundle, never fabricated passes.

CAD_EVIDENCE_BUNDLE=/path/to/bundle python3 -m unittest discover -s scripts/tests
"""
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]
SCRIPT = ROOT / 'scripts/evidence.py'


def digest(path):
    h = hashlib.sha256()
    with path.open('rb') as stream:
        while chunk := stream.read(1024 * 1024):
            h.update(chunk)
    return {'sha256': h.hexdigest(), 'size': path.stat().st_size}


class EvidencePathContract(unittest.TestCase):
    def test_paths_are_relative_on_both_posix_and_windows(self):
        spec = importlib.util.spec_from_file_location('cad_evidence', SCRIPT)
        module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(module)
        for value in ['../outside', '/outside', 'C:/outside', 'C:outside',
                      'a\\b', 'a//b', './part', 'part/../outside']:
            with self.subTest(path=value), self.assertRaises(ValueError):
                module.safe(value)
        self.assertEqual(module.safe('source/examples/part.star'), 'source/examples/part.star')


@unittest.skipUnless(os.environ.get('CAD_EVIDENCE_BUNDLE'), 'requires actual collected evidence')
class CollectedEvidenceAcceptance(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.original = Path(os.environ['CAD_EVIDENCE_BUNDLE']).resolve()
        cls.manifest = json.loads((cls.original/'evidence.json').read_text())
        cls.pin = digest(cls.original/'evidence.json')['sha256']
        result = cls.invoke(cls.original, '--manifest-hash', cls.pin)
        if result.returncode:
            raise AssertionError(result.stderr)

    @staticmethod
    def invoke(bundle, *args):
        return subprocess.run([sys.executable, str(SCRIPT), 'verify', str(bundle), *args],
                              capture_output=True, text=True)

    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix='cad-evidence-test-')
        self.bundle = Path(self.temporary.name)/'bundle'
        shutil.copytree(self.original, self.bundle)

    def tearDown(self):
        self.temporary.cleanup()

    def reject(self, *args):
        result = self.invoke(self.bundle, *args)
        self.assertNotEqual(result.returncode, 0, result.stdout)
        self.assertIs(json.loads(result.stderr)['valid'], False)

    def write_manifest(self, data):
        (self.bundle/'evidence.json').write_text(json.dumps(data, indent=2)+'\n')

    def test_real_bundle_is_read_only_and_source_bound(self):
        before = digest(self.bundle/'evidence.json')
        result = self.invoke(self.bundle, '--manifest-hash', self.pin, '--source-root', str(ROOT))
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIs(json.loads(result.stdout)['valid'], True)
        self.assertEqual(before, digest(self.bundle/'evidence.json'))
        self.assertEqual(set(self.manifest['files']),
                         {p.relative_to(self.bundle).as_posix() for p in self.bundle.rglob('*')
                          if p.is_file() and p.name != 'evidence.json'})

    def test_modified_binary_source_oracle_and_result_are_rejected(self):
        oracle = next(c for c in self.manifest['checks'] if c['kind']=='independent')
        paths = ['tools/cad', 'source/'+self.manifest['capability']['fixture'],
                 'source/crates/cad-export/tools/check_counterbored_fixture_ocp.py', oracle['stdout'],
                 'artifacts/counterbored_fixture/reports/0.json']
        for name in paths:
            with self.subTest(path=name):
                path = self.bundle/name
                original = path.read_bytes()
                path.write_bytes(bytes([original[0]^1])+original[1:])
                self.reject()
                path.write_bytes(original)

    def test_manifest_pin_and_duplicate_keys_are_checked(self):
        path = self.bundle/'evidence.json'
        path.write_bytes(path.read_bytes()+b'\n')
        self.reject('--manifest-hash', self.pin)
        self.assertEqual(self.invoke(self.bundle).returncode, 0)
        text = path.read_text()
        path.write_text('{"schema_version":1,'+text.lstrip()[1:])
        self.reject()

    def test_coverage_and_claims_cannot_be_relabelled(self):
        original = json.loads((self.bundle/'evidence.json').read_text())
        mutations = [
            lambda d: d['checks'].pop(),
            lambda d: d['checks'][0].update(exit_code=1),
            lambda d: d['checks'][0].update(kind='independent'),
            lambda d: d['checks'][0]['argv'].append('--ignored'),
            lambda d: d['capability'].update(supported_domain='All commercial CAD geometry'),
            lambda d: d['capability'].update(fixture='examples/counterbore.star'),
            lambda d: d['capability'].update(domain_ids=['invented.domain']),
            lambda d: d['capability'].update(status='experimental'),
            lambda d: d['checks'][-1]['result'].update(valid=False),
            lambda d: d['generation_verifications'].pop(),
            lambda d: d['generation_verifications'][0]['manifest_record'].update(sha256='0'*64),
        ]
        for index, mutate in enumerate(mutations):
            with self.subTest(case=index):
                data = json.loads(json.dumps(original))
                mutate(data)
                self.write_manifest(data)
                self.reject()
        self.write_manifest(original)

    def test_rehashed_payload_still_needs_matching_source_and_result_bindings(self):
        original = json.loads((self.bundle/'evidence.json').read_text())
        for name in ['source/'+original['capability']['fixture'], original['checks'][-1]['stdout']]:
            path = self.bundle/name
            saved = path.read_bytes()
            path.write_bytes(saved+b'\n')
            data = json.loads(json.dumps(original))
            data['files'][name] = digest(path)
            # For JSON result logs change a semantic field as whitespace is
            # intentionally harmless without an external manifest pin.
            if name.endswith('.stdout'):
                value = json.loads(saved)
                value['valid'] = False
                path.write_text(json.dumps(value))
                data['files'][name] = digest(path)
            self.write_manifest(data)
            self.reject()
            path.write_bytes(saved)
        self.write_manifest(original)

    def test_collection_refuses_prior_bundles_planned_claims_and_dirty_source(self):
        def collect(capability, destination):
            return subprocess.run([sys.executable, str(SCRIPT), 'collect',
                                   '--capability', capability, '--out-dir', str(destination)],
                                  cwd=ROOT, capture_output=True, text=True)
        before = digest(self.bundle/'evidence.json')
        blocked = collect(self.manifest['capability']['id'], self.bundle)
        self.assertNotEqual(blocked.returncode, 0)
        self.assertIn('destination already exists', blocked.stderr)
        self.assertEqual(before, digest(self.bundle/'evidence.json'))
        missing = Path(self.temporary.name)/'planned'
        blocked = collect('manufacturing.drive-module-release', missing)
        self.assertNotEqual(blocked.returncode, 0)
        self.assertIn('planned capabilities', blocked.stderr)
        self.assertFalse(missing.exists())
        with tempfile.NamedTemporaryFile(prefix='evidence-dirty-', suffix='.txt', dir=ROOT):
            blocked = collect(self.manifest['capability']['id'], missing)
            self.assertNotEqual(blocked.returncode, 0)
            self.assertIn('clean Git checkout', blocked.stderr)
            self.assertFalse(missing.exists())

    def test_missing_extra_and_symlinked_artifacts_fail(self):
        binary = self.bundle/'tools/cad'
        saved = binary.read_bytes()
        binary.unlink()
        self.reject()
        binary.write_bytes(saved)
        extra = self.bundle/'untracked.txt'
        extra.write_text('outside evidence')
        self.reject()
        extra.unlink()
        (self.bundle/'empty').mkdir()
        self.reject()
        (self.bundle/'empty').rmdir()
        binary.unlink()
        binary.symlink_to(self.original/'tools/cad')
        self.reject()


if __name__ == '__main__':
    unittest.main()
