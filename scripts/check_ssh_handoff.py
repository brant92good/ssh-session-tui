"""Disposable Linux CI check: native TUI -> loopback OpenSSH -> TUI.

Creates fresh client/host keys, an isolated sshd and catalog; never reads the
owner's SSH files. Run on an isolated CI runner with openssh-server installed.
"""
import argparse
import getpass
import json
import os
from pathlib import Path
import select
import signal
import socket
import subprocess
import tempfile
import time
import uuid


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', type=Path, required=True)
    parser.add_argument('--sudo-sshd', action='store_true', help='Disposable Linux CI runner only')
    options = parser.parse_args()
    if os.name != 'posix':
        parser.error('This loopback sshd fixture is for Linux CI')
    import fcntl
    import pty
    import struct
    import termios
    binary = options.binary.resolve(strict=True)
    with tempfile.TemporaryDirectory(prefix='native-ssh-loopback-') as temporary:
        root = Path(temporary)
        for name in ('client', 'host'):
            subprocess.run(['ssh-keygen', '-q', '-t', 'ed25519', '-N', '', '-f', str(root/name)], check=True)
        with socket.socket() as probe:
            probe.bind(('127.0.0.1', 0))
            port = probe.getsockname()[1]
        known = root/'known_hosts'
        known.write_text(f'[127.0.0.1]:{port} ' + (root/'host.pub').read_text())
        server_config = root/'sshd_config'
        server_config.write_text(f'''Port {port}
ListenAddress 127.0.0.1
HostKey {root/'host'}
PidFile {root/'sshd.pid'}
AuthorizedKeysFile {root/'client.pub'}
StrictModes no
UsePAM yes
PasswordAuthentication no
KbdInteractiveAuthentication no
PermitRootLogin prohibit-password
PrintMotd no
LogLevel VERBOSE
ForceCommand /bin/sh -c 'printf "ssh-loopback-ready\\n"; exec /bin/sh -i'
''')
        config = root/'ssh_config'
        config.write_text(f'''Host loopback-test
 HostName 127.0.0.1
 Port {port}
 User {getpass.getuser()}
 IdentityFile {root/'client'}
 IdentitiesOnly yes
 UserKnownHostsFile {known}
 StrictHostKeyChecking yes
 BatchMode yes
''')
        prefix = ['sudo', '-n'] if options.sudo_sshd else []
        with (root/'sshd.log').open('wb') as log:
            server = subprocess.Popen([*prefix, '/usr/sbin/sshd', '-D', '-e', '-f', str(server_config)], stdout=log, stderr=log)
        child = terminal = None
        reaped = False
        try:
            deadline = time.monotonic() + 10
            while True:
                if server.poll() is not None:
                    raise AssertionError((root/'sshd.log').read_text())
                try:
                    with socket.create_connection(('127.0.0.1', port), timeout=.2):
                        break
                except OSError:
                    assert time.monotonic() < deadline, 'Owned sshd did not start'
                    time.sleep(.05)
            args = [str(binary), '--catalog', str(root/'catalog.json'), '--state-dir', str(root/'device')]
            # Hosted runner accounts can have locked passwords. PAM account
            # validation supports public-key login without unlocking that account.
            # Qualify the SSH fixture before attributing a failure to the TUI.
            preflight = subprocess.run(['ssh', '-F', str(config), 'loopback-test'], input='exit\n',
                                       capture_output=True, text=True, timeout=15)
            assert preflight.returncode == 0 and 'ssh-loopback-ready' in preflight.stdout, (
                preflight.stdout, preflight.stderr, (root/'sshd.log').read_text())
            def command(*tail):
                result = subprocess.run([*args, *tail, '--json'], capture_output=True, text=True, timeout=10)
                assert result.returncode == 0, (result.stdout, result.stderr)
                return json.loads(result.stdout)
            command('import-ssh', '--config', str(config), '--apply', '--host', 'loopback-test')
            machine = command('list')['machines'][0]
            command('favorites', 'set', '1', '--machine', machine['id'])
            child, terminal = pty.fork()
            if child == 0:
                environment = dict(os.environ, TERM='xterm-256color')
                os.execve(binary, args, environment)
            fcntl.ioctl(terminal, termios.TIOCSWINSZ, struct.pack('HHHH', 26, 100, 0, 0))
            received = bytearray()
            def expect(text):
                needle = text.encode()
                deadline = time.monotonic() + 15
                while needle not in received:
                    assert time.monotonic() < deadline, (text, received[-2000:].decode(errors='replace'), (root/'sshd.log').read_text())
                    if select.select([terminal], [], [], .1)[0]:
                        received.extend(os.read(terminal, 65536))
                del received[:received.index(needle)+len(needle)]
            def marker():
                marker = 'remote-' + uuid.uuid4().hex
                escaped = ''.join('\\%03o' % ord(c) for c in marker)
                os.write(terminal, ("printf '" + escaped + "\\n'\n").encode())
                expect(marker)
            expect('Local terminal')
            os.write(terminal, b'1\r')
            expect('ssh-loopback-ready')
            marker()
            os.write(terminal, b'sleep 30\n')
            time.sleep(.3)
            os.write(terminal, b'\x03')
            time.sleep(.3)
            marker()
            fcntl.ioctl(terminal, termios.TIOCSWINSZ, struct.pack('HHHH', 30, 110, 0, 0))
            os.write(terminal, b'exit\n')
            expect('SSH session ended (exit 0)')
            os.write(terminal, b'q')
            deadline = time.monotonic() + 8
            while time.monotonic() < deadline:
                waited, status = os.waitpid(child, os.WNOHANG)
                if waited:
                    reaped = True
                    assert os.waitstatus_to_exitcode(status) == 0
                    break
                if select.select([terminal], [], [], .05)[0]:
                    try:
                        os.read(terminal, 65536)
                    except OSError:
                        pass
            assert reaped, 'Picker did not exit after Q'
            print('PASS: native picker, isolated OpenSSH alias/config/key, remote shell commands, Ctrl+C, resize, SSH logout and picker exit0.')
        finally:
            if child and not reaped:
                try:
                    if os.getpgid(child) == child and child != os.getpgrp():
                        os.killpg(child, signal.SIGKILL)
                except ProcessLookupError:
                    pass
            if terminal is not None:
                os.close(terminal)
            if child and not reaped:
                os.waitpid(child, 0)
            server.terminate()
            try:
                server.wait(timeout=5)
            except subprocess.TimeoutExpired:
                server.kill()
                server.wait(timeout=5)


if __name__ == '__main__':
    main()
