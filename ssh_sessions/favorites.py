"""Numbered picker favorites, scoped to one catalog on one device."""
from dataclasses import dataclass
import hashlib
import json

from .catalog import CatalogError, atomic_write, identifier, locked

LOCAL = '@local'


def slot_number(value):
    if isinstance(value, bool) or str(value) not in '123456789' or len(str(value)) != 1:
        raise CatalogError('Choose a favorite slot from 1 to 9.')
    return str(value)


def target_id(value):
    if value == LOCAL:
        return LOCAL
    return identifier(value)


def validate_slots(slots):
    if not isinstance(slots, dict) or len(slots) > 9:
        raise CatalogError('Favorites must map slots 1–9 to machine IDs or @local.')
    result = {slot_number(slot): target_id(target) for slot, target in slots.items()}
    if len(set(result.values())) != len(result):
        raise CatalogError('Each machine or local terminal can occupy only one favorite slot.')
    return result


def target_name(target, machines):
    if target == LOCAL:
        return 'Local terminal'
    return next((m.name for m in machines if m.id == target), 'Missing machine: ' + target)


@dataclass(frozen=True)
class SavedFavorites:
    slots: dict[str, str]
    revision: str


class Favorites:
    def __init__(self, catalog):
        self.catalog = catalog
        self.path = catalog.device_path.with_suffix('.favorites.json')

    def load(self):
        raw = self.path.read_bytes() if self.path.exists() else b''
        slots = {}
        if self.path.exists():
            try:
                data = json.loads(raw) if len(raw) <= 8192 else None
                if not isinstance(data, dict) or set(data) != {'version', 'slots'} or type(data['version']) is not int or data['version'] != 1:
                    raise ValueError()
                slots = validate_slots(data['slots'])
            except (ValueError, TypeError, KeyError) as error:
                raise CatalogError('Local favorites are invalid; keep a backup and repair the .favorites.json file.') from error
        return SavedFavorites(slots, hashlib.sha256(raw).hexdigest())

    def _write(self, slots):
        raw = (json.dumps({'version': 1, 'slots': validate_slots(slots)}, indent=2) + '\n').encode()
        atomic_write(self.path, raw)

    def assign(self, slot, target, expected=None):
        slot = slot_number(slot)
        if target is not None:
            target = target_id(target)
        with locked(self.catalog.lock_path):
            saved = self.load()
            if expected is not None and saved.revision != expected:
                raise CatalogError('Favorites changed in another tab. Reopen Favorites before saving.')
            if target not in (None, LOCAL) and not any(m.id == target for m in self.catalog.load().machines):
                raise CatalogError('That machine is no longer in the catalog. Reload before assigning a favorite.')
            slots = {key: value for key, value in saved.slots.items() if key != slot and value != target}
            if target is not None:
                slots[slot] = target
            self._write(slots)

    def seed(self, slots):
        """Install portable defaults once; preserve later user edits, including empty slots."""
        slots = validate_slots(slots)
        with locked(self.catalog.lock_path):
            if self.path.exists():
                self.load()
                return False
            machine_ids = {m.id for m in self.catalog.load().machines}
            if set(slots.values()) - machine_ids - {LOCAL}:
                raise CatalogError('Favorite defaults reference a machine missing from the catalog.')
            self._write(slots)
            return True
