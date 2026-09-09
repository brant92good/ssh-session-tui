"""Validated shared metadata and separate, device-local route preferences."""
from contextlib import contextmanager
from dataclasses import asdict, dataclass
import hashlib
import ipaddress
import json
import os
from pathlib import Path
import re
import tempfile
import time
import uuid


class CatalogError(ValueError):
    pass


def fields(value, allowed, required=None):
    if not isinstance(value, dict) or set(value) - set(allowed) or set(required or allowed) - set(value):
        raise CatalogError('Unexpected or missing catalog fields. Check the catalog format in docs/design.md.')


def label(value, name='Name'):
    if not isinstance(value, str) or not value.strip() or len(value) > 100 or any(ord(c) < 32 or ord(c) == 127 for c in value):
        raise CatalogError(f'{name} must be 1–100 readable characters.')
    return value.strip()


def identifier(value):
    if not isinstance(value, str) or not re.fullmatch(r'[A-Za-z0-9_-]{1,80}', value):
        raise CatalogError('IDs must contain letters, numbers, underscores or hyphens.')
    return value


def group_path(value):
    if not isinstance(value, str) or len(value) > 160:
        raise CatalogError('Group names must be at most 160 characters.')
    if not value.strip():
        return ''
    parts = value.strip().split('/')
    if len(parts) > 8 or any(not part.strip() or part.strip() in ('.', '..') for part in parts):
        raise CatalogError('Use up to 8 group names separated by /, such as Work/Production.')
    return '/'.join(label(part, 'Group name') for part in parts)


def parse_tags(value):
    if isinstance(value, str):
        value = [tag.strip() for tag in value.split(',') if tag.strip()]
    if not isinstance(value, (list, tuple)) or len(value) > 30:
        raise CatalogError('Use up to 30 tags, separated by commas.')
    tags = []
    for tag in value:
        tag = label(tag, 'Tag')
        if len(tag) > 40 or ',' in tag:
            raise CatalogError('Each tag must be at most 40 characters, without commas.')
        if tag.casefold() not in {existing.casefold() for existing in tags}:
            tags.append(tag)
    return tuple(tags)


def address(value):
    value = label(value, 'Address')
    try:
        ipaddress.ip_address(value)
        return value
    except ValueError:
        if not re.fullmatch(r'[A-Za-z0-9_][A-Za-z0-9_.-]*', value):
            raise CatalogError('Use an IP address, hostname or an existing SSH alias; do not enter a command or URL.')
    return value


def login(value):
    value = label(value, 'Username')
    if value.startswith('-') or any(c.isspace() for c in value):
        raise CatalogError('Enter a username without spaces or a leading hyphen.')
    return value


def port_number(value):
    if isinstance(value, bool) or not str(value).isascii() or not str(value).isdigit() or not 1 <= int(value) <= 65535:
        raise CatalogError('Port must be a number from 1 to 65535.')
    return int(value)


@dataclass(frozen=True)
class Route:
    id: str
    name: str
    host: str
    port: int = 22
    ssh_alias: str | None = None

    @classmethod
    def parse(cls, data):
        fields(data, ('id', 'name', 'host', 'port', 'ssh_alias'), ('id', 'name', 'host', 'port'))
        alias = address(data['ssh_alias']) if data.get('ssh_alias') is not None else None
        return cls(identifier(data['id']), label(data['name'], 'Route name'), address(data['host']), port_number(data['port']), alias)


@dataclass(frozen=True)
class Machine:
    id: str
    name: str
    user: str
    routes: tuple[Route, ...]
    group: str = ''
    tags: tuple[str, ...] = ()

    @classmethod
    def parse(cls, data):
        fields(data, ('id', 'name', 'user', 'routes', 'group', 'tags'), ('id', 'name', 'user', 'routes'))
        if not isinstance(data['routes'], list) or not 1 <= len(data['routes']) <= 30:
            raise CatalogError('Each machine needs 1–30 routes.')
        routes = tuple(Route.parse(route) for route in data['routes'])
        if len({r.id for r in routes}) != len(routes):
            raise CatalogError('Route IDs must be unique within a machine.')
        return cls(identifier(data['id']), label(data['name']), login(data['user']), routes,
                   group_path(data.get('group', '')), parse_tags(data.get('tags', [])))

    def route(self, route_id):
        return next((r for r in self.routes if r.id == route_id), None)


