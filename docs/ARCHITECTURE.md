# Architecture

Chroma is a synchronous CLI/library with no daemon, async runtime or desktop
dependency. The core is independent of the target application configuration language.

| Module | Responsibility |
| --- | --- |
| `main`, `cli` | clap parsing, command orchestration, diagnostics and exit codes |
| `config` | Strict trusted TOML schema, XDG/override paths and adapter policy |
| `palette` | Semantic/ANSI model, strict colors, presets and wallpaper extraction |
| `wallpaper` | Opt-in post-success hook, debounce/coalescing, validation and shared transaction orchestration |
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

The wallpaper hook registers a private request token under a short separate
lock, then takes the existing state lock through extraction, planning, validation,
duplicate comparison and commit. New requests supersede waiting/planning workers;
requests arriving during commit are applied by their own waiting invocations.
There is no background queue consumer, persistent palette cache or new state
schema. The latest-token file is coordination metadata only. Enable/disable
submits a `chroma-setup` config entry to the same journal. See
[WALLPAPER_SYNC.md](WALLPAPER_SYNC.md) for ordering and failure boundaries.

Manual apply, rollback and wallpaper controls advance a cancellation epoch
inside the existing writer lock file, without replacing its inode. Older admitted
hooks exit after acquiring the lock instead of undoing the manual operation.
Epoch data is outside the manifest/history schema. Duplicate shortcuts recheck
the complete plan and current destinations; automatic hooks preserve deletion
of previously managed outputs as an external edit.

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
before pixel decoding. Up to 128×128 actual stratified pixels supply an alpha-weighted,
sorted histogram, without a full-size RGBA conversion or thumbnail color blending.
Population-aware seeding and Lloyd k-means operate in OKLab; close centers merge
and tiny noise clusters are discarded. `generator::extract` exposes representatives
and normalized populations for the offline report example. Source hue/chroma,
scene-influenced tonal ranges, gamut reduction and contrast refinement produce
separate dark/light palettes with explicit ANSI entries. The generator enforces
4.5:1 for text/ANSI and 3:1 for borders against the documented backgrounds;
user-palette validation and preset defaults remain compatible. No randomness,
subprocesses or services participate in generation.

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


## v0.2 source-aware desktop adapters

`integration::colors` implements restricted, lossless color-token replacement
and native Niri/Kitty/GTK color outputs. `integration` plans separate review
artifacts and disabled standalone adapter files, with its own private journal.
It never mutates a desktop source. `integration::validate` uses temporary copies
and bounded noninteractive Niri/Fuzzel validators during optional installation.
The palette engine accepts initial ownership only for exact completed bundle
snapshots and rejects pending integration recovery. Desktop activation stays
manual; palette and integration rollback are separate operations.
See [desktop guide](DESKTOP_INTEGRATION.md) for the precise supported grammar,
source formats, watcher behavior and limitations.


## Adapter discovery and setup

`adapters::discovery` reads conventional main configs beneath an explicit XDG
root, classifies missing/setup-required/ready/unsupported cases, and never follows
application includes or changes desktop files. Ready classification is limited
to recognizable Waybar/Fuzzel native connections, not arbitrary syntax validation.
Other built-ins require manual composition/version validation.

`adapters::setup` edits the enabled flag, diagnostic config path and inferred
Fuzzel alpha in Chroma TOML using toml_edit, preserving unrelated configuration
and comments. It submits a validated plan to the existing `engine::apply::commit`
writer with reloads disabled. It uses the existing state lock, version-1 manifest,
backup format, pending-operation recovery and reverse rollback history. No new
setup journal exists. A reserved `chroma-setup` entry identifies narrowly scoped
Chroma config destinations during recovery; ordinary adapter outputs remain
restricted. Setup records palette metadata to retain the existing state schema;
`current` reports no applied palette until an actual adapter output is committed.

Dry-run uses the same planning checks without staging, locking or creating state.
The existing optional review-bundle journal remains compatible and separate from
normal setup/application history.


## Final audit input consistency

`safe_fs::Inputs` holds read provenance only; it is excluded from Config's TOML
schema. It records file snapshots (including absence and modes) and descriptor-read
adapter membership, and validates them during apply/setup. Rendering carries the
actual source/template reads into the shared plan. State and output directory
checks reject detected intervening edits before committing. `preview_transaction`
constructs and validates the exact backup payload for both dry-run and commit;
version-1 transaction/state serialization remains unchanged.
