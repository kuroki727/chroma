# Validation record — 2026-11-16

Validated locally on Linux with rustc/cargo 1.100.0. The declared 1.85 minimum
toolchain was not separately tested. No active desktop configuration was changed.

| Required check | Result |
| --- | --- |
| `cargo fmt --check` | Passed |
| `cargo clippy --all-targets -- -D warnings` | Passed, no warnings |
| `cargo test` | 42 passed, 0 failed, 0 ignored |
| `cargo build --release` | Passed |
| `sh -n scripts/chroma-fuzzel` | Passed |

The tests include 2 unit tests and 40 integration tests. Partial write/crash
recovery uses constructed journals and simulated partially applied files;
physical disk/power failures were not injected. Reload failures and timeouts
are exercised with real subprocesses. The GitHub CI workflow is provided but
has not been run on GitHub.

Additional native syntax checks used temporary Chroma configurations and all
six presets, with reload commands disabled:

- Niri 26.04: `niri validate -c TEMP/config.kdl`, with a real top-level include
  of each generated Niri color file. All passed.
- Fuzzel 1.15.0: `fuzzel --check-config --config TEMP/fuzzel.ini
  --log-no-syslog`, with a real include of each generated Fuzzel color file.
  All passed. Explicit alpha bytes were supplied by the test configuration.
- Kitty 0.50.0: Kitty's own `parse_config` loaded each generated color file
  using `kitty +runpy`, with error collection and executable `geninclude`
  processing disabled.
  All passed; no terminal window or remote-control command was invoked.
- Waybar CSS: the installed GTK3 CSS provider parsed a stylesheet importing
  each generated definition file and referencing `@chroma_background` and
  `@chroma_foreground`. All passed without initializing a desktop display.

Native parser checks are supplemental local checks, not requirements of the
portable Cargo test suite. They do not establish live visual or reload behavior.
Foot and Quickshell were not installed as available commands; their templates/
consumer examples were checked against the documented formats but not run in
those applications. The Dynamic Island example is deliberately illustrative;
its real format still needs to be supplied. GTK/Qt global theming is unfinished.

The Fuzzel selector was syntax-checked, not opened in a live Wayland session.
No system packages were installed. The release executable is about 2.4 MiB on
this toolchain. Future builds can differ.

See the README's temporary-directory walkthrough for safe end-to-end testing.
