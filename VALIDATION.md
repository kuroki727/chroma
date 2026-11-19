# T470 stabilization audit — v0.2, 2026-11-17

Fetched main `54ebaba3e8da8b1432e1bcf31d8630cafe392480`: baseline **108 passed**.
Final suite: **114 passed, 0 failed, 0 ignored**. Formatting, strict workspace
Clippy, locked release build and diff checks passed. All six presets passed
30 native parser checks on isolated copies of the T470's actual configurations;
five isolated ENOSPC write-injection cases also passed.

Read-only DMI/config inspection and isolated execution occurred on the T470 host
inside a sandbox. Interactive visual/reload validation remains **pending explicit
approval**. No active desktop config or process was modified. GTK3 coverage means
CSS-provider acceptance, not a universal GTK settings adapter.

See [the audit, limitations, exact reproduction commands, live checklist and
recovery procedure](docs/T470_VALIDATION.md). The older records below are historical.

# Final PR #3 audit — v0.2, 2026-11-16

The complete diff against origin/main (including the inherited review-bundle
commit) was audited. No live desktop configuration was modified, no reload or
lock-screen command was run, and native checks used temporary XDG roots.

Confirmed defects fixed:

- Planning inputs were not retained through commit. Added byte/mode snapshots for
  config/templates/sources, adapter directory membership, integration journal and
  selected local palette; output directory identity and expected manifest state
  are rechecked. External state changes survive failed recovery with backups
  retained. Bundle setup is blocked while palette recovery is pending.
- Offline native validation could follow Fuzzel includes and Niri includes using
  tabs/comments/quoted names. Include chains are now refused before invocation;
  conservative KDL string cases are left for manual validation.
- A legitimate v0.1 custom adapter named chroma-setup collided with the reserved
  setup entry. Only the exact Chroma config destination receives the exception;
  normal adapter output restrictions remain in force for that name.
- Review previews could emit terminal controls preserved in source comments.
  Display escaping now preserves the original artifact bytes without executing
  source terminal sequences.
- Dry-run could pass a plan whose serialized transaction exceeded the backup
  limit. Dry-run and commit now validate the same exact transaction payload
  before staging or replacement.

Setup history was separately checked before the first apply, after multiple
setup changes, after an actual palette apply, during reverse mixed history, and
with interrupted setup. Setup does not report a palette as applied and does not
change the previously applied palette's ID/colors. The version-1 state/backup
schema and existing CLI/configuration behavior remain compatible.

Local release gates executed successfully:

- cargo fmt --all -- --check
- cargo clippy --locked --all-targets --all-features -- -D warnings
- cargo test --locked --all-targets --all-features: **108 passed, 0 failed, 0 ignored**
  (14 unit tests, 94 integration tests)
- cargo build --locked --release
- git diff --check
- sh -n scripts/chroma-fuzzel

Ten new regression tests include deterministic intervening edits before commit
and after an actual replacement, stale config/state, source/template changes,
new standalone adapters, directory replacement, target/source symlinks, recovery
conflicts, full setup/apply history, native include refusal, terminal escaping,
backup limits and the legacy adapter name. Existing mutation-free preview tests
compare bytes, modes and modification times across the entire temporary tree.

Native checks passed for all six presets using Niri 26.04, GTK3 3.24.52 via the
checked-in CSS harness, Fuzzel 1.15.0, Kitty 0.50.0 via its native parse_config,
and the checked-in C++ Island harness compiled against the standalone project's
actual config.hpp. Foot's executable was unavailable; its colors-dark section,
20 generated keys and RRGGBB values passed structural checks only. Island was not
treated as Quickshell. No visual/session/reload validation is claimed.

Dependency versions were unchanged by the audit fixes. No advisory database scan,
MSRV toolchain run or physical power-loss campaign is claimed. Manual imports,
restricted conventional configuration detection, unbounded backup retention and
the final external-writer race window remain documented limitations. GitHub gates
are verified separately for the final pushed SHA before merge.

---

# Validation record — guided v0.2 theme integration, 2026-11-16

Branch: `feat/v0.2-theme-integration`, building on the existing review-bundle
implementation at `8c4a47d`. No active desktop files were edited and no application
reload, GUI, wallpaper or lock-screen command was run. All mutating tests and
native parser checks used temporary XDG roots.

| Executed check | Result |
| --- | --- |
| `cargo fmt --all -- --check` | Passed |
| `cargo clippy --all-targets --all-features -- -D warnings` | Passed, no warnings |
| `cargo test --all-targets --all-features` | 98 passed, 0 failed, 0 ignored |
| `cargo build --locked --release` | Passed |
| `sh -n scripts/chroma-fuzzel` | Passed |
| `git diff --check` | Passed |

