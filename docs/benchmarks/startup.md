# CLI startup: Python to Rust

On September 10, 2026, the compiled SSH Sessions CLI returned an equivalent
`list --json` response in **17.56 ms median**, compared with **80.82 ms** for
the legacy Python implementation: 63.26 ms less, or 4.60 times faster in this run.

| Runtime | Median | 95th percentile | Range |
| --- | ---: | ---: | ---: |
| Python 3.12.11 | 80.82 ms | 92.72 ms | 77.38–94.87 ms |
| Rust 0.6.0, release, static Windows CRT | 17.56 ms | 22.99 ms | 16.06–24.56 ms |

The harness starts a fresh process for each sample and verifies equal JSON
output. Both use the same temporary single-machine catalog and device directory.
There are three warmups per runtime, followed by 30 measurements each in a
seeded, interleaved order. The OS file cache stays warm. The Python command uses
`-E -s` to ignore inherited Python settings and user site packages. No SSH
connection, shell profile or desktop window is opened.

This was one run on a shared Windows 11 x64 workstation, build 26200, while
other background work was possible. It is not a promise about another computer,
a cold disk cache, the picker's first frame, Windows Terminal tab focus, or SSH
login. Those need separate measurements. Antivirus scanning, CPU scheduling,
storage and catalog size can change process startup. Shell profiles, Conda
initialization and terminal startup can add time before this app even starts;
DNS, routing and authentication affect the later SSH connection.

The improvement removes Python interpreter and import startup from the normal
command. It does not show that every Python operation is slow or that rewriting
network-bound work in Rust makes the network faster.

The native binary was built from `79686f9008fde08a4e54a340a4e4baa4eaf0ee80`
using Rust 1.94.0. Its SHA-256 and all observations are in
[the raw report](startup-windows-2026-09-10.json).

To reproduce from a developer checkout with the legacy Python dependencies:

```powershell
$env:RUSTFLAGS = '-C target-feature=+crt-static'
cargo build --release --locked --target x86_64-pc-windows-msvc
python scripts/benchmark_startup.py --binary target/x86_64-pc-windows-msvc/release/ssh-sessions.exe --output artifacts/startup-windows.json
```

Python is the measurement harness and old-runtime reference; the installed app
does not require it. Run without another benchmark competing for the machine.
