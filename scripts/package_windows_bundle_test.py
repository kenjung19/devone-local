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
