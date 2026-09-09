param(
    [string]$InstallDir = '',
    [string]$Package = 'https://github.com/brant92good/ssh-session-tui/archive/refs/tags/v0.5.0.zip',
    [switch]$NoPath
)
$ErrorActionPreference = 'Stop'
# A Windows PowerShell child launched through Python can inherit PowerShell 7's
# module search order. Load this host's own module before uv checks policy.
Import-Module (Join-Path $PSHOME 'Modules\Microsoft.PowerShell.Security\Microsoft.PowerShell.Security.psd1')
if (-not $InstallDir) { $InstallDir = Join-Path $env:LOCALAPPDATA 'Programs\SSHSessions' }
$InstallDir = [IO.Path]::GetFullPath($InstallDir)
$marker = Join-Path $InstallDir '.ssh-sessions-installer'
if ((Test-Path -LiteralPath $InstallDir) -and -not (Test-Path -LiteralPath $marker) -and
    @(Get-ChildItem -LiteralPath $InstallDir -Force).Count) {
    throw "Choose an empty install directory: $InstallDir already contains other files."
}
New-Item -ItemType Directory -Path $InstallDir -Force | Out-Null
[IO.File]::WriteAllText($marker, 'ssh-session-tui')
$uvSettings = @{
    UV_UNMANAGED_INSTALL = (Join-Path $InstallDir 'uv')
    UV_TOOL_DIR = (Join-Path $InstallDir 'tools')
    UV_TOOL_BIN_DIR = (Join-Path $InstallDir 'tool-bin')
    UV_PYTHON_INSTALL_DIR = (Join-Path $InstallDir 'python')
    UV_PYTHON_INSTALL_BIN = '0'; UV_PYTHON_INSTALL_REGISTRY = '0'; UV_NO_CONFIG = '1'
    UV_CONCURRENT_DOWNLOADS = '2'; UV_CONCURRENT_BUILDS = '1'; UV_CONCURRENT_INSTALLS = '2'
}
$previous = @{}
try {
    foreach ($key in $uvSettings.Keys) {
        $previous[$key] = [Environment]::GetEnvironmentVariable($key, 'Process')
        [Environment]::SetEnvironmentVariable($key, $uvSettings[$key], 'Process')
    }
    $uvApp = Join-Path $InstallDir 'uv\uv.exe'
    if (-not (Test-Path -LiteralPath $uvApp)) {
        Write-Output 'Preparing the installer...'
        [Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12
        $download = Invoke-RestMethod 'https://astral.sh/uv/0.10.10/install.ps1'
        & ([scriptblock]::Create($download))
        if (-not (Test-Path -LiteralPath $uvApp)) { throw 'Could not prepare uv. Check the download error above.' }
    }
    Write-Output 'Installing SSH Sessions and its Python runtime...'
    & $uvApp --no-config --quiet tool install --managed-python --python 3.12 --reinstall --force $Package
    if ($LASTEXITCODE -ne 0) { throw 'Installation failed. Check the error above and run this command again.' }
    $appPython = Join-Path $InstallDir 'tools\ssh-session-tui\Scripts\python.exe'
    & $appPython -E -s -m ssh_sessions.install_support $InstallDir
    if ($LASTEXITCODE -ne 0) { throw 'Could not create the app command.' }
    $env:UV_TOOL_BIN_DIR = Join-Path $InstallDir 'bin'
    if (-not $NoPath) {
        & $uvApp --no-config tool update-shell
        if ($LASTEXITCODE -ne 0) { throw 'The app installed, but PATH could not be updated. Use the bin command in the install directory.' }
        if ($env:PATH.Split(';') -notcontains $env:UV_TOOL_BIN_DIR) { $env:PATH += ';' + $env:UV_TOOL_BIN_DIR }
    }
    & $appPython -E -s -m ssh_sessions --version
    if ($LASTEXITCODE -ne 0) { throw 'The installed app did not start.' }
    Write-Output 'Ready. Run ssh-sessions. Press A to add a machine, or I to import SSH hosts.'
    Write-Output ('Command: ' + (Join-Path $InstallDir 'bin\ssh-sessions.cmd'))
} finally {
    foreach ($key in $previous.Keys) { [Environment]::SetEnvironmentVariable($key, $previous[$key], 'Process') }
}
