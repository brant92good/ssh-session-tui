from dataclasses import replace
from pathlib import Path
import tempfile
import unittest

from textual.widgets import DataTable, Input
from ssh_sessions.catalog import Catalog
from ssh_sessions.ui import Form, Picker, Routes
from test_catalog import sample


class PickerTests(unittest.IsolatedAsyncioTestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        root = Path(self.temporary.name)
        self.catalog = Catalog(root / 'shared.json', root / 'device')
        self.catalog.save((sample(),), self.catalog.load().revision)

    async def test_ambiguous_machine_opens_route_picker_before_connecting(self):
        app = Picker(self.catalog)
        async with app.run_test(size=(100, 32)) as pilot:
            await pilot.press('enter')
            await pilot.pause()
            self.assertIsInstance(app.screen, Routes)
            self.assertIsNone(app.return_value)
            await pilot.press('down', 'enter')
            await pilot.pause()
            self.assertEqual(self.catalog.preferred(sample()).id, 'vpn')
            self.assertIsNone(app.return_value)
            await pilot.press('enter')
        self.assertEqual(app.return_value.route.id, 'vpn')

    async def test_failure_route_choice_is_temporary(self):
        self.catalog.choose(sample(), 'lan')
        app = Picker(self.catalog, notice='SSH failed', failed_machine=sample().id)
        async with app.run_test(size=(100, 32)) as pilot:
            await pilot.pause()
            self.assertIsInstance(app.screen, Routes)
            await pilot.press('down', 'enter')
        self.assertEqual(app.return_value.route.id, 'vpn')
        self.assertEqual(self.catalog.preferred(sample()).id, 'lan')

    async def test_escape_from_failure_does_not_connect(self):
        app = Picker(self.catalog, failed_machine=sample().id)
        async with app.run_test(size=(80, 25)) as pilot:
            await pilot.press('escape')
            await pilot.pause()
            self.assertIsNone(app.return_value)
            await pilot.press('q')
        self.assertIsNone(app.return_value)

    async def test_search_and_local_shell_shortcut(self):
        app = Picker(self.catalog)
        async with app.run_test(size=(70, 20)) as pilot:
            await pilot.press('/', 'n', 'o', 'n', 'e')
            self.assertEqual(app.query_one('#machines', DataTable).row_count, 0)
            await pilot.press('escape')
            self.assertEqual(app.query_one('#machines', DataTable).row_count, 1)
            await pilot.press('ctrl+n')
        self.assertEqual(app.return_value.kind, 'local')

    async def test_keyboard_add_machine(self):
        app = Picker(self.catalog)
        async with app.run_test(size=(100, 35)) as pilot:
            await pilot.press('a')
            await pilot.pause()
            for key, value in {'name': 'Another server', 'user': 'dev', 'route_name': 'LAN', 'host': '192.0.2.20', 'port': '22'}.items():
                app.screen.query_one('#' + key, Input).value = value
            await pilot.press('ctrl+s')
            await pilot.pause()
            self.assertEqual(len(self.catalog.load().machines), 2)
            await pilot.press('q')

    async def test_delete_defaults_to_cancel(self):
        app = Picker(self.catalog)
        async with app.run_test(size=(100, 32)) as pilot:
            await pilot.press('d', 'enter')
            await pilot.pause()
            self.assertEqual(len(self.catalog.load().machines), 1)
            await pilot.press('q')

    async def test_invalid_form_keeps_entered_values_for_correction(self):
        app = Picker(self.catalog)
        async with app.run_test(size=(80, 25)) as pilot:
            await pilot.press('a')
            await pilot.pause()
            for key, value in {'name': 'Draft machine', 'user': 'dev', 'route_name': 'LAN', 'host': '192.0.2.20', 'port': 'invalid'}.items():
                app.screen.query_one('#' + key, Input).value = value
            await pilot.press('ctrl+s')
            await pilot.pause()
            self.assertIsInstance(app.screen, Form)
            self.assertEqual(app.screen.query_one('#name', Input).value, 'Draft machine')
            self.assertEqual(len(self.catalog.load().machines), 1)
            await pilot.press('escape', 'q')
