from dataclasses import replace
import json
from pathlib import Path
import tempfile
import unittest

from ssh_sessions.catalog import Catalog, CatalogError, Machine, Route, address, decode, encode


def sample():
    return Machine('workbox', 'Work box 開發', 'developer', (
        Route('lan', 'LAN', '192.0.2.10'), Route('vpn', 'VPN', 'workbox.example.test', 2222)))


class CatalogTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.catalog = Catalog(self.root / 'shared/catalog.json', self.root / 'device-a')

    def save_sample(self):
        return self.catalog.save((sample(),), self.catalog.load().revision)

    def test_missing_catalog_is_empty_and_read_only(self):
        self.assertEqual(self.catalog.load().machines, ())
        self.assertFalse(self.catalog.path.exists())

    def test_unicode_metadata_round_trip(self):
        self.assertEqual(decode(encode((sample(),))), (sample(),))

    def test_multiple_routes_require_a_device_choice(self):
        self.save_sample()
        self.assertIsNone(self.catalog.preferred(sample()))
        self.catalog.choose(sample(), 'lan')
        self.assertEqual(self.catalog.preferred(sample()).id, 'lan')
        device_b = Catalog(self.catalog.path, self.root / 'device-b')
        self.assertIsNone(device_b.preferred(sample()))
        device_b.choose(sample(), 'vpn')
        self.assertEqual(self.catalog.preferred(sample()).id, 'lan')
        self.assertEqual(device_b.preferred(sample()).id, 'vpn')
        self.assertNotIn('selected', self.catalog.path.read_text(encoding="utf-8"))

    def test_single_route_is_unambiguous(self):
        machine = replace(sample(), routes=sample().routes[:1])
        self.assertEqual(self.catalog.preferred(machine).id, 'lan')

    def test_deleted_preferred_route_does_not_silently_select_another_of_many(self):
        self.catalog.choose(sample(), 'lan')
        changed = replace(sample(), routes=(sample().routes[1], Route('other', 'Other', '192.0.2.30')))
        self.assertIsNone(self.catalog.preferred(changed))

    def test_second_tab_cannot_overwrite_newer_catalog(self):
        before = self.save_sample()
        other = Catalog(self.catalog.path, self.root / 'device-a')
        changed = replace(sample(), name='Changed by another tab')
        other.save((changed,), before.revision)
        with self.assertRaisesRegex(CatalogError, 'changed in another'):
            self.catalog.save((sample(),), before.revision)
        self.assertEqual(self.catalog.load().machines[0].name, changed.name)

    def test_credentials_and_unknown_fields_rejected_at_every_level(self):
        for level in ('root', 'machine', 'route'):
            with self.subTest(level=level):
                value = json.loads(encode((sample(),)))
                target = value if level == 'root' else value['machines'][0] if level == 'machine' else value['machines'][0]['routes'][0]
                target['private_key'] = 'not allowed'
                with self.assertRaises(CatalogError): decode(json.dumps(value).encode())

    def test_empty_or_malformed_existing_file_is_not_silently_replaced(self):
        self.catalog.path.parent.mkdir()
        for raw in (b'', b'{bad', b'[]'):
            self.catalog.path.write_bytes(raw)
            with self.assertRaises(CatalogError): self.catalog.load()
            self.assertEqual(self.catalog.path.read_bytes(), raw)

    def test_addresses_reject_commands_urls_options_and_control_characters(self):
        for value in ('-oProxyCommand=bad', 'host;touch', 'ssh://host', 'user@host', 'host\nother', 'host name', '/tmp/socket'):
            with self.subTest(value=value), self.assertRaises(CatalogError): address(value)
        for value in ('::1', '2001:db8::1', '192.0.2.1', 'my-existing-alias'):
            self.assertEqual(address(value), value)

    def test_duplicate_machine_and_route_ids_are_rejected(self):
        with self.assertRaises(CatalogError): encode((sample(), sample()))
        with self.assertRaises(CatalogError): encode((replace(sample(), routes=(sample().routes[0], sample().routes[0])),))

    def test_invalid_port_and_empty_route_list_are_rejected(self):
        for port in (0, 65536, True, '-1', '22.0', '２２'):
            with self.subTest(port=port), self.assertRaises(CatalogError):
                encode((replace(sample(), routes=(replace(sample().routes[0], port=port),)),))
        with self.assertRaises(CatalogError): encode((replace(sample(), routes=()),))

    def test_device_preferences_stay_outside_the_shared_folder(self):
        self.save_sample()
        original = self.catalog.path.read_bytes()
        self.catalog.choose(sample(), 'vpn')
        self.assertEqual(self.catalog.path.read_bytes(), original)
        self.assertFalse(self.catalog.device_path.is_relative_to(self.catalog.path.parent))
