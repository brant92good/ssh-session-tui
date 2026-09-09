"""Read-only SSH metadata import. Never run ssh -G, Match exec, or key helpers."""
from dataclasses import dataclass, replace
import fnmatch
import getpass
import glob
import hashlib
import json
import os
from pathlib import Path
import re
import shlex

from .catalog import CatalogError, Machine, Route, address, atomic_write, encode, locked, login, new_id, port_number


def default_config():
    return Path.home() / '.ssh/config'


def system_config():
    return Path(os.environ.get('PROGRAMDATA', 'C:/ProgramData')) / 'ssh/ssh_config' if os.name == 'nt' else Path('/etc/ssh/ssh_config')


@dataclass(frozen=True)
class Entry:
    alias: str
    host: str = ''
    user: str = ''
    port: int = 22
    problem: str = ''


@dataclass(frozen=True)
class Scan:
    config: Path
    entries: tuple[Entry, ...]
    digest: str


def matches(alias, patterns):
    # OpenSSH Host patterns are case-sensitive and use only * and ?; unlike
    # Python's fnmatch, brackets are literal characters rather than ranges.
    def match(pattern):
        return fnmatch.fnmatchcase(alias, pattern.replace('[', '[[]'))
    return (any(match(p) for p in patterns if not p.startswith('!'))
            and not any(match(p[1:]) for p in patterns if p.startswith('!')))


def scan_ssh(config=None):
    """Evaluate static Host metadata with first-value precedence and bounded Includes.

    Authentication/proxy directives are deliberately left to the local SSH alias.
    Unresolved conditional metadata is refused, never guessed or executed.
    """
    source = Path(config or default_config()).expanduser().resolve()
    files, aliases = {}, set()
    total = 0

    def read(path, stack=(), include_base=None):
        nonlocal total
        path = path.resolve()
        include_base = include_base or Path.home() / '.ssh'
        file_key = (path, include_base)
        if path in stack or len(stack) >= 16:
            raise CatalogError('SSH Include cycle or excessive nesting. Review the configuration before importing.')
        if file_key in files:
            return files[file_key][1]
        if len(files) >= 128:
            raise CatalogError('Too many SSH Include files (limit 128).')
        raw = path.read_bytes()
        total += len(raw)
        if len(raw) > 1024 * 1024 or total > 4 * 1024 * 1024:
            raise CatalogError('SSH configuration exceeds the import size limit.')
        rows = []
        # Cache only after parsing, so a recursive Include cannot bypass the cycle check.
        for number, line in enumerate(raw.decode('utf-8-sig').splitlines(), 1):
            line = re.sub(r'^\s*([A-Za-z]+)\s*=\s*', r'\1 ', line)
            lexer = shlex.shlex(line, posix=True)
            lexer.whitespace_split, lexer.escape = True, ''
            try:
                tokens = list(lexer)
            except ValueError:
                raise CatalogError(f'Unclosed quote in {path.name}, line {number}.') from None
            if not tokens:
                continue
            keyword, values = tokens[0].lower(), tokens[1:]
            if keyword == 'host':
                for value in values:
                    if any(c in value for c in '*?!['):
                        continue
                    try:
                        aliases.add(address(value))
                    except ValueError:
                        pass
                if len(aliases) > 1000:
                    raise CatalogError('Too many SSH aliases (limit 1,000).')
            children = []
            if keyword == 'include':
                for value in values:
                    if '%' in value or '${' in value:
                        children.append(None)
                        continue
                    candidate = Path(value).expanduser()
                    if not candidate.is_absolute():
                        candidate = include_base / candidate
                    for name in sorted(glob.glob(str(candidate))):
                        child = Path(name).resolve()
                        if child.is_file():
                            children.append(read(child, (*stack, path), include_base))
            rows.append((keyword, values, children))
        files[file_key] = (raw, rows)
        return rows

    rows = read(source)
    user_aliases = set(aliases)
    defaults = []
    system = system_config()
    if source == default_config().resolve() and system.is_file():
        defaults = read(system, include_base=system.parent)

    def resolve(alias):
        values, uncertain = {}, set()
        def visit(lines, active=True):
            for keyword, args, children in lines:
                if keyword == 'host':
                    active = matches(alias, args)
                elif keyword == 'match':
                    active = True if args == ['all'] else None
                elif keyword == 'include' and active is not False:
                    for child in children:
                        if child is None or active is None:
                            uncertain.update(k for k in ('hostname', 'user', 'port') if k not in values)
                        else:
                            visit(child, active)  # Include does not change the parent's Host/Match state.
                elif keyword in ('hostname', 'user', 'port', 'canonicalizehostname') and active is not False:
                    if keyword not in values:
                        if active is None:
                            uncertain.add(keyword)
                        elif len(args) == 1:
                            values[keyword] = args[0]
                        else:
                            uncertain.add(keyword)
        visit(rows)
        visit(defaults)
        try:
            if uncertain or values.get('canonicalizehostname', 'no').lower() not in ('no', 'false'):
                raise CatalogError('Conditional or canonicalized address settings need manual setup; no commands were evaluated.')
            host = values.get('hostname', alias).lower()
            if '%' in host or '${' in host:
                raise CatalogError('Address tokens need manual setup; no expansion commands were run.')
            return Entry(alias, address(host), login(values.get('user', getpass.getuser())), port_number(values.get('port', 22)))
        except ValueError as error:
            return Entry(alias, problem=str(error))

    digest = hashlib.sha256()
    for (path, base), (raw, _) in sorted(files.items()):
        digest.update(str(path).encode('utf-8') + b'\0' + str(base).encode('utf-8') + b'\0' + raw + b'\0')
    return Scan(source, tuple(resolve(a) for a in sorted(user_aliases, key=str.casefold)), digest.hexdigest())


