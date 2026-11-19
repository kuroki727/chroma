# High-resolution wallpapers and C++ Island

PR #8 on `feat/t470-island-integration` now targets `main`, after the normal
merges of #4 and #5 and the squash merge of #9. The conflict-resolution update
merges main `edaba34c7236e16639e7cb3276f25afd44c9385d` without rewriting the
published branch. Main's tree exactly matches the final #9 tree; resolving the
four conflicting files preserves the previous #8 source tree byte-for-byte.
The PR diff therefore contains only Island/high-resolution additions, with no
reapplication of prerequisite changes. Copied prerequisite commits already in
the published #8 ancestry remain historical ancestors of this ordinary merge.

This merge preparation uses isolated tests only and does not change the live
T470 configuration. The activation evidence below records earlier work. The
[Island preparation report](T470_ISLAND.md) describes commit 317025f.

## Cause and decoding design

The old extractor opened a bounded input, read headers, then rejected more than
16,777,216 pixels or 64 MiB decoded output **before** fully decoding and sampling.
Consequently 6016×4016 photos and 8K wallpapers failed; even 5K 16-bit RGBA could
exceed the byte limit. Resizing after `DynamicImage::from_decoder` cannot fix
that allocation problem. The installed hotkey log contains the exact reported
pixel-limit error.

`src/palette/sampling.rs` now supplies the same bounded RGBA samples to the
existing histogram, OKLab clustering and palette generator:

* Compressed files remain limited to 64 MiB, frozen before codec processing.
  Nonregular inputs are opened nonblocking and refused. Magic bytes select
  PNG/JPEG; filenames do not select codecs.
* Nonzero dimensions, <=16384 on each axis and <=8192×4320 pixels bound decoding
  work. The area is explicitly DCI 8K, including UHD 8K, Retina 6K and 5K; it is
  independent of a full-frame allocation budget. Larger areas fail before pixel
  decoding. Narrow panoramas retain the previous per-axis allowance.
* PNG uses the locked `png` codec's scanline API with a 16 MiB internal allocation
  budget. It expands palette/subbyte formats but retains 16-bit precision until
  sample conversion. Adam7 passes are traversed in their actual spatial order;
  no full-frame reconstruction buffer is allocated. Only the existing <=128×128
  deterministic strata are retained. Colors, alpha thresholds/weights and u16
  rounding match the previous native-pixel extraction exactly. All rows and
  trailing CRC/IEND data must succeed before extraction can return. Text and ICC
  chunks are ignored; APNG is explicitly refused instead of sampling a subframe.
* JPEG within the old pixel/output budgets uses the same `image`/zune codec and
  identical positions, preserving ordinary-image behavior. Its headers are read
  without pixel decoding. Larger RGB/grayscale JPEG uses `jpeg-decoder`'s 1/8 DCT
  decoding, then samples the reduced image. No full RGB original is allocated.
  Block averaging preserves broad populations/hues; small details may differ.
  Two header readers can be used, but pixel decoding occurs only once.

