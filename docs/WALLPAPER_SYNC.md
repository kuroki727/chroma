# Automatic wallpaper synchronization

`chroma wallpaper changed IMAGE` is an explicit **post-success** hook. It does
not set a wallpaper, poll files, start a service, install desktop imports, or
restart applications. Automatic synchronization is disabled by default. It uses
only adapters you already enabled and the existing transaction/recovery writer.

## Branch prerequisites and inspection

Inspected main: `54ebaba3e8da8b1432e1bcf31d8630cafe392480`. At implementation
time PRs [#4](https://github.com/kuroki727/chroma/pull/4),
[#5](https://github.com/kuroki727/chroma/pull/5),
[#6](https://github.com/kuroki727/chroma/pull/6),
[#7](https://github.com/kuroki727/chroma/pull/7), and
[#8](https://github.com/kuroki727/chroma/pull/8) were independent Draft PRs
based on main, all open and unmerged.

This feature branch reuses the complete commits from #4 (recovery/Kitty fixes)
and #5 (wallpaper extraction/contrast), retaining their regression tests. They
are prerequisites, not claims that those PRs were merged. Before merging this
feature, land/rebase those prerequisites through their own review. #6–#8 contain
separate opt-in desktop preparations and are not incorporated or required. The
small generated-Niri include refusal also identified in #7 is included here
because hook validation must never follow external includes; reconcile that
overlap when rebasing. #7's raw-string handling is not included.

Read-only inspection found `~/.local/bin/wall`, a shell picker that calls:

```sh
awww img "$dir/$file" --transition-type fade --transition-duration 1
```

That file was not changed. The provided wrapper and the optional edit below
make manager success the hook's trigger. Calling `awww` directly bypasses sync.

## Enable and disable

First build/install Chroma using the README. Review adapter generation and native
imports separately; enabling this hook neither enables adapters nor connects
applications. Existing main configs are preserved. Commands below are for later
explicit use; implementation/testing did not execute them against the desktop.

```sh
chroma init                              # keeps existing files
chroma wallpaper status
chroma wallpaper enable --dry-run       # preview Chroma TOML only
chroma wallpaper enable
chroma wallpaper changed "$HOME/Pictures/wallpapers/foggy_forest.jpg" --dry-run
```

Install the optional wrapper from the repository, at a new path (the `test`
prevents replacing an existing wrapper):

```sh
test ! -e "$HOME/.local/bin/chroma-wall" && \
    install -m 755 scripts/chroma-wall "$HOME/.local/bin/chroma-wall"
chroma-wall "$HOME/Pictures/wallpapers/foggy_forest.jpg" \
    --transition-type fade --transition-duration 1
```

`chroma-wall IMAGE [awww img options...]` invokes `awww img` and waits for its
successful exit before invoking the hook. An image path containing spaces or
shell metacharacters stays a single literal argument. Manager failure returns
its exit code and does not invoke Chroma. Hook failure is reported and returns
nonzero; the successfully selected wallpaper stays in place and the previous
theme is preserved/recovered. Manager exit success is the boundary: the wrapper
does not verify displayed pixels or wait for the fade to finish.

To retain the current Fuzzel picker, back up `~/.local/bin/wall` yourself and
replace only its final command with this reviewed line:

```sh
chroma-wall "$dir/$file" --transition-type fade --transition-duration 1
```

Alternatively retain its `awww` command and append `&& chroma wallpaper changed
"$dir/$file"` to that command. Do not put the hook before the manager or launch
both independently. Keep the hook in the foreground; simultaneous invocations
are supported without a persistent worker. Only one wrapper/hook is needed.

Disable synchronization immediately with:

```sh
chroma wallpaper disable --dry-run
chroma wallpaper disable
chroma wallpaper status
```

The wrapper can remain installed: subsequent hooks succeed as no-ops while
disabled, without opening the image or creating request/state files. Generated
outputs, native imports and the current theme remain in place. To disconnect
the picker, restore its original final `awww img ...` command. No desktop restart
is needed. Enable/disable edits preserve comments, unrelated TOML and file mode,
and use the existing journal; each successful change is independently undoable
with `chroma rollback --no-reload`.

## Generation, validation and reload

The hook initially supports **dark PNG/JPEG themes**. It calls #5's bounded,
deterministic OKLab extractor/generator directly, without saving or replacing a
user palette TOML. The active palette ID is `wallpaper-auto`, stored in the
normal manifest/backup with its full colors. No new dependency is added.
Identical images/settings produce identical palette/config bytes on the same
algorithm/toolchain. The image filename does not influence colors or identity.

Up to 16,384 stratified actual pixels are sampled with visible-alpha weights;
dominant populations and source hues guide accents, while neutral surfaces
have subdued saturation. Text roles meet 4.5:1 against three opaque surfaces,
ANSI entries meet 4.5:1 against the terminal background, and borders meet 3:1.
See [palette measurements and limits](WALLPAPER_GENERATION.md). These guarantees
do not certify translucent composition, every ANSI pair or custom UI mappings.

All enabled templates render and pass the existing palette/token/source-format
checks before staging. Generated Niri fragments additionally require isolated
`niri validate -c TEMP`; generated Fuzzel uses its noninteractive check when
available. Niri includes are refused, including in generated output. Main Niri
include trees are not followed or revalidated by the hook. Desktop wiring must
already have been independently reviewed. Other built-in formats retain their
existing renderer checks; they do not acquire universal native validators.

For additional checks, an adapter may explicitly configure a noninteractive
validator (also executed during wallpaper dry-run):

```toml
[adapters.my_adapter]
enabled = true
template = "templates/my-theme"
output = "generated/my-theme"
validate = ["/absolute/path/to/check-theme", "{file}"]
validation_timeout_ms = 3000
```

`{file}` names a private temporary copy of the exact generated bytes, never the
destination. Arguments are passed directly, with no shell expansion. Validators
must return zero, finish within their timeout, and leave input bytes unchanged.
They are trusted local executables: use genuine offline validators, not commands
that launch or reload desktop applications. This field is used by the wallpaper
workflow; ordinary `apply` retains its existing behavior. Missing, failing or
timed-out configured validators abort before managed files or history change.

Existing `general.auto_reload` and per-adapter `reload` argv remain authoritative.
Both are opt-in; `--no-reload` suppresses commands for one invocation. No reload
command is invented or enabled. Niri's established includes can update through
its native watcher; Waybar needs an established watcher or explicitly configured
reload. Fuzzel reads configuration on its next launch. Kitty/Foot runtime support
depends on existing native wiring and configured commands. File watchers may
react even with `--no-reload`. Duplicate hooks skip reloads as well as writes;
after a deliberate `--no-reload`, perform any desired reload explicitly.

### C++ Island

Read-only checks of `/home/kuroki/Projects/island/main.cpp` and `config.hpp`
confirmed a C++ application with literal `.flash`, `.dnd`, `.tile` and `.tile-on`
selectors. It is independent of Quickshell. The existing `desktop = "island"`
renderer handles those supported selectors using literal colors. It introduces
no CSS imports, variables or browser states.

Island loads defaults plus one stylesheet selected by `-s PATH` at startup.
`island ctl reload` rereads that same selected path; it cannot select a new path,
and successful datagram delivery is not an acknowledgment of stylesheet parsing.
Only configure `reload = ["island", "ctl", "reload"]` after Island already uses
the separately generated full stylesheet. Do not append repeated `-s` options
or use imports to combine stylesheets. No Island source change, launch, signal
or restart is part of this feature. The offline actual-parser harness remains
available in `scripts/check-island-parser.cpp`.

PR #8's fuller, contrast-aware Island profile remains a separate prerequisite
for that prepared desktop integration. The legacy mapper can put foreground
text on a bright primary-colored tile; palette contrast alone does not make
that UI pair readable. Review mappings/contrast before enabling a legacy Island
adapter. This feature does not silently opt into that pending profile.

## Concurrency, duplicates and recovery

`wallpaper.debounce_ms` defaults to 200 (range 0–2000). A short private request
lock serializes registration under `STATE/wallpaper-sync/latest`. Each invocation
waits for the existing theme lock and checks whether it is still the latest
registered event, both before extraction and before commit. Superseded workers
exit successfully. `wallpaper.wait_timeout_ms` defaults to 30000 (1–300000) and
bounds lock contention; expiry reports failure, rather than dropping it silently.

An event arriving during an already-started transaction can briefly show the
older palette; the newer worker waits and then applies its palette. Order is
registration order, not image timestamp or process creation order. The wallpaper
manager must emit hooks in its successful application order. Multiple monitors
share one global palette; choose one authoritative wallpaper hook.

Duplicate detection compares the generated palette and every enabled output's
actual bytes/mode under the theme lock. It skips a backup/transaction, replacement
and reload only when all match. A different path containing identical input is
also a duplicate. Rollback or a newly enabled adapter is not masked by an image
cache. There is no cache or schema change to recover separately.

Automatic operation refuses unmanaged/edited outputs even when the adapter's
manual `allow_overwrite` setting is true. Reconcile customizations explicitly in
templates or through the normal reviewed manual workflow. Existing safe-file
checks, input provenance, staging/fsync, pending journal, per-file atomic rename,
backups and conflict-aware restoration are reused. Failed extraction/validation
leaves the prior theme intact; non-dry hooks may leave coordination metadata.
Failed writes/reloads recover the prior files/state when possible. External edit
conflicts remain untouched, with pending state and backups retained.

For recovery, stop issuing hooks, retain all backups and inspect diagnostics:

```sh
chroma rollback --no-reload
```

Use the same `--config-dir`/`--state-dir` as the failed hook. This command also
works with broken local adapter configuration. Each successful rollback pops
one transaction, including enable/disable changes. Resolve conflicting edits
before retrying; never delete pending state to bypass recovery. Once recovered,
disable sync if you want to keep the restored theme instead of recoloring on the
next wallpaper event.

Dry-run creates only temporary validation files: no persistent request, state,
backup, palette, configuration or output writes and no reloads. With a concurrent
writer its snapshot can become stale, in which case it fails for retry. No
multi-file/global visual atomicity, external-command reversal, power-loss proof,
ACL/xattr restoration or conditional rename against uncooperative writers is
claimed; existing [transaction limits](SECURITY.md) apply. Backups remain unbounded.

No daemon replays requests after a killed worker, reboot or lock timeout. Reissue
the hook for the actual current wallpaper after recovery. The marker contains
only a request token and does not trigger work on its own. JPEG/PNG compressed
and decoded sizes remain bounded at 64 MiB and 16,777,216 pixels (5K supported);
other formats, ICC/HDR, tiny-feature saliency and algorithm-version byte stability
are outside the initial scope. Live appearance and daily-use reload behavior
require a later user-controlled desktop check.

## Verification executed

Executed on 2026-11-18 in the available Linux environment identifying as ThinkPad
T470, with Rust 1.100.0, Niri 26.04 and Fuzzel 1.15.0. All writes/applications
used temporary or feature-worktree paths; no live installation was performed.

| Check | Result |
| --- | --- |
| `cargo fmt --all -- --check` | Passed |
| `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings` | Passed |
| `cargo test --workspace --all-targets --all-features --locked` | 144 passed, 0 failed, 0 ignored |
| `cargo test --doc --locked` | Passed (no doc tests) |
| `cargo build --release --locked` | Passed |
| `git diff --check`, `sh -n scripts/chroma-wall` | Passed |
| Release hook with real 5120×2880 wallpaper | Six outputs generated/applied in isolation |
| Offline Niri, Fuzzel, Kitty, GTK3 CSS, actual Island C++ parser | Five passed |
| Release dry-run, duplicate invocation, rollback and identical reapply | Passed |
| Read-only live dependency snapshots | 41 files unchanged in hash/mode/UID/GID |

The 22 new hook tests cover valid PNG/JPEG, missing/corrupt/transparent images,
identical bytes at different paths, repeated changes, eight concurrent identical
invocations, coalesced requests under a held lock, a newer request during reload,
lock timeout, disablement while waiting, default disablement, enable/disable and
legacy-config compatibility, no-write dry-run, failed/mutating/timed-out validators,
invalid templates/Niri, failed reload restoration, SIGKILL with durable recovery,
conflicting edits and queue symlink refusal. Existing writer fault tests continue
to cover failure after the first, second and third actual output replacement.

Real input: `/home/kuroki/Pictures/wallpapers/foggy_forest.jpg`, SHA256
`c8bff4db842c59b882fae179170af89f75fc044651edadf754333b2278027ce5`.
One release apply with debounce/reloads disabled, including built-in Niri/Fuzzel
checks, took 0.339 seconds. This is one smoke observation, not a latency guarantee.
The Island parser fixture also included `.flash`, `.dnd`, `.tile` and `.tile-on`;
parser acceptance does not certify their visual contrast. No compositor, bar,
Island process, wallpaper manager or application window was launched/reloaded.
