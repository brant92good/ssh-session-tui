"""Only construct argv and run the existing client; no SSH config or key writes."""
import os
import shutil
import subprocess
import sys


def set_title(title):
    if os.name == 'nt':
        import ctypes
        setter = ctypes.windll.kernel32.SetConsoleTitleW
        setter.argtypes = [ctypes.c_wchar_p]
        setter(title)
    elif sys.stdout.isatty():
        safe = ''.join(c for c in title if c.isprintable())
        print('\033]0;' + safe + '\007', end='', flush=True)


def ssh_command(machine, route, executable=None, config=None):
    executable = executable or shutil.which('ssh.exe' if os.name == 'nt' else 'ssh')
    if not executable:
        raise ValueError('OpenSSH client is missing. Install it before connecting.')
    args = [executable, '-o', 'ConnectTimeout=10', '-o', 'ConnectionAttempts=1']
    if config is not None:
        args += ['-F', str(config)]
    if route.ssh_alias:
        args += ['-o', 'HostName=' + route.host]
    return [*args, '-p', str(route.port), '-l', machine.user, '--', route.ssh_alias or route.host]


def local_command():
    shell = shutil.which('pwsh.exe' if os.name == 'nt' else 'pwsh')
    if shell:
        return [shell, '-NoLogo']
    if os.name != 'nt':
        return [os.environ.get('SHELL', '/bin/sh')]
    raise ValueError('PowerShell 7 was not found. Install PowerShell 7 or open a local profile from Terminal.')


def run_session(command):
    # The picker has exited its alternate screen. The child owns the real TTY,
    # including host-key questions and local key-agent authentication.
    return subprocess.call(command)
