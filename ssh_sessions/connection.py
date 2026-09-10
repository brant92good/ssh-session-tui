"""Only construct argv and run the existing client; no SSH config or key writes."""
import os
import shutil
import signal
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
    # Ctrl+C belongs to the foreground shell. A custom parent handler survives
    # here but resets in an exec'd Unix child; SIG_IGN would also disable the
    # child's interrupt handling. Restore our caller's handler after the wait.
    previous = signal.signal(signal.SIGINT, lambda _signal, _frame: None)
    try:
        return subprocess.call(command)
    finally:
        signal.signal(signal.SIGINT, previous)


def clear_session_screen():
    """Clear the interactive primary display/scrollback, never history files."""
    if not sys.stdout.isatty():
        return
    # Textual has restored the primary screen by this point. Windows consoles
    # need VT processing for ED3 (saved lines); restore their original mode.
    restore = None
    if os.name == 'nt':
        import ctypes
        from ctypes import wintypes
        kernel = ctypes.WinDLL('kernel32', use_last_error=True)
        kernel.GetStdHandle.argtypes = [wintypes.DWORD]
        kernel.GetStdHandle.restype = wintypes.HANDLE
        kernel.GetConsoleMode.argtypes = [wintypes.HANDLE, ctypes.POINTER(wintypes.DWORD)]
        kernel.SetConsoleMode.argtypes = [wintypes.HANDLE, wintypes.DWORD]
        handle, mode = kernel.GetStdHandle(-11), wintypes.DWORD()
        if not kernel.GetConsoleMode(handle, ctypes.byref(mode)):
            raise ctypes.WinError(ctypes.get_last_error())
        if not kernel.SetConsoleMode(handle, mode.value | 0x0004):
            raise ctypes.WinError(ctypes.get_last_error())
        restore = lambda: kernel.SetConsoleMode(handle, mode.value)
    try:
        sys.stdout.write('\033[0m\033[2J\033[3J\033[H\033[?25h')
        sys.stdout.flush()
    finally:
        if restore is not None:
            restore()
