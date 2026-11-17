# Island C++ integration candidate — no live installation

This work is preparation only. Island was not launched, reloaded, signalled or
restarted. No live configuration, generated output, transaction, wallpaper or
Island source file was changed. All installation commands below require later
explicit approval; generation and startup wiring are separate approval gates.

## Actual architecture

The inspected source tree is `/home/kuroki/Projects/island`. It is **not a Git
checkout**; no origin or source commit can be established. Its local `island`
binary matches `/home/kuroki/.local/bin/island` and running PID 775, SHA256
`930ffcdd06654bfe4e062d19da49a8a79506796a161636793cebe6de45c43349`.
The source files are snapshotted by hash; compiling the offline checker writes
only inside this Chroma worktree. No Island repository was modified or pushed.

The running arguments are `/home/kuroki/.local/bin/island` with no overrides.
`config.hpp` resolves XDG_CONFIG_HOME, otherwise HOME/.config/island, then loads
embedded configuration/style defaults, user JSONC, and **one** user stylesheet.
Actual files are `/home/kuroki/.config/island/config.jsonc` and
`/home/kuroki/.config/island/style.css`. Missing files silently retain defaults.
Chroma's opt-in adapter instead refuses a missing source.

The parser is a small literal C++ parser, not GTK, browser CSS, QML or Quickshell:

- No imports, variables, nested rules, pseudoclasses or general cascade engine.
- Comma-separated simple selectors, optionally prefixed by `.` or `#`, share
  declarations. Prefixes do not mean browser classes/IDs here.
- `island`/`window`: background/background-color, border-color, border-width.
- `time`, `date`, `text`, `title`, `body`, `label`, `value`, `flash`/`layout`,
  `dnd`: color/font-size.
- `track`, `fill`, `tile`, `tile-on`: background/background-color.
- Native literal color forms include short/full hex, hex alpha and comma rgb(a).
  The new opt-in contrast-certified profile deliberately requires opaque hex;
  the existing legacy Island renderer remains unchanged for other formats.
- Unknown properties and bad colors can log warnings while apply_style still
  returns true. Native validation therefore rejects diagnostics as well as
  checking the parsed fields; the Rust profile also validates its restricted input.

`main.cpp` accepts `-s`/`--style` and `-c`/`--config`. Repeated `-s` assigns the
last path, not multiple sheets. `island ctl reload` and its SIGUSR2 handler load
defaults plus the **same startup-selected paths**, reapply settings, and reconcile
services. There is no stylesheet watcher. Reload cannot retarget the current
process from style.css to a new generated path.

PID 775 owns `org.freedesktop.Notifications`. `dbus.hpp` reads urgency, and
main.cpp uses it for longer critical notices and DND bypass. It does not choose
different CSS by urgency. Notification text uses text/title/body; history uses
title/body/label. Do not invent `.critical` or add another notification daemon.

Island's control center implements tiles, rows, buttons, sliders, notification
history and power confirmation. The application launcher, Wi-Fi/volume/brightness
menus and password prompt use Fuzzel externally; no Island launcher widget exists
to theme. Existing Fuzzel integration and launcher commands remain untouched.
The layer surface requests keyboard interactivity NONE; there is no CSS focus
state. Hover feedback is the renderer's fixed 6% white overlay, not `:hover` CSS.

## Opt-in profile and measured mapping

The existing adapter remains unchanged until approval. Its candidate adds only:

```toml
island_readable = true
```

The flag is accepted only with `desktop = "island"`. It uses a self-contained
generated stylesheet at `/home/kuroki/.config/chroma/generated/desktop-island.css`.
Legacy Island rendering, CLI syntax, palette definitions and transaction formats
are unchanged. This option requires a binary containing this PR; older binaries
reject the unknown option rather than silently ignoring the profile.

The profile preserves every existing non-color CSS byte and fills missing known
color declarations without specifying new font sizes or changing behavioral JSON.
Derived colors adjust OKLCH lightness, retaining seed hue/chroma with gamut
reduction. Normal/hovered text targets 4.6:1 and indicators 3.1:1 to leave a margin
above 4.5/3.0. Hover is modeled from the actual white overlay at alpha 0.06 in
panel.hpp. No CSS function or variable is emitted.

| Field / actual UI role | Seed / correction | Boats Preview | Rustwood |
| --- | --- | --- | --- |
| Background | background | #0E0B09 | #17130F |
| Border | border, lift if needed | #75706D | #746051 |
| time/text/body/value | foreground; also covers danger buttons | #F4F1EF | #FBEFDA |
| date/label; disabled slider thumb | muted, adjusted against surfaces/track | #BAB5B1 | #C8B9A6 |
| title/flash/layout; normal thumb | primary, adjusted | #EDA76F | #EAB07F |
| track | surface_alt lifted for visibility | #635F5C | #6C6259 |
| fill | primary adjusted against track | #EDA76F | #EAB07F |
| tile (off) | surface | #1C1816 | #241D17 |
| tile-on | primary hue at dark surface lightness | #3B1F05 | #3F2000 |
| dnd; confirmation-button fill | error tint constrained for text and border | #8B504B | #A2483D |

