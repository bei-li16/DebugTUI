"""Exercise fail-closed packaging checks against real compiled Windows binaries."""
import argparse
import json
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest
import zipfile


sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import windows


class WindowsPackageTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.binaries = self.root / 'bin'
        self.binaries.mkdir()

    def copy_binaries(self, omit=None):
        for name in ['openocd.exe', 'libusb-1.0.dll', 'libhidapi-0.dll']:
            if name != omit:
                shutil.copyfile(CANDIDATE / 'bin' / name, self.binaries / name)

    def closure(self):
        return windows.dll_closure(self.binaries, OBJDUMP, SYSTEM_DLLS)

    def test_real_pe_dependencies_are_complete(self):
        self.copy_binaries()
        dependencies = self.closure()
        self.assertIn('libusb-1.0.dll', dependencies['openocd.exe'])
        self.assertIn('libhidapi-0.dll', dependencies['openocd.exe'])
        self.assertEqual(set(dependencies), {'openocd.exe', 'libusb-1.0.dll', 'libhidapi-0.dll'})

    def test_missing_hid_cannot_resolve_from_other_directory(self):
        self.copy_binaries(omit='libhidapi-0.dll')
        shutil.copyfile(CANDIDATE / 'bin/libhidapi-0.dll', self.root / 'libhidapi-0.dll')
        with self.assertRaisesRegex(ValueError, 'libhidapi-0.dll'):
            self.closure()

    def test_missing_usb_is_rejected_before_execution(self):
        self.copy_binaries(omit='libusb-1.0.dll')
        with self.assertRaisesRegex(ValueError, 'libusb-1.0.dll'):
            self.closure()

    def test_non_pe_dependency_is_rejected(self):
        self.copy_binaries()
        (self.binaries / 'libusb-1.0.dll').write_bytes(b'\x7fELF' + bytes(64))
        with self.assertRaises((ValueError, subprocess.CalledProcessError)):
            self.closure()

    def test_same_size_changed_file_is_rejected(self):
        configuration = self.root / 'profile.cfg'
        configuration.write_bytes(b'original')
        expected = windows.manifest(self.root)
        configuration.write_bytes(b'modified')
        with self.assertRaisesRegex(ValueError, 'Staged files changed'):
            windows.check_manifest(self.root, expected)

    def test_added_dll_is_rejected(self):
        self.copy_binaries()
        expected = windows.manifest(self.root)
        (self.binaries / 'unexpected.dll').write_bytes(b'unapproved')
        with self.assertRaisesRegex(ValueError, 'Staged files changed'):
            windows.check_manifest(self.root, expected)

    def test_corresponding_source_contains_the_compiled_adapter(self):
        windows.check_source_archive(SOURCE_ARCHIVE, SOURCE_LOCK)

    def test_valid_zip_with_changed_patch_is_rejected(self):
        changed = self.root / 'changed-source.zip'
        with zipfile.ZipFile(SOURCE_ARCHIVE) as original, zipfile.ZipFile(changed, 'w') as bundle:
            for name in SOURCE_LOCK['patched_sources_sha256']:
                member = 'openocd-source/' + name
                bundle.writestr(member, original.read(member))
            member = 'recipe/openocd-adapter/' + SOURCE_LOCK['patch']
            bundle.writestr(member, original.read(member) + b'altered')
        with self.assertRaisesRegex(ValueError, 'different patch'):
            windows.check_source_archive(changed, SOURCE_LOCK)

    def test_source_archive_cannot_omit_empty_git_refs(self):
        changed = self.root / 'lost-refs.zip'
        with zipfile.ZipFile(SOURCE_ARCHIVE) as original, zipfile.ZipFile(changed, 'w') as bundle:
            for name in SOURCE_LOCK['patched_sources_sha256']:
                member = 'openocd-source/' + name
                bundle.writestr(member, original.read(member))
            member = 'recipe/openocd-adapter/' + SOURCE_LOCK['patch']
            bundle.writestr(member, original.read(member))
            # This reproduces the original files-only ZIP packaging bug.
        with self.assertRaisesRegex(ValueError, 'lost its refs directory'):
            windows.check_source_archive(changed, SOURCE_LOCK)


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--candidate', required=True, type=Path)
    parser.add_argument('--source-archive', required=True, type=Path)
    parser.add_argument('--objdump', default='objdump')
    args = parser.parse_args()
    CANDIDATE = args.candidate.resolve()
    OBJDUMP = args.objdump
    SYSTEM_DLLS = windows.read_json(windows.HERE / 'windows-dependencies.lock.json')['system_dlls']
    SOURCE_ARCHIVE = args.source_archive.resolve()
    SOURCE_LOCK = windows.read_json(windows.HERE / 'source.lock.json')
    suite = unittest.defaultTestLoader.loadTestsFromTestCase(WindowsPackageTests)
    result = unittest.TextTestRunner(verbosity=2).run(suite)
    print(json.dumps({'passed': result.testsRun - len(result.errors) - len(result.failures),
                      'failed': len(result.errors) + len(result.failures),
                      'board_tests_executed': False}))
    sys.exit(0 if result.wasSuccessful() else 1)
