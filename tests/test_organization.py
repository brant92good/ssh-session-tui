import contextlib
from dataclasses import replace
import io
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

from textual.widgets import DataTable, Input, Static
from ssh_sessions.catalog import Catalog, CatalogError, Machine, Route, decode, encode, group_path, parse_tags
from ssh_sessions.cli import main
from ssh_sessions.favorites import Favorites, LOCAL
from ssh_sessions.groups_ui import GroupBrowser
from ssh_sessions.organization import edit_many, filtered, groups, rename_group
from ssh_sessions.ssh_import import import_selected, scan_ssh
from ssh_sessions.ui import Picker


def fleet():
    route = (Route('lan', 'LAN', '192.0.2.10'),)
    return (Machine('train', 'Trainer', 'dev', route, 'Work/GPU', ('gpu', 'linux')),
            Machine('build', 'Builder', 'dev', route, 'Work/Build', ('ci',)),
            Machine('home', 'Home box', 'dev', route, 'Home', ('linux',)),
            Machine('spare', 'Spare box', 'dev', route))


class OrganizationTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)
        self.catalog = Catalog(self.root / 'catalog.json', self.root / 'state')
        self.snapshot = self.catalog.save(fleet(), self.catalog.load().revision)

    def test_old_catalog_roundtrip_and_v2_grouped_roundtrip(self):
        old = replace(fleet()[0], group='', tags=())
        raw = encode((old,))
        self.assertEqual(json.loads(raw)['version'], 1)
        self.assertEqual(decode(raw), (old,))
        new = encode(fleet())
        self.assertEqual(json.loads(new)['version'], 2)
        self.assertEqual(decode(new), fleet())
        mismatched = json.loads(new)
        mismatched['version'] = 1
        with self.assertRaisesRegex(CatalogError, 'version 2'):
            decode(json.dumps(mismatched).encode())

    def test_group_counts_include_descendants_but_not_similar_prefixes(self):
        self.assertEqual(groups(fleet()), {'Home': 1, 'Work': 2, 'Work/Build': 1, 'Work/GPU': 1})
        self.assertEqual([m.id for m in filtered(fleet(), group='work')], ['train', 'build'])
        self.assertEqual([m.id for m in filtered(fleet(), group='')], ['spare'])
        self.assertEqual(filtered(fleet(), group='Wor'), [])

    def test_search_combines_words_tags_and_group_scope(self):
        self.assertEqual([m.id for m in filtered(fleet(), 'group:Work tag:LINUX train')], ['train'])
        self.assertEqual(filtered(fleet(), 'tag:gpu', group='Home'), [])
        self.assertEqual([m.id for m in filtered(fleet(), '192.0.2.10', tag='ci')], ['build'])
        filtered(fleet(), '"unfinished')

    def test_bulk_move_and_tag_edit_preserve_ids_routes_and_other_machines(self):
        Favorites(self.catalog).assign(1, 'train')
        self.catalog.choose(fleet()[0], 'lan')
        preferences = self.catalog.device_path.read_bytes()
        changed = edit_many(self.catalog, ['train', 'build'], self.snapshot.revision,
                            group='Research/GPU', add_tags=['Reviewed'], remove_tags=['linux'])
        self.assertEqual(changed.machines[0].tags, ('gpu', 'Reviewed'))
        self.assertEqual(changed.machines[1].tags, ('ci', 'Reviewed'))
        self.assertEqual(changed.machines[2:], fleet()[2:])
        self.assertEqual(changed.machines[0].routes, fleet()[0].routes)
        self.assertEqual(Favorites(self.catalog).load().slots, {'1': 'train'})
        self.assertEqual(self.catalog.device_path.read_bytes(), preferences)

    def test_parent_rename_moves_children_and_preserves_ids(self):
        changed = rename_group(self.catalog, 'Work', 'Projects', self.snapshot.revision)
        self.assertEqual([m.group for m in changed.machines], ['Projects/GPU', 'Projects/Build', 'Home', ''])
        self.assertEqual([m.id for m in changed.machines], [m.id for m in fleet()])

    def test_stale_and_invalid_bulk_edits_preserve_catalog(self):
        raw = self.catalog.path.read_bytes()
        for ids, kwargs in [(['missing'], {'group': 'New'}), (['train'], {'group': 'Bad//Path'}),
                             (['train'], {'add_tags': ['same'], 'remove_tags': ['SAME']})]:
            with self.subTest(ids=ids, kwargs=kwargs), self.assertRaises(CatalogError):
                edit_many(self.catalog, ids, self.snapshot.revision, **kwargs)
            self.assertEqual(self.catalog.path.read_bytes(), raw)
        edit_many(self.catalog, ['home'], self.snapshot.revision, group='Personal')
        latest = self.catalog.path.read_bytes()
        with self.assertRaises(CatalogError):
            edit_many(self.catalog, ['train'], self.snapshot.revision, group='New')
        self.assertEqual(self.catalog.path.read_bytes(), latest)

    def test_group_rename_conflict_and_self_nesting_are_rejected(self):
        raw = self.catalog.path.read_bytes()
        for new in ('Home', 'Work/Child', ''):
            with self.subTest(new=new), self.assertRaises(CatalogError):
                rename_group(self.catalog, 'Work', new, self.snapshot.revision)
            self.assertEqual(self.catalog.path.read_bytes(), raw)

    def test_group_and_tag_validation(self):
        self.assertEqual(group_path(' Work / GPU '), 'Work/GPU')
        self.assertEqual(parse_tags('gpu, Linux, GPU'), ('gpu', 'Linux'))
        for value in ('a//b', '/a', 'a/', 'a/../b', 'a\nb', '/'.join(['a'] * 9)):
            with self.subTest(value=value), self.assertRaises(CatalogError):
                group_path(value)

    def test_import_adds_new_hosts_to_group_and_preserves_existing_organization(self):
        config = self.root / 'ssh-config'
        config.write_text('Host imported\n HostName 192.0.2.30\n User dev\nHost trainer\n HostName 192.0.2.10\n User dev\n')
        import_selected(self.catalog, scan_ssh(config), ['imported', 'trainer'], self.snapshot.revision, group='Imported')
        machines = self.catalog.load().machines
        self.assertEqual(next(m.group for m in machines if m.name == 'imported'), 'Imported')
        self.assertEqual(next(m.group for m in machines if m.id == 'train'), 'Work/GPU')
        self.assertEqual(next(m.tags for m in machines if m.id == 'train'), ('gpu', 'linux'))

    def test_cli_bulk_edit_filter_and_ungroup(self):
        prefix = ['--catalog', str(self.catalog.path), '--state-dir', str(self.catalog.state_dir)]
        with patch('subprocess.call', side_effect=AssertionError('Organization must not connect')):
            for args in (['organize', '--machine', 'train', '--machine', 'build', '--group', 'Lab', '--add-tag', 'gpu'],
                         ['groups', 'rename', 'Lab', 'Research'], ['list', '--group', 'Research', '--tag', 'gpu'],
                         ['organize', '--machine', 'train', '--ungrouped'], ['list', '--ungrouped']):
                with contextlib.redirect_stdout(io.StringIO()) as output:
                    self.assertEqual(main([*prefix, *args, '--json']), 0)
                self.assertTrue(json.loads(output.getvalue())['ok'])
        self.assertEqual({m['id'] for m in json.loads(output.getvalue())['machines']}, {'train', 'spare'})

    def test_one_thousand_machines_can_be_grouped_and_searched(self):
        machines = tuple(Machine(f'host{i}', f'Host {i}', 'dev', (Route('lan', 'LAN', '192.0.2.1'),),
                                 f'Fleet/Batch{i // 100}', ('gpu' if i % 2 else 'cpu',)) for i in range(1000))
        self.catalog.save(machines, self.snapshot.revision)
        self.assertEqual(len(filtered(self.catalog.load().machines, 'tag:gpu', 'Fleet/Batch9')), 50)
        changed = rename_group(self.catalog, 'Fleet', 'Servers', self.catalog.load().revision)
        self.assertEqual(groups(changed.machines)['Servers'], 1000)