The C++ parser dump and sRGB calculations give these worst-case ratios:

| Actual fully visible pair(s) | Boats Preview | Rustwood |
| --- | ---: | ---: |
| Foreground on background/off/on, including hovered tiles | 11.40 | 10.98 |
| Muted text on those surfaces | 6.31 | 6.51 |
| Title/flash on those surfaces | 6.32 | 6.55 |
| Confirmation text on danger, including hover | 4.85 | 4.62 |
| Opaque border/background | 4.01 | 3.11 |
| Opaque DND/background | 3.13 | 3.11 |
| Track/background | 3.10 | 3.11 |
| Fill/track | 3.12 | 3.12 |
| Normal/muted slider thumb against track, minimum | 3.11 | 3.10 |

All normal text exceeds 4.5:1, including 40px clock text that only requires 3:1.
The old bright-primary tile-on mapping gave about 2.11:1 for Boats foreground;
dark tinted active surfaces avoid that failure. On/off labels already identify
state; fill hue is supplementary. Dark off/on fills do not themselves have a 3:1
separation. Stronger fill-only state cues would require separate active-state
foreground fields in Island; do not trade readable shared text for bright fills.

**Limits that CSS cannot fix:** the same dnd field is both a breathing border and
confirmation fill. Border alpha breathes from 0.35 to 0.9 and composites against
arbitrary wallpaper, so the opaque border numbers do not certify 3:1 throughout
animation. Text fades likewise cannot retain full contrast during transitions.
Preserving those behaviors is intentional. A future separate Island change could
split DND indicator/danger surface and expose per-state foregrounds; urgency
styling would need a separate urgency-aware renderer field. None is implemented
or modified here. Anti-aliasing and physical appearance still need visual review.

## Validation and regression evidence

All five requested checks passed: fmt, Clippy with -D warnings, workspace tests,
release build and diff whitespace. Rust: **114 passed, 0 failed, 0 ignored**
(108 stable-main tests plus six focused profile/adapter/transaction tests).
Python guard suite: **12 passed, 0 skipped**, added to CI.

Native checker compilation and reproduction:

```sh
c++ -std=c++17 -I /home/kuroki/Projects/island scripts/check-island-parser.cpp -o local/check-island
cargo build --release --locked --example island_candidate
PYTHONDONTWRITEBYTECODE=1 python3 scripts/validate-island-review.py \
  local/check-island /home/kuroki/.config/island/style.css \
  docs/t470-island/boats-preview.css docs/t470-island/rustwood.css
PYTHONDONTWRITEBYTECODE=1 python3 scripts/test-island-candidate.py
PYTHONDONTWRITEBYTECODE=1 python3 scripts/test-island-native.py \
  target/release/chroma target/release/examples/island_candidate local/check-island
```

Both candidates pass the actual config.hpp parser without diagnostics, populate
all 15 color fields and preserve all nine dumped font-size/border-width values.
The last command executes configure, real isolated Chroma transactions, fresh
startup-manifest preparation, native Niri validation, startup install and ordered
reversal entirely under a temporary fixture directory. It never launches Island.
The full private startup candidate also passes `niri validate -c PATH`, exit 0.

Tests cover color rendering, determinism, selector compatibility, real contrast
pairs, legacy compatibility, opt-in dispatch, source tracking/missing files,
invalid CSS, opacity refusal, unchanged configuration bytes, isolated palette
apply/rollback, guarded reversal and repeated-install refusal, external edits,
symlinks/hardlinks/unsafe ownership, staging/fsync failure and collision handling,
missing CSS, parser failure, and refusal to prepare startup before generation.

## Private review and future approval sequence

Worktree: `/home/kuroki/Projects/chroma/local/island-integration`, branch
`feat/t470-island-integration`, based on stable main
`54ebaba3e8da8b1432e1bcf31d8630cafe392480`. PRs #4–#7 are not incorporated or
changed. Before **live generation**, separately review a compatible binary that
also retains the audited PR #4 transaction fixes; do not substitute this main-based
development binary for the audited live writer. No binary upgrade is authorized
or performed by this preparation task.

