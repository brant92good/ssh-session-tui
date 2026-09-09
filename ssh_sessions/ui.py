"""Keyboard picker. SSH runs after this app releases the terminal."""
from dataclasses import dataclass, replace

from rich.text import Text
from textual import on, work
from textual.app import App, ComposeResult
from textual.binding import Binding
from textual.containers import Horizontal, Vertical, VerticalScroll
from textual.screen import ModalScreen
from textual.widgets import Button, DataTable, Footer, Input, Label, Static

from .catalog import Machine, Route, address, label, login, new_id, port_number
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
        validators = {'name': label, 'user': login, 'route_name': label, 'host': address, 'port': port_number}
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
            yield Static('Pull downloads the private repo. Publish commits and pushes only the tracked catalog file. Neither action installs settings or copies private keys.', markup=False)
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
                         '/: search; Enter returns to the list; Esc clears search\n'
                         'A / E / D: add, edit or delete a machine\n'
                         'R: manage routes and choose one for this device\n'
                         'I: preview and import local SSH settings\n'
                         'S: explicit Git Pull / Publish\n'
                         'F5: reload changes from another tab or editor\n'
                         'Ctrl+L: local PowerShell; Q: close the picker\n\n'
                         'Forms: Tab moves fields, Ctrl+S saves, Esc cancels.\n'
                         'An alternative route is never started without your choice.\n\n'
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
            yield Static('Connection failed. You can choose a different route below. Enter tries it once; Esc returns without connecting.' if self.fallback
                         else 'Enter selects the route for this device. Other devices keep their own choice.', id='route-help', markup=False)
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
                Binding('escape', 'clear_search', 'Back', show=False)]
    CSS = '''
    Screen { background: #101923; color: #dfebf2; }
    #top { height: auto; padding: 1 2 0 2; }
    .heading { color: #79dac3; text-style: bold; margin-bottom: 1; }
    .muted { color: #9aafbd; height: auto; }
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
    '''

    def __init__(self, catalog, notice='', failed_machine=None):
        super().__init__()
        self.catalog, self.notice, self.failed_machine = catalog, notice, failed_machine
        self.filtered_machines = []
        self.syncing = False

    def compose(self) -> ComposeResult:
        with Vertical(id='top'):
            yield Label('SSH Sessions', classes='heading')
            yield Static('Choose a machine. Addresses can sync; private keys stay on this device.', classes='muted', markup=False)
        yield Input(placeholder='Press / to search machines or addresses', id='search')
        yield DataTable(id='machines', cursor_type='row', zebra_stripes=True)
        yield Static('', id='details', markup=False)
        yield Static(self.notice, id='notice', markup=False)
        yield Footer()

    def on_mount(self):
        self.query_one('#machines', DataTable).add_columns('Machine', 'Route on this device', 'Address', 'Username')
        self.action_reload()
        self.query_one('#machines', DataTable).focus()
        if self.failed_machine and any(m.id == self.failed_machine and len(m.routes) > 1 for m in self.snapshot.machines):
            self.push_screen(Routes(self.catalog, self.failed_machine, fallback=True), self.after_routes)

    def action_reload(self):
        try:
            self.snapshot = self.catalog.load()
            self.refresh_rows()
        except (OSError, ValueError) as error:
            self.query_one('#notice', Static).update(str(error))

    def refresh_rows(self):
        query = self.query_one('#search', Input).value.casefold()
        self.filtered_machines = [m for m in self.snapshot.machines if query in ' '.join([m.name, m.user, *(r.host for r in m.routes), *(r.name for r in m.routes)]).casefold()]
        table = self.query_one('#machines', DataTable)
        table.clear()
        preferences = self.catalog.preferences()
        for machine in self.filtered_machines:
            route = self.catalog.preferred(machine, preferences)
            table.add_row(Text(machine.name), Text(route.name if route else 'Choose a route'),
                          Text(route.host if route else f'{len(machine.routes)} routes available'), Text(machine.user), key=machine.id)
        self.query_one('#details', Static).update('Enter connects · R chooses a route · F1 shows all keys.' if self.filtered_machines else
            'No matches. Clear the search, or press A to add your first machine.')

    @on(Input.Changed, '#search')
    def search_changed(self):
        if hasattr(self, 'snapshot'): self.refresh_rows()

    @on(Input.Submitted, '#search')
    def search_submitted(self):
        self.query_one('#machines', DataTable).focus()

    @on(DataTable.RowHighlighted, '#machines')
    def show_destination(self, event):
        machine = next((m for m in self.filtered_machines if m.id == str(event.row_key.value)), None)
        if machine:
            route = self.catalog.preferred(machine)
            text = f'{machine.user}@{route.host}:{route.port} · {route.name}' if route else 'Press R to choose a route for this device.'
            self.query_one('#details', Static).update(text + '\nEnter connects · F1 shows all keys.')

    def selected(self):
        table = self.query_one('#machines', DataTable)
        return self.filtered_machines[table.cursor_row] if table.row_count else None

    @on(DataTable.RowSelected, '#machines')
    def connect(self, event):
        machine = self.selected()
        if machine:
            route = self.catalog.preferred(machine)
            if route:
                self.exit(Choice('connect', machine, route))
            else:
                self.push_screen(Routes(self.catalog, machine.id), self.after_routes)

    def action_routes(self):
        machine = self.selected()
        if machine: self.push_screen(Routes(self.catalog, machine.id), self.after_routes)

    def after_routes(self, result):
        if isinstance(result, Choice):
            self.exit(result)
        else:
            self.action_reload()

    def action_search(self): self.query_one('#search', Input).focus()
    def action_help(self): self.push_screen(Help())
    def action_import_ssh(self):
        from .import_ui import ImportSSH
        def finished(message):
            self.action_reload()
            if message:
                self.query_one('#notice', Static).update(message)
        self.push_screen(ImportSSH(self.catalog), finished)
    def action_clear_search(self):
        self.query_one('#search', Input).value = ''
        self.query_one('#machines', DataTable).focus()
    def action_local(self): self.exit(Choice('local'))

    def action_add(self):
        self.push_screen(Form('Add a machine', [('name', 'Machine name', ''), ('user', 'Remote username', ''),
            ('route_name', 'First route name (for example: LAN)', 'Default'), ('host', 'IP address, hostname or existing SSH alias', ''),
            ('port', 'SSH port', 22)]), self.add_machine)

    def add_machine(self, values):
        if values is None: return
        try:
            machine = Machine.parse({'id': new_id(), 'name': values['name'], 'user': values['user'], 'routes': [
                {'id': new_id(), 'name': values['route_name'], 'host': values['host'], 'port': values['port']}]})
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
                changed = replace(machine, name=values['name'], user=values['user'])
                self.catalog.save(tuple(changed if m.id == machine.id else m for m in self.snapshot.machines), self.snapshot.revision)
                self.action_reload()
            except (OSError, ValueError) as error:
                self.notify(str(error), severity='error', timeout=8)
        self.push_screen(Form('Edit machine', [('name', 'Machine name', machine.name), ('user', 'Remote username', machine.user)]), edited)

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
