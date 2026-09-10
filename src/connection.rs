use crate::catalog::{Machine, Route};
use anyhow::{Context, Result};
use std::{
    ffi::{OsStr, OsString},
    io::{self, IsTerminal},
    path::{Path, PathBuf},
    process::Command,
};

pub fn home() -> PathBuf {
    dirs::home_dir().unwrap_or_else(|| PathBuf::from("."))
}
pub fn default_directory() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(|| home().join(".local/share"))
        .join("SSHSessions")
}
pub fn ssh_command(
    machine: &Machine,
    route: &Route,
    config: Option<&Path>,
) -> Result<Vec<OsString>> {
    let ssh = which::which(if cfg!(windows) { "ssh.exe" } else { "ssh" })
        .context("OpenSSH was not found. Install the SSH client and try again.")?;
    let mut args = vec![
        ssh.into_os_string(),
        "-o".into(),
        "ConnectTimeout=10".into(),
        "-o".into(),
        "ConnectionAttempts=1".into(),
    ];
    if let Some(config) = config {
        args.extend(["-F".into(), config.as_os_str().into()]);
    }
    if route.ssh_alias.is_some() {
        args.extend(["-o".into(), format!("HostName={}", route.host).into()]);
    }
    args.extend([
        "-p".into(),
        route.port.to_string().into(),
        "-l".into(),
        machine.user.clone().into(),
        "--".into(),
        route.ssh_alias.as_ref().unwrap_or(&route.host).into(),
    ]);
    Ok(args)
}
pub fn local_command() -> Result<Vec<OsString>> {
    #[cfg(windows)]
    {
        for name in ["pwsh.exe", "powershell.exe", "cmd.exe"] {
            if let Ok(path) = which::which(name) {
                let mut args = vec![path.into_os_string()];
                if name != "cmd.exe" {
                    args.push("-NoLogo".into());
                }
                return Ok(args);
            }
        }
        anyhow::bail!("No local shell was found.");
    }
    #[cfg(not(windows))]
    {
        let shell = std::env::var_os("SHELL").unwrap_or_else(|| "/bin/sh".into());
        Ok(vec![
            which::which(shell)
                .context("The local SHELL executable was not found.")?
                .into_os_string(),
        ])
    }
}
pub fn local_shell_name() -> String {
    local_command()
        .ok()
        .and_then(|args| {
            Path::new(&args[0])
                .file_stem()
                .map(|s| s.to_string_lossy().into_owned())
        })
        .unwrap_or_else(|| "shell".into())
}
pub fn set_title(title: &str) {
    let title: String = title.chars().filter(|c| !c.is_control()).collect();
    #[cfg(windows)]
    {
        let wide: Vec<_> = title.encode_utf16().chain(Some(0)).collect();
        unsafe {
            windows_sys::Win32::System::Console::SetConsoleTitleW(wide.as_ptr());
        }
    }
    #[cfg(not(windows))]
    {
        use std::io::Write;
        let _ = write!(std::io::stdout(), "\x1b]0;{title}\x07");
        let _ = std::io::stdout().flush();
    }
}
pub fn run_session(args: &[OsString]) -> Result<i32> {
    let (program, args) = args.split_first().context("Missing session executable")?;
    // The picker must release raw mode and the alternate screen before this handoff.
    // Parent and child share the foreground process group; handle Ctrl+C in the parent
    // so it cannot terminate the picker while the child shell is running.
    let status = Command::new(program).args(args).status()?;
    Ok(status.code().unwrap_or(130))
}

/// The picker uses the alternate screen; shells use the primary screen. Clear
/// that primary screen only after the picker has released its terminal guard.
/// This changes terminal pixels/scrollback, never a shell's command history.
pub fn clear_session_screen() -> Result<()> {
    if io::stdout().is_terminal() {
        crossterm::execute!(
            io::stdout(),
            crossterm::style::SetAttribute(crossterm::style::Attribute::Reset),
            crossterm::terminal::Clear(crossterm::terminal::ClearType::All),
            crossterm::terminal::Clear(crossterm::terminal::ClearType::Purge),
            crossterm::cursor::MoveTo(0, 0),
            crossterm::cursor::Show
        )?;
    }
    Ok(())
}
pub fn display_command(args: &[OsString]) -> String {
    args.iter()
        .map(|arg| quote(arg))
        .collect::<Vec<_>>()
        .join(" ")
}
fn quote(arg: &OsStr) -> String {
    let arg = arg.to_string_lossy();
    #[cfg(not(windows))]
    {
        shlex::try_quote(&arg)
            .map(|s| s.into_owned())
            .unwrap_or_else(|_| "<invalid argument>".into())
    }
    #[cfg(windows)]
    {
        if !arg.is_empty() && !arg.chars().any(|c| c.is_whitespace() || c == '"') {
            return arg.into();
        }
        let mut result = String::from("\"");
        let mut slashes = 0;
        for c in arg.chars() {
            if c == '\\' {
                slashes += 1;
            } else {
                result.push_str(&"\\".repeat(if c == '"' { slashes * 2 + 1 } else { slashes }));
                result.push(c);
                slashes = 0;
            }
        }
        result.push_str(&"\\".repeat(slashes * 2));
        result.push('"');
        result
    }
}
