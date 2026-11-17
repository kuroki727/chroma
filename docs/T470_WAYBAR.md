# T470 Waybar integration candidate

Preparation only. **The live stylesheet has not been installed or reloaded.**
This is a color-only migration, not a bar redesign. Layout and behavior stay
unchanged; the proposed colors intentionally follow the current Chroma palette.

## Repository and live baseline

The dedicated `feat/t470-waybar-integration` worktree starts at stable main
`54ebaba3e8da8b1432e1bcf31d8630cafe392480`. Its existing Waybar migration API is
sufficient. Neither PR #4's transaction patches nor PR #5's generator patches
are needed, copied or changed. Their original worktrees are preserved.

Read-only inspection on the physical Arch T470 found Waybar 0.16.0 running as
PID 764 with plain `waybar` arguments, launched by Niri. Its configuration is:

- `/home/kuroki/.config/waybar/config.jsonc`: bottom, 650 by 32, spacing 1;
  niri/workspaces, pulseaudio, backlight, network, BAT0 and BAT1. Click commands
  invoke the existing menu scripts. This file is never replaced by this procedure.
- `/home/kuroki/.config/waybar/style.css`: regular file, UID/GID 1000:1000,
  mode 0644; SHA256
  `a947a037df6f9f98f75fbb555af1e870b92d7672bd9d95861f0b862222ac4fbc`.
  Its bytes match the repository's existing desktop Waybar fixture.
- `/home/kuroki/.config/chroma/generated/waybar.css`: eleven GTK named color
  definitions. No current Waybar import or local Chroma definitions exist.
- `/home/kuroki/.config/chroma/config.toml`: built-in Waybar generator enabled;
  auto_reload false, no enabled Waybar reload command.
- `/home/kuroki/.config/chroma/adapters/desktop-island.toml`: the only adapter
  file; unrelated Island generation remains unchanged.
- `/home/kuroki/.local/state/chroma/state.toml`: current `boats-preview`, head
  `tx-18762-1794935644727889796`, no pending transaction.

No symlinks, hardlinks or extended attributes were found on the two Waybar input
files. Relevant user path units were absent. Accessible Waybar watcher metadata
did not identify either configuration inode as watched. Installed configuration
has no `reload_style_on_change` setting (default false).

**Known external writer:** `/home/kuroki/dotfiles/deploy.sh` installs its own
`waybar/style.css` over the live stylesheet. Do not run deployment concurrently
with installation/reversal. The inspected menu sources/scripts use Fuzzel and
do not rewrite Waybar styling. No active deployment writer was identified;
this inspection cannot prove that an arbitrary future process will not edit it.

## Candidate, mapping and preservation

The tracked candidate is [t470-waybar/style.css](t470-waybar/style.css). It adds:

```css
@import "/home/kuroki/.config/chroma/generated/waybar.css";
```

Nine literal color values are replaced, and every other source byte is retained.
There are no competing local definitions or new selectors; existing selector
specificity and declaration order are unchanged.

| Existing role / literal | Candidate value | Boats Preview | Rustwood |
| --- | --- | --- | --- |
| Bar background `#1c1714` | `@chroma_background` | `#0E0B09` | `#17130F` |
| Bar/module text `#d6c3a5`, active workspace `#ead7bd` | `@chroma_foreground` | `#F4F1EF` | `#E8DCC8` |
| Inactive workspace/disconnected `#796657` | `@chroma_muted` | `#938E8B` | `#A99B88` |
| Active underline `#b58a62` | `@chroma_primary` | `#DE9961` | `#B98252` |
| Warning `#c18b55` | `@chroma_warning` | `#ECD69A` | `#D6A45C` |
| Critical `#b45e48` | `mix(@chroma_error, @chroma_foreground, 0.15)` | approximately `#CA938D` | approximately `#BF6F62` |

