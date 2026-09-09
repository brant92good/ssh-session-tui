"""A real Unix terminal, owned by the test; no desktop windows or remote hosts."""
import os
from pathlib import Path
import select
import signal
import subprocess
import sys
import tempfile
import time
import unittest
import uuid


@unittest.skipIf(os.name == 'nt', 'Unix pseudo-terminal check')
class UnixTerminalTests(unittest.TestCase):
    def test_local_shell_ctrl_c_resize_and_return_to_picker(self):
        # Start a fresh interpreter before forking the PTY. The full suite has
        # already created UI threads; forking that process is unsafe on macOS.
        if os.environ.get('SSH_SESSIONS_PTY_CHILD') != '1':
            try:
                result = subprocess.run([sys.executable, str(Path(__file__).resolve())],
                    env=dict(os.environ, SSH_SESSIONS_PTY_CHILD='1'),
                    stdout=subprocess.PIPE, stderr=subprocess.STDOUT, timeout=45)
            except subprocess.TimeoutExpired as error:
                self.fail('PTY harness timed out:\n' + (error.stdout or b'').decode(errors='replace'))
            self.assertEqual(result.returncode, 0, result.stdout.decode(errors='replace'))
            return
        import fcntl
        import faulthandler
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
                os.environ.update(TERM='xterm-256color', SHELL='/bin/sh', PS1='ssh-test-ready> ')
                os.environ.pop('ENV', None)
                os.execv(sys.executable, [sys.executable, '-E', '-s', str(root/'app.py'),
                    '--catalog', str(catalog.path), '--state-dir', str(catalog.state_dir)])
            received = bytearray()
            reaped = False
            os.set_blocking(terminal, False)
            faulthandler.dump_traceback_later(15, repeat=True)
            def expect(text, timeout=10):
                deadline = time.monotonic() + timeout
                while text.encode() not in received and time.monotonic() < deadline:
                    if select.select([terminal], [], [], .1)[0]:
                        try:
                            received.extend(os.read(terminal, 65536))
                        except BlockingIOError:
                            pass
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
                expect('ssh-test-ready> ')
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
                expect('Local shell ended (exit 0)')
                os.write(terminal, b'q')
                deadline = time.monotonic() + 5
                while time.monotonic() < deadline:
                    waited, status = os.waitpid(pid, os.WNOHANG)
                    if waited:
                        reaped = True
                        self.assertEqual(os.waitstatus_to_exitcode(status), 0)
                        return
                    # Terminal drivers can still be flushing their final screen.
                    # Drain output while waiting, including on macOS PTYs.
                    if select.select([terminal], [], [], .05)[0]:
                        try:
                            received.extend(os.read(terminal, 65536))
                        except (BlockingIOError, OSError):
                            pass
                    time.sleep(.05)
                self.fail('Picker did not close after Q')
            finally:
                if not reaped:
                    try:
                        # Only signal the still-owned child's private group.
                        if os.getpgid(pid) == pid and pid != os.getpgrp():
                            os.killpg(pid, signal.SIGKILL)
                        os.kill(pid, signal.SIGKILL)
                    except ProcessLookupError:
                        pass
                os.close(terminal)
                # Close the PTY before waiting: macOS can hold a dying child's
                # terminal close until the unread master side is released.
                deadline = time.monotonic() + 3
                while not reaped and time.monotonic() < deadline:
                    try:
                        waited, _ = os.waitpid(pid, os.WNOHANG)
                        reaped = bool(waited)
                    except ChildProcessError:
                        break
                    time.sleep(.05)
                faulthandler.cancel_dump_traceback_later()


if __name__ == '__main__':
    sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
    unittest.main()
