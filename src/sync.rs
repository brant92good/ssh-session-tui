//! Explicit synchronization of the catalog and its saved-path sidecar only.
mod process;
use crate::catalog::{Catalog, absolute, decode, locked};
use crate::file_presets::{self, FilePresets};
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
    presets_relative: String,
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
        .arg("--literal-pathspecs")
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
        stdout: String::from_utf8(stdout)
            .context("Git returned non-UTF-8 metadata or filenames.")?,
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
            .to_str()
            .context("Catalog path must be valid UTF-8 for synchronization.")?
            .replace('\\', "/");
        let presets_relative = format!("{relative}.files.json");
        let mut sync = Self {
            catalog,
            root,
            relative,
            presets_relative,
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
    fn blob_at(&self, revision: &str, path: &str, optional: bool) -> Result<Option<String>> {
        let tree = self.git(&["ls-tree", "-z", revision, "--", path])?;
        if tree.is_empty() {
            ensure!(optional, "The incoming revision has no catalog.");
            return Ok(None);
        }
        let entries: Vec<_> = tree.split('\0').filter(|entry| !entry.is_empty()).collect();
        ensure!(
            entries.len() == 1,
            "Metadata must name exactly one Git file."
        );
        let (header, name) = entries[0]
            .split_once('\t')
            .context("Invalid Git tree entry.")?;
        let fields: Vec<_> = header.split_whitespace().collect();
        ensure!(
            name == path
                && fields.len() == 3
                && matches!(fields[0], "100644" | "100755")
                && fields[1] == "blob",
            "Synced metadata must be a regular file, not a symlink or directory."
        );
        if path == self.presets_relative {
            let size: usize = self.git(&["cat-file", "-s", fields[2]])?.trim().parse()?;
            ensure!(
                size <= file_presets::MAX_BYTES,
                "Saved paths exceed the 128 KiB limit."
            );
        }
        Ok(Some(self.git(&["cat-file", "blob", fields[2]])?))
    }
    fn validate_at(&self, revision: &str) -> Result<()> {
        decode(
            self.blob_at(revision, &self.relative, false)?
                .context("Catalog missing.")?
                .as_bytes(),
        )?;
        if let Some(raw) = self.blob_at(revision, &self.presets_relative, true)? {
            FilePresets::decode(raw.as_bytes())?;
        }
        Ok(())
    }
    fn validate_unpublished(&self, base: &str) -> Result<()> {
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
            &format!("{base}..HEAD"),
        ])?;
        ensure!(
            changed.split('\0').all(|name| name.is_empty()
                || name == self.relative
                || name == self.presets_relative),
            "Unpublished commits include other files. Review and publish them outside this app first."
        );
        for revision in self.git(&["rev-list", &format!("{base}..HEAD")])?.lines() {
            self.validate_at(revision)?;
        }
        Ok(())
    }
    pub fn run(&self, action: &str) -> Result<String> {
        let _guard = locked(&self.catalog.lock_path)?;
        file_presets::regular_file(&self.catalog.path, false)?;
        self.catalog.load()?;
        FilePresets::load(self.catalog)?;
        if action == "pull" {
            ensure!(
                self.git(&["status", "--porcelain"])?.trim().is_empty(),
                "Commit or move local changes before pulling."
            );
            self.git(&["fetch", "--quiet", &self.remote, &self.remote_ref])?;
            let target = self.git(&["rev-parse", "--verify", "FETCH_HEAD^{commit}"])?;
            let target = target.trim();
            // Validate both incoming files before changing HEAD or the checkout.
            self.validate_at(target)?;
            self.git(&[
                "-c",
                "submodule.recurse=false",
                "merge",
                "--ff-only",
                target,
            ])?;
            self.catalog.load()?;
            FilePresets::load(self.catalog)?;
            return Ok("Catalog and saved paths updated from their remote.".into());
        }
        ensure!(action == "publish", "Choose pull or publish.");
        self.git(&["fetch", "--quiet", &self.remote, &self.remote_ref])?;
        let target = self.git(&["rev-parse", "--verify", "FETCH_HEAD^{commit}"])?;
        let target = target.trim();
        self.validate_at(target)?;
        let behind = self.git(&["rev-list", "--count", &format!("HEAD..{target}")])?;
        ensure!(
            behind.trim() == "0",
            "Remote changes are waiting. Pull before publishing."
        );
        self.validate_unpublished(target)?;
        let mut paths = vec![self.relative.as_str()];
        // An optional sidecar may never have existed, or may be deleted in the
        // worktree/index. Avoid an unmatched pathspec without losing deletions.
        let tracked = self.git(&["ls-files", "-z", "--", &self.presets_relative])?;
        let in_head = !self
            .git(&["ls-tree", "-z", "HEAD", "--", &self.presets_relative])?
            .is_empty();
        self.git(&["add", "--", &self.relative])?;
        if file_presets::regular_file(&file_presets::sidecar_path(self.catalog), true)? {
            self.git(&["add", "--", &self.presets_relative])?;
            paths.push(&self.presets_relative);
        } else if in_head {
            // commit --only needs a tracked index entry to match a deleted
            // worktree path. Restore just that owned index entry, then let the
            // partial commit record its worktree deletion. Other staging stays.
            self.git(&["reset", "--quiet", "HEAD", "--", &self.presets_relative])?;
            paths.push(&self.presets_relative);
        } else if !tracked.is_empty() {
            // A never-committed addition was subsequently removed locally.
            self.git(&["add", "-A", "--", &self.presets_relative])?;
        }
        let mut diff = vec!["diff", "--quiet", "HEAD", "--"];
        diff.extend(&paths);
        let difference = invoke(&self.root, &diff)?;
        ensure!(
            difference.code == 0 || difference.code == 1,
            "Could not inspect the staged catalog."
        );
        if difference.code == 1 {
            let mut commit = vec![
                "commit",
                "--only",
                "-m",
                "Update SSH catalog and saved paths",
                "--",
            ];
            commit.extend(&paths);
            self.git(&commit)?;
        }
        self.validate_at("HEAD")?;
        // A repository hook can change history during commit; inspect the exact
        // history about to be pushed instead of trusting the pre-commit check.
        self.validate_unpublished(target)?;
        self.git(&["push", &self.remote, &format!("HEAD:{}", self.remote_ref)])?;
        Ok("Catalog and saved paths published.".into())
    }
}