Rustwood's unadjusted error color reaches only 4.12:1 on its background. The
reviewed critical-text expression retains 85% error and 15% foreground, raising
contrast without changing a palette or Chroma's general migration algorithm.
GTK3 supports `mix()` and named colors; browser CSS variables are not used.
The calculated values below use the installed GTK symbolic-color mix resolver
and unrounded sRGB components, not just the approximate displayed hex values.

| Contrast against the opaque bar background | Boats Preview | Rustwood |
| --- | ---: | ---: |
| Foreground, including active workspace | 17.45:1 | 13.64:1 |
| Muted / inactive / disconnected | 6.06:1 | 6.80:1 |
| Active underline | 8.25:1 | 5.61:1 |
| Warning text | 13.69:1 | 8.20:1 |
| Adjusted critical text | 7.51:1 | 5.00:1 |

Text meets 4.5:1 and the underline exceeds 3:1 in both reviewed palettes. The
bar remains opaque; both module/workspace `background: transparent` declarations
remain unchanged, so their explicit normal-state background is the bar itself.
No hover overrides or animations are added. GTK theme-provided hover/pressed
appearance is inherited as before and still needs later visual verification.
There are no additional local separator/border colors to migrate: `border: none`
and the active underline's 2px width remain unchanged.

JetBrainsMono Nerd Font, Symbols Nerd Font, monospace fallback, 13px sizing,
padding, margins, radii and min-height are preserved. JSON, module order, actions,
scripts, Niri, Island, Kitty, Fuzzel, wallpaper and transaction records are untouched.

## Offline verification

Executed in the dedicated worktree:

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
cargo build --release --locked
git diff --check
PYTHONDONTWRITEBYTECODE=1 python3 scripts/test-waybar-candidate.py
```

All five requested project checks passed. Rust results: **110 passed, 0 failed,
0 ignored** (108 stable-main tests plus two candidate tests). Python safety
results: **9 passed, 0 skipped**; these are also added to the existing CI workflow.

The Python tests use temporary fixture directories only. They cover a successful
install/reverse round trip, preserved permissions/backups/dependencies, later
edits, changed manifests/candidates/backups/dependencies, symlink/hardlink refusal,
ownership/mode/traversal refusal, concurrent operations, staging failure and
staging-name collision preservation, edits during staging and xattr refusal.

Native validation uses the existing headless `scripts/check-gtk-css.c`, compiled
against installed GTK3 3.24.52 using `pkg-config --cflags --libs gtk+-3.0`.
It does not call `gtk_init`, open a display or launch Waybar. Four complete styles
were parsed: the original, candidate with its real import, and isolated candidate
copies importing Boats Preview and Rustwood definitions. All exited 0 without
diagnostics. The live Boats definitions were verified against palette values;
both isolated definition files were rendered from the existing template. All
candidate references have definitions. Rust tests independently check exact
migration bytes and both palettes' role contrasts.

Waybar has no available standalone full-bar validation operation. GTK acceptance
does not validate runtime module behavior or certify visual appearance. No
Waybar window was launched and no visual or reload test is claimed.

## Private review bundle and guarded commands

On this T470 the isolated bundle is:

`/home/kuroki/Projects/chroma/local/waybar-integration/local/review`

Directory mode 0700; original, candidate, manifest, diff and color samples 0600.
It is ignored by Git. The original stylesheet backup is `original.css`, the
installation payload `candidate.css`, and the complete diff `candidate.diff`.
The manifest guards the live stylesheet and eleven dependencies: Waybar JSON,
Chroma configuration/adapter, generated Waybar colors, state and six transaction
records. It cannot be refreshed silently: changing its bytes requires a new
explicitly reviewed manifest hash.

Candidate SHA256:
`6dff0725f64c47e833e85593c0edf300e47baed4be394bcfe1ca4c57f9373e64`.

Manifest SHA256:
`beec69203c9f0e6c0c80d47a2e1c3a2bfa63e26a2e5f0e764efc57e7fbd6ea3c`.

Read-only pre-install check (executed successfully during preparation):

```sh
cd /home/kuroki/Projects/chroma/local/waybar-integration
PYTHONDONTWRITEBYTECODE=1 python3 scripts/waybar-candidate.py check \
  --review-dir /home/kuroki/Projects/chroma/local/waybar-integration/local/review \
  --manifest-sha256 beec69203c9f0e6c0c80d47a2e1c3a2bfa63e26a2e5f0e764efc57e7fbd6ea3c
