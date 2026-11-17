# T470 Niri integration: reviewed candidate only

**No live installation, configuration change, palette application, signal or
compositor reload has occurred during this task.** Installing or reversing this
candidate requires separate approval: Niri automatically watches its main file
and includes, so atomic replacement can immediately change the running desktop.

## Baseline and exact proposal

Worktree: `/home/kuroki/Projects/chroma/local/niri-integration`, branch
`feat/t470-niri-integration`, based on stable main
`54ebaba3e8da8b1432e1bcf31d8630cafe392480`. No changes from PR #4, #5 or #6 are
required or incorporated; their branches/worktrees are preserved. Existing CLI
syntax, palette schemas, templates and transaction formats are unchanged.

Read-only physical T470 inspection found Niri 26.04 (`8ed0da4`), package 26.04-1:

| Input | Exact path / inspected state |
| --- | --- |
| Main configuration | `/home/kuroki/.config/niri/config.kdl`; UID/GID 1000:1000, 0644, single-link regular file |
| Generated colors | `/home/kuroki/.config/chroma/generated/niri.kdl`; current Boats Preview colors |
| External config writer | `/home/kuroki/.local/bin/niri-opacity`; modifies the main config and can restore its own earlier backup |
| Existing opacity backup | `/home/kuroki/.config/niri/config.kdl.niri-opacity.bak`; preserved, separately guarded |
| Palette state | `/home/kuroki/.local/state/chroma/state.toml`; `boats-preview`, no pending transaction, head `tx-18762-1794935644727889796` |

Original main SHA256:
`2dfb14c50c294edb13fc85b9cdb6093e177bc3d9846b4b2772d470c07667872c`.

Original/generated color mapping:

| Niri property | Existing literal | Chroma role | Boats Preview | Rustwood |
| --- | --- | --- | --- | --- |
| `layout.border.active-color` | `#6f7f4f` | primary | `#DE9961` | `#B98252` |
| `layout.border.inactive-color` | `#2a2e22` | border | `#75706D` | `#574436` |
| `layout.border.urgent-color` | `#b5683c` | warning | `#ECD69A` | `#D6A45C` |
| `layout.background-color` | `#12140f` | background | `#0E0B09` | `#17130F` |

There are no existing includes in the inspected source. Retain every original
declaration and append [this exact suffix](t470-niri/include.kdl) after the opacity
block. The complete proposed diff is:

```diff
@@ -281,3 +281,5 @@
     opacity 1.00
 }
 // END NiriOpacity
+
+include "/home/kuroki/.config/chroma/generated/niri.kdl"
```

Niri includes are positional: later values override earlier values. Layout
properties merge individually. The final fragment contains only the four color
properties above; it cannot replace gaps (10), border width (2), disabled focus
ring, input/environment settings, bindings, startup commands, animation settings,
window rules, geometry or workspace behavior. No original declarations need
removing or relocating. The original file is an exact byte prefix of the candidate.
The raw `match app-id=r#"firefox$"# title="^Picture-in-Picture$"` and the entire
opacity block remain byte-for-byte intact.

These precedence conclusions follow Niri 26.04's sequential config merging and
the documented include rules. Native validation verifies accepted composition;
it does not expose a resolved-config dump or prove visual behavior. No live
compositor observation or restart is substituted for offline validation.

## Guard changes and security evidence

Two focused regression tests failed before the fixes:

1. A correctly delimited raw regex was rejected because the include scanner
   treated its opening quote as an unsupported string. The scanner now recognizes
   KDL v1 raw delimiters and consumes literal content through the matching close.
   It does not alter the raw string. Bare, quoted and raw-spelled `include` tokens
   remain forbidden, including after another raw string or unusual separators.
2. The native-validation guard inspected only the main source, allowing generated
   fragments to introduce another include before Niri was invoked. Both source
   and generated fragment are now scanned before any native process starts.

Malformed/unclosed raw delimiters, unsupported KDL v2 raw syntax, escaped quoted
strings and triple-quoted strings remain fail-closed/manual. Comment nesting and
quoted include checks remain intact. This is a bounded lexical guard, not a
replacement for Niri's parser. Existing include chains are not followed or
silently accepted, and no safety-check bypass flag is introduced.

`examples/niri_candidate.rs` reuses that guard and the installed native validator.
Its compose/check operations snapshot the explicit fragment into a temporary
directory and validate the complete original plus exactly that local include.
Only after success does compose emit the proposed live-path candidate. Source
and fragment reads reject symlinks. The helper requires the candidate bytes to
equal that validated composition, so a pinned candidate cannot hide other edits.

## Native validation and tests actually executed

The private review directory is:

`/home/kuroki/Projects/chroma/local/niri-integration/local/review`

Full host configurations and original backups stay there, ignored by Git. The
repository commits only a minimal public suffix, representative test fixtures,
code and documentation. The directory is 0700; original/candidate/manifest are
0600, owner 1000:1000. A private copy of the release validation helper is 0700
and hash-pinned, so rebuilding examples cannot silently change the reviewed tool.

The installed native commands are noninteractive parser operations, not IPC:

```sh
niri validate -c /home/kuroki/Projects/chroma/local/niri-integration/local/review/original.kdl
niri validate -c /home/kuroki/Projects/chroma/local/niri-integration/local/review/candidate.kdl
niri validate -c /home/kuroki/Projects/chroma/local/niri-integration/local/review/boats-preview-candidate.kdl
niri validate -c /home/kuroki/Projects/chroma/local/niri-integration/local/review/rustwood-candidate.kdl
```

All four exit 0 and report `config is valid`. The exact candidate references the
existing live generated fragment, already inspected and confirmed include-free;
the other two compositions reference isolated template-rendered fragments.
Generated Boats values were checked against the current palette and state.
Invalid generated KDL and missing fragments were rejected in isolated tests.