The suite contains 9 unit tests and 89 integration tests. Eighteen new tests
exercise adapter discovery, four detection states, missing/fresh XDG roots,
isolation, existing fonts/layout/literal colors, duplicate/conflicting imports,
malformed INI/CSS, alpha inference and explicit opacity, guided setup/disable,
comment/mode preservation, standalone adapters, idempotent enablement, byte-identical
reapplication, all eleven semantic CSS roles, Fuzzel propagation, source/target/root
symlinks, traversal, whole-tree dry-run snapshots (bytes/modes/mtimes), unmanaged
output conflicts across adapters, setup failure injection, interrupted setup,
external-edit preservation and mixed setup/palette rollback history. Original
v0.1 transaction/CLI tests and the existing review-bundle tests remain passing.
Setup uses the original writer, version-1 snapshots and ordered rollback history.

Additional native smoke checks compiled `scripts/check-gtk-css.c` against the
installed GTK3 libraries, then loaded a temporary Waybar stylesheet importing the
actual generated fragment. Fuzzel `--check-config --config TEMP --log-no-syslog`
parsed a temporary main INI with the actual generated native include. Both passed
for rustwood, gruvbox-dark, nord, catppuccin-mocha, tokyo-night and paper.
No display was initialized. These checks establish parser acceptance, not visual
correctness or live reload behavior. The release build and selector shell syntax
check also passed.

Limitations: discovery intentionally inspects conventional paths and a restricted
CSS/INI integration shape; it does not establish arbitrary application semantics,
follow include chains, or probe installed versions. Other native/custom formats
require manual composition validation. Desktop wiring remains manual and needs
separate application backups. Repeated palette application retains history even
when resulting file bytes match. Existing filesystem/durability and MSRV limits
below still apply. No new dependency versions were resolved; toml_edit was
promoted from an already locked transitive dependency to preserve TOML comments.

---

# Validation record — v0.2 desktop review bundles, 2026-11-16

Branch: `feat/desktop-integration-v0.2`, based on `main` at `b0969a0`.
No active desktop configuration was modified. No integration installation,
application reload/restart, wallpaper change or lock-screen launch was performed
against the live desktop. Mutating integration tests use temporary directories.

## v0.2 results

- `cargo fmt --check`: passed.
- `cargo clippy --all-targets -- -D warnings`: passed.
- `cargo test`: **80 passed, 0 failed, 0 ignored** (8 unit, 72 integration).
- `cargo build --release`: passed.
- Existing optional Fuzzel selector tested with a mock executable; no real UI opened.

The new suite tests dedicated Island mapping and complete non-color byte
preservation; restricted/invalid CSS; Waybar GTK references, exact diff and alpha;
all ten Fuzzel alpha bytes; Niri KDL composition; sixteen Kitty ANSI slots;
Swaylock states; read-only/empty/missing-file planning; explicit confirmation;
idempotent bundles; durable interrupted-write recovery; bad journal paths;
concurrent source/destination changes; later-edit-preserving rollback; separate
palette rollback; output restrictions; and no-adapter diagnostics. Existing
invalid-TOML, reload-argv, symlink/hardlink, privacy and transaction tests remain.
5K PNG and JPEG regressions use generated 5120×2880 fixtures; header/byte limits
and compressed-file limits remain tested.

## Native checks on isolated copies of the actual configurations

| Consumer | Offline check | Result |
| --- | --- | --- |
| Niri 26.04 | Full temporary config with final native color include, `niri validate -c TEMP` | Passed |
| Waybar GTK3 CSS | Headless `GtkCssProvider` loads migrated copy and generated definitions | Passed |
| Fuzzel 1.15.0 | `--check-config --config TEMP --log-no-syslog` | Passed |
| Kitty 0.50.0 | `kitty +runpy` invokes native `parse_config`, collects bad lines, disables executable geninclude | Passed |
| Standalone Island | Offline C++ harness includes the actual local `config.hpp`, calls `apply_style` and `parse_color` | Passed |
| Swaylock-effects 1.7.0.0 | Installed manual/package inspection; no process launched | Source has invalid `exec swayidle`/shell continuation syntax; integration reported unsupported until manually corrected |
| Qt6ct | Installed qt6ct-kde package and referenced KDE `.colors` inspected read-only | Generator deferred; existing scheme/selection untouched |

Both actual `lain.png` and `foggy_forest.jpg` successfully generated validated
palettes into `/tmp`. The latter is 5120×2880. The new 16,777,216-pixel bound
admits it without changing the 64 MiB decoded-byte limit. No wallpaper was set.

Validation harness sources are under `scripts/check-{island-parser,gtk-css,kitty-config}`.
No system packages were installed. Parser checks do not establish visual
correctness or live reload behavior. Automatic desktop wiring, Qt generation,
streaming/codec-native image downsampling, and universal GTK/Qt theming remain
outside the implemented scope; see [integration guide](docs/DESKTOP_INTEGRATION.md).
Dependencies were not updated; tempfile is now also used by runtime validators.
No vulnerability-database scan or alternate-toolchain/MSRV run is claimed.

