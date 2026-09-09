import contextlib
import io
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

from ssh_sessions.catalog import Catalog
from ssh_sessions.cli import main, picker_loop
from ssh_sessions.connection import ssh_command
from ssh_sessions.ui import Choice
from test_catalog import sample


class ConnectionTests(unittest.TestCase):
    def setUp(self):
        self.enterContext(patch('ssh_sessions.cli.sys.stdin', io.StringIO()))
        self.enterContext(patch('ssh_sessions.cli.set_title'))

    def test_argv_uses_existing_client_without_remote_commands_or_key_paths(self):
        args = ssh_command(sample(), sample().routes[1], 'C:/Program Files/OpenSSH/ssh.exe')
        self.assertEqual(args[-7:], ['ConnectionAttempts=1', '-p', '2222', '-l', 'developer', '--', 'workbox.example.test'])
        self.assertNotIn('-i', args)
        self.assertNotIn('-F', args)
        self.assertNotIn('StrictHostKeyChecking=no', args)

    def test_failure_offers_ui_choice_and_does_not_auto_try_another_route(self):
        with tempfile.TemporaryDirectory() as folder:
            catalog = Catalog(Path(folder) / 'catalog.json', Path(folder) / 'device')
            catalog.choose(sample(), 'lan')
            choices = iter((Choice('connect', sample(), sample().routes[0]), None))
            prompts, commands = [], []
            class FakePicker:
                def __init__(self, _catalog, **kwargs): prompts.append(kwargs)
                def run(self): return next(choices)
            with patch('ssh_sessions.cli.ssh_command', return_value=['ssh', 'example']), contextlib.redirect_stdout(io.StringIO()):
                picker_loop(catalog, FakePicker, lambda cmd: commands.append(cmd) or 255)
            self.assertEqual(len(commands), 1)
            self.assertEqual(prompts[1]['failed_machine'], sample().id)
            self.assertEqual(catalog.preferred(sample()).id, 'lan')

    def test_explicit_fallback_connects_once_without_replacing_preference(self):
        with tempfile.TemporaryDirectory() as folder:
            catalog = Catalog(Path(folder) / 'catalog.json', Path(folder) / 'device')
            catalog.choose(sample(), 'lan')
            choices = iter((Choice('connect', sample(), sample().routes[0]), Choice('connect', sample(), sample().routes[1]), None))
            commands = []
            class FakePicker:
                def __init__(self, *args, **kwargs): pass
                def run(self): return next(choices)
            with patch('ssh_sessions.connection.shutil.which', return_value='ssh'), contextlib.redirect_stdout(io.StringIO()):
                picker_loop(catalog, FakePicker, lambda command: commands.append(command) or 255)
            self.assertEqual([cmd[-1] for cmd in commands], ['192.0.2.10', 'workbox.example.test'])
            self.assertEqual(catalog.preferred(sample()).id, 'lan')

    def test_cli_list_does_not_connect_or_create_catalog(self):
        with tempfile.TemporaryDirectory() as folder:
            path = Path(folder) / 'missing.json'
            with patch('subprocess.call', side_effect=AssertionError('Must not connect')), contextlib.redirect_stdout(io.StringIO()) as output:
                self.assertEqual(main(['--catalog', str(path), 'list', '--json']), 0)
            self.assertIn('"machines": []', output.getvalue())
            self.assertFalse(path.exists())

    def test_local_selection_runs_shell_then_returns_to_picker_without_ssh(self):
        choices = iter((Choice('local'), None))
        commands = []
        class FakePicker:
            def __init__(self, *args, **kwargs): pass
            def run(self): return next(choices)
        with patch('ssh_sessions.cli.local_command', return_value=['pwsh', '-NoLogo']), \
             patch('ssh_sessions.cli.ssh_command', side_effect=AssertionError('Local must not invoke SSH')):
            self.assertEqual(picker_loop(None, FakePicker, lambda cmd: commands.append(cmd) or 0), 0)
        self.assertEqual(commands, [['pwsh', '-NoLogo']])