Required project verification:

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
cargo build --release --locked
git diff --check
cargo build --release --locked --example niri_candidate
PYTHONDONTWRITEBYTECODE=1 CHROMA_NIRI_VALIDATOR=/home/kuroki/Projects/chroma/local/niri-integration/local/review/niri-validator \
  python3 scripts/test-niri-candidate.py
```

All required checks passed. Rust: **114 passed, 0 failed, 0 ignored** (108 stable
tests plus six focused tests). Python: **11 passed, 0 skipped**, plus the optional
real-native isolated check/install/repeated-install-refusal/reverse/invalid-KDL/
missing-fragment smoke sequence. Writer unit tests mock native process responses
to exercise filesystem failures; the separately executed native sequence does
not mock them. CI runs the eleven writer tests; native branches require installed
Niri and were executed on this T470, not assumed available on CI.

Coverage includes final include placement, retained literals/precedence ordering,
raw regex and opacity preservation, unsafe path/include rejection, generated
include chains, native malformed KDL, missing fragments, repeated operations,
external edits before/during staging, guarded reversal, ownership/permissions,
symlinks/hardlinks, manifest/candidate/validator changes, xattrs/ACL refusal,
concurrent bundle operations, staging fsync failure and staging-name collision.

## Reviewed installation and reversal commands

The only later writable live target is `/home/kuroki/.config/niri/config.kdl`.
The generated fragment, opacity script/backup, Chroma config/adapter, state and
six transaction files are read-only dependencies. Twelve dependencies are pinned
in the manifest. Candidate SHA256:

`ca656ac77b5650dff9d021fa19b130799b42c72600fff5fee7465e5842bfa183`

Fragment SHA256:
`86a4b3d5c335c7f43eb45af699af544af0764e5b33f8664575bab86403dc740d`

Manifest SHA256:
`2481ad4eeb5aa502b79341548104fa8f9ac83b5c317197d902c90e14e9d84afe`

Set these explicit values in the dedicated worktree:

```sh
cd /home/kuroki/Projects/chroma/local/niri-integration
NIRI_REVIEW=/home/kuroki/Projects/chroma/local/niri-integration/local/review
NIRI_MANIFEST=2481ad4eeb5aa502b79341548104fa8f9ac83b5c317197d902c90e14e9d84afe
```

Read-only live preflight with isolated native validation (executed successfully):

```sh
PYTHONDONTWRITEBYTECODE=1 python3 scripts/niri-candidate.py check --review-dir "$NIRI_REVIEW" --manifest-sha256 "$NIRI_MANIFEST"
```

**Not executed. Separate approval required; this may activate colors immediately:**

```sh
PYTHONDONTWRITEBYTECODE=1 python3 scripts/niri-candidate.py install --review-dir "$NIRI_REVIEW" --manifest-sha256 "$NIRI_MANIFEST"
```

After an approved installation, verify the exact candidate hash, 0644 mode,
1000:1000 ownership, native composition and unchanged dependencies, then request
user confirmation of visual results. No explicit Niri reload command is needed.

**Not executed. Separate reversal approval required; this also activates live:**

```sh
PYTHONDONTWRITEBYTECODE=1 python3 scripts/niri-candidate.py reverse --review-dir "$NIRI_REVIEW" --manifest-sha256 "$NIRI_MANIFEST"
```

The helper preserves the original backup, reads via anchored no-follow paths,
requires a single-link user-owned regular target, checks hashes/modes, refuses
ACLs/xattrs it cannot preserve, validates natively, rechecks after validation and
staging, fsyncs the staged file, atomically replaces it and fsyncs its parent.
Operations on the same private bundle are serialized. Repeated installation
fails because the original hash no longer matches. Reversal requires the active
file to equal the exact installed candidate and original mode; later edits,
including opacity-only edits, cause refusal instead of stale restoration.
Reversal validates the original without depending on a still-existing generated
fragment, so later palette changes need not prevent disconnection. No Chroma
transaction rollback or history editing is involved.

## External writers, activation and recovery limits

`niri-opacity` normally replaces only its marked block, preserving the appended
include. Any such edit makes this review's installation/reversal guard stale.
Stop and prepare a newly reviewed candidate rather than overriding that edit.
Its failure path copies its older first backup over the main config, potentially
removing the include and unrelated later edits. This script was inspected but
not changed or run. Do not invoke it or edit/deploy configs concurrently with a
later approved integration operation; its recovery behavior deserves a separate
review before relying on it as an integration-aware recovery mechanism.

Niri watches both its main configuration and included files. Installation,
reversal and future Chroma fragment writes can all become visible without a
signal, even with Chroma `--no-reload`. No watcher was enabled by this task:
this is Niri's native behavior. Atomic replacement avoids partial-file syntax,
but does not make multi-adapter palette updates globally atomic or visually
invisible during rollback. Contrast over arbitrary wallpaper/window edges and
actual appearance are not certified by parser checks.

Uncooperative writers can still race the last comparison and rename; POSIX rename
is not a conditional hash compare-and-swap. If staging fails, the active file is
preserved and handled temporary files are cleaned. Failure after replacement or
directory fsync has an uncertain durability outcome: inspect actual hashes and
retain the bundle; do not retry or roll back blindly. Abrupt process/power loss
may leave an uninstalled `.chroma-niri-*.tmp` file. No backup deletion or automatic
reversal is performed. If active bytes differ from both reviewed versions,
preserve them and reconcile manually under new approval.

References: [Niri include semantics](https://github.com/niri-wm/niri/wiki/Configuration:-Include),
[Niri 26.04 sequential config merge implementation](https://github.com/niri-wm/niri/blob/v26.04/niri-config/src/lib.rs).