The locked `image` 0.25.9 JPEG wrapper uses zune-jpeg, which exposes no reduced
DCT/scanline output interface. PNG's `ImageDecoder::read_image` similarly expects
a frame, motivating direct codec use. See the [JPEG scaling API](https://docs.rs/jpeg-decoder/0.3.2/jpeg_decoder/struct.Decoder.html#method.scale)
and [PNG scanline API](https://docs.rs/png/0.18.1/png/struct.Reader.html#method.next_row).
Both added dependencies are pure Rust; PNG was already transitive. Rayon is
disabled for the added JPEG dependency; it can still use its built-in component
workers.

**Unavoidable JPEG limitation:** progressive JPEG retains full-resolution DCT
coefficients even when output is reduced. The decoder's output limit alone does
not constrain those coefficients. Chroma separately checks conservative
32-pixel-padded coefficient storage **plus worst-case queued row copies**
(`padded_width * padded_height * channels * 4 <= 416 MiB`), while reduced output
remains <=64 MiB. High-resolution CMYK/lossless JPEG is refused. The area bound
also bounds ordinary JPEG internal buffers. These are storage bounds, not a hard
RSS/CPU deadline: compressed input, metadata, planes, threads and allocator
overhead add memory. Malformed files may still consume decode work up to these
limits. No OS resource sandbox or broad codec fuzz campaign is claimed.

Measured release process peak RSS (Linux `wait4`, this T470, synthetic fixture or
local image; not a universal maximum):

| Input | Peak RSS | User CPU |
| --- | ---: | ---: |
| 8192×4320 RGB 4:4:4 progressive JPEG, synthetic two-color bars | 216920 KiB | 0.634 s |
| `falg.jpg`, 6016×4016 baseline JPEG | 47708 KiB | 0.514 s |
| `Eibsee, Grainau.png`, 3840×2160 PNG | 19032 KiB | 0.170 s |

ICC/HDR/wide-gamut color management remains unsupported. Inputs are treated as
sRGB. The old and new installed binaries produced byte-identical palette TOML
for `Eibsee, Grainau.png`; explicit unit comparisons also cover 16-bit/alpha and
Adam7 PNG sample equivalence.

## Island architecture and live activation

Actual source inspected: `/home/kuroki/Projects/island`, an unversioned local
tree, unchanged by this work. The local source-tree and installed Island
binaries both have SHA256
`930ffcdd06654bfe4e062d19da49a8a79506796a161636793cebe6de45c43349`.
Inspected `config.hpp` SHA256:
`2bd8a8322f68a7f90fb8670ca033d035249ca0cf2a9d35b75ff3286a23f450af`;
`main.cpp` SHA256:
`91d7ddf59a975cedcdfc9b24d4e09dc50071e8ca1e7046ad0395d2527f745321`.
Binary equality is not proof of a reproducible source build.

Island is C++, independent of Quickshell. It loads embedded defaults plus one
stylesheet. PR #8's opt-in `desktop="island"`, `island_readable=true` adapter
renders existing CSS into `generated/desktop-island.css`, preserving non-color
declarations and adding supported color roles absent from the source. Only the
actual simple selectors/properties are used, including `.flash`, `.dnd`, `.tile`
and `.tile-on`. Output is standalone literal CSS: no imports, variables, browser
states, JSON behavior changes or geometry changes. Its existing contrast profile
accounts for Island's actual hover overlay, shared foregrounds and slider roles.

Island's `ctl reload` sends `reload` to `$XDG_RUNTIME_DIR/island.sock` over a Unix
datagram socket; SIGUSR2 also sets its reload flag. `App::reload()` rereads the
original selected CSS/JSON paths. No file watcher or IPC retargeting exists.
The client reports delivery failure, not a parse acknowledgment. Rust rendering
and offline native parsing therefore validate the CSS before use. Native checker
warnings now count as errors, since Island itself can warn and return success.

The T470 was activated on 2026-11-19 after native parsing and full Niri validation:

```toml
# ~/.config/chroma/config.toml
[general]
auto_reload = true

# ~/.config/chroma/adapters/desktop-island.toml
enabled = true
desktop = "island"
config_path = "/home/kuroki/.config/island/style.css"
output = "generated/desktop-island.css"
island_readable = true
reload = ["/home/kuroki/.local/bin/island", "ctl", "reload"]
```

```kdl
spawn-at-startup "/home/kuroki/.local/bin/island" "-s" "/home/kuroki/.config/chroma/generated/desktop-island.css"
```

The existing no-argument PID 770 was stopped gracefully and allowed to exit
before Niri spawned PID 22236 with that exact `-s` path. Exactly one Island was
verified, and its notification bus ownership resolves to PID 22236. The startup
edit ensures subsequent sessions use the same path; Niri was not restarted.
Island's singleton lock is retained. A one-time Island restart necessarily
reinitializes its in-memory UI/notification history; subsequent updates use reload.

The latest wallpaper (`sea.png`) was kept throughout. The initial activation
guard noticed a wallpaper change during preparation and refused to write; the
candidate was refreshed before activation. Only Island's generated CSS changed;
Niri/Waybar/Fuzzel/Kitty outputs stayed byte-identical. Source Island CSS/JSON
stayed byte-identical, and the actual parser confirmed unchanged font sizes and
border width. This verifies startup/IPC/files programmatically, **not physical
visual appearance**. No live high-resolution wallpaper switch is claimed.

Private backup, retained under the ignored worktree-local directory:
`/home/kuroki/Projects/chroma/local/island-integration/local/high-resolution/live-backup-20261119-085510`.
It contains original Chroma configuration/state/binary, Niri configuration and
Island configuration. Do not blindly restore an old state snapshot after later
transactions; use Chroma rollback for generated palette changes.

The installed release SHA256 is
`ea2d2cfc836558f08b2c05b825f783cd5fae4971519468f52e195b747ae8c8d6`.
New installations retain optional adapters and `auto_reload=false` defaults.

## Transactions and wallpaper synchronization

The inspected `~/.local/bin/wall` retains its Fuzzel picker and fade, with one
runtime `flock` held across successful `awww img` and `chroma wallpaper changed`.
The Niri hotkey's diagnostic wrapper records errors. Neither was modified.
Calling `awww` directly bypasses the hook; wrappers must serialize manager plus
hook together to maintain event order. Chroma does not set/revert wallpaper.

The shared Chroma writer lock, debounce/latest-event token, cancellation epoch,
input snapshots, validation, durable journal and checked atomic file replacement
remain in place. All outputs stage before replacement. Decode/validation failure
leaves the active palette intact; reload/write failure uses existing checked
rollback, preserving external edits and reporting recovery failures. Unchanged
adapter bytes now skip reloads in both commit and rollback. Identical hooks still
skip the entire transaction. File replacement is atomic per file; simultaneous
visual changes across processes are not guaranteed, and watchers can briefly
observe output later restored after a failure.

Waybar already watches stylesheet changes; Fuzzel reads on launch; Kitty retains
its existing include/watcher arrangement. The active machine has no configured
GTK stylesheet adapter. The five-adapter **isolated** fixture verifies GTK through
the existing generic template mechanism using GTK named-color definitions; no
GTK/libadwaita-wide recoloring or active GTK integration is claimed.

## Validation results

Merge preparation reran formatting, Clippy and all 164 Rust tests with all
workspace targets/features enabled, plus doc tests and the locked release and
example builds. All passed. The 12 guarded-writer tests, 10 mocked-wallpaper-hook
checks, native stylesheet contrast/size checks, native startup/reversal fixture,
five-adapter actual-client private-socket fixture and five injected ENOSPC cases
also passed. Config, state and IPC paths for these fixtures were temporary;
no live T470 configuration or process was changed by merge preparation.

* `cargo fmt --all -- --check`: passed.
* `cargo clippy --all-targets --locked -- -D warnings`: passed.
* `cargo test --locked`: **164 passed, zero failed/ignored**.
* `cargo build --release --locked`: passed; renderer example also built.
* `python3 scripts/test-island-candidate.py`: 12 passed.
* `scripts/check-wall-hook.py --self-test`: 10 checks passed with mocked desktop commands.
* `scripts/test-island-native.py`: real isolated generation/startup/reversal passed.
* `scripts/check-wallpaper-island.py`: real Chroma plus actual Island parser/client,
  private socket, Waybar/Fuzzel/Kitty/GTK/Island outputs, duplicate suppression,
  invalid decode, rollback and real failed IPC send recovery passed. No desktop
  application is launched by either fixture. Private-socket testing required
  escaping the tool sandbox; native Island source is not assumed present in CI.
* Portable regression coverage includes 4K/5K/6K/UHD-8K/DCI-8K PNG, 16-bit RGBA,
  Adam7/alpha equivalence, 6K/8K baseline and 8K progressive JPEG, population/hue
  retention, malformed/truncated data/CRCs, absurd/zero dimensions, unsupported
  formats, edited outputs, adapter validation/reload failures, injected writes,
  concurrent/coalesced hooks, manual-operation cancellation and existing recovery.
* Read-only dry-runs of `falg.jpg`, `house.jpg`, `leavs.jpg` (6016×4016) passed in
  the staged five-adapter configuration. Private files only; no wallpaper switch.
* Live: single process/startup args/notification ownership; successful reload
  delivery with stable PID; duplicate current-image hook left manifest unchanged;
  invalid-image hook failed with unchanged manifest; wallpaper remained `sea.png`.
  Island has no reload acknowledgment; no log/visual proof of rendered pixels is
  claimed. Failure/recovery/concurrency scenarios use isolated fixtures, not
  deliberate failures against the user's live configuration.

## Exact T470 verification commands

```sh
cd /home/kuroki/Projects/chroma/local/island-integration
cargo fmt --all -- --check
cargo clippy --all-targets --locked -- -D warnings
cargo test --locked
cargo build --release --locked

c++ -std=c++17 -I /home/kuroki/Projects/island scripts/check-island-parser.cpp -o /tmp/chroma-check-island
/tmp/chroma-check-island "$HOME/.config/chroma/generated/desktop-island.css" --dump
python3 scripts/check-wallpaper-island.py target/release/chroma /tmp/chroma-check-island "$HOME/.local/bin/island"

"$HOME/.local/bin/chroma" wallpaper status
"$HOME/.local/bin/chroma" wallpaper changed "$HOME/Pictures/wallpapers/falg.jpg" --dry-run
"$HOME/.local/bin/chroma" wallpaper changed "$HOME/Pictures/wallpapers/house.jpg" --dry-run
pgrep -a -x island
busctl --user call org.freedesktop.DBus /org/freedesktop/DBus org.freedesktop.DBus GetConnectionUnixProcessID s org.freedesktop.Notifications
awww query
```

For the actual switch test, run `~/.local/bin/wall` (or Mod+Shift+W), select
`falg.jpg`, then `sea.png`, then repeat `sea.png`. Check Island, Waybar and a newly
opened Fuzzel/Kitty visually; inspect `tail -80 ~/.local/state/chroma/wall-hotkey.log`
and `pgrep -a -x island`. The same Island PID should survive palette updates.
Do not run an applying post-success hook for an image the manager has not selected.

To disable just explicit reload commands, set `general.auto_reload=false`;
to disable automatic palette generation, use `chroma wallpaper disable`.
Existing watchers can still react to file replacements with reloads disabled.

## Files in this update

* `Cargo.toml`, `Cargo.lock`: direct scanline PNG/reduced-DCT JPEG dependencies.
* `src/palette/sampling.rs`, `src/palette/{generator,mod}.rs`: bounded decode and
  existing extractor handoff.
* `src/engine/{apply,rollback}.rs`: skip unchanged adapter reloads.
* `scripts/check-island-parser.cpp`, `scripts/check-wallpaper-island.py`:
  strict actual-parser checks and isolated five-adapter/IPC fixture.
* `tests/{generation,high_resolution,wallpaper_sync}.rs`,
  `tests/fixtures/{8k-progressive.jpg,interlaced-rgba.png,IMAGE_FIXTURES.md}`:
  portable image, transaction/reload regressions and synthetic fixture provenance.
* `docs/t470-island/desktop-island.toml`: explicit real Island reload argv example.
* `README.md`, `docs/{SECURITY,T470_ISLAND,WALLPAPER_SYNC,HIGH_RES_ISLAND}.md`:
  current bounds, architecture, historical-report links and validation evidence.

The existing PR #8 renderer in `src/integration/island.rs`, adapter dispatch/config
flag and guarded preparation helpers are reused. Wallpaper synchronization,
transaction fixes and their tests come from the merged-in #9 branch. Local
activation artifacts/backups and personal wallpapers are ignored and not committed.