---

# Validation record — v0.1 pre-merge audit, 2026-11-16

Local Linux toolchain: rustc/cargo 1.100.0. Branch: `feat/initial-implementation`.
The clean baseline `05cc8b1` independently passed all original 42 tests and all
four required checks before modifications. No active desktop configuration was
modified; all generated files, markers and application parser inputs were temporary.

## Final local results

| Check | Result |
| --- | --- |
| `cargo fmt --check` | Passed |
| `cargo clippy --all-targets -- -D warnings` | Passed, no warnings |
| `cargo test` | **61 passed, 0 failed, 0 ignored** |
| `cargo build --release` | Passed |
| `sh -n scripts/chroma-fuzzel` | Passed |

The suite contains 6 unit tests and 55 integration tests. Nineteen new tests
exercise transaction failure injection, corruption/privacy checks, lock release,
path handling, missing-template reload, image resource limits and edge cases,
XDG overrides, template line numbers, terminal-control escaping and the selector. A mocked Fuzzel is used;
no real menu, desktop reload or terminal window is opened by tests.

The actual apply state machine is tested with injected errors after its first,
second and third replacements, asserting durable pending records precede writes,
and that previous files/modes/manifest are restored. Real backup-directory
permission failure is tested before replacement. Other tests construct interrupted
journals and conflicting outputs to verify retry and preservation of external edits.
There was no physical disk/full-filesystem/power-failure campaign.

Image fixtures are generated deterministically in Rust: PNG/JPEG color blocks,
single pixels, grayscale/low-saturation/near-black/near-white gradients, transparent
images, corrupt data, a sparse oversized input, mutated oversized PNG headers,
and a valid 4096×2048 grayscale image at the 8,388,608-pixel boundary. Oversized
headers fail before pixel allocation. A separate isolated FIFO reproduction
confirmed the old blocking behavior; its regression now checks bounded CLI exit.
These tests establish limits/behavior, not a hard process RSS cap or image-parser
fuzz coverage.

## Native integration checks repeated during this audit

All six presets were generated with adapters enabled only in temporary config,
`auto_reload = false`, display variables removed for subprocesses, and private
temporary XDG/Kitty config/cache locations. No installed application was restarted.

| Consumer | Check | Result |
| --- | --- | --- |
| Niri 26.04 | `niri validate -c TEMP/config.kdl`, including generated colors | Six presets passed |
| Fuzzel 1.15.0 | `fuzzel --check-config --config TEMP/fuzzel.ini --log-no-syslog`, including generated colors with explicit DD alpha | Six presets passed |
| Kitty 0.50.0 | `kitty +runpy` calling its native `parse_config`, collecting bad lines and disabling executable geninclude | Six presets passed |
| GTK3 / Waybar CSS | GTK3 CSS provider loaded a stylesheet importing definitions and referencing named colors, without display initialization | Six presets passed |
| Quickshell QML example | Installed Qt6 `qmlformat` parsed grammar without modifying the source | Passed grammar only |

Foot and Quickshell executables were unavailable. Foot's template was compared
with its upstream man-page source, including `colors-dark`, plain RRGGBB colors
and main-section includes. Quickshell's consumer matches the documented FileView
watcher pattern; import typing and live reload are unverified. Dynamic Island's
format remains intentionally unspecified. No adapter imposes fonts, geometry,
animation, keybindings or opacity; custom templates remain the user's responsibility.
No GTK/Qt universal integration is claimed.

## Dependency review and limits of validation

The lockfile was retained; runtime dependencies remain anyhow, clap, serde,
toml, image (PNG/JPEG only) and rustix. The rustix minimum is now 1.1.6, already
locked and patched for [RUSTSEC-2026-0486](https://rustsec.org/advisories/RUSTSEC-2026-0486.html).
The affected xattr APIs are not used. Image 0.25.9's local JPEG wrapper and
upstream [allocation-limit documentation](https://docs.rs/image/0.25.9/image/struct.Limits.html)
were inspected; compressed/pixel/output limits supplement its non-strict internal
allocation setting.

`cargo-audit` is not installed. Shell DNS/network access to GitHub remains blocked;
no full advisory database scan or dependency update is claimed. Manual source and
targeted RustSec review are narrower than such a scan. No system packages were
installed. The declared Rust 1.85 minimum was not separately tested.

Local checks do not establish production readiness, visual correctness or live
application reload behavior. Remote CI status must be checked independently on
the updated PR. Detailed findings: [docs/AUDIT.md](docs/AUDIT.md); guarantees and
recovery: [docs/SECURITY.md](docs/SECURITY.md).
