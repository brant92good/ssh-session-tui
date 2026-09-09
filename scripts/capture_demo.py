"""Render the real picker with example metadata; never opens SSH."""
import asyncio
import os
from pathlib import Path
import re
import sys
import tempfile
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT))
# Render a normal true-color terminal, independent of the invoking CI shell.
os.environ.pop('NO_COLOR', None)
os.environ['TERM'] = 'xterm-256color'
os.environ['COLORTERM'] = 'truecolor'
from ssh_sessions.catalog import Catalog
from ssh_sessions.ui import Picker
from textual.widgets import Input


async def main():
    output = ROOT / 'docs/screenshots'
    output.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix='session-demo-') as name:
        directory = Path(name)
        catalog = Catalog(directory / 'catalog.json', directory / 'device')
        catalog.path.write_bytes((ROOT / 'examples/catalog.json').read_bytes())
        catalog.choose(catalog.load().machines[0], 'lan')
        app = Picker(catalog)
        async with app.run_test(size=(100, 30)) as pilot:
            await pilot.pause(.3)
            app.save_screenshot('picker.svg', str(output))
            await pilot.press('r')
            await pilot.pause(.3)
            app.save_screenshot('routes.svg', str(output))
            await pilot.press('escape')
            with patch('ssh_sessions.import_ui.default_config', return_value=ROOT / 'examples/ssh_config'):
                await pilot.press('i')
                await pilot.pause(.3)
                app.screen.query_one(Input).value = 'examples/ssh_config'
                await pilot.press('space')
                await pilot.pause(.3)
                app.save_screenshot('import.svg', str(output))
                await pilot.press('escape', 'q')
        compact = Picker(catalog)
        async with compact.run_test(size=(70, 20)) as pilot:
            await pilot.pause(.3)
            (ROOT / 'artifacts').mkdir(exist_ok=True)
            compact.save_screenshot('compact.svg', str(ROOT / 'artifacts'))
            await pilot.press('q')
    for path in (output / 'picker.svg', output / 'routes.svg', output / 'import.svg', ROOT / 'artifacts/compact.svg'):
        path.write_text(re.sub(r'(?m)^[ \t]+$', '', path.read_text(encoding='utf-8')), encoding='utf-8')
    print('Captured example picker and route screens without SSH.')


if __name__ == '__main__':
    asyncio.run(main())
