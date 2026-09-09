"""Keyboard review and selection before importing local SSH metadata."""
from rich.text import Text
from textual import on
from textual.containers import Vertical
from textual.screen import ModalScreen
from textual.widgets import DataTable, Input, Label, Static

from .ssh_import import default_config, import_selected, import_status, scan_ssh


class ImportSSH(ModalScreen):
    BINDINGS = [('escape', 'cancel', 'Cancel'), ('space', 'toggle', 'Select'), ('a', 'all', 'All / none')]

    def __init__(self, catalog):
        super().__init__()
        self.catalog = catalog
        self.selected = set()
        self.scan = None

    def compose(self):
        with Vertical(classes='dialog import-dialog'):
            yield Label('Import from local SSH settings', classes='heading')
            yield Static('Preview names, addresses and usernames. Keys and proxy commands stay in local SSH config.', markup=False)
            yield Input(str(default_config()), id='config-path', placeholder='SSH config path; Enter reloads')
            yield DataTable(id='import-hosts', cursor_type='row', zebra_stripes=True)
            yield Static('', id='import-detail', markup=False)
            yield Static('Space: select · A: all/none · Enter: import selected (or highlighted) · Esc: cancel', markup=False)

    def on_mount(self):
        self.query_one(DataTable).add_columns('Pick', 'SSH name', 'Address', 'Username', 'Port')
        self.load_preview()

    @on(Input.Submitted, '#config-path')
    def load_preview(self):
        self.scan = None
        self.selected.clear()
        table = self.query_one(DataTable)
        table.clear()
        try:
            self.snapshot = self.catalog.load()
            self.scan = scan_ssh(self.query_one(Input).value)
            for entry in self.scan.entries:
                table.add_row(Text('[ ]' if self.allowed(entry) else ' ! '), Text(entry.alias), Text(entry.host),
                              Text(entry.user), str(entry.port), key=entry.alias)
            self.query_one('#import-detail', Static).update('No named Host entries found. Wildcards are patterns, not machines.' if not self.scan.entries
                else 'No changes until you press Enter. Import does not connect or publish.')
            table.focus()
        except (OSError, ValueError) as error:
            self.query_one('#import-detail', Static).update(str(error))

    def allowed(self, entry):
        return import_status(self.snapshot.machines, entry) in ('Ready to import', 'Already imported (select to bind this device)')

    def current(self):
        table = self.query_one(DataTable)
        return self.scan.entries[table.cursor_row] if self.scan and table.row_count else None

    @on(DataTable.RowHighlighted, '#import-hosts')
    def details(self):
        entry = self.current()
        if entry:
            self.query_one('#import-detail', Static).update(f'{entry.user}@{entry.host}:{entry.port}\n' +
                import_status(self.snapshot.machines, entry) + f' · {len(self.selected)} selected')

    def redraw_checks(self):
        table = self.query_one(DataTable)
        for index, entry in enumerate(self.scan.entries):
            table.update_cell_at((index, 0), Text('[x]' if entry.alias in self.selected else '[ ]' if self.allowed(entry) else ' ! '))
        self.details()

    def action_toggle(self):
        entry = self.current()
        if entry and self.allowed(entry):
            self.selected.symmetric_difference_update((entry.alias,))
            self.redraw_checks()

    def action_all(self):
        if self.scan:
            available = {e.alias for e in self.scan.entries if self.allowed(e)}
            self.selected = set() if self.selected == available else available
            self.redraw_checks()

    @on(DataTable.RowSelected, '#import-hosts')
    def accept(self, event):
        event.stop()
        entry = self.current()
        chosen = self.selected or ({entry.alias} if entry and self.allowed(entry) else set())
        try:
            result = import_selected(self.catalog, self.scan, chosen, self.snapshot.revision)
            self.dismiss(f"Imported {result['added_machines']} machines and {result['added_routes']} routes; "
                         f"bound {result['bound_existing']} existing routes. Nothing connected or published.")
        except (OSError, ValueError) as error:
            self.query_one('#import-detail', Static).update(str(error))

    def action_cancel(self):
        self.dismiss(None)
