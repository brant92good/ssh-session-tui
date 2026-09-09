# Decisions reserved for a later design discussion

The current priority is a reliable TUI selector and correct private catalog.
These items are intentionally not implemented as part of that first version.

- **Local SSH configuration ownership.** Read-only preview/import is now implemented
  ([scope](ssh-import.md)). Generating, composing and modifying config files,
  richer conditional evaluation, and authoring ProxyCommand/ProxyJump settings
  still need the later design discussion.
- **Agent-heavy SSH use.** Agents may open more sessions than humans. Decide
  session identity, connection reuse, concurrency, approval boundaries, audit
  records and how agents consume local configuration before designing APIs
  that mutate that configuration.
- **Public-key inventory and authorization status.** Public keys/fingerprints
  can identify a device's intended key. They do not establish whether a remote
  server still accepts that key. Discuss consent for probes, the meaning and
  age of last-verified results, multiple keys, agent-loaded keys, revocation and
  unknown states. Private keys and credentials must remain local.
- **Catalog interoperability with Port Forward TUI and Herdr.** Decide a shared
  stable machine identity and route mapping before merging their catalogs or
  changing the existing paired workspace's return-shortcut context.
- **Linux and macOS qualification.** Version 0.5 adds installers and CI for all
  three operating systems, including a Unix local-shell pseudo-terminal test.
  Real remote login in macOS/Linux terminal applications remains to be checked.
  The cross-project [platform plan](https://github.com/brant92good/terminal-workspace/blob/main/docs/platforms.md)
  keeps global/tab shortcuts in the terminal integration layer. Nested groups,
  tags and bulk organization are implemented in 0.4; inherited SSH settings
  remain outside the current grouping feature.
- **Further Termius-like features.** SFTP, snippets, agent automation, jump-host
  authoring, background sync, key enrollment and a new SSH implementation are
  outside the initial picker. No automatic fallback routes are planned without
  an explicit future change in the user's preference.

Bring these questions to the requested grilling/documentation discussion. The
first version uses the installed SSH client and existing local configuration.
