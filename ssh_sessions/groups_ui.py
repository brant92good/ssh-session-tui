"""Keyboard browsing and renaming of nested machine groups."""
from rich.text import Text
from textual import on
from textual.containers import Vertical
from textual.screen import ModalScreen
from textual.widgets import DataTable, Input, Label, Static

from .organization import groups, rename_group


class GroupBrowser(ModalScreen):
    BINDINGS = [('escape', 'cancel', 'Cancel'), ('e', 'rename', 'Rename group')]

    def __init__(self, catalog, current=None):
        super().__init__()
        self.catalog, self.current = catalog, current
        self.snapshot = catalog.load()
        self.paths = []

    def compose(self):
        with Vertical(classes='dialog groups-dialog'):
            yield Label('Groups', classes='heading')
            yield Input(placeholder='Find a group', id='group-search')
            yield DataTable(id='groups', cursor_type='row', zebra_stripes=True)
            yield Static('Enter: open group · E: rename · Esc: back\nCreate a group with M from the machine list.', markup=False)

    def on_mount(self):
        self.query_one(DataTable).add_columns('Group', 'Machines')
        self.reload()
        table = self.query_one(DataTable)
        if self.current in self.paths:
            table.move_cursor(row=self.paths.index(self.current))
        table.focus()

    def reload(self):
        try:
            self.snapshot = self.catalog.load()
        except (OSError, ValueError) as error:
            self.notify(str(error), severity='error', timeout=8)
            return
        query = self.query_one(Input).value.casefold()
        table = self.query_one(DataTable)
        table.clear()
        counts = groups(self.snapshot.machines)
        entries = [(None, 'All machines', len(self.snapshot.machines)),
                   ('', 'Ungrouped', sum(not m.group for m in self.snapshot.machines)),
                   *[(path, '  ' * path.count('/') + path, count) for path, count in counts.items()]]
        self.paths = []
        for path, name, count in entries:
            if query in name.casefold():
                self.paths.append(path)
                table.add_row(Text(name), str(count), key=str(len(self.paths) - 1))

    @on(Input.Changed, '#group-search')
    def search(self):
        if self.is_mounted:
            self.reload()

    @on(Input.Submitted, '#group-search')
    def focus_list(self):
        self.query_one(DataTable).focus()

    def on_data_table_row_selected(self, event):
        event.stop()
        self.dismiss(('select', self.paths[self.query_one(DataTable).cursor_row]))

    def action_rename(self):
        table = self.query_one(DataTable)
        if not table.row_count:
            return
        path = self.paths[table.cursor_row]
        if not path:
            self.notify('Choose a named group to rename.')
            return
        from .ui import Form
        def rename(values):
            if values is None:
                return
            try:
                rename_group(self.catalog, path, values['group'], self.snapshot.revision)
                self.dismiss(('renamed', path, values['group']))
            except (OSError, ValueError) as error:
                self.notify(str(error), severity='error', timeout=8)
        self.app.push_screen(Form('Rename group and its subgroups', [('group', 'New group path', path)]), rename)

    def action_cancel(self):
        self.dismiss(None)