class GroupKeyboardTests(unittest.IsolatedAsyncioTestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        root = Path(temporary.name)
        self.catalog = Catalog(root / 'catalog.json', root / 'device')
        self.catalog.save(fleet(), self.catalog.load().revision)

    async def test_group_browser_filters_and_escape_restores_all(self):
        app = Picker(self.catalog)
        async with app.run_test(size=(100, 32)) as pilot:
            await pilot.press('g')
            self.assertIsInstance(app.screen, GroupBrowser)
            app.screen.query_one(Input).value = 'Work/GPU'
            await pilot.pause()
            app.screen.query_one(DataTable).focus()
            await pilot.press('enter')
            self.assertEqual([m.id for m in app.filtered_machines], ['train'])
            await pilot.press('escape')
            self.assertEqual(len(app.filtered_machines), 4)
            await pilot.press('q')

    async def test_bulk_selection_move_and_tag_edit(self):
        app = Picker(self.catalog)
        async with app.run_test(size=(100, 35)) as pilot:
            await pilot.press('space', 'down', 'space', 'm')
            app.screen.query_one('#group', Input).value = 'Projects/GPU'
            await pilot.press('ctrl+s')
            self.assertEqual([m.group for m in self.catalog.load().machines[:2]], ['Projects/GPU'] * 2)
            self.assertEqual(str(app.query_one('#notice', Static).content), 'Updated 2 machines.')
            await pilot.press('ctrl+a', 't')
            app.screen.query_one('#add_tags', Input).value = 'reviewed'
            app.screen.query_one('#remove_tags', Input).value = 'linux'
            await pilot.press('ctrl+s')
            self.assertTrue(all('reviewed' in m.tags and 'linux' not in m.tags for m in self.catalog.load().machines))
            self.assertIsNone(app.return_value)
            await pilot.press('q')

    async def test_favorite_reaches_machine_outside_filtered_group(self):
        Favorites(self.catalog).seed({'1': 'train', '2': LOCAL})
        app = Picker(self.catalog)
        async with app.run_test(size=(70, 20)) as pilot:
            app.group_filter = 'Home'
            app.refresh_rows()
            await pilot.press('1')
            self.assertEqual(app.selected_key(), 'train')
            self.assertIsNone(app.group_filter)
            await pilot.press('enter')
        self.assertEqual(app.return_value.machine.id, 'train')

    async def test_search_drops_hidden_bulk_selection_and_space_stays_text(self):
        app = Picker(self.catalog)
        async with app.run_test(size=(100, 32)) as pilot:
            await pilot.press('ctrl+a', '/', *'tag:gpu')
            self.assertEqual(app.checked, set())
            await pilot.press('space', *'trainer')
            self.assertEqual(app.query_one('#search', Input).value, 'tag:gpu trainer')
            await pilot.press('escape', 'q')

    async def test_keyboard_group_rename(self):
        app = Picker(self.catalog)
        async with app.run_test(size=(100, 32)) as pilot:
            await pilot.press('g')
            app.screen.query_one(Input).value = 'Work'
            await pilot.pause()
            app.screen.query_one(DataTable).focus()
            await pilot.press('e')
            app.screen.query_one('#group', Input).value = 'Projects'
            await pilot.press('ctrl+s')
            self.assertEqual(groups(self.catalog.load().machines)['Projects'], 2)
            self.assertIsNone(app.return_value)
            await pilot.press('q')
