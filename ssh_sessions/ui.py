"""Keyboard picker. SSH runs after this app releases the terminal."""
from dataclasses import dataclass, replace

from rich.text import Text
from textual import on, work
from textual.app import App, ComposeResult
from textual.binding import Binding
from textual.containers import Horizontal, Vertical, VerticalScroll
from textual.screen import ModalScreen
from textual.widgets import Button, DataTable, Footer, Input, Label, Static

from .catalog import Machine, Route, Snapshot, address, group_path, label, login, new_id, parse_tags, port_number
from .favorites import LOCAL, Favorites, target_name
from .connection import local_shell_name
from .organization import edit_many, filtered, within
from .sync import GitSync


@dataclass(frozen=True)
class Choice:
    kind: str
    machine: Machine | None = None
    route: Route | None = None


class Form(ModalScreen):
    BINDINGS = [('escape', 'cancel', 'Cancel'), ('ctrl+s', 'save', 'Save')]

    def __init__(self, title, fields):
        super().__init__()
        self.heading, self.fields = title, fields

    def compose(self):
        with VerticalScroll(classes='dialog'):
            yield Label(self.heading, classes='heading')
            for key, caption, value in self.fields:
                yield Label(caption)
                yield Input(value=str(value), id=key)
            yield Static('Ctrl+S saves · Esc cancels', classes='muted')
            yield Static('', id='form-error', markup=False)
            yield Button('Save', id='save', variant='primary')

    def on_mount(self):
        self.query_one(Input).focus()

    def action_save(self):
        values = {key: self.query_one('#' + key, Input).value for key, _, _ in self.fields}
        validators = {'name': label, 'user': login, 'route_name': label, 'host': address, 'port': port_number,
                      'group': group_path, 'tags': parse_tags, 'add_tags': parse_tags, 'remove_tags': parse_tags}
        for key, value in values.items():
            try:
                validators[key](value)
            except ValueError as error:
                self.query_one('#form-error', Static).update(str(error))
                self.query_one('#' + key, Input).focus()
                return
        self.dismiss(values)

    @on(Button.Pressed, '#save')
    def save_button(self):
        self.action_save()

    def action_cancel(self):
        self.dismiss(None)


class Confirm(ModalScreen):
    BINDINGS = [('escape', 'cancel', 'Cancel'), ('n', 'cancel', 'No'), ('y', 'accept', 'Yes')]

    def __init__(self, message):
        super().__init__()
        self.message = message

    def compose(self):
        with Vertical(classes='dialog'):
            yield Static(self.message, markup=False)
            with Horizontal(classes='buttons'):
                yield Button('Cancel', id='no')
                yield Button('Delete', id='yes', variant='error')

    def on_mount(self):
        self.query_one('#no', Button).focus()

    def action_cancel(self):
        self.dismiss(False)

    def action_accept(self):
        self.dismiss(True)

    def on_button_pressed(self, event):
        self.dismiss(event.button.id == 'yes')


class SyncMenu(ModalScreen):
    BINDINGS = [('escape', 'cancel', 'Cancel'), ('p', 'pull', 'Pull'), ('u', 'publish', 'Publish')]

    def compose(self):
        with Vertical(classes='dialog'):
            yield Label('Sync the catalog', classes='heading')
            yield Static('Pull updates the repository from its remote. Publish saves and uploads the machine catalog.', markup=False)
            with Horizontal(classes='buttons'):
                yield Button('Pull [P]', id='pull', variant='primary')
                yield Button('Publish [U]', id='publish')
                yield Button('Cancel', id='cancel')

    def action_cancel(self): self.dismiss(None)
    def action_pull(self): self.dismiss('pull')
    def action_publish(self): self.dismiss('publish')

    def on_button_pressed(self, event):
        self.dismiss(None if event.button.id == 'cancel' else event.button.id)


