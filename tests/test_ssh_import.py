import contextlib
from dataclasses import replace
import io
import getpass
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest
from unittest.mock import patch

from ssh_sessions.catalog import Catalog, CatalogError, Machine, Route
from ssh_sessions.cli import main
from ssh_sessions.connection import ssh_command
from ssh_sessions.ssh_import import bindings_path, connection_config, import_selected, scan_ssh


class SSHImportTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.config = self.root / 'SSH settings 測試/config'
        self.config.parent.mkdir()
        self.catalog = Catalog(self.root / 'shared/catalog.json', self.root / 'device')

    def write_config(self, text):
        self.config.write_text(text, encoding='utf-8')
        return scan_ssh(self.config)

    def import_hosts(self, scan, *aliases):
        return import_selected(self.catalog, scan, aliases, self.catalog.load().revision)

    def test_static_metadata_include_precedence_negation_and_parent_scope(self):
        included = self.config.parent / 'included file'
        included.write_text('Host web second\n HostName 192.0.2.10\n User dev\n Port 2222\nHost other\n User other\n', encoding='utf-8')
        scan = self.write_config(f'Include "{included.as_posix()}"\nHost * !other\n User fallback\nHost web\n User too-late\n')
        rows = {e.alias: e for e in scan.entries}
        self.assertEqual((rows['web'].host, rows['web'].user, rows['web'].port), ('192.0.2.10', 'dev', 2222))
        self.assertEqual(rows['second'].user, 'dev')
        self.assertEqual(rows['other'].user, 'other')
        self.assertEqual(set(rows), {'web', 'second', 'other'})

    def test_inactive_include_does_not_apply_its_host_blocks(self):
        included = self.config.parent / 'conditional'
        included.write_text('Host web\n HostName 192.0.2.99\n', encoding='utf-8')
        scan = self.write_config(f'Host other\n Include "{included.as_posix()}"\nHost web\n HostName 192.0.2.10\n User dev\n')
        self.assertEqual(next(e.host for e in scan.entries if e.alias == 'web'), '192.0.2.10')

    def test_match_exec_is_never_run_and_uncertain_metadata_is_refused(self):
        with patch('subprocess.run', side_effect=AssertionError('Import must not execute commands')):
            scan = self.write_config('Host web\n HostName 192.0.2.10\n User dev\nMatch exec "touch never-run"\n Port 2222\n')
        self.assertIn('Conditional', scan.entries[0].problem)
        with self.assertRaises(CatalogError): self.import_hosts(scan, 'web')
        self.assertFalse(self.catalog.path.exists())

    def test_key_and_proxy_settings_are_not_exported_or_executed(self):
        scan = self.write_config('Host cloud\n HostName cloud.example.test\n User dev\n IdentityFile sensitive-key-path\n ProxyCommand helper --hostname %h\n')
        self.import_hosts(scan, 'cloud')
        shared = self.catalog.path.read_text()
        for value in ('sensitive-key-path', 'ProxyCommand', 'IdentityFile', str(self.config)):
            self.assertNotIn(value, shared)
        machine = self.catalog.load().machines[0]
        route = machine.routes[0]
        args = ssh_command(machine, route, 'ssh', config=connection_config(self.catalog, machine, route))
        self.assertEqual(args[-1], 'cloud')
        self.assertIn('HostName=cloud.example.test', args)
        self.assertEqual(args[args.index('-F') + 1], str(self.config))
        self.assertNotIn('-i', args)

    def test_existing_machine_gets_route_and_keeps_previous_device_choice(self):
        original = Machine('existing', 'My saved name', 'dev', (Route('lan', 'LAN', '192.0.2.10'),))
        self.catalog.save((original,), self.catalog.load().revision)
        scan = self.write_config('Host web\n HostName 192.0.2.10\n User dev\n')
        result = self.import_hosts(scan, 'web')
        machine = self.catalog.load().machines[0]
        self.assertEqual(result, {'added_machines': 0, 'added_routes': 1, 'bound_existing': 0})
        self.assertEqual(machine.name, original.name)
        self.assertEqual(len(machine.routes), 2)
        self.assertEqual(self.catalog.preferred(machine).id, 'lan')
        before = self.catalog.path.read_bytes()
        self.assertEqual(self.import_hosts(scan, 'web')['bound_existing'], 1)
        self.assertEqual(self.catalog.path.read_bytes(), before)
        self.assertFalse(bindings_path(self.catalog).is_relative_to(self.catalog.path.parent))

    def test_another_device_requires_the_local_alias_and_can_bind_by_import(self):
        scan = self.write_config('Host web\n HostName 192.0.2.10\n User dev\n')
        self.import_hosts(scan, 'web')
        other = Catalog(self.catalog.path, self.root / 'device-b')
        machine = other.load().machines[0]
        with patch('ssh_sessions.ssh_import.default_config', return_value=self.root / 'missing'):
            with self.assertRaisesRegex(CatalogError, 'needs local SSH alias'):
                connection_config(other, machine, machine.routes[0])
        import_selected(other, scan, ['web'], other.load().revision)
        self.assertEqual(connection_config(other, machine, machine.routes[0]), self.config)

    def test_changed_config_or_catalog_after_preview_preserves_saved_data(self):
        scan = self.write_config('Host web\n HostName 192.0.2.10\n User dev\n')
        expected = self.catalog.load().revision
        self.config.write_text('Host changed\n', encoding='utf-8')
        with self.assertRaisesRegex(CatalogError, 'changed after preview'):
            import_selected(self.catalog, scan, ['web'], expected)
        scan = self.write_config('Host web\n HostName 192.0.2.10\n User dev\n')
        self.catalog.save((), expected)
        before = self.catalog.path.read_bytes()
        with self.assertRaisesRegex(CatalogError, 'another tab'):
            import_selected(self.catalog, scan, ['web'], expected)
        self.assertEqual(self.catalog.path.read_bytes(), before)

    def test_changed_imported_alias_is_not_silently_overwritten(self):
        scan = self.write_config('Host web\n HostName 192.0.2.10\n User dev\n')
        self.import_hosts(scan, 'web')
        before = self.catalog.path.read_bytes()
        scan = self.write_config('Host web\n HostName 192.0.2.20\n User dev\n')
        with self.assertRaisesRegex(CatalogError, 'different details'):
            self.import_hosts(scan, 'web')
        self.assertEqual(self.catalog.path.read_bytes(), before)

    def test_cycles_tokens_and_malformed_quotes_are_handled(self):
        with self.assertRaisesRegex(CatalogError, 'cycle'):
            self.write_config(f'Include "{self.config.as_posix()}"\n')
        scan = self.write_config('Include ${UNKNOWN}/config\nHost web\n HostName 192.0.2.10\n User dev\n')
        self.assertTrue(scan.entries[0].problem)
        with self.assertRaisesRegex(CatalogError, 'Unclosed quote'):
            self.write_config('Host "unterminated\n')

    def test_cli_preview_is_read_only_and_apply_requires_selection(self):
        self.write_config('Host web\n HostName 192.0.2.10\n User dev\n')
        args = ['--catalog', str(self.catalog.path), '--state-dir', str(self.catalog.state_dir), 'import-ssh', '--config', str(self.config), '--json']
        with contextlib.redirect_stdout(io.StringIO()) as output:
            self.assertEqual(main(args), 0)
        self.assertFalse(json.loads(output.getvalue())['applied'])
        self.assertFalse(self.catalog.path.exists())
        with contextlib.redirect_stdout(io.StringIO()):
            self.assertEqual(main([*args, '--apply']), 1)
            self.assertEqual(main([*args, '--apply', '--host', 'web']), 0)
        self.assertEqual(len(self.catalog.load().machines), 1)

    def test_default_user_config_uses_system_defaults_but_custom_config_does_not(self):
        system = self.root / 'system-config'
        system.write_text('Host *\n User system-user\n Port 2200\n', encoding='utf-8')
        self.write_config('Host web\n HostName 192.0.2.10\n')
        with patch('ssh_sessions.ssh_import.system_config', return_value=system), patch('ssh_sessions.ssh_import.default_config', return_value=self.config):
            entry = scan_ssh().entries[0]
        self.assertEqual((entry.user, entry.port), ('system-user', 2200))
        with patch('ssh_sessions.ssh_import.system_config', return_value=system), patch('ssh_sessions.ssh_import.getpass.getuser', return_value='local-user'):
            entry = scan_ssh(self.config).entries[0]
        self.assertEqual((entry.user, entry.port), ('local-user', 22))

    @unittest.skipUnless(shutil.which('ssh'), 'OpenSSH is needed for the isolated parser comparison')
    def test_static_fixture_matches_real_openssh_configuration_output(self):
        included = self.config.parent / 'defaults'
        included.write_text('Host web second\n HostName 192.0.2.10\n User dev\n Port 2222\n'
                           'Host UPPER\n User uppercase\nHost upper\n User lowercase\n'
                           'Host web[12]\n User brackets\nHost web1\n', encoding='utf-8')
        scan = self.write_config(f'Include "{included.as_posix()}"\nHost *\n User fallback\n Port 22\n')
        if os.name == 'nt':
            # Shared runner/temp ACLs can contain extra writable principals.
            # Restrict only this generated Include file, never the user's config.
            subprocess.run(['icacls', str(included), '/inheritance:r', '/grant:r', getpass.getuser() + ':(F)'],
                check=True, capture_output=True, timeout=8, creationflags=subprocess.CREATE_NO_WINDOW)
        # This generated fixture has no Match exec, helper, or user configuration.
        # Only this test invokes -G; production discovery never invokes SSH.
        for alias in ('web', 'UPPER', 'upper', 'web1'):
            with self.subTest(alias=alias):
                result = subprocess.run(['ssh', '-G', '-F', str(self.config), alias], capture_output=True,
                    text=True, timeout=8, creationflags=getattr(subprocess, 'CREATE_NO_WINDOW', 0))
                self.assertEqual(result.returncode, 0, result.stderr)
                actual = dict(line.split(' ', 1) for line in result.stdout.splitlines() if ' ' in line)
                entry = next(e for e in scan.entries if e.alias == alias)
                self.assertEqual((entry.host, entry.user, entry.port), (actual['hostname'], actual['user'], int(actual['port'])))
