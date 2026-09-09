# Working on SSH Sessions

Read README.md and docs/design.md. This public leaf owns the machine picker,
metadata schema, device-only route preferences and explicit catalog sync.
Terminal Workspace owns Windows Terminal profiles and hotkeys. A user's private
settings repository may store the metadata file and pin the public parent.

Never put real machine details, private keys or credentials in this repository.
Examples and screenshots use demonstration addresses only. Catalog validation
allows only the documented metadata fields; device preferences live outside
Git. Never change SSH config, authorized_keys, known_hosts or key-agent state
as part of setup or sync. The existing SSH client owns authentication.

A connection failure must not automatically try another route or replace the
device's preferred route. Show alternatives and wait for the user's choice.
Do not claim a public-key fingerprint proves current server authorization.
SSH-config management, agent session policy and key authorization status are
deliberately deferred in docs/backlog.md.

Run `python -m unittest discover -s tests -v`. Desktop checks require opt-in and
small test-owned windows; never activate unrelated existing windows. Do not
run real remote commands through tests without authorization. Git sync tests
use isolated local repositories, not an account's real remote.

In this owner's agent session all Git/gh commands, including tests that run Git,
must execute outside the sandbox. Do not use GitHub MCP. Publish the leaf first,
then Terminal Workspace's submodule pin, then the private parent's pin.
