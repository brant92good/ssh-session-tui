//! Explicit catalog-only Git synchronization. Nothing calls this during startup.
mod process;
use crate::catalog::{Catalog, absolute, decode, locked};
use anyhow::{Context, Result, ensure};
use std::{
    path::PathBuf,
    process::{Command, Stdio},
    time::Duration,
};

pub struct GitSync<'a> {
    catalog: &'a Catalog,
    root: PathBuf,
    relative: String,
    remote: String,
    remote_ref: String,
}
struct Output {
    code: i32,
    stdout: String,
    stderr: String,
}
fn invoke(root: &std::path::Path, args: &[&str]) -> Result<Output> {
    let mut command = Command::new("git");
    command
        .args(args)
        .current_dir(root)
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GCM_INTERACTIVE", "Never")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let (code, stdout, stderr) = process::run(command, Duration::from_secs(45))?;
    Ok(Output {
        code,
        stdout: String::from_utf8_lossy(&stdout).into_owned(),
        stderr: String::from_utf8_lossy(&stderr).into_owned(),
    })
}
impl<'a> GitSync<'a> {
    pub fn new(catalog: &'a Catalog) -> Result<Self> {
        let output = invoke(
            catalog
                .path
                .parent()
                .context("Catalog needs a parent directory")?,
            &["rev-parse", "--show-toplevel"],
        )?;
        ensure!(
            output.code == 0,
            "Place the catalog in a private Git repository before using sync."
        );
        let root = absolute(&PathBuf::from(output.stdout.trim()))?;
        let relative = catalog
            .path
            .strip_prefix(&root)
            .context("Catalog must be inside its Git repository.")?
            .to_string_lossy()
            .replace('\\', "/");
        let mut sync = Self {
            catalog,
            root,
            relative,
            remote: String::new(),
            remote_ref: String::new(),
        };
        sync.git(&["ls-files", "--error-unmatch", "--", &sync.relative])?;
        let branch = sync.git(&["symbolic-ref", "--quiet", "--short", "HEAD"])?;
        sync.remote = sync
            .git(&[
                "config",
                "--get",
                &format!("branch.{}.remote", branch.trim()),
            ])?
            .trim()
            .into();
        sync.remote_ref = sync
            .git(&[
                "config",
                "--get",
                &format!("branch.{}.merge", branch.trim()),
            ])?
            .trim()
            .into();
        ensure!(
            !sync.remote.is_empty()
                && sync.remote != "."
                && !sync.remote.starts_with('-')
                && sync.remote_ref.starts_with("refs/heads/"),
            "Configure a remote upstream branch before syncing."
        );
        Ok(sync)
    }
    fn git(&self, args: &[&str]) -> Result<String> {
        let output = invoke(&self.root, args)?;
        ensure!(
            output.code == 0,
            "Git: {}",
            if output.stderr.trim().is_empty() {
                output.stdout.trim()
            } else {
                output.stderr.trim()
            }
        );
        Ok(output.stdout)
    }
    fn validate_at(&self, revision: &str) -> Result<()> {
        let raw = self.git(&["show", &format!("{revision}:{}", self.relative)])?;
        decode(raw.as_bytes())?;
        Ok(())
    }
    fn validate_unpublished(&self) -> Result<()> {
        // Inspect every merge parent as well as ordinary commits. Plain git log
        // omits merge-only changes. NUL fields preserve exact Unicode/newline
        // filenames, and disabling rename detection checks both affected paths.
        let changed = self.git(&[
            "log",
            "-m",
            "--format=",
            "--name-only",
            "-z",
            "--no-renames",
            "@{upstream}..HEAD",
        ])?;
        ensure!(
            changed
                .split('\0')
                .all(|name| name.is_empty() || name == self.relative),
            "Unpublished commits include other files. Review and publish them outside this app first."
        );
        for revision in self.git(&["rev-list", "@{upstream}..HEAD"])?.lines() {
            self.validate_at(revision)?;
        }
        Ok(())
    }
    pub fn run(&self, action: &str) -> Result<String> {
        let _guard = locked(&self.catalog.lock_path)?;
        self.catalog.load()?;
        if action == "pull" {
            ensure!(
                self.git(&["status", "--porcelain"])?.trim().is_empty(),
                "Commit or move local changes before pulling."
            );
            self.git(&[
                "-c",
                "submodule.recurse=false",
                "pull",
                "--ff-only",
                &self.remote,
                &self.remote_ref,
            ])?;
            self.catalog.load()?;
            return Ok("Catalog updated from its remote.".into());
        }
        ensure!(action == "publish", "Choose pull or publish.");
        self.git(&["fetch", "--quiet", &self.remote, &self.remote_ref])?;
        let behind = self.git(&["rev-list", "--count", "HEAD..@{upstream}"])?;
        ensure!(
            behind.trim() == "0",
            "Remote changes are waiting. Pull before publishing."
        );
        self.validate_unpublished()?;
        self.git(&["add", "--", &self.relative])?;
        let difference = invoke(
            &self.root,
            &["diff", "--cached", "--quiet", "--", &self.relative],
        )?;
        ensure!(
            difference.code == 0 || difference.code == 1,
            "Could not inspect the staged catalog."
        );
        if difference.code == 1 {
            self.git(&[
                "commit",
                "--only",
                "-m",
                "Update SSH session catalog",
                "--",
                &self.relative,
            ])?;
        }
        self.validate_at("HEAD")?;
        // A repository hook can change history during commit; inspect the exact
        // history about to be pushed instead of trusting the pre-commit check.
        self.validate_unpublished()?;
        self.git(&["push", &self.remote, &format!("HEAD:{}", self.remote_ref)])?;
        Ok("Catalog published.".into())
    }
}
