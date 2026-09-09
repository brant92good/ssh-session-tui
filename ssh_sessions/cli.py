"""Launch the picker or inspect/sync its metadata without importing the UI."""
import argparse
from dataclasses import asdict
import json
import os
from pathlib import Path
import shlex
import shutil
import subprocess
import sys

from .catalog import Catalog, CatalogError
from .connection import local_command, local_shell_name, run_session, set_title, ssh_command
from .ssh_import import connection_config


def default_directory():
    if os.name == 'nt':
        base = os.environ.get('LOCALAPPDATA') or Path.home() / 'AppData/Local'
    elif sys.platform == 'darwin':
        base = Path.home() / 'Library/Application Support'
    else:
        base = os.environ.get('XDG_DATA_HOME') or Path.home() / '.local/share'
    return Path(base) / 'SSHSessions'


def picker_loop(catalog, picker_factory=None, runner=run_session):
    if picker_factory is None:
        from .ui import Picker
        picker_factory = Picker
    notice = ''
    failed = None
    while True:
        set_title('SSH Sessions')
        choice = picker_factory(catalog, notice=notice, failed_machine=failed).run()
        if choice is None:
            return 0
        failed = None
        try:
            command = local_command() if choice.kind == 'local' else ssh_command(choice.machine, choice.route,
                config=connection_config(catalog, choice.machine, choice.route))
            set_title('Local ' + local_shell_name() if choice.kind == 'local' else f'{choice.machine.name} | {choice.route.name}')
            if choice.kind == 'connect':
                print(f'Connecting to {choice.machine.name} via {choice.route.name} ({choice.route.host}:{choice.route.port})', flush=True)
            code = runner(command)
            notice = f'{"Local shell" if choice.kind == "local" else "SSH session"} ended (exit {code}). Choose a machine to connect again.'
            if choice.kind == 'connect' and code == 255:
                failed = choice.machine.id
                notice = 'SSH connection failed (exit 255). Check the SSH error above or choose another route.'
                if sys.stdin.isatty():
                    input('SSH failed. Read the message above, then press Enter to return to the picker and choose a route. ')
        except KeyboardInterrupt:
            notice = 'Session interrupted. Choose a machine to connect again.'
        except (OSError, ValueError) as error:
            notice = str(error)


