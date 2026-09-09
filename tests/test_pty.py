"""A real Unix terminal, owned by the test; no desktop windows or remote hosts."""
import os
from pathlib import Path
import select
import signal
import sys
import tempfile
import time
import unittest
import uuid


@unittest.skipIf(os.name == 'nt', 'Unix pseudo-terminal check')
class UnixTerminalTests(unittest.TestCase):
    def test_local_shell_ctrl_c_resize_and_return_to_picker(self):
        import fcntl
        import pty
        import struct
        import termios
        from ssh_sessions.catalog import Catalog
        from ssh_sessions.favorites import Favorites, LOCAL
        root = Path(__file__).resolve().parents[1]
        with tempfile.TemporaryDirectory(prefix='ssh-pty-') as folder:
            catalog = Catalog(Path(folder) / 'catalog.json', Path(folder) / 'device')
            catalog.save((), catalog.load().revision)
            Favorites(catalog).seed({'2': LOCAL})
            pid, terminal = pty.fork()
            if pid == 0:
                os.environ.update(TERM='xterm-256color', SHELL='/bin/sh')
                os.environ.pop('ENV', None)
                os.execv(sys.executable, [sys.executable, '-E', '-s', str(root/'app.py'),
                    '--catalog', str(catalog.path), '--state-dir', str(catalog.state_dir)])
            received = bytearray()
            def expect(text, timeout=10):
                deadline = time.monotonic() + timeout
                while text.encode() not in received and time.monotonic() < deadline:
                    if select.select([terminal], [], [], .1)[0]:
                        received.extend(os.read(terminal, 65536))
                self.assertIn(text.encode(), received, received[-1500:].decode(errors='replace'))
                del received[:received.index(text.encode()) + len(text.encode())]
            def marker():
                value = 'shell-' + uuid.uuid4().hex
                encoded = ''.join('\\%03o' % ord(c) for c in value)
                os.write(terminal, ("printf '" + encoded + "\\n'\n").encode())
                expect(value)
            try:
                fcntl.ioctl(terminal, termios.TIOCSWINSZ, struct.pack('HHHH', 24, 90, 0, 0))
                expect('Local terminal')
                os.write(terminal, b'2\r')
                expect('# ')
                marker()
                os.write(terminal, b'sleep 30\n')
                time.sleep(.2)
                os.write(terminal, b'\x03')
                time.sleep(.2)
                marker()
                fcntl.ioctl(terminal, termios.TIOCSWINSZ, struct.pack('HHHH', 30, 100, 0, 0))
                os.write(terminal, b'exit\n')
                expect('SSH Sessions')
                expect('Local terminal')
                os.write(terminal, b'q')
                deadline = time.monotonic() + 5
                while time.monotonic() < deadline:
                    waited, status = os.waitpid(pid, os.WNOHANG)
                    if waited:
                        self.assertEqual(os.waitstatus_to_exitcode(status), 0)
                        return
                    time.sleep(.05)
                self.fail('Picker did not close after Q')
            finally:
                # pty.fork creates this child's own session/process group.
                try:
                    os.killpg(pid, signal.SIGKILL)
                except ProcessLookupError:
                    pass
                try:
                    os.waitpid(pid, 0)
                except ChildProcessError:
                    pass
                os.close(terminal)
