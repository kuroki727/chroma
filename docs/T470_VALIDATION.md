# v0.2 T470 stabilization audit — 2026-11-17

Baseline fetched from `origin/main`: `54ebaba3e8da8b1432e1bcf31d8630cafe392480`
(PR #3 merge), clean checkout, version 0.2.0. Work is on
`audit/v0.2-real-hardware`; main was not changed.

## Confirmed defects

| Defect and root cause | Reproduction | Minimal correction |
| --- | --- | --- |
| Rollback opened an output parent before considering whether a newly created output was already absent. Removing that output and its parent left recovery permanently pending until the directory was recreated. | `rollback_accepts_an_already_absent_new_output_parent` failed with ENOENT before the fix. | Treat a missing parent as already restored only for a creation (`before = None`). Do not create directories. Permission/symlink errors and missing originals still fail and retain recovery data. |
| Kitty color scanning treated the first whitespace on a line as the key/value separator, so indentation hid directives and duplicate keys. | `kitty_indented_directives_preserve_whitespace_and_recolor_every_key` failed with “no recognized color keys”; native Kitty accepted the same indentation. | Find the separator after indentation and retain original byte offsets. Recognize all colors and duplicates while preserving whitespace, CRLF and unrelated settings. |

No CLI, palette format, transaction schema, dependency or architecture changes.
Six new Rust tests cover these defects, missing-original recovery, missing or
unparseable backups, every disabled adapter, and actual killed-process recovery
with a competing invocation. Other fault scenarios retain the existing tests.

## Accessible environment and discovered paths

Execution was local inside Codex's filesystem/network/process sandbox on an Arch
Linux host. DMI reports LENOVO `20HD0001RT`, **ThinkPad T470**; `uname` reports
`t470`, kernel `7.3.4-arch1-2`. This is evidence of T470 host execution, including
isolated native tests. The sandbox exposes only its own process namespace;
currently running desktop argv and on-screen results were not verified.

`XDG_CONFIG_HOME` and `NIRI_CONFIG` were unset. Inspection of the existing tree,
Niri launch/bind declarations and Island source established the following paths;
these are observations for this machine, not portable path discovery guarantees.

| Component | Observed path / status |
| --- | --- |
| Niri | `/home/kuroki/.config/niri/config.kdl`; startup launches Waybar and `/home/kuroki/.local/bin/island`, binds launch plain Kitty/Fuzzel |
| Waybar | `/home/kuroki/.config/waybar/config.jsonc`, `style.css`, `scripts/`; JSONC includes user click actions, none executed |
| Fuzzel | `/home/kuroki/.config/fuzzel/fuzzel.ini`; all ten colors; background alpha EE |
| Kitty | `/home/kuroki/.config/kitty/kitty.conf`; complete 16 ANSI colors |
| GTK3 | `/home/kuroki/.config/gtk-3.0/` exists, empty; no settings.ini or gtk.css |
| Island | `/home/kuroki/.config/island/config.jsonc`, `style.css`; defaults verified in `/home/kuroki/Projects/island/config.hpp`; accepts `-s` stylesheet argument, no CSS imports |
| Chroma | `/home/kuroki/.config/chroma/` contains a local `generated` palette; config.toml absent; `/home/kuroki/.local/state/chroma` absent |

Inspected desktop directories are user-owned UID/GID 1000, mode 0755 (GTK3 and
Chroma 0700); inspected config leaves are regular 0644 files, without symlinks.
No active imports connect Chroma. `adapters detect` reports setup-required for
Niri/Waybar/Fuzzel/Kitty and missing Foot/Quickshell. It inspects conventional
names only, does not discover arbitrary process flags or follow include trees.
Island is a dedicated desktop adapter, not Quickshell. GTK3 CSS parser acceptance
is supported by the harness; **there is no universal GTK3 settings adapter**.

Read-only commands: `doctor` correctly exits nonzero for missing config.toml;
`current` reports no applied palette; `validate` passes **7 palettes, 0 configured
adapters** (six embedded presets plus one local generated palette). That last
result alone says nothing about desktop composition.

Installed: rustc/cargo 1.100.0, Niri 26.04, Waybar 0.16.0, Fuzzel 1.15.0,
Kitty 0.50.0, GTK3 3.24.52, C/C++ compilers and pkg-config. Foot and Quickshell
are unavailable for native checks. Installed Island and its project executable
have matching SHA256 `930ffcdd06654bfe4e062d19da49a8a79506796a161636793cebe6de45c43349`.
Offline parser header SHA256:
`2bd8a8322f68a7f90fb8670ca033d035249ca0cf2a9d35b75ff3286a23f450af`.

## Implementation and CLI audit

`src/cli.rs` exposes `init`, `list [--names-only]`, `current`,
`apply [PALETTE] [--no-reload] [--dry-run]`,
`generate IMAGE [--mode dark|light] [--id ID] [--name NAME] [--output PATH] [--force]`,
`rollback [--no-reload]`, `validate [PALETTE]`, `doctor`,
`adapters list|detect|enable|disable`, and
`integrate --plan|--status|--apply --confirm|--rollback`.
Global overrides are `--config-dir` and `--state-dir`; adapter detection/enablement
and integration planning accept an explicit `--desktop-dir`. Enable/disable have
`--dry-run`, enable also has `--opaque`. Standard help/version flags remain.

Built-in fragments: Niri, Waybar, Fuzzel, Kitty, Foot, Quickshell JSON. Dedicated
review adapters: Niri, Waybar, Fuzzel, Kitty, Island, Swaylock. Custom trusted
adapters remain possible. Strict TOML loading records input snapshots and adapter
membership; unknown fields and duplicate adapters fail. Explicit `--config-dir`
isolates state unless `--state-dir` is also supplied. Outputs protect Chroma
inputs/state and declared main configs; symlink trees require explicit real paths.

`safe_fs` holds directory descriptors, refuses symlinks/hardlinks/unsafe ownership,
stages/fsyncs files, compares snapshots and renames in the same directory. Apply
stages every output before recording a durable backup and pending manifest, then
commits each output and final state. Failure restores matching before/after
snapshots; conflicts preserve external edits and retain an explicit pending state.
Rollback validates the backup and manifest endpoints before restoration. One
advisory lock covers operations sharing a state directory. Integration has a
separate fixed-allowlist, disabled-bundle journal sharing that lock.

Potentially destructive operations: authorized replacement of unmanaged outputs
(`allow_overwrite`), `generate --force --output`, adapter setup writes to Chroma
TOML, rollback replacement/removal of managed files, integration artifact removal,
and arbitrary explicitly configured reload executable side effects. `init`
creates missing files without overwriting. No backup pruning exists. Dry-runs
never run reloads. **`--no-reload` cannot suppress existing application watchers.**

## Verification and coverage

Baseline independently rerun: **108 passed, 0 failed, 0 ignored**.
Final Rust suite: **114 passed, 0 failed, 0 ignored**: 14 library unit tests,
20 adapter workflow, 13 CLI, 17 desktop integration, 9 generation, 9 palette and
template, 2 reload, 1 selector, 29 transaction tests. Main/doc test targets contain
zero tests. No native Niri branch was skipped on this machine.

| Command | Result |
| --- | --- |
| `cargo fmt --all -- --check` | Passed |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | Passed |
| `cargo test --workspace --locked` | 114 passed |
| `cargo build --release --locked` | Passed |
| `git diff --check` | Passed |

Reproduce native and error-injection checks from this repository:

```sh
python3 scripts/validate-desktop-copies.py --chroma target/release/chroma \
  --desktop-dir /home/kuroki/.config --island-source /home/kuroki/Projects/island
python3 scripts/check-enospc.py --chroma target/release/chroma
```

The native runner creates private temporary roots, copies inspected sources,
generates palettes there and disconnects display environment variables. All six
presets pass **30 native checks**: Niri's full composed copy, GTK3's provider for
migrated Waybar CSS/imports, Fuzzel's complete INI, Kitty's composed native parser
(with executable geninclude disabled), and Island's actual C++ parser offline.
Each preset also passes repeat apply and two rollbacks. Explicit test enablement
edits are restored before integration rollback; edited adapter TOML otherwise
correctly blocks bundle removal. Original source bytes, modes and ownership are
compared afterward. Nothing starts Waybar, Island, a compositor, launcher UI or
lock screen.

The ENOSPC runner compiles a test-only libc write interposer and sets LD_PRELOAD
only on isolated child invocations. **Five cases passed**: first output staging,
second output staging, backup staging, pending-manifest staging, final-manifest
staging after both output commits. Each verifies failure, original outputs and
manifest, temporary-file cleanup, and a later successful apply/rollback. A static
or incompatible binary fails the check rather than being reported as tested.

| Required scenario | Evidence / boundary |
| --- | --- |
| Success, before-first-write failure | Multi-adapter workflows; invalid template/late adapter preflight |
| Staging, backup, manifest write failure | Five ENOSPC injections; backup-directory permissions regression |
| Commit / partial commit failure | Existing actual-writer hooks after 1/2/3 replacements; target changes and symlink conflict tests; automatic restoration verified |
| Interruption and concurrency | New SIGKILL test during reload barrier, real competing CLI rejected, pending apply blocks new applies/current, command-free recovery succeeds; existing simulated partial-commit and every-bundle-boundary tests |
| Missing/corrupt backup | New missing/unparseable cases and existing structurally corrupt/endpoint tests; no output/manifest changes; retained/repaired records recover |
| Permission denied | Unsafe directory/record modes and read-only output parent rejected; missing-original rollback remains pending and retryable |
| Unexpected filesystem changes | Input/template/main-config/output/state edits, directory moves, symlinks/hardlinks checked; edited outputs preserved |
| Repeated rollback / external edits | History walked including repeated palette; terminal empty-history error; conflicting external edits retained; retry succeeds after explicit resolution |
| Adapter safety matrix | Shared filesystem engine tests apply to all writers; format-specific missing/malformed sources, unrelated-setting/alpha preservation and repeat rendering in workflow/desktop suites; new disabled test covers every built-in and desktop kind |

This is not an exhaustive cross-product of every adapter and filesystem fault.
Real exhausted disks, failing rename/fsync syscalls, power cuts, firmware behavior,
ACL/xattr restoration and adversarial same-user races were not physically tested.
Final compare-to-rename/unlink races with uncooperative writers remain; multiple
files and external reload effects cannot be globally atomic. Backup retention is
unbounded. Foot/Quickshell have structural tests only. GTK3 application appearance
and Swaylock remain manual; the local Swaylock source contains unsupported shell
continuation syntax and was not executed.

The actual Niri config contains raw KDL regex strings. `integrate --apply` refuses
these during conservative include-chain inspection. The production check was
kept intact. The native runner separately refuses include/escape chains, then
validates the inspected self-contained copy including its raw strings. A passing
`integrate --plan` does not imply the full bundle can be installed automatically.

## Manual live gate — pending explicit approval

**No active desktop configuration was changed, no live palette was applied, and
no desktop process was restarted, signalled or terminated.** Physical-host parser
and isolated transaction checks are complete; interactive visual/reload/daily-use
validation is pending. No logout, reboot or backup deletion is part of this plan.
Approval must name the files/activation steps below; approval of generation alone
does not authorize desktop wiring or process control.

1. Reinspect in the actual desktop session; confirm paths and process arguments
   before any write. These commands are read-only:

   ```sh
   cd /home/kuroki/Projects/chroma
   ./target/release/chroma doctor
   ./target/release/chroma adapters detect
   ps -C niri,waybar,kitty,island -o pid,args
   ./target/release/chroma integrate --plan --desktop-dir /home/kuroki/.config
   ```

   Expected today: missing Chroma config diagnostic, setup-required integrations,
   no enabled adapters. Review the complete plan, particularly Waybar's diff.
   An audit-session copy is in ignored `local/t470-integration-plan.txt`.

2. **After approval**, make a private pre-test backup. Keep this path written down:

   ```sh
   umask 077
   backup_dir="$HOME/chroma-t470-backup-$(date +%Y%m%d-%H%M%S)"
   mkdir -m 700 "$backup_dir"
   tar -C "$HOME" -cpf "$backup_dir/config.tar" .config
   if test -d "$HOME/.local/state/chroma"; then
     tar -C "$HOME/.local/state" -cpf "$backup_dir/chroma-state.tar" chroma
   fi
   sha256sum "$backup_dir/config.tar" > "$backup_dir/SHA256SUMS"
   tar -tf "$backup_dir/config.tar" > "$backup_dir/contents.txt"
   ```

   Do not continue if backup creation or archive listing fails. The archive
   contains personal settings: retain it privately, do not attach it to the PR.

3. Approved generation uses explicit, persistent paths. Stop if config.toml now
   exists; review its contents rather than replacing it. `init` preserves existing
   palettes, including this machine's local generated preset.

   ```sh
   chroma_bin=/home/kuroki/Projects/chroma/target/release/chroma
   chroma_config=/home/kuroki/.config/chroma
   chroma_state=/home/kuroki/.local/state/chroma
   test ! -e "$chroma_config/config.toml" || exit 1
   "$chroma_bin" --config-dir "$chroma_config" --state-dir "$chroma_state" init
   for adapter in niri waybar fuzzel kitty; do
     "$chroma_bin" --config-dir "$chroma_config" --state-dir "$chroma_state" \
       adapters enable "$adapter" --desktop-dir /home/kuroki/.config --dry-run || break
   done
   ```

   Review all four previews before running the approved setup commands:

   ```sh
   for adapter in niri waybar fuzzel kitty; do
     "$chroma_bin" --config-dir "$chroma_config" --state-dir "$chroma_state" \
       adapters enable "$adapter" --desktop-dir /home/kuroki/.config || break
   done
   ```

   Stop on any failed command. Defaults keep auto_reload false; Foot/Quickshell stay disabled.
   Island requires this explicit custom desktop adapter (create only if absent):

   ```sh
   python3 - <<'PY'
   from pathlib import Path
   p = Path('/home/kuroki/.config/chroma/adapters/desktop-island.toml')
   with p.open('x') as f:
       f.write('enabled = true\ndesktop = "island"\n'
               'config_path = "/home/kuroki/.config/island/style.css"\n'
               'output = "generated/desktop-island.css"\n')
   p.chmod(0o600)
   PY
   "$chroma_bin" --config-dir "$chroma_config" --state-dir "$chroma_state" validate
   "$chroma_bin" --config-dir "$chroma_config" --state-dir "$chroma_state" \
     apply rustwood --dry-run --no-reload
   "$chroma_bin" --config-dir "$chroma_config" --state-dir "$chroma_state" \
     apply rustwood --no-reload
   ```

   Expected: five separate generated outputs, durable private backup/state,
   original desktop files unchanged. Initially no desktop imports exist, so this
   generates files only. Do not set allow_overwrite to bypass a conflict.

4. **Separate approval of reviewed desktop wiring is required.** Prepare copies
   in `$backup_dir`, edit only those copies, show `diff -u ORIGINAL COPY`, and
   validate before installing any candidate. Use `vi "$backup_dir/review-FILE"`
   or the user's editor. Changes to review:

   | File | Exact connection / preservation requirement |
   | --- | --- |
   | Niri config.kdl | Append `include "/home/kuroki/.config/chroma/generated/niri.kdl"` once at top level. Preserve every other byte. Validate the full candidate with `niri validate -c "$backup_dir/review-niri.kdl"`. Installing the candidate triggers Niri's existing watcher; no restart. |
   | Waybar style.css | Use the plan's color-only diff, changing its import to `@import "/home/kuroki/.config/chroma/generated/waybar.css";`. Keep JSONC, scripts, font, padding, geometry and transparent backgrounds. Validate the candidate with the GTK harness compiled below. |
   | Fuzzel fuzzel.ini | Move the reviewed ten local [colors] entries out of the candidate and set `[main] include=/home/kuroki/.config/chroma/generated/fuzzel.ini`. Keep font, terminal, geometry and border settings. Validate with `fuzzel --check-config --config "$backup_dir/review-fuzzel.ini" --log-no-syslog`. |
   | Kitty kitty.conf | Append `include /home/kuroki/.config/chroma/generated/kitty.conf` once, last. Validate the candidate through the checked-in native Kitty parser command below; existing terminal processes need separate user action for visual changes. |
   | Island startup in Niri | Only after separate review, add `"-s" "/home/kuroki/.config/chroma/generated/desktop-island.css"` to its existing spawn-at-startup argv. Validate full Niri candidate again. This takes effect at the next ordinary session start; do not start a second Island or restart the current one. config.jsonc stays untouched. |
   | GTK3 | No global theme/settings change. CSS-provider validation is not a global GTK appearance test. |

   Native candidate checks (all offline):

   ```sh
   cc scripts/check-gtk-css.c -o "$backup_dir/check-gtk" $(pkg-config --cflags --libs gtk+-3.0)
   "$backup_dir/check-gtk" "$backup_dir/review-waybar.css"
   kitty +runpy 'import runpy,sys; sys.argv=sys.argv[1:]; runpy.run_path(sys.argv[0],run_name="__main__")' \
     scripts/check-kitty-config.py "$backup_dir/review-kitty.conf"
   c++ -std=c++17 -I /home/kuroki/Projects/island scripts/check-island-parser.cpp \
     -o "$backup_dir/check-island"
   "$backup_dir/check-island" "$chroma_config/generated/desktop-island.css"
   ```

   Approve each displayed diff before installing that candidate. Never replace a
   whole application config with a colors-only fragment. No restart/kill/reload
   command is authorized by this document. Waybar/Kitty/Island live reload may
   need a separately approved action or the next ordinary application/session
   launch. Niri's automatic watching makes the first candidate installation live.

5. After approved wiring, run these exact palette checks, observing the desktop
   after each command. Imports/watchers can make `--no-reload` visibly disruptive:

   ```sh
   "$chroma_bin" --config-dir "$chroma_config" --state-dir "$chroma_state" apply nord --dry-run --no-reload
   "$chroma_bin" --config-dir "$chroma_config" --state-dir "$chroma_state" apply nord --no-reload
   "$chroma_bin" --config-dir "$chroma_config" --state-dir "$chroma_state" current
   "$chroma_bin" --config-dir "$chroma_config" --state-dir "$chroma_state" apply nord --no-reload
   "$chroma_bin" --config-dir "$chroma_config" --state-dir "$chroma_state" rollback --no-reload
   "$chroma_bin" --config-dir "$chroma_config" --state-dir "$chroma_state" rollback --no-reload
   ```

   Expected: current=nord after apply; same-palette apply is byte-identical; first
   rollback remains nord, second returns rustwood. Check Niri borders/background,
   Waybar labels/workspaces/battery warning contrast, Fuzzel alpha/selection,
   Kitty foreground/ANSI selection, and Island clock/labels/progress at its next
   ordinary launch. Verify fonts, bindings, geometry, click actions and Island
   behavior remain intact. Record each component as observed/pass, failure, or
   pending; parser success cannot substitute for observation. Do not induce
   crashes, disk pressure or process termination on the live desktop.

## Rollback and recovery for the approved live test

For an ordinary palette reversal or interrupted apply, retain the same paths:

```sh
/home/kuroki/Projects/chroma/target/release/chroma \
  --config-dir /home/kuroki/.config/chroma \
  --state-dir /home/kuroki/.local/state/chroma rollback --no-reload
```

Expected: previous managed files/state restored, or explicit conflict with pending
record and backup retained. This works with broken/missing adapter TOML/templates.
Each successful call pops one history entry, including setup entries. Never repeat
blindly after a “files restored, reload failed” message. Never delete transactions,
pending state or lock files to force progress.

Chroma rollback does **not** undo manual application imports/startup edits or
remove the manually created Island adapter. Disconnect reviewed imports before
popping the transaction that originally created their fragments; otherwise a
watcher may observe a missing include. Before unwiring/removing generated
outputs, compare the pre-test archive against current files. Extract into a
private review directory, never over the whole live tree:

```sh
mkdir -m 700 "$backup_dir/restore-review"
tar -C "$backup_dir/restore-review" -xpf "$backup_dir/config.tar"
diff -u "$backup_dir/restore-review/.config/niri/config.kdl" /home/kuroki/.config/niri/config.kdl
```

Repeat diffs for Waybar/Fuzzel/Kitty/Island/Chroma as applicable. Reverse only the
approved test edits after saving any later user changes; show/approve that diff
before installing it. Niri restoration also triggers its watcher. Do not wholesale
restore `.config` or copy an old state manifest over newer history. If state or a
backup is missing/corrupt, first preserve a private copy of all current records
and outputs, inspect retained transaction snapshots, and recover only reviewed
files. Resolve permission/ownership issues explicitly; do not chmod unrelated
settings. On a conflict, preserve the external edit separately and restore the
exact expected bytes **and mode** only after review, then retry rollback. Lost
backup contents cannot be reconstructed automatically. Keep all pre-test and
transaction backups; deletion is outside this task.