```

**Not executed. Requires separate explicit installation approval:**

```sh
cd /home/kuroki/Projects/chroma/local/waybar-integration
PYTHONDONTWRITEBYTECODE=1 python3 scripts/waybar-candidate.py install \
  --review-dir /home/kuroki/Projects/chroma/local/waybar-integration/local/review \
  --manifest-sha256 beec69203c9f0e6c0c80d47a2e1c3a2bfa63e26a2e5f0e764efc57e7fbd6ea3c
```

After any later approved installation, verify SHA256, UID/GID 1000:1000, mode
0644 and native GTK parsing again. No command above reloads Waybar. Installation
replaces **only** `/home/kuroki/.config/waybar/style.css`, with a same-directory
staged write, file fsync, atomic replacement and directory fsync. The original
private backup is retained. Refusals include changed dependencies, unsafe paths,
symlinks in any path component, hardlinks, unexpected ownership/modes and xattrs
or ACLs that this minimal tool cannot preserve.

**Not executed. Requires separate reversal approval:**

```sh
cd /home/kuroki/Projects/chroma/local/waybar-integration
PYTHONDONTWRITEBYTECODE=1 python3 scripts/waybar-candidate.py reverse \
  --review-dir /home/kuroki/Projects/chroma/local/waybar-integration/local/review \
  --manifest-sha256 beec69203c9f0e6c0c80d47a2e1c3a2bfa63e26a2e5f0e764efc57e7fbd6ea3c
```

Reversal requires the active stylesheet to still match the exact candidate hash
and original permissions. Later user edits cause refusal; they are never replaced
by a stale original. Palette/state guards are intentionally not required for
reversal, allowing disconnection after a subsequent palette change. No Chroma
rollback command is needed: this stylesheet operation does not alter Chroma state.

## Reload policy, recovery and remaining limits

Installed Waybar documents `reload_style_on_change=false` by default. Its normal
SIGUSR2 action reloads/resets the bar; this configuration does not override that
action. A later approved reload is required to display the import, and later
palette updates need another reload while the watcher remains disabled. No
reload command has been executed or enabled. Native watching would cover imported
files and make future palette writes visible even with `--no-reload`; enabling
it requires a separate reviewed configuration change.

If a write, fsync or post-write check fails, stop and inspect the active hash and
retained bundle; do not retry installation or run Chroma rollback blindly. If
the active file is exactly the candidate, the guarded reversal above remains
available after approval. If it differs from both reviewed versions, preserve
it for manual reconciliation. Temporary files are cleaned on handled staging
failures; abrupt termination may leave a harmless `.chroma-waybar-*.tmp` file.
The tool does not automatically delete backups or recover after power loss.

The helper serializes operations using the same private review bundle, checks
inputs again immediately before replacement and rejects changed parent paths.
An uncooperative external writer can still race the final comparison/rename;
POSIX replacement is not a conditional hash compare-and-swap. Avoid concurrent
dotfiles deployment/editing. Future palettes need new contrast review: the
critical blend is verified for these two palettes, not every possible palette.
Inherited GTK states, font rendering and actual bar appearance await a separately
approved installation/reload and manual visual check.

References: [GTK3 named colors and mix()](https://docs.gtk.org/gtk3/css-overview.html),
[Waybar reload settings](https://github.com/Alexays/Waybar/blob/master/man/waybar.5.scd.in).