class Help(ModalScreen):
    BINDINGS = [('escape', 'close', 'Back'), ('f1', 'close', 'Back')]

    def compose(self):
        with VerticalScroll(classes='dialog'):
            yield Label('Keyboard guide', classes='heading')
            yield Static('Up / Down, Enter: select a machine and connect\n'
                         '1–9, Enter: select a numbered favorite and open it\n'
                         'F: pin the highlighted machine or Local terminal to a number\n'
                         'G: browse groups; E in Groups renames a group\n'
                         'Space: select a machine; Ctrl+A: select all shown\n'
                         'M: move selected machines to an existing or new group\n'
                         'T: add or remove tags from selected machines\n'
                         'Search accepts multiple words, tag:gpu and group:Work\n'
                         '/: search; Enter returns to the list; Esc clears search\n'
                         'A / E / D: add, edit or delete a machine\n'
                         'R: manage routes and choose one for this device\n'
                         'I: preview and import local SSH settings\n'
                         'S: Pull / Publish\n'
                         'F5: reload changes from another tab or editor\n'
                         'Local terminal row or Ctrl+L: local shell; Q: close the picker\n\n'
                         'Forms: Tab moves fields, Ctrl+S saves, Esc cancels.\n'
                         'Esc returns to the list.', markup=False)

    def action_close(self): self.dismiss(None)


class Routes(ModalScreen):
    BINDINGS = [('escape', 'cancel', 'Back'), ('a', 'add', 'Add route'), ('e', 'edit', 'Edit route'), ('d', 'delete', 'Delete route')]

    def __init__(self, catalog, machine_id, fallback=False):
        super().__init__()
        self.catalog, self.machine_id, self.fallback = catalog, machine_id, fallback

    def compose(self):
        with Vertical(classes='dialog routes-dialog'):
            yield Label('Choose a connection route', classes='heading')
            yield Static('Connection failed. Select another route to try once.' if self.fallback
                         else 'Select the route to use on this device.', id='route-help', markup=False)
            yield DataTable(id='routes', cursor_type='row', zebra_stripes=True)
            yield Static('Enter: ' + ('try once' if self.fallback else 'use on this device') + ' · A: add · E: edit · D: delete · Esc: back', classes='muted')

    def on_mount(self):
        self.query_one(DataTable).add_columns('Route', 'Address', 'Port', 'This device')
        self.reload()
        self.query_one(DataTable).focus()

    def reload(self):
        self.snapshot = self.catalog.load()
        self.machine = next((m for m in self.snapshot.machines if m.id == self.machine_id), None)
        if self.machine is None:
            self.dismiss(None)
            return
        table = self.query_one(DataTable)
        table.clear()
        preferred = self.catalog.preferred(self.machine)
        for route in self.machine.routes:
            table.add_row(Text(route.name), Text(route.host), str(route.port), 'Selected' if preferred == route else '', key=route.id)

    def selected(self):
        table = self.query_one(DataTable)
        return self.machine.routes[table.cursor_row] if table.row_count else None

    def on_data_table_row_selected(self, event):
        event.stop()
        route = self.machine.route(str(event.row_key.value))
        try:
            if not self.fallback:
                self.catalog.choose(self.machine, route.id)
            self.dismiss(Choice('connect', self.machine, route) if self.fallback else route.id)
        except (OSError, ValueError) as error:
            self.notify(str(error), severity='error', timeout=8)

    def action_cancel(self):
        self.dismiss(None)

    def route_form(self, route=None):
        self.app.push_screen(Form('Edit route' if route else 'Add route', [
            ('route_name', 'Route name (for example: LAN or Tailscale)', route.name if route else ''),
            ('host', 'IP address, hostname or existing SSH alias', route.host if route else ''),
            ('port', 'SSH port', route.port if route else 22)]), lambda values: self.save_route(values, route))

    def action_add(self): self.route_form()
    def action_edit(self): self.route_form(self.selected())

    def save_route(self, values, old):
        if values is None: return
        try:
            route = Route.parse({'id': old.id if old else new_id(), 'name': values['route_name'], 'host': values['host'], 'port': values['port'],
                                 'ssh_alias': old.ssh_alias if old else None})
            routes = tuple(route if r.id == route.id else r for r in self.machine.routes) if old else (*self.machine.routes, route)
            self.save_machine(replace(self.machine, routes=routes))
        except (OSError, ValueError) as error:
            self.notify(str(error), severity='error', timeout=8)

    def save_machine(self, machine):
        self.catalog.save(tuple(machine if m.id == machine.id else m for m in self.snapshot.machines), self.snapshot.revision)
        self.reload()

    def action_delete(self):
        route = self.selected()
        if len(self.machine.routes) <= 1:
            self.notify('Keep at least one route. Delete the machine from the main list instead.', severity='warning')
            return
        def remove(confirmed):
            if confirmed:
                try:
                    self.save_machine(replace(self.machine, routes=tuple(r for r in self.machine.routes if r.id != route.id)))
                except (OSError, ValueError) as error:
                    self.notify(str(error), severity='error', timeout=8)
        self.app.push_screen(Confirm(f'Delete route {route.name}?'), remove)


