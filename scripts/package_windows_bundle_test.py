import importlib.util
from contextlib import closing
import json
from pathlib import Path
import sqlite3
import tempfile
import unittest
import zipfile

script = Path(__file__).with_name('package-windows-bundle.py')
spec = importlib.util.spec_from_file_location('windows_bundle', script)
bundle = importlib.util.module_from_spec(spec)
spec.loader.exec_module(bundle)
root = script.parent.parent


class BundleTests(unittest.TestCase):
    def test_release_directory_removes_generated_files_but_never_a_home(self):
        with tempfile.TemporaryDirectory() as temporary:
            release = Path(temporary) / 'release'
            release.mkdir()
            generated = ['devone-local.exe', 'bundled-artifacts.json', 'release-artifacts.json',
                         'DEVONE-Local-0.0.9-Windows-x64-Setup.exe',
                         'DEVONE-Local-0.1.0-Windows-x64-Portable.zip']
            for name in generated:
                (release / name).write_bytes(b'old')
            # A Home created by running an executable inside release/.
            (release / 'www' / 'project').mkdir(parents=True)
            (release / 'devone.db').write_bytes(b'user state')

            bundle.prepare_release_directory(release)

            for name in generated:
                self.assertFalse((release / name).exists(), name)
            self.assertTrue((release / 'www' / 'project').is_dir())
            self.assertEqual((release / 'devone.db').read_bytes(), b'user state')

    def test_publish_moves_both_artifacts_and_writes_one_metadata_file(self):
        with tempfile.TemporaryDirectory() as temporary:
            release = Path(temporary) / 'release'
            release.mkdir()
            staging = release / '.build-test'
            staging.mkdir()
            names = bundle.release_names('0.1.0')
            artifacts = []
            for kind, name in names.items():
                (staging / name).write_bytes(kind.encode())
                artifacts.append({'filename': name, 'kind': kind})
            (release / 'bundled-artifacts.json').write_text('{}')

            bundle.publish(release, staging, artifacts)

            self.assertEqual(sorted(p.name for p in release.iterdir() if p != staging),
                             sorted([*names.values(), 'release-artifacts.json']))
            self.assertEqual((release / names['installer']).read_bytes(), b'installer')
            metadata = json.loads((release / 'release-artifacts.json').read_text())
            self.assertEqual([a['filename'] for a in metadata], [a['filename'] for a in artifacts])

    def test_installer_hooks_guard_home_and_remove_only_bundled_runtimes(self):
        manifests = json.loads((script.parent / 'windows-bundle.json').read_text())['manifests']
        with tempfile.TemporaryDirectory() as temporary:
            home = Path(temporary) / 'devone-local'
            (home / 'runtimes').mkdir(parents=True)
            (home / 'README.txt').write_text('readme')
            hooks = bundle.installer_hooks(home, manifests)
        self.assertIn('Function .onVerifyInstDir', hooks)
        self.assertIn('"$PROGRAMFILES64" devone_bad_dir', hooks)
        uninstall = hooks.split('!macro NSIS_HOOK_POSTUNINSTALL', 1)[1]
        self.assertIn('${If} $UpdateMode <> 1', uninstall)
        recursive = [line.strip() for line in uninstall.splitlines() if 'RMDir /r' in line]
        self.assertEqual(recursive, [
            f'RMDir /r "$INSTDIR\\runtimes\\{m["runtime"]}\\{m["version"]}"'
            for m in sorted(manifests, key=lambda m: (m['runtime'], m['version']))])
        for data in ['www', 'database', 'backups', 'certs', 'devone.db']:
            self.assertNotIn(f'RMDir /r "$INSTDIR\\{data}', uninstall)
            self.assertNotIn(f'Delete "$INSTDIR\\{data}', uninstall)

    def test_registry_contains_relative_runtimes_and_no_user_state(self):
        manifests = json.loads((script.parent / 'windows-bundle.json').read_text())['manifests']
        with tempfile.TemporaryDirectory() as temporary:
            home = Path(temporary)
            bundle.seed_registry(root, home, manifests)
            with closing(sqlite3.connect(home / 'devone.db')) as conn:
                for kind, version, relative in conn.execute('SELECT kind,version,relative_path FROM runtime_installations'):
                    self.assertEqual(relative, f'runtimes/{kind}/{version}')
                    self.assertEqual(conn.execute('SELECT value FROM settings WHERE key=?', (f'default.{kind}',)).fetchone()[0], version)
                for table in ['sites', 'database_instances', 'certificates', 'process_state']:
                    self.assertEqual(conn.execute(f'SELECT count(*) FROM {table}').fetchone()[0], 0)
                self.assertEqual(conn.execute('SELECT count(*) FROM settings').fetchone()[0], 4)

    def test_archive_traversal_is_rejected_without_writing_outside_runtime(self):
        with tempfile.TemporaryDirectory() as temporary:
            home = Path(temporary)
            archive = home / 'bad.zip'
            with zipfile.ZipFile(archive, 'w') as output:
                output.writestr('../escaped.exe', b'bad')
            with self.assertRaisesRegex(ValueError, 'Unsafe archive path'):
                bundle.extract_runtime(archive, {}, home / 'runtime')
            self.assertFalse((home / 'escaped.exe').exists())

    def test_archive_link_is_rejected(self):
        with tempfile.TemporaryDirectory() as temporary:
            home = Path(temporary)
            archive = home / 'bad.zip'
            with zipfile.ZipFile(archive, 'w') as output:
                item = zipfile.ZipInfo('link.exe')
                item.external_attr = (0o120777 << 16)
                output.writestr(item, 'elsewhere')
            with self.assertRaisesRegex(ValueError, 'contains a link'):
                bundle.extract_runtime(archive, {}, home / 'runtime')
            self.assertFalse((home / 'runtime/link.exe').exists())


if __name__ == '__main__':
    unittest.main()
