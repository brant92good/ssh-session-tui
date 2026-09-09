import contextlib
import io
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

from ssh_sessions.catalog import Catalog, CatalogError
from ssh_sessions.cli import main
from ssh_sessions.favorites import LOCAL, Favorites
from test_catalog import sample


class FavoritesTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        root = Path(temporary.name)
        self.catalog = Catalog(root / 'shared.json', root / 'device')
        self.catalog.save((sample(),), self.catalog.load().revision)
        self.store = Favorites(self.catalog)

    def test_numbered_favorites_are_local_and_do_not_change_machine_or_route_data(self):
        original = self.catalog.path.read_bytes()
        self.catalog.choose(sample(), 'vpn')
        routes = self.catalog.device_path.read_bytes()
        self.store.assign(1, sample().id)
        self.store.assign(2, LOCAL)
        self.assertEqual(Favorites(self.catalog).load().slots, {'1': sample().id, '2': LOCAL})
        self.assertEqual(self.catalog.path.read_bytes(), original)
        self.assertEqual(self.catalog.device_path.read_bytes(), routes)
        other = Catalog(self.catalog.path, self.catalog.state_dir / 'other-device')
        self.assertEqual(Favorites(other).load().slots, {})

    def test_assignment_moves_target_and_replaces_only_selected_slot(self):
        self.store.assign(1, sample().id)
        self.store.assign(2, LOCAL)
        self.store.assign(2, sample().id)
        self.assertEqual(self.store.load().slots, {'2': sample().id})
        self.store.assign(2, None)
        self.assertEqual(self.store.load().slots, {})

    def test_stale_tab_cannot_overwrite_a_newer_assignment(self):
        before = self.store.load()
        Favorites(self.catalog).assign(1, LOCAL)
        with self.assertRaisesRegex(CatalogError, 'another tab'):
            self.store.assign(1, sample().id, before.revision)
        self.assertEqual(self.store.load().slots, {'1': LOCAL})

    def test_invalid_slot_and_unknown_machine_are_rejected_without_writes(self):
        for slot in (0, 10, True, '01', 'x', None):
            with self.subTest(slot=slot), self.assertRaises(CatalogError):
                self.store.assign(slot, LOCAL)
        with self.assertRaises(CatalogError):
            self.store.assign(1, 'missing')
        self.assertFalse(self.store.path.exists())

    def test_invalid_file_is_preserved(self):
        for raw in (b'', b'{"version":1,"slots":{"1":"@local"},"token":"bad"}',
                    b'{"version":1,"slots":{"1":"@local","2":"@local"}}'):
            self.store.path.write_bytes(raw)
            with self.assertRaises(CatalogError):
                self.store.assign(1, LOCAL)
            self.assertEqual(self.store.path.read_bytes(), raw)

    def test_first_install_seed_preserves_user_changes_including_cleared_slots(self):
        self.assertTrue(self.store.seed({'1': sample().id, '2': LOCAL}))
        self.store.assign(1, None)
        self.assertFalse(self.store.seed({'1': sample().id, '2': LOCAL}))
        self.assertEqual(self.store.load().slots, {'2': LOCAL})

    def test_cli_lists_sets_and_removes_without_starting_a_session(self):
        common = ['--catalog', str(self.catalog.path), '--state-dir', str(self.catalog.state_dir), 'favorites']
        with patch('subprocess.call', side_effect=AssertionError('Must not connect')):
            for args in (['set', '2', '--local'], ['list'], ['remove', '2']):
                with contextlib.redirect_stdout(io.StringIO()) as output:
                    self.assertEqual(main([*common, *args, '--json']), 0)
                self.assertTrue(json.loads(output.getvalue())['ok'])
        self.assertEqual(self.store.load().slots, {})