Private root: `/home/kuroki/Projects/chroma/local/island-integration/local/review`,
0700. It contains original Island JSON/CSS, adapter and Niri originals/candidates,
diffs, native reports and pinned executable checker/renderer copies. Data files
are 0600, executables 0700, owner 1000:1000. Backups remain ignored by Git.

Adapter manifest SHA256:
`4b62b82340a542042c6d5e1c3bc7df4a60e2439eaae96c5f56cb88c81f3b8bde`.
Boats stylesheet SHA256:
`81a3a72e48eaa9b63fa1053431409136a064d718ca5b6e715317da9dc41d1bc3`.

1. After approval of the compatible binary and a fresh verified pre-test backup,
   re-run the adapter check. Separately approve installation of the **Chroma
   adapter flag only**. No CSS/output or Island/Niri config is written here.
2. Separately approve one current-palette apply using that reviewed binary,
   explicit live config/state paths, and `--no-reload`, after reviewing its dry-run.
   This records the new generated Island CSS in Chroma's transaction history.
   **Never manually copy a candidate over a managed generated file:** that would
   make Chroma's recorded output differ from the filesystem.
3. Run prepare-startup only after that transaction. It checks exactly one new
   transaction with the same palette, unchanged other outputs, matching state
   snapshots, exact reviewed Island CSS, and fixed source/configuration guards.
   It writes a new private startup manifest and prints a **new hash for review**.
   It does not modify live startup. Review/approve this new manifest separately.
4. Install the startup candidate with its new approved hash. Its sole edit is:

```diff
-spawn-at-startup "/home/kuroki/.local/bin/island"
+spawn-at-startup "/home/kuroki/.local/bin/island" "-s" "/home/kuroki/.config/chroma/generated/desktop-island.css"
```

Niri reparses its config automatically, but spawn-at-startup does not execute on
reload. On a later ordinary session start, verify Island's actual arguments.
The singleton lock can prevent a new instance if the old one survives; do not
launch a replacement, terminate the old one or log out automatically. The current
process cannot be retargeted by ctl reload. After Island normally starts with -s,
future generated colors need a separately approved `island ctl reload`; no
automatic reload command or watcher is enabled by this candidate.

Prepared commands, **installation/reversal not executed against live paths**:

```sh
cd /home/kuroki/Projects/chroma/local/island-integration
ISLAND_REVIEW=/home/kuroki/Projects/chroma/local/island-integration/local/review
ISLAND_MANIFEST=4b62b82340a542042c6d5e1c3bc7df4a60e2439eaae96c5f56cb88c81f3b8bde
python3 scripts/island-candidate.py check --review-dir "$ISLAND_REVIEW/adapter" --manifest-sha256 "$ISLAND_MANIFEST"
# Separate adapter-install approval required:
python3 scripts/island-candidate.py install --review-dir "$ISLAND_REVIEW/adapter" --manifest-sha256 "$ISLAND_MANIFEST"
# STOP: separately approve binary/dry-run/current-palette generation before this:
python3 scripts/island-candidate.py prepare-startup --review-dir "$ISLAND_REVIEW/adapter" --manifest-sha256 "$ISLAND_MANIFEST"
# STOP: review the newly printed startup hash and approve wiring before using it:
python3 scripts/island-candidate.py install --review-dir "$ISLAND_REVIEW/startup" --manifest-sha256 NEW_APPROVED_STARTUP_HASH
# Reversal, separately approved and ordered startup first:
python3 scripts/island-candidate.py reverse --review-dir "$ISLAND_REVIEW/startup" --manifest-sha256 NEW_APPROVED_STARTUP_HASH
python3 scripts/island-candidate.py reverse --review-dir "$ISLAND_REVIEW/adapter" --manifest-sha256 "$ISLAND_MANIFEST"
```

Startup install is intentionally **not ready now**: its manifest does not exist
until transactional generation has passed the gate. Adapter reversal refuses
while the startup config differs from its original. Both reversals require exact
installed bytes/modes; later opacity or other user edits are preserved by refusal.
No helper applies palettes, rolls transactions back, removes generated CSS,
controls processes or changes notification ownership. Do not delete an output
still referenced by a running Island instance; startup reversal affects its next
ordinary launch, not its current selected path.

Each live write uses no-follow anchored directories, single-link/ownership/mode
checks, retained backups, same-directory staging, file fsync, final rechecks,
atomic replacement and directory fsync. ACLs/xattrs this helper cannot preserve
are refused. Uncooperative writers can still race the final check/rename; avoid
concurrent opacity/deployment/editing. fsync/power-loss errors require inspecting
retained hashes; do not retry or reverse blindly. No backups are deleted.

Public candidates are under [t470-island](t470-island); complete private desktop
configurations are not committed. Read-only notification-owner/runtime checks,
source snapshots and all live file checks are repeated before publication.