def main(argv=None):
    parser = argparse.ArgumentParser(description='Browse, organize and connect to your SSH machines.')
    from . import __version__
    parser.add_argument('--version', action='version', version='SSH Sessions ' + __version__)
    parser.add_argument('--catalog', type=Path, default=default_directory() / 'catalog.json', help='Shared metadata JSON; point this at a file in your private Git repo')
    parser.add_argument('--state-dir', type=Path, default=default_directory() / 'device', help='Device-only preferences; keep outside your shared repository')
    commands = parser.add_subparsers(dest='command')
    listing = commands.add_parser('list', help='List machines and this device\'s selected routes')
    listing.add_argument('--json', action='store_true')
    list_group = listing.add_mutually_exclusive_group()
    list_group.add_argument('--group', help='Group path, including its subgroups')
    list_group.add_argument('--ungrouped', action='store_const', const='', dest='group', help='Show machines without a group')
    listing.add_argument('--tag', help='Tag to match')
    listing.add_argument('--search', default='', help='Search words, group:PATH or tag:NAME')
    commands.add_parser('init', help='Create an empty catalog if missing; preserve an existing file')
    doctor = commands.add_parser('doctor', help='Check local setup')
    doctor.add_argument('--json', action='store_true')
    sync = commands.add_parser('sync', help='Pull or publish the catalog through Git')
    sync.add_argument('action', choices=('pull', 'publish'))
    sync.add_argument('--json', action='store_true')
    command = commands.add_parser('command', help='Print the selected SSH command')
    command.add_argument('machine', help='Machine ID or exact name')
    command.add_argument('--route', help='Route ID or exact name; required when this device has no preference')
    command.add_argument('--json', action='store_true')
    importing = commands.add_parser('import-ssh', help='Preview local SSH hosts; import only with --apply and --host NAME or --all')
    importing.add_argument('--config', type=Path, help='Read a custom SSH config file')
    importing.add_argument('--host', action='append', default=[], help='Alias to import; repeat for multiple hosts')
    importing.add_argument('--all', action='store_true', help='Select all entries without unresolved metadata')
    importing.add_argument('--apply', action='store_true', help='Import selected hosts')
    importing.add_argument('--group', default='', help='Group for newly imported machines')
    importing.add_argument('--json', action='store_true')
    favorites = commands.add_parser('favorites', help='List or edit numbered favorites')
    favorites.add_argument('action', choices=('list', 'set', 'remove'), nargs='?', default='list')
    favorites.add_argument('slot', nargs='?', help='Favorite number 1–9 for set/remove')
    favorite_target = favorites.add_mutually_exclusive_group()
    favorite_target.add_argument('--machine', help='Exact machine ID from list --json')
    favorite_target.add_argument('--local', action='store_true', help='Use the local shell')
    favorites.add_argument('--json', action='store_true')
    grouping = commands.add_parser('groups', help='List groups or rename a group and its subgroups')
    grouping.add_argument('action', choices=('list', 'rename'), nargs='?', default='list')
    grouping.add_argument('old', nargs='?')
    grouping.add_argument('new', nargs='?')
    grouping.add_argument('--json', action='store_true')
    organize = commands.add_parser('organize', help='Move machines to a group or add/remove tags')
    organize.add_argument('--machine', action='append', required=True, help='Machine ID; repeat to select several')
    edit_group = organize.add_mutually_exclusive_group()
    edit_group.add_argument('--group', help='Destination group')
    edit_group.add_argument('--ungrouped', action='store_const', const='', dest='group', help='Remove group membership')
    organize.add_argument('--add-tag', action='append', default=[])
    organize.add_argument('--remove-tag', action='append', default=[])
    organize.add_argument('--json', action='store_true')
    options = parser.parse_args(argv)
    structured = getattr(options, 'json', False)
    try:
        catalog = Catalog(options.catalog, options.state_dir)
        snapshot = catalog.load()
        if options.command in ('groups', 'organize'):
            from .organization import edit_many, groups, rename_group
            if options.command == 'organize':
                if options.group is None and not options.add_tag and not options.remove_tag:
                    raise CatalogError('Specify --group, --add-tag or --remove-tag.')
                snapshot = edit_many(catalog, options.machine, snapshot.revision, group=options.group,
                                     add_tags=options.add_tag, remove_tags=options.remove_tag)
            elif options.action == 'rename':
                if options.old is None or options.new is None:
                    raise CatalogError('Use groups rename OLD_PATH NEW_PATH.')
                snapshot = rename_group(catalog, options.old, options.new, snapshot.revision)
            elif options.old is not None or options.new is not None:
                raise CatalogError('Use groups list without path arguments.')
            rows = [{'path': path, 'machines': count} for path, count in groups(snapshot.machines).items()]
            result = {'ok': True, 'groups': rows, 'ungrouped': sum(not m.group for m in snapshot.machines)}
            print(json.dumps(result) if structured else '\n'.join(f"{r['path']} ({r['machines']})" for r in rows) or 'No groups yet. Use organize --group to create one.')
            return 0
        if options.command == 'favorites':
            from .favorites import LOCAL, Favorites, target_name
            store = Favorites(catalog)
            if options.action == 'set':
                if options.slot is None or (not options.local and not options.machine):
                    raise CatalogError('Use favorites set NUMBER with --machine MACHINE_ID or --local.')
                store.assign(options.slot, LOCAL if options.local else options.machine)
            elif options.action == 'remove':
                if options.slot is None or options.local or options.machine:
                    raise CatalogError('Use favorites remove NUMBER without a target.')
                store.assign(options.slot, None)
            elif options.slot is not None or options.local or options.machine:
                raise CatalogError('Use favorites list without a slot or target.')
            rows = [{'slot': int(slot), 'target': target, 'name': target_name(target, snapshot.machines),
                     'target_exists': target == LOCAL or any(m.id == target for m in snapshot.machines)}
                    for slot, target in sorted(store.load().slots.items())]
            result = {'ok': True, 'favorites': rows, 'scope': 'device', 'activation': 'number_then_enter'}
            print(json.dumps(result) if structured else '\n'.join(f"{r['slot']}: {r['name']}" for r in rows) or
                  'No numbered favorites. In the picker, select a row and press F.')
            return 0
        if options.command == 'import-ssh':
            from .ssh_import import scan_ssh, import_status, import_selected
            scan = scan_ssh(options.config)
            rows = [{**asdict(entry), 'status': import_status(snapshot.machines, entry)} for entry in scan.entries]
            result = {'ok': True, 'source': str(scan.config), 'hosts': rows, 'applied': False}
            if options.apply:
                selected = [e.alias for e in scan.entries if import_status(snapshot.machines, e) in
                    ('Ready to import', 'Already imported (select to bind this device)')] if options.all else options.host
                result.update(import_selected(catalog, scan, selected, snapshot.revision, group=options.group), applied=True)
            print(json.dumps(result, ensure_ascii=True) if structured else '\n'.join(
                [f"{r['alias']} | {r['user']}@{r['host']}:{r['port']} | {r['status']}" for r in rows] +
                (['Hosts imported.'] if options.apply else
                 ['Use --apply with --host NAME or --all to import.'])))
            return 0
        if options.command == 'doctor':
            checks = {'catalog_valid': True, 'machine_count': len(snapshot.machines),
                      'ssh_available': bool(shutil.which('ssh.exe' if os.name == 'nt' else 'ssh')),
                      'git_available': bool(shutil.which('git')), 'local_pwsh_available': bool(shutil.which('pwsh')),
                      'local_shell': local_shell_name()}
            catalog.preferences()
            from .favorites import Favorites
            Favorites(catalog).load()
            result = {'ok': checks['ssh_available'], 'checks': checks,
                      'next_step': 'Run without a command to choose or add a machine. Git is needed only for sync.'}
            print(json.dumps(result) if structured else '\n'.join([*(f'{k}: {v}' for k, v in checks.items()), result['next_step']]))
            return 0 if result['ok'] else 1
        if options.command == 'init':
            if not catalog.path.exists():
                catalog.save((), snapshot.revision)
            print(f'Catalog ready: {catalog.path}')
            return 0
        if options.command == 'sync':
            from .sync import GitSync
            message = GitSync(catalog).run(options.action)
            print(json.dumps({'ok': True, 'message': message}) if structured else message)
            return 0
        if options.command == 'list':
            from .organization import filtered
            rows = []
            preferences = catalog.preferences()
            for machine in filtered(snapshot.machines, options.search, options.group, options.tag):
                route = catalog.preferred(machine, preferences)
                rows.append({**asdict(machine), 'selected_route': route.id if route else None})
            if structured:
                print(json.dumps({'ok': True, 'version': 1, 'machines': rows}, ensure_ascii=True))
            else:
                for machine in rows:
                    route = next((r for r in machine['routes'] if r['id'] == machine['selected_route']), None)
                    print(f"{machine['name']}  |  {route['name'] + ': ' + route['host'] if route else 'Choose a route'}  |  {machine['user']}")
                if not rows:
                    print('No machines yet. Run without a command and press A to add one.')
            return 0
        if options.command == 'command':
            matches = [m for m in snapshot.machines if options.machine in (m.id, m.name)]
            if len(matches) != 1:
                raise CatalogError('Select one machine by its unique ID or exact name.')
            machine = matches[0]
            if options.route:
                routes = [r for r in machine.routes if options.route in (r.id, r.name)]
                if len(routes) != 1: raise CatalogError('Select a unique route ID or name.')
                route = routes[0]
            else:
                route = catalog.preferred(machine)
            if route is None:
                raise CatalogError('Choose a route explicitly with --route or in the TUI.')
            args = ssh_command(machine, route, config=connection_config(catalog, machine, route))
            print(json.dumps({'ok': True, 'argv': args}) if structured else (subprocess.list2cmdline(args) if os.name == 'nt' else shlex.join(args)))
            return 0
        return picker_loop(catalog)
    except (OSError, ValueError, subprocess.SubprocessError) as error:
        if structured:
            print(json.dumps({'ok': False, 'error': str(error)}))
        else:
            print(f'SSH Sessions: {error}', file=sys.stderr)
        return 1


if __name__ == '__main__':
    raise SystemExit(main())
