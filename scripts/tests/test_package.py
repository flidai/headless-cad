"""Publication tests for completed distribution archives, with an isolated fixture."""
import os
from pathlib import Path
import shutil
import subprocess
import tarfile
import tempfile
import time
import unittest

SCRIPT = Path(__file__).resolve().parents[1] / 'package.sh'

class PackagePublication(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix='cad-package-test-')
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        for directory in ['scripts', 'bin', 'target/release', 'docs', 'examples', 'dist']:
            (self.root / directory).mkdir(parents=True, exist_ok=True)
        shutil.copyfile(SCRIPT, self.root / 'scripts/package.sh')
        (self.root / 'README.md').write_text('Fixture documentation\n')
        (self.root / 'LICENSE').write_text('Apache-2.0 fixture\n')
        (self.root / 'target/release/cad').write_bytes(b'complete fixture executable')
        (self.root / 'examples/part.star').write_text('fixture source\n')
        self.executable('cargo', '#!/bin/sh\nexit 0\n')
        platform = subprocess.check_output(['uname', '-s'], text=True).strip().lower()
        machine = subprocess.check_output(['uname', '-m'], text=True).strip()
        self.name = f'headless-cad-{platform}-{machine}'
        self.destination = self.root / 'dist' / (self.name + '.tar.gz')
        self.previous = b'previous complete release fixture'
        self.destination.write_bytes(self.previous)
        self.env = dict(os.environ, PATH=str(self.root/'bin')+os.pathsep+os.environ['PATH'])
        self.env.pop('CARGO_TARGET_DIR', None)
        self.env['PACKAGE_TEST_ROOT'] = str(self.root)
        self.env['PACKAGE_TEST_TAR'] = shutil.which('tar')

    def executable(self, name, source):
        path = self.root / 'bin' / name
        path.write_text(source)
        path.chmod(0o755)

    def run_package(self):
        return subprocess.run(['bash', str(self.root/'scripts/package.sh')], env=self.env, capture_output=True, timeout=20)

    def test_failed_archive_keeps_previous_release_and_cleans_staging(self):
        self.executable('tar', '#!/usr/bin/env python3\nimport pathlib,sys\npathlib.Path(sys.argv[2]).write_bytes(b"truncated archive")\nsys.exit(7)\n')
        result = self.run_package()
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(self.destination.read_bytes(), self.previous)
        self.assertEqual(list((self.root/'dist').iterdir()), [self.destination])

    def test_readers_keep_old_archive_until_complete_replacement(self):
        self.executable('tar', '''#!/usr/bin/env python3
import os,pathlib,sys,time
root=pathlib.Path(os.environ['PACKAGE_TEST_ROOT'])
if sys.argv[1] == '-czf':
    pathlib.Path(sys.argv[2]).write_bytes(b'partial new archive')
    (root/'writing').touch()
    while not (root/'continue').exists(): time.sleep(.01)
os.execv(os.environ['PACKAGE_TEST_TAR'],['tar',*sys.argv[1:]])
''')
        process = subprocess.Popen(['bash',str(self.root/'scripts/package.sh')],env=self.env,stdout=subprocess.PIPE,stderr=subprocess.PIPE)
        try:
            deadline = time.monotonic()+10
            while not (self.root/'writing').exists():
                self.assertIsNone(process.poll(), 'packager exited before controlled write')
                self.assertLess(time.monotonic(), deadline)
                time.sleep(.01)
            self.assertEqual(self.destination.read_bytes(), self.previous)
            (self.root/'continue').touch()
            stdout, stderr = process.communicate(timeout=20)
            self.assertEqual(process.returncode,0,stderr.decode())
            self.assertEqual(stdout.decode().strip(),str(self.destination))
            with tarfile.open(self.destination) as archive:
                self.assertEqual(archive.extractfile(self.name+'/cad').read(),b'complete fixture executable')
                self.assertEqual(archive.extractfile(self.name+'/examples/part.star').read(),b'fixture source\n')
            self.assertEqual(list((self.root/'dist').iterdir()),[self.destination])
        finally:
            (self.root/'continue').touch()
            if process.poll() is None:
                process.kill()
            process.communicate()

if __name__ == '__main__':
    unittest.main()
