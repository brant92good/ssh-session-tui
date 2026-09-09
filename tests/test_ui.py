from dataclasses import replace
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

from textual.widgets import DataTable, Input
from ssh_sessions.catalog import Catalog
from ssh_sessions.ui import Form, Picker, Routes
from ssh_sessions.import_ui import ImportSSH
from ssh_sessions.favorites import LOCAL, Favorites
from ssh_sessions.favorites_ui import FavoriteSlots
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
            self.assertEqual(app.query_one('#machines', DataTable).row_count, 2)
            await pilot.press('ctrl+l')
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

    async def test_keyboard_import_preview_selection_and_escape(self):
        config = Path(self.temporary.name) / 'config'
        config.write_text('Host one\n HostName 192.0.2.40\n User dev\nHost two\n HostName 192.0.2.50\n User dev\n', encoding='utf-8')
        app = Picker(self.catalog)
        with patch('ssh_sessions.import_ui.default_config', return_value=config):
            async with app.run_test(size=(100, 32)) as pilot:
                await pilot.press('i')
                await pilot.pause()
                self.assertIsInstance(app.screen, ImportSSH)
                self.assertEqual(len(self.catalog.load().machines), 1)
                await pilot.press('escape')
                self.assertEqual(len(self.catalog.load().machines), 1)
                await pilot.press('i', 'space', 'enter')
                await pilot.pause()
                self.assertEqual([m.name for m in self.catalog.load().machines], ['Work box 開發', 'one'])
                self.assertIsNone(app.return_value)
                await pilot.press('i', 'a', 'enter')
                await pilot.pause()
                self.assertEqual(len(self.catalog.load().machines), 3)
                await pilot.press('q')

    async def test_visible_local_terminal_works_with_an_empty_catalog(self):
        self.catalog.save((), self.catalog.load().revision)
        app = Picker(self.catalog)
        async with app.run_test(size=(70, 20)) as pilot:
            self.assertEqual(app.row_targets, [LOCAL])
            await pilot.press('enter')
        self.assertEqual(app.return_value.kind, 'local')

    async def test_number_selects_then_enter_connects_using_device_route(self):
        self.catalog.choose(sample(), 'vpn')
        Favorites(self.catalog).assign(1, sample().id)
        app = Picker(self.catalog)
        async with app.run_test(size=(100, 30)) as pilot:
            await pilot.press('down', '1')
            self.assertEqual(app.selected_key(), sample().id)
            self.assertIsNone(app.return_value)
            await pilot.press('enter')
        self.assertEqual(app.return_value.route.id, 'vpn')

    async def test_local_favorite_opens_local_shell(self):
        Favorites(self.catalog).assign(2, LOCAL)
        app = Picker(self.catalog)
        async with app.run_test(size=(70, 20)) as pilot:
            await pilot.press('2')
            self.assertIsNone(app.return_value)
            await pilot.press('enter')
        self.assertEqual(app.return_value.kind, 'local')

    async def test_compact_window_leaves_room_for_common_connections(self):
        Favorites(self.catalog).seed({'1': sample().id, '2': LOCAL})
        app = Picker(self.catalog)
        async with app.run_test(size=(70, 18)) as pilot:
            table = app.query_one('#machines', DataTable)
            self.assertGreaterEqual(table.content_size.height, 5)
            self.assertEqual(table.row_count, 2)
            await pilot.press('q')

    async def test_arrows_resume_selection_after_an_empty_number(self):
        self.catalog.choose(sample(), 'lan')
        app = Picker(self.catalog)
        async with app.run_test(size=(80, 25)) as pilot:
            await pilot.press('9', 'enter')
            self.assertIsNone(app.return_value)
            await pilot.press('down', 'enter')
        self.assertEqual(app.return_value.kind, 'local')

    async def test_numbers_are_text_while_searching_or_editing(self):
        Favorites(self.catalog).assign(1, sample().id)
        app = Picker(self.catalog)
        async with app.run_test(size=(100, 32)) as pilot:
            await pilot.press('/', '1', '2')
            self.assertEqual(app.query_one('#search', Input).value, '12')
            await pilot.press('escape', 'a', '1', '2')
            self.assertIsInstance(app.screen, Form)
            self.assertEqual(app.screen.query_one('#name', Input).value, '12')
            self.assertIsNone(app.return_value)
            await pilot.press('escape', 'q')

    async def test_favorites_menu_assigns_and_clears_without_connecting(self):
        app = Picker(self.catalog)
        async with app.run_test(size=(100, 32)) as pilot:
            await pilot.press('f')
            self.assertIsInstance(app.screen, FavoriteSlots)
            await pilot.press('3', 'enter')
            self.assertEqual(Favorites(self.catalog).load().slots, {'3': sample().id})
            self.assertIsNone(app.return_value)
            await pilot.press('f', '3', 'd')
            self.assertEqual(Favorites(self.catalog).load().slots, {})
            await pilot.press('q')

    async def test_unassigned_or_deleted_favorite_cannot_connect_an_adjacent_row(self):
        self.catalog.choose(sample(), 'lan')
        store = Favorites(self.catalog)
        store.assign(1, sample().id)
        self.catalog.save((), self.catalog.load().revision)
        app = Picker(self.catalog)
        async with app.run_test(size=(100, 30)) as pilot:
            await pilot.press('1', 'enter', '9', 'enter')
            self.assertIsNone(app.return_value)
            await pilot.press('escape', 'enter')
        self.assertEqual(app.return_value.kind, 'local')

    async def test_catalog_change_requires_review_before_enter_connects(self):
        self.catalog.choose(sample(), 'lan')
        app = Picker(self.catalog)
        async with app.run_test(size=(100, 30)) as pilot:
            changed = replace(sample(), routes=(replace(sample().routes[0], host='192.0.2.90'),))
            self.catalog.save((changed,), self.catalog.load().revision)
            await pilot.press('enter')
            self.assertIsNone(app.return_value)
            await pilot.press('enter')
        self.assertEqual(app.return_value.route.host, '192.0.2.90')

    async def test_number_keys_do_not_escape_route_modal(self):
        Favorites(self.catalog).assign(2, LOCAL)
        app = Picker(self.catalog)
        async with app.run_test(size=(100, 30)) as pilot:
            await pilot.press('down', 'r')
            self.assertIsInstance(app.screen, Routes)
            await pilot.press('2')
            self.assertIsInstance(app.screen, Routes)
            self.assertIsNone(app.return_value)
            await pilot.press('escape', 'q')