def import_status(machines, entry):
    if entry.problem:
        return entry.problem
    for machine in machines:
        for route in machine.routes:
            if route.ssh_alias == entry.alias:
                if (route.host, machine.user, route.port) == (entry.host, entry.user, entry.port):
                    return 'Already imported (select to bind this device)'
                return 'Alias already saved with different details; edit or remove that route first.'
    return 'Ready to import'


def bindings_path(catalog):
    return catalog.device_path.with_suffix('.ssh-configs.json')


def read_bindings(catalog):
    path = bindings_path(catalog)
    if not path.exists():
        return {}
    value = json.loads(path.read_bytes())
    if not isinstance(value, dict) or any(not isinstance(k, str) or not isinstance(v, str) for k, v in value.items()):
        raise CatalogError('Local SSH import paths are invalid. Keep a backup and repair the .ssh-configs.json file.')
    return value


def import_selected(catalog, scan, aliases, expected):
    chosen = set(aliases)
    if not chosen or chosen - {e.alias for e in scan.entries}:
        raise CatalogError('Select at least one listed SSH alias.')
    fresh = scan_ssh(scan.config)
    if fresh.digest != scan.digest:
        raise CatalogError('SSH config changed after preview. Reload the import screen before importing.')
    with locked(catalog.lock_path):
        snapshot = catalog.load()
        if snapshot.revision != expected:
            raise CatalogError('Catalog changed in another tab. Reload before importing.')
        machines, bindings, preferences = list(snapshot.machines), read_bindings(catalog), catalog.preferences()
        added, routes_added, rebound = 0, 0, 0
        for entry in scan.entries:
            if entry.alias not in chosen:
                continue
            status = import_status(machines, entry)
            if status not in ('Ready to import', 'Already imported (select to bind this device)'):
                raise CatalogError(f'{entry.alias}: {status}')
            existing = next(((m, r) for m in machines for r in m.routes
                             if r.ssh_alias == entry.alias), None)
            if existing:
                machine, route = existing
                rebound += 1
            else:
                route = Route(new_id(), 'SSH: ' + entry.alias[:95], entry.host, entry.port, entry.alias)
                index = next((i for i, m in enumerate(machines) if m.user == entry.user
                              and any((r.host, r.port) == (entry.host, entry.port) for r in m.routes)), None)
                if index is None:
                    machine = Machine(new_id(), entry.alias, entry.user, (route,))
                    machines.append(machine)
                    added += 1
                else:
                    old = machines[index]
                    preferred = catalog.preferred(old, preferences)
                    if preferred:
                        preferences[old.id] = preferred.id
                    machine = replace(old, routes=(*old.routes, route))
                    machines[index] = machine
                    routes_added += 1
            bindings[machine.id + '/' + route.id] = str(scan.config)
        raw = encode(machines)
        # Local bindings first: if catalog replacement fails, unused bindings are
        # harmless. Never leave a published custom-config route pointing at a guessed file.
        atomic_write(bindings_path(catalog), (json.dumps(bindings, indent=2) + '\n').encode('utf-8'))
        atomic_write(catalog.device_path, (json.dumps({'version': 1, 'routes': preferences}, indent=2) + '\n').encode('utf-8'))
        if raw != encode(snapshot.machines):
            atomic_write(catalog.path, raw)
    return {'added_machines': added, 'added_routes': routes_added, 'bound_existing': rebound}


def connection_config(catalog, machine, route):
    if not route.ssh_alias:
        return None
    config = Path(read_bindings(catalog).get(machine.id + '/' + route.id, str(default_config())))
    if not config.is_file() or route.ssh_alias not in {e.alias for e in scan_ssh(config).entries}:
        raise CatalogError(f'This route needs local SSH alias "{route.ssh_alias}". Press I to import/bind it on this device, or choose another route. No connection was started.')
    return config if config.resolve() != default_config().resolve() else None
