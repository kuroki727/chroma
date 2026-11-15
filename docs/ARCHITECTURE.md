# Architecture

Chroma is a synchronous CLI/library with no daemon, async runtime or desktop
dependency. The core is independent of the target application configuration language.

| Module | Responsibility |
| --- | --- |
| `main`, `cli` | clap parsing, command orchestration, diagnostics and exit codes |
| `config` | Strict trusted TOML schema, XDG/override paths and adapter policy |
| `palette` | Semantic/ANSI model, strict colors, presets and wallpaper extraction |
| `adapters` | Separate output resolution, safe text substitution and bounded argv |
| `engine/apply` | Preflight, staged outputs and transaction orchestration |
| `engine/backup` | Private serialized transactions and transaction identifiers |
| `engine/rollback` | Endpoint validation, conflict-aware restoration and reload reporting |
| `safe_fs` | Descriptor-relative Linux I/O, private state policy, rename/fsync and scoped locks |

## Apply lifecycle

The CLI loads trusted configuration and the requested palette. Apply acquires the
state lock, loads/validates the manifest and refuses pending recovery. Planning
renders every enabled adapter before staging, protects all declared input paths,
checks output uniqueness/permissions and compares existing managed snapshots.

Every output is staged and fsynced in its destination directory. A complete
before/after transaction is validated and persisted under `transactions/`. The
manifest then records its pending ID. Outputs are replaced one by one, with a
snapshot recheck immediately before each rename. New files use no-replace rename.
Only after all replacements can trusted reload argv run. Success commits the new
active palette, file map and history head, clearing pending recovery.

An error after replacements enters restoration using the durable transaction.
Completed snapshots remain available for rollback; unreferenced backups are not
automatically pruned. Failures before replacements can leave state directories
or staged/unreferenced records, but must not alter managed outputs.

## State and recovery

The version-1 manifest stores the active palette/managed file snapshots, history
head and optional pending operation. A transaction stores its parent ID, before
and after active state, plus per-destination before/after snapshots. Before may
be absent for an initial apply or a newly created file. Ordinary Unix mode bits
are part of snapshot equality.

Before recovery, validation checks IDs, palette/permission/path validity, duplicate
destinations, complete after-state file maps and correspondence between the
manifest and transaction endpoints. This protects against structurally corrupt
records, not malicious same-user tampering. The on-disk format is unchanged;
valid journals from the initial implementation remain supported.

Recovery marks restoration pending, then visits entries in reverse order. Matching
before snapshots are already restored; matching after snapshots can be restored
or removed. Different current data is preserved and reported. Restoration is
retryable. On success, the prior active state/head are persisted. Optional reloads
use current trusted config and resolve output paths without reading templates.
Emergency `rollback --no-reload` needs neither config parsing nor templates.

These are recoverable multi-file transactions, not a globally atomic switch.
See [SECURITY.md](SECURITY.md) for concurrent-writer, durability and metadata limits.

## Palette generation and rendering

PNG/JPEG input is opened nonblocking, checked as regular and copied through a
bounded read. Decoder headers are checked against dimension/pixel/output budgets
before pixel decoding. A 128×128 thumbnail supplies a sorted, weighted histogram.
Deterministic farthest-point seeding and Lloyd k-means operate in OKLab. Hue/chroma
selection, fixed layer lightnesses, gamut reduction and contrast refinement produce
dark/light semantic palettes. No randomness, subprocesses or services participate.

Templates only reference validated colors or locally configured alpha bytes.
Literal escaping makes the format usable with CSS, INI, JSON, KDL, QML and other
text. Line-number accounting is incremental, avoiding repeated whole-prefix scans.
Rendering is separate from application parsing: native validators are supplemental
checks, and application imports remain manual.

## Testing boundaries

Portable tests use temporary files and explicit config directories. Unit fault
injection calls the actual apply implementation after the first, second and third
replacement; no production environment-variable failure switches are exposed.
Recovery/conflict/permission tests use real filesystem operations. Deterministic
fixtures cover image edge cases and invalid/oversized headers without allocating
an image bomb. A mocked Fuzzel exercises the selector without opening a window.

Native parser checks and version coverage are recorded separately in VALIDATION.md.
They do not establish visual correctness, live reload behavior or power-failure
resilience. The declared minimum Rust toolchain has not been tested separately.
