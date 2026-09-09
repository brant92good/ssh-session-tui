"""Explicit Git sync of one tracked metadata file; never stage keys or settings."""
import os
from pathlib import Path
import subprocess

from .catalog import CatalogError, decode, locked


class GitSync:
    def __init__(self, catalog):
        self.catalog = catalog
        self.root = catalog.path.parent
        self.root = Path(self.git('rev-parse', '--show-toplevel').stdout.strip()).resolve()
        if not catalog.path.is_relative_to(self.root):
            raise CatalogError('Catalog must be inside its Git working tree.')
        self.relative = catalog.path.relative_to(self.root).as_posix()
        self.git('ls-files', '--error-unmatch', '--', self.relative)
        self.branch = self.git('symbolic-ref', '--short', 'HEAD').stdout.strip()
        self.remote = self.git('config', '--get', f'branch.{self.branch}.remote').stdout.strip()
        self.remote_ref = self.git('config', '--get', f'branch.{self.branch}.merge').stdout.strip()
        if not self.remote or self.remote == '.' or not self.remote_ref.startswith('refs/heads/'):
            raise CatalogError('Use a branch with an upstream remote before syncing.')

    def git(self, *args, allowed=(0,)):
        env = dict(os.environ, GIT_TERMINAL_PROMPT='0', GCM_INTERACTIVE='Never')
        try:
            result = subprocess.run(['git', '-C', str(self.root), *args], capture_output=True,
                text=True, encoding='utf-8', errors='replace', timeout=45, env=env,
                creationflags=getattr(subprocess, 'CREATE_NO_WINDOW', 0))
        except (OSError, subprocess.TimeoutExpired) as error:
            raise CatalogError('Git did not finish. Check Git installation, network and account sign-in.') from error
        if result.returncode not in allowed:
            message = (result.stderr or result.stdout).strip()[-800:]
            raise CatalogError('Git sync failed: ' + message)
        return result

    def run(self, action):
        if action not in ('pull', 'publish'):
            raise CatalogError('Choose Pull or Publish.')
        with locked(self.catalog.lock_path):
            self.catalog.load()  # Reject invalid or credential-bearing metadata.
            if action == 'pull':
                if self.git('status', '--porcelain').stdout.strip():
                    raise CatalogError('The repository has local changes. Publish catalog edits, or commit/stash other work before Pull.')
                self.git('-c', 'submodule.recurse=false', 'pull', '--ff-only', self.remote, self.remote_ref)
                self.catalog.load()
                return 'Repository updated.'
            self.git('fetch', '--quiet', self.remote, self.remote_ref)
            behind, ahead = map(int, self.git('rev-list', '--left-right', '--count', '@{upstream}...HEAD').stdout.split())
            if behind:
                raise CatalogError('The remote has newer commits. Reconcile local edits with Git and Pull first.')
            if ahead:
                # Inspect history, not only the net diff: a reverted file or
                # credential-bearing catalog would still travel in a push.
                changed = self.git('log', '--format=', '--name-only', '@{upstream}..HEAD').stdout.splitlines()
                if any(name and name != self.relative for name in changed):
                    raise CatalogError('The repo has unpublished work outside this catalog. Publish that work with Git first.')
                for commit in self.git('rev-list', '@{upstream}..HEAD').stdout.splitlines():
                    decode(self.git('show', commit + ':' + self.relative).stdout.encode('utf-8'))
            self.git('add', '--', self.relative)
            changed = self.git('diff', '--cached', '--quiet', '--', self.relative, allowed=(0, 1)).returncode
            if changed:
                self.git('commit', '--only', '-m', 'Update SSH session catalog', '--', self.relative)
            # Validate exactly what will be shared, not only the working copy.
            decode(self.git('show', 'HEAD:' + self.relative).stdout.encode('utf-8'))
            self.git('push', self.remote, 'HEAD:' + self.remote_ref)
            return 'Catalog published.'
