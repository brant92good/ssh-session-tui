"""Choose or clear a numbered slot without connecting to a machine."""
from rich.text import Text
from textual.binding import Binding
from textual.containers import Vertical
from textual.screen import ModalScreen
from textual.widgets import DataTable, Label, Static

from .favorites import Favorites, target_name


class FavoriteSlots(ModalScreen):
    BINDINGS = [Binding('escape', 'cancel', 'Cancel'), Binding('d', 'clear', 'Clear slot'),
                *[Binding(str(n), f'select_slot({n})', show=False) for n in range(1, 10)]]

    def __init__(self, catalog, target):
        super().__init__()
        self.store = Favorites(catalog)
        self.machines = catalog.load().machines
        self.saved = self.store.load()
        self.target = target

    def compose(self):
        with Vertical(classes='dialog favorites-dialog'):
            yield Label('Numbered favorites', classes='heading')
            yield Static('Pin ' + target_name(self.target, self.machines) +
                         '. Choose a slot; Enter replaces its current favorite.', markup=False)
            yield DataTable(id='favorite-slots', cursor_type='row', zebra_stripes=True)
            yield Static('1–9 or arrows: choose slot · Enter: save · D: clear slot · Esc: cancel', markup=False)

    def on_mount(self):
        table = self.query_one(DataTable)
        table.add_columns('Key', 'Current favorite')
        for n in range(1, 10):
            target = self.saved.slots.get(str(n))
            table.add_row(str(n), Text(target_name(target, self.machines) if target else 'Empty'), key=str(n))
        table.focus()

    def action_select_slot(self, slot):
        self.query_one(DataTable).move_cursor(row=slot - 1)

    def save(self, target):
        try:
            slot = self.query_one(DataTable).cursor_row + 1
            self.store.assign(slot, target, self.saved.revision)
            self.dismiss(f'Favorite {slot} saved.' if target else f'Favorite {slot} cleared.')
        except (OSError, ValueError) as error:
            self.notify(str(error), severity='error', timeout=8)

    def on_data_table_row_selected(self, event):
        event.stop()
        self.save(self.target)

    def action_clear(self):
        self.save(None)

    def action_cancel(self):
        self.dismiss(None)
