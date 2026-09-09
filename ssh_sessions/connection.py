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
    if os.name != 'nt':
        shell = os.environ.get('SHELL') or '/bin/sh'
        executable = shutil.which(shell)
        if not executable:
            raise ValueError(f'Your configured shell was not found: {shell}. Check SHELL and try again.')
        return [executable]
    for name, arguments in (('pwsh.exe', ['-NoLogo']), ('powershell.exe', ['-NoLogo']), ('cmd.exe', [])):
        shell = shutil.which(name)
        if shell:
            return [shell, *arguments]
    raise ValueError('No local shell was found. Check that PowerShell or cmd is on PATH.')


def local_shell_name():
    try:
        return os.path.basename(local_command()[0]).removesuffix('.exe')
    except ValueError:
        return 'Local shell'


def run_session(command):
    # The picker has exited its alternate screen. The child owns the real TTY,
    # including host-key questions and local key-agent authentication.
    return subprocess.call(command)
