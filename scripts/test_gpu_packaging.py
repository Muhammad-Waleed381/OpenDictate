"""Packaging regressions, run with: python3 -m unittest discover -s scripts -p 'test_*.py'."""
import io
import json
import os
from pathlib import Path
import subprocess
import tarfile
import tempfile
import unittest
import zipfile

SCRIPTS = Path(__file__).resolve().parent


class GpuPackagingTests(unittest.TestCase):
    def test_extensionless_zip_and_tar_are_both_extracted(self):
        for kind in ('zip', 'tar'):
            with self.subTest(kind=kind), tempfile.TemporaryDirectory() as tmp:
                root = Path(tmp)
                archive = root / 'libs.archive'
                if kind == 'zip':
                    with zipfile.ZipFile(archive, 'w') as zipped:
                        zipped.writestr('lib/libsherpa-onnx-c-api.so', b'library')
                else:
                    with tarfile.open(archive, 'w:bz2') as tar:
                        entry = tarfile.TarInfo('lib/libsherpa-onnx-c-api.so')
                        entry.size = 7
                        tar.addfile(entry, io.BytesIO(b'library'))
                result = subprocess.run(['python3', str(SCRIPTS / 'extract-gpu-libs.py'), str(archive), str(root / 'out')], capture_output=True, text=True)
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertEqual((root / 'out/lib/libsherpa-onnx-c-api.so').read_bytes(), b'library')

    def test_runtime_libraries_are_packaged_without_losing_other_options(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            (root / 'src-tauri').mkdir()
            libraries = root / 'libraries'
            libraries.mkdir()
            (libraries / 'libsherpa-onnx-c-api.so').write_bytes(b'library')
            (libraries / 'libonnxruntime.so.1').write_bytes(b'runtime')
            config = root / 'src-tauri/tauri.conf.json'
            config.write_text(json.dumps({'bundle': {'linux': {'deb': {'depends': ['libasound2'], 'files': {'/usr/share/example': 'example.txt'}}}}}))
            result = subprocess.run(['python3', str(SCRIPTS / 'inject-linux-libs.py')], cwd=root, env={**os.environ, 'SHERPA_ONNX_LIB_DIR': str(libraries)}, capture_output=True, text=True)
            self.assertEqual(result.returncode, 0, result.stderr)
            linux = json.loads(config.read_text())['bundle']['linux']
            self.assertEqual(linux['deb']['depends'], ['libasound2'])
            self.assertEqual(linux['deb']['files']['/usr/share/example'], 'example.txt')
            for package in ('deb', 'rpm'):
                self.assertEqual(linux[package]['files']['/usr/lib/opendictate/libonnxruntime.so.1'], str(libraries / 'libonnxruntime.so.1'))

    def test_archive_cannot_write_outside_destination(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            archive = root / 'libs.archive'
            with zipfile.ZipFile(archive, 'w') as zipped:
                zipped.writestr('../escaped', b'bad')
            result = subprocess.run(['python3', str(SCRIPTS / 'extract-gpu-libs.py'), str(archive), str(root / 'out')], capture_output=True)
            self.assertNotEqual(result.returncode, 0)
            self.assertFalse((root / 'escaped').exists())


if __name__ == '__main__':
    unittest.main()
