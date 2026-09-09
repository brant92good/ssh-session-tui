from dataclasses import replace
from pathlib import Path
import subprocess
import tempfile
import unittest

from ssh_sessions.catalog import Catalog, CatalogError, decode, encode
from ssh_sessions.sync import GitSync
from ssh_sessions.favorites import Favorites, LOCAL
from test_catalog import sample


def git(root, *args):
    return subprocess.check_output(['git', '-C', str(root), *args], text=True, encoding='utf-8', stderr=subprocess.STDOUT)


class SyncTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix='session-sync-')
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.remote = self.root / 'remote.git'
        self.remote.mkdir()
        git(self.remote, 'init', '--bare', '--initial-branch=main')
        self.a = self.root / 'device A'
        git(self.root, 'clone', str(self.remote), str(self.a))
        self.identity(self.a)
        (self.a / 'catalog.json').write_bytes(encode((sample(),)))
        (self.a / 'README.md').write_text('Fixture')
        git(self.a, 'add', 'catalog.json', 'README.md')
        git(self.a, 'commit', '-m', 'Initial fixture')
        git(self.a, 'push', '-u', 'origin', 'main')
        self.catalog = Catalog(self.a / 'catalog.json', self.root / 'local preferences')

    def identity(self, root):
        git(root, 'config', 'user.name', 'Fixture')
        git(root, 'config', 'user.email', 'fixture@example.invalid')
        git(root, 'config', 'commit.gpgsign', 'false')
        git(root, 'config', 'core.autocrlf', 'false')

    def test_publish_changes_only_catalog_preserves_unrelated_staging(self):
        self.catalog.choose(sample(), 'vpn')
        before = self.catalog.load()
        self.catalog.save((replace(sample(), name='New name'),), before.revision)
        (self.a / 'unrelated.txt').write_text('Leave this staged')
        git(self.a, 'add', 'unrelated.txt')
        GitSync(self.catalog).run('publish')
        self.assertEqual(git(self.a, 'diff-tree', '--no-commit-id', '--name-only', '-r', 'HEAD').strip(), 'catalog.json')
        self.assertIn('A  unrelated.txt', git(self.a, 'status', '--porcelain'))
        self.assertEqual(decode(git(self.remote, 'show', 'main:catalog.json').encode())[0].name, 'New name')
        self.assertNotIn('device', git(self.remote, 'ls-tree', '-r', '--name-only', 'main'))
        self.assertEqual(self.catalog.preferred(sample()).id, 'vpn')

    def test_pull_gets_other_device_edit_without_changing_device_route(self):
        self.catalog.choose(sample(), 'lan')
        other = self.root / 'device B'
        git(self.root, 'clone', str(self.remote), str(other))
        self.identity(other)
        (other / 'catalog.json').write_bytes(encode((replace(sample(), name='From device B'),)))
        git(other, 'commit', '-am', 'Edit catalog on B')
        git(other, 'push')
        GitSync(self.catalog).run('pull')
        self.assertEqual(self.catalog.load().machines[0].name, 'From device B')
        self.assertEqual(self.catalog.preferred(sample()).id, 'lan')

    def test_pull_refuses_local_changes_without_discarding_them(self):
        before = self.catalog.load()
        self.catalog.save((replace(sample(), name='Local draft'),), before.revision)
        with self.assertRaisesRegex(CatalogError, 'local changes'):
            GitSync(self.catalog).run('pull')
        self.assertEqual(self.catalog.load().machines[0].name, 'Local draft')

    def test_groups_and_tags_sync_while_favorites_and_routes_stay_device_specific(self):
        machine = sample()
        self.catalog.choose(machine, 'lan')
        Favorites(self.catalog).assign(1, machine.id)
        other = self.root / 'device B'
        git(self.root, '-c', 'core.autocrlf=false', 'clone', str(self.remote), str(other))
        self.identity(other)
        catalog_b = Catalog(other / 'catalog.json', self.root / 'preferences B')
        catalog_b.choose(machine, 'vpn')
        Favorites(catalog_b).assign(2, LOCAL)
        grouped = replace(machine, group='Work/Production', tags=('linux', 'gpu'))
        self.catalog.save((grouped,), self.catalog.load().revision)
        GitSync(self.catalog).run('publish')
        GitSync(catalog_b).run('pull')
        self.assertEqual(catalog_b.load().machines, (grouped,))
        self.assertEqual(catalog_b.preferred(grouped).id, 'vpn')
        self.assertEqual(self.catalog.preferred(grouped).id, 'lan')
        self.assertEqual(Favorites(catalog_b).load().slots, {'2': LOCAL})
        self.assertEqual(Favorites(self.catalog).load().slots, {'1': machine.id})

    def test_publish_refuses_unrelated_unpublished_commits(self):
        (self.a / 'README.md').write_text('Unpublished work')
        git(self.a, 'commit', '-am', 'Keep local for now')
        with self.assertRaisesRegex(CatalogError, 'unpublished work'):
            GitSync(self.catalog).run('publish')
        self.assertEqual(git(self.remote, 'show', 'main:README.md'), 'Fixture')

    def test_invalid_catalog_cannot_be_published(self):
        (self.a / 'catalog.json').write_text('{"version":1,"machines":[],"password":"not allowed"}')
        with self.assertRaises(CatalogError): GitSync(self.catalog).run('publish')
        self.assertEqual(decode(git(self.remote, 'show', 'main:catalog.json').encode()), (sample(),))

    def test_untracked_catalog_requires_explicit_repository_setup(self):
        path = self.a / 'untracked.json'
        path.write_bytes(encode((sample(),)))
        with self.assertRaises(CatalogError): GitSync(Catalog(path, self.root / 'device')).run('publish')
        self.assertNotIn('untracked.json', git(self.a, 'ls-files'))

    def test_reverting_unrelated_work_does_not_make_its_history_publishable(self):
        (self.a / 'README.md').write_text('Unrelated draft')
        git(self.a, 'commit', '-am', 'Draft')
        git(self.a, 'revert', '--no-edit', 'HEAD')
        self.assertEqual(git(self.a, 'diff', '@{upstream}..HEAD'), '')
        with self.assertRaisesRegex(CatalogError, 'unpublished work'):
            GitSync(self.catalog).run('publish')

    def test_fixed_catalog_does_not_publish_invalid_earlier_catalog_commit(self):
        (self.a / 'catalog.json').write_text('{"version":1,"machines":[],"password":"not allowed"}')
        git(self.a, 'commit', '-am', 'Invalid draft')
        (self.a / 'catalog.json').write_bytes(encode((sample(),)))
        git(self.a, 'commit', '-am', 'Repair current catalog')
        with self.assertRaises(CatalogError): GitSync(self.catalog).run('publish')
        self.assertEqual(git(self.remote, 'log', '--format=%s', '-1', 'main').strip(), 'Initial fixture')
