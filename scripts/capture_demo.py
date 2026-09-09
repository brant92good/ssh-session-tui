"""Render the real picker with example metadata; never opens SSH."""
import asyncio
import base64
from html import escape
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
from ssh_sessions.favorites import Favorites, LOCAL
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
        favorites = Favorites(catalog)
        favorites.assign(1, catalog.load().machines[0].id)
        favorites.assign(2, LOCAL)
        favorites.assign(3, catalog.load().machines[1].id)
        app = Picker(catalog)
        async with app.run_test(size=(100, 30)) as pilot:
            await pilot.pause(.3)
            app.save_screenshot('picker.svg', str(output))
            await pilot.press('g')
            await pilot.pause(.3)
            app.save_screenshot('groups.svg', str(output))
            await pilot.press('escape')
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
    for path in (*output.glob('*.svg'), ROOT / 'artifacts/compact.svg'):
        svg = path.read_text(encoding='utf-8')
        # Embedded SVG images cannot fetch web fonts. Include the intended cell
        # font so GitHub and offline previews keep the same layout.
        fonts = ROOT / 'docs/fonts'
        for weight, name in [(400, 'Regular'), (700, 'Bold')]:
            encoded = base64.b64encode((fonts / f'FiraCode-{name}.woff2').read_bytes()).decode('ascii')
            face = ('@font-face { font-family: "Fira Code"; '
                    f'src: url("data:font/woff2;base64,{encoded}") format("woff2"); '
                    f'font-style: normal; font-weight: {weight}; }}')
            svg = re.sub(r'@font-face\s*\{[^}]*font-weight:\s*' + str(weight) + r';[^}]*\}',
                         lambda match: face, svg, count=1)
        license_text = escape((fonts / 'LICENSE').read_text(encoding='utf-8'))
        svg = svg.replace('<!-- Generated with Rich https://www.textualize.io -->',
                          '<!-- Generated with Rich https://www.textualize.io -->\n'
                          f'<metadata>Embedded Fira Code 6.2 font license:\n{license_text}</metadata>')
        path.write_text(re.sub(r'(?m)^[ \t]+$', '', svg), encoding='utf-8')
    print('Captured example picker and route screens without SSH.')


if __name__ == '__main__':
    asyncio.run(main())