class Picker(App[Choice]):
    TITLE = 'SSH Sessions'
    ENABLE_COMMAND_PALETTE = False
    BINDINGS = [Binding('f1', 'help', 'Help'), Binding('q', 'quit', 'Quit'), Binding('a', 'add', 'Add'), Binding('i', 'import_ssh', 'Import'), Binding('e', 'edit', 'Edit'),
                Binding('r', 'routes', 'Routes'), Binding('d', 'delete', 'Delete'), Binding('/', 'search', 'Search'),
                Binding('s', 'sync', 'Sync'), Binding('f5', 'reload', 'Reload'), Binding('ctrl+l', 'local', 'Local shell'),
                Binding('f', 'favorites', 'Favorites'),
                Binding('g', 'groups', 'Groups'), Binding('space', 'toggle_selection', 'Select', show=False),
                Binding('ctrl+a', 'select_all', 'Select all', show=False), Binding('m', 'move', 'Move', show=False),
                Binding('t', 'tags', 'Tags', show=False),
                *[Binding(str(n), f'favorite({n})', show=False) for n in range(1, 10)],
                Binding('escape', 'clear_search', 'Back', show=False)]
    CSS = '''
    Screen { background: #101923; color: #dfebf2; }
    #top { height: auto; padding: 1 2 0 2; }
    .heading { color: #79dac3; text-style: bold; margin-bottom: 1; }
    .muted { color: #9aafbd; height: auto; }
    #hotbar { height: auto; margin: 1 2 0 2; color: #79dac3; }
    #scope { height: auto; margin: 0 2; color: #9aafbd; }
    #search { margin: 1 2 0 2; height: 3; }
    #machines { margin: 0 2; height: 1fr; min-height: 3; }
    #details { margin: 1 2 0 2; height: auto; color: #bdcdda; }
    #notice { margin: 0 2 1 2; height: auto; color: #efc982; }
    Footer { background: #203342; }
    ModalScreen { align: center middle; background: #000000 60%; }
    .dialog { width: 82; max-width: 94%; height: auto; max-height: 92%; padding: 1 2; border: round #79dac3; background: #162532; }
    .dialog Input { margin-bottom: 1; }
    .dialog Static { height: auto; margin-bottom: 1; }
    .buttons { height: auto; margin-top: 1; }
    .buttons Button { margin-right: 1; min-width: 12; }
    .routes-dialog { height: 22; }
    #routes { height: 1fr; min-height: 3; }
    .import-dialog { width: 98; height: 27; }
    #import-hosts { height: 1fr; min-height: 3; }
    .favorites-dialog { height: 22; }
    #favorite-slots { height: 1fr; min-height: 3; }
    .groups-dialog { height: 25; }
    #groups { height: 1fr; min-height: 3; }
    .compact #top { padding: 0 2; }
    .compact .heading { margin-bottom: 0; }
    .compact #hotbar { margin: 0 2; }
    .compact #search { margin: 0 2; }
    .compact #details { margin: 0 2; }
    .compact #notice { margin: 0 2; }
    '''

    def __init__(self, catalog, notice='', failed_machine=None):
        super().__init__()
        self.catalog, self.notice, self.failed_machine = catalog, notice, failed_machine
        self.filtered_machines = []
        self.row_targets = []
        self.snapshot = Snapshot((), '')
        self.favorite_slots = {}
        self.failed_favorite = False
        self.group_filter = None
        self.checked = set()
        self.syncing = False

    def compose(self) -> ComposeResult:
        with Vertical(id='top'):
            yield Label('SSH Sessions', classes='heading')
        yield Static('', id='hotbar', markup=False)
        yield Input(placeholder='Search names, addresses, groups or tags  [/]', id='search')
        yield Static('', id='scope', markup=False)
        yield DataTable(id='machines', cursor_type='row', zebra_stripes=True)
        yield Static('', id='details', markup=False)
        yield Static(self.notice, id='notice', markup=False)
        yield Footer()

    def on_mount(self):
        self.screen.set_class(self.size.height < 26, 'compact')
        self.query_one('#machines', DataTable).add_columns('', 'Key', 'Machine', 'Group', 'Route', 'Address')
        self.action_reload()
        self.query_one('#machines', DataTable).focus()
        if self.failed_machine and any(m.id == self.failed_machine and len(m.routes) > 1 for m in self.snapshot.machines):
            self.push_screen(Routes(self.catalog, self.failed_machine, fallback=True), self.after_routes)

    def on_resize(self, event):
        if self.screen_stack:
            self.screen_stack[0].set_class(event.size.height < 26, 'compact')

    def action_reload(self):
        try:
            self.snapshot = self.catalog.load()
        except (OSError, ValueError) as error:
            self.query_one('#notice', Static).update(str(error))
        try:
            self.favorite_slots = Favorites(self.catalog).load().slots
        except (OSError, ValueError) as error:
            self.favorite_slots = {}
            self.query_one('#notice', Static).update(str(error))
        self.refresh_rows()

    def refresh_rows(self):
        previous = self.selected_key()
        query = self.query_one('#search', Input).value.casefold()
        self.filtered_machines = filtered(self.snapshot.machines, query, self.group_filter)
        self.checked.intersection_update(m.id for m in self.filtered_machines)
        table = self.query_one('#machines', DataTable)
        table.clear()
        try:
            preferences = self.catalog.preferences()
        except (OSError, ValueError) as error:
            preferences = {}
            self.query_one('#notice', Static).update(str(error))
        keys = {target: slot for slot, target in self.favorite_slots.items()}
        entries = {m.id: m for m in self.filtered_machines}
        if query in 'local terminal powershell this device':
            entries[LOCAL] = None
        self.row_targets = sorted(entries, key=lambda target: int(keys.get(target, '10')))
        for target in self.row_targets:
            machine = entries[target]
            if machine is None:
                table.add_row('', keys.get(LOCAL, ''), 'Local terminal', '', local_shell_name(), 'This computer', key=LOCAL)
                continue
            route = self.catalog.preferred(machine, preferences)
            table.add_row(Text('[x]' if machine.id in self.checked else '[ ]'), keys.get(machine.id, ''), Text(machine.name),
                          Text(machine.group or 'Ungrouped'), Text(route.name if route else 'Choose route'),
                          Text(route.host if route else f'{len(machine.routes)} routes'), key=machine.id)
        if previous in self.row_targets:
            table.move_cursor(row=self.row_targets.index(previous))
        def short_name(target):
            name = target_name(target, self.snapshot.machines)
            return name if len(name) <= 24 else name[:23] + '…'
        hotbar = '   '.join(f'[{slot}] {short_name(target)}' for slot, target in sorted(self.favorite_slots.items()))
        self.query_one('#hotbar', Static).update(Text((hotbar + '\n1–9 then Enter: open · F: edit favorites') if hotbar else
            'Common connections: select a row and press F to assign a number.'))
        scope = 'All machines' if self.group_filter is None else self.group_filter or 'Ungrouped'
        self.query_one('#scope', Static).update(f'{scope} · {len(self.filtered_machines)} machines · {len(self.checked)} selected · G: groups · M: move · T: tags')
        self.query_one('#details', Static).update('Enter opens · F pins a favorite · R chooses a route · F1: help.' if self.row_targets else
            'No matches. Esc clears the search; A adds a machine; Ctrl+L opens a local shell.')

    @on(Input.Changed, '#search')
    def search_changed(self):
        if hasattr(self, 'snapshot'): self.refresh_rows()

    @on(Input.Submitted, '#search')
    def search_submitted(self):
        self.failed_favorite = False
        self.query_one('#machines', DataTable).focus()

    def on_key(self, event):
        if event.key in ('up', 'down', 'home', 'end', 'pageup', 'pagedown') and len(self.screen_stack) == 1:
            self.failed_favorite = False

    @on(DataTable.RowHighlighted, '#machines')
    def show_destination(self, event):
        if str(event.row_key.value) == LOCAL:
            self.query_one('#details', Static).update('Local shell on this computer. Exit returns to this picker.\nEnter opens · F pins a favorite.')
            return
        machine = next((m for m in self.filtered_machines if m.id == str(event.row_key.value)), None)
        if machine:
            try:
                route = self.catalog.preferred(machine)
            except (OSError, ValueError) as error:
                self.query_one('#details', Static).update(str(error))
                return
            text = f'{machine.user}@{route.host}:{route.port} · {route.name}' if route else 'Press R to choose a route.'
            details = ' · '.join(filter(None, (machine.group, ', '.join(machine.tags))))
            self.query_one('#details', Static).update(text + '\n' + (details + ' · ' if details else '') + 'Enter connects · F1: help')

    def selected(self):
        return next((m for m in self.filtered_machines if m.id == self.selected_key()), None)

    def selected_key(self):
        table = self.query_one('#machines', DataTable)
        return self.row_targets[table.cursor_row] if table.row_count and table.cursor_row < len(self.row_targets) else None

    @on(DataTable.RowSelected, '#machines')
    def connect(self, event):
        if self.failed_favorite:
            self.notify('That favorite is unavailable. Choose a valid number or use the arrow keys before Enter.', severity='warning')
            return
        if self.selected_key() == LOCAL:
            self.action_local()
            return
        machine = self.selected()
        if machine:
            try:
                if self.catalog.load().revision != self.snapshot.revision:
                    self.action_reload()
                    self.notify('The catalog changed. Review the updated destination and press Enter again.', severity='warning')
                    return
                route = self.catalog.preferred(machine)
                if route:
                    self.exit(Choice('connect', machine, route))
                else:
                    self.push_screen(Routes(self.catalog, machine.id), self.after_routes)
            except (OSError, ValueError) as error:
                self.notify(str(error), severity='error', timeout=8)

    def action_favorite(self, slot):
        if len(self.screen_stack) != 1:
            return
        self.failed_favorite = True
        try:
            target = Favorites(self.catalog).load().slots.get(str(slot))
            if target is None:
                self.notify(f'Slot {slot} is empty. Select a row and press F to assign it.')
                return
            self.query_one('#search', Input).value = ''
            self.group_filter = None
            self.checked.clear()
            self.action_reload()
            if target not in self.row_targets:
                self.notify('This favorite refers to a missing machine. Press F to replace or clear its slot.', severity='warning')
                return
            table = self.query_one('#machines', DataTable)
            table.move_cursor(row=self.row_targets.index(target))
            table.focus()
            self.failed_favorite = False
        except (OSError, ValueError) as error:
            self.notify(str(error), severity='error', timeout=8)

    def action_favorites(self):
        if len(self.screen_stack) != 1 or self.selected_key() is None:
            return
        from .favorites_ui import FavoriteSlots
        def finished(message):
            self.action_reload()
            if message:
                self.query_one('#notice', Static).update(message)
        try:
            self.push_screen(FavoriteSlots(self.catalog, self.selected_key()), finished)
        except (OSError, ValueError) as error:
            self.notify(str(error), severity='error', timeout=8)

    def action_groups(self):
        if len(self.screen_stack) != 1:
            return
        from .groups_ui import GroupBrowser
        def chosen(result):
            if result:
                if result[0] == 'select':
                    self.group_filter = result[1]
                elif self.group_filter and within(self.group_filter, result[1]):
                    suffix = self.group_filter.split('/')[len(result[1].split('/')):]
                    self.group_filter = '/'.join([group_path(result[2]), *suffix])
                self.checked.clear()
                self.query_one('#search', Input).value = ''
            self.action_reload()
        try:
            self.push_screen(GroupBrowser(self.catalog, self.group_filter), chosen)
        except (OSError, ValueError) as error:
            self.notify(str(error), severity='error', timeout=8)

    def action_toggle_selection(self):
        if len(self.screen_stack) != 1:
            return
        machine = self.selected()
        if machine:
            self.checked.symmetric_difference_update((machine.id,))
            self.refresh_rows()

    def action_select_all(self):
        if len(self.screen_stack) != 1:
            return
        ids = {m.id for m in self.filtered_machines}
        self.checked = set() if self.checked == ids else ids
        self.refresh_rows()

    def edit_targets(self):
        if len(self.screen_stack) != 1:
            return set()
        machine = self.selected()
        return set(self.checked) if self.checked else ({machine.id} if machine else set())

    def bulk_edit(self, heading, fields, apply):
        targets = self.edit_targets()
        if not targets:
            self.notify('Select a machine first.')
            return
        revision = self.snapshot.revision
        def saved(values):
            if values is None:
                return
            try:
                edit_many(self.catalog, targets, revision, **apply(values))
                self.checked.clear()
                self.action_reload()
                self.query_one('#notice', Static).update(f'Updated {len(targets)} machines.')
            except (OSError, ValueError) as error:
                self.notify(str(error), severity='error', timeout=8)
        self.push_screen(Form(f'{heading} · {len(targets)} machines', fields), saved)

    def action_move(self):
        self.bulk_edit('Move to group', [('group', 'Group path (e.g. Work/Production; blank for Ungrouped)', self.group_filter or '')],
                       lambda values: {'group': values['group']})

    def action_tags(self):
        self.bulk_edit('Edit tags', [('add_tags', 'Add tags (comma-separated)', ''), ('remove_tags', 'Remove tags', '')],
                       lambda values: {'add_tags': values['add_tags'], 'remove_tags': values['remove_tags']})

    def action_routes(self):
        machine = self.selected()
        if machine: self.push_screen(Routes(self.catalog, machine.id), self.after_routes)

    def after_routes(self, result):
        if isinstance(result, Choice):
            self.exit(result)
        else:
            self.action_reload()

    def action_search(self):
        self.checked.clear()
        self.refresh_rows()
        self.query_one('#search', Input).focus()
    def action_help(self): self.push_screen(Help())
    def action_import_ssh(self):
        from .import_ui import ImportSSH
        def finished(message):
            self.action_reload()
            if message:
                self.query_one('#notice', Static).update(message)
        self.push_screen(ImportSSH(self.catalog, group=self.group_filter or ''), finished)
    def action_clear_search(self):
        self.failed_favorite = False
        self.group_filter = None
        self.checked.clear()
        self.query_one('#search', Input).value = ''
        self.refresh_rows()
        self.query_one('#machines', DataTable).focus()
    def action_local(self): self.exit(Choice('local'))

    def action_add(self):
        self.push_screen(Form('Add a machine', [('name', 'Machine name', ''), ('user', 'Remote username', ''),
            ('route_name', 'First route name (for example: LAN)', 'Default'), ('host', 'IP address, hostname or existing SSH alias', ''),
            ('port', 'SSH port', 22), ('group', 'Group (optional)', self.group_filter or ''),
            ('tags', 'Tags (comma-separated, optional)', '')]), self.add_machine)

    def add_machine(self, values):
        if values is None: return
        try:
            machine = Machine.parse({'id': new_id(), 'name': values['name'], 'user': values['user'], 'routes': [
                {'id': new_id(), 'name': values['route_name'], 'host': values['host'], 'port': values['port']}],
                'group': values['group'], 'tags': parse_tags(values['tags'])})
            self.catalog.save((*self.snapshot.machines, machine), self.snapshot.revision)
            self.action_reload()
        except (OSError, ValueError) as error:
            self.notify(str(error), severity='error', timeout=8)

    def action_edit(self):
        machine = self.selected()
        if not machine: return
        def edited(values):
            if values is None: return
            try:
                changed = replace(machine, name=values['name'], user=values['user'], group=group_path(values['group']), tags=parse_tags(values['tags']))
                self.catalog.save(tuple(changed if m.id == machine.id else m for m in self.snapshot.machines), self.snapshot.revision)
                self.action_reload()
            except (OSError, ValueError) as error:
                self.notify(str(error), severity='error', timeout=8)
        self.push_screen(Form('Edit machine', [('name', 'Machine name', machine.name), ('user', 'Remote username', machine.user),
                                              ('group', 'Group (optional)', machine.group), ('tags', 'Tags (comma-separated)', ', '.join(machine.tags))]), edited)

    def action_delete(self):
        machine = self.selected()
        if not machine: return
        def remove(confirmed):
            if confirmed:
                try:
                    self.catalog.save(tuple(m for m in self.snapshot.machines if m.id != machine.id), self.snapshot.revision)
                    self.action_reload()
                except (OSError, ValueError) as error:
                    self.notify(str(error), severity='error', timeout=8)
        self.push_screen(Confirm(f'Delete {machine.name} from the shared catalog?'), remove)

    def action_sync(self):
        if self.syncing:
            self.notify('A sync operation is already running.')
            return
        self.push_screen(SyncMenu(), self.start_sync)

    def start_sync(self, action):
        if action:
            self.syncing = True
            self.query_one('#notice', Static).update('Syncing the catalog…')
            self.sync_catalog(action)

    def finish_sync(self, message, failed=False):
        self.syncing = False
        self.query_one('#notice', Static).update(message)
        if failed:
            self.notify(message, severity='error', timeout=12)
        else:
            self.action_reload()

    @work(thread=True, exclusive=True)
    def sync_catalog(self, action):
        try:
            message = GitSync(self.catalog).run(action)
            self.call_from_thread(self.finish_sync, message)
        except (OSError, ValueError) as error:
            self.call_from_thread(self.finish_sync, str(error), True)