def decode(raw):
    if len(raw) > 512 * 1024:
        raise CatalogError('Catalog exceeds 512 KiB.')
    try:
        data = json.loads(raw)
    except (ValueError, UnicodeError) as error:
        raise CatalogError('Catalog is not valid UTF-8 JSON; the file was left unchanged.') from error
    fields(data, ('version', 'machines'))
    if type(data['version']) is not int or data['version'] not in (1, 2) or not isinstance(data['machines'], list) or len(data['machines']) > 1000:
        raise CatalogError('Expected catalog version 1 or 2 and at most 1,000 machines.')
    if data['version'] == 1 and any(isinstance(m, dict) and ('group' in m or 'tags' in m) for m in data['machines']):
        raise CatalogError('Groups and tags require catalog version 2 (SSH Sessions 0.4 or newer).')
    machines = tuple(Machine.parse(machine) for machine in data['machines'])
    if len({m.id for m in machines}) != len(machines):
        raise CatalogError('Machine IDs must be unique.')
    return machines


def encode(machines):
    rows = [asdict(m) for m in machines]
    version = 2 if any(m['group'] or m['tags'] for m in rows) else 1
    for machine in rows:
        if not machine['group']:
            del machine['group']
        if not machine['tags']:
            del machine['tags']
        for route in machine['routes']:
            if route['ssh_alias'] is None:
                del route['ssh_alias']
    raw = (json.dumps({'version': version, 'machines': rows}, ensure_ascii=False, indent=2) + '\n').encode('utf-8')
    decode(raw)
    return raw


def atomic_write(path, raw):
    path.parent.mkdir(parents=True, exist_ok=True)
    descriptor, temporary = tempfile.mkstemp(prefix=path.name + '.', suffix='.tmp', dir=path.parent)
    try:
        with os.fdopen(descriptor, 'wb') as output:
            output.write(raw)
            output.flush()
            os.fsync(output.fileno())
        os.replace(temporary, path)
    finally:
        Path(temporary).unlink(missing_ok=True)


@contextmanager
def locked(path):
    """Serialize this app's writes on one device; never place locks in Git."""
    path.parent.mkdir(parents=True, exist_ok=True)
    with path.open('a+b') as stream:
        if stream.tell() == 0:
            stream.write(b'0')
            stream.flush()
        stream.seek(0)
        deadline = time.monotonic() + 3
        while True:
            try:
                if os.name == 'nt':
                    import msvcrt
                    msvcrt.locking(stream.fileno(), msvcrt.LK_NBLCK, 1)
                else:
                    import fcntl
                    fcntl.flock(stream, fcntl.LOCK_EX | fcntl.LOCK_NB)
                break
            except OSError:
                if time.monotonic() >= deadline:
                    raise CatalogError('Another catalog operation is running. Try again shortly.')
                time.sleep(.05)
        try:
            yield
        finally:
            if os.name == 'nt':
                stream.seek(0)
                msvcrt.locking(stream.fileno(), msvcrt.LK_UNLCK, 1)
            else:
                fcntl.flock(stream, fcntl.LOCK_UN)


@dataclass(frozen=True)
class Snapshot:
    machines: tuple[Machine, ...]
    revision: str


class Catalog:
    def __init__(self, path, state_dir):
        self.path = Path(path).resolve()
        self.state_dir = Path(state_dir).resolve()
        key = hashlib.sha256(str(self.path).casefold().encode()).hexdigest()[:24]
        self.lock_path = self.state_dir / (key + '.lock')
        self.device_path = self.state_dir / (key + '.device.json')

    def load(self):
        exists = self.path.exists()
        raw = self.path.read_bytes() if exists else b''
        return Snapshot(decode(raw) if exists else (), hashlib.sha256(raw).hexdigest())

    def save(self, machines, expected):
        raw = encode(machines)
        with locked(self.lock_path):
            if self.load().revision != expected:
                raise CatalogError('The catalog changed in another tab or editor. Press F5 to reload before saving.')
            atomic_write(self.path, raw)
        return self.load()

    def preferences(self):
        if not self.device_path.exists():
            return {}
        try:
            data = json.loads(self.device_path.read_bytes())
            fields(data, ('version', 'routes'))
            if data['version'] != 1 or not isinstance(data['routes'], dict):
                raise ValueError()
            return {identifier(k): identifier(v) for k, v in data['routes'].items()}
        except (ValueError, TypeError, KeyError) as error:
            raise CatalogError('Device route preferences are invalid; keep a backup and repair the local .device.json file.') from error

    def preferred(self, machine, preferences=None):
        values = self.preferences() if preferences is None else preferences
        selected = machine.route(values.get(machine.id))
        # A single available route is unambiguous. Several routes require a choice.
        return selected or (machine.routes[0] if len(machine.routes) == 1 else None)

    def choose(self, machine, route_id):
        if machine.route(route_id) is None:
            raise CatalogError('That route is no longer available. Reload the catalog.')
        with locked(self.lock_path):
            values = self.preferences()
            values[machine.id] = route_id
            atomic_write(self.device_path, (json.dumps({'version': 1, 'routes': values}, indent=2) + '\n').encode())


def new_id():
    return uuid.uuid4().hex
