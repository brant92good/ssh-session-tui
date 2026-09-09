"""Package the exact tested target executable; no rebuilding or source archive."""
import argparse
import hashlib
from pathlib import Path
import shutil
import subprocess
import tomllib

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--target', required=True, choices=[
    'x86_64-pc-windows-msvc', 'x86_64-unknown-linux-musl',
    'aarch64-unknown-linux-musl', 'aarch64-apple-darwin', 'x86_64-apple-darwin'])
options = parser.parse_args()
root = Path(__file__).resolve().parents[1]
version = tomllib.loads((root/'Cargo.toml').read_text())['package']['version']
suffix = '.exe' if 'windows' in options.target else ''
source = root / 'target' / options.target / 'release' / ('ssh-sessions' + suffix)
assert subprocess.check_output([str(source), '--version'], text=True).strip() == 'ssh-sessions ' + version
directory = root / 'dist'
directory.mkdir(exist_ok=True)
binary = directory / ('ssh-sessions-' + options.target + suffix)
shutil.copy2(source, binary)
digest = hashlib.sha256(binary.read_bytes()).hexdigest()
Path(str(binary) + '.sha256').write_text(digest + '  ' + binary.name + '\n', encoding='ascii')
(directory/'LICENSE.txt').write_text((root/'LICENSE').read_text() + '\n' +
                                  (root/'docs/licenses/UNICODE.txt').read_text(encoding='utf-8'), encoding='utf-8')
print(binary.name + '  ' + digest)
