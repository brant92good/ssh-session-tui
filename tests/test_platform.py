import os
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

from ssh_sessions.catalog import Catalog
from ssh_sessions.cli import default_directory
from ssh_sessions.connection import local_command


class PlatformTests(unittest.TestCase):
    @unittest.skipIf(os.name == 'nt', 'Unix shell selection')
    def test_uses_configured_shell_even_when_pwsh_is_installed(self):
        with patch.dict(os.environ, {'SHELL': '/bin/sh'}):
            self.assertEqual(local_command(), ['/bin/sh'])

    @unittest.skipUnless(os.name == 'nt', 'Windows shell selection')
    def test_windows_without_pwsh_can_open_builtin_shell(self):
        with patch('ssh_sessions.connection.shutil.which', side_effect=lambda name:
                   'C:/Windows/powershell.exe' if name == 'powershell.exe' else None):
            self.assertEqual(local_command(), ['C:/Windows/powershell.exe', '-NoLogo'])

    @unittest.skipIf(os.name == 'nt', 'Unix data paths')
    def test_platform_data_locations_and_case_sensitive_catalog_identity(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            with patch('ssh_sessions.cli.sys.platform', 'linux'), patch.dict(os.environ, {'XDG_DATA_HOME': folder}):
                self.assertEqual(default_directory(), root / 'SSHSessions')
            with patch('ssh_sessions.cli.sys.platform', 'darwin'), patch('pathlib.Path.home', return_value=root):
                self.assertEqual(default_directory(), root / 'Library/Application Support/SSHSessions')
            lower = Catalog(root / 'one.json', root / 'device')
            upper = Catalog(root / 'ONE.json', root / 'device')
            self.assertNotEqual(lower.device_path, upper.device_path)
