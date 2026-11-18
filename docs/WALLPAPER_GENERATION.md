# Wallpaper palette audit and evaluation — 2026-11-17

Base: stable main `54ebaba3e8da8b1432e1bcf31d8630cafe392480`, independently
verified at **108 passing tests**. Branch: `feat/wallpaper-palette-quality` in a
separate worktree. The existing audit branch and Draft PR #4 are preserved.
Only generator logic, its regression tests and offline reporting/documentation
change. No dependency, CLI syntax, palette schema, preset, adapter or transaction
implementation changes.

## Audit and confirmed weaknesses

The existing generator already used sorted weighted OKLab k-means, bounded
PNG/JPEG decoding, deterministic initialization and gamut reduction. It was not
blindly choosing a histogram mode. The principal defects were downstream:

| Confirmed weakness | Reproducible evidence | Correction |
| --- | --- | --- |
| Grayscale and very muted images were assigned hue 220, chroma >=.08 and a +75° secondary fallback. | Gray and muted controlled fixtures produced exactly the same blue/purple accents. The new neutral-fidelity test failed on baseline `#050F13` background saturation. | Tone derives from the image mean; neutral source accents remain neutral. Single-hue sources vary lightness/chroma without inventing a complementary hue. |
| Secondary hue cutoff >=45° discarded smaller, legitimate source hue differences. | Forest's secondary became cyan, brown's became green, and sky's became purple despite those colors being absent from the fixtures. | Compare source chromatic coordinates and choose an actual second family. |
| Thumbnail interpolation could mix hidden transparent RGB into visible colors; aspect-preserving thumbnails lost sampling capacity on thin images. | A green/transparent-magenta checkerboard made the baseline primary pink `#E984EF`; it is now green `#86B693`. Thin-axis tests verify 50/50 coverage. | Independently stratified axes sample actual native-format pixels; deterministic within-cell offsets reduce periodic center aliasing. Retained alpha weights coverage. |
| Unweighted farthest seeding could spend clusters on outliers; final representatives were not merged or exposed. | A 6.25% red accent plus a single blue noise pixel now preserves green/red and rejects blue; adjacent near-equal colors merge. | Population-aware seeding, weighted Lloyd updates, final count reassignment, .04 OKLab merge threshold and <0.5% coverage rejection. |
| A 3:1 accent/status target was insufficient for adapters using these colors as small text; borders had no contrast target. | Real wallpaper minimum text-role contrast was 4.22 dark / 3.74 light; border minimum was 1.68 / 2.09. | Enforce 4.5:1 for foreground, muted, primary, secondary and statuses against all three surfaces; 3:1 for borders. |
| Generated ANSI map was empty, invoking generic fallback hues and primary+60° “magenta”. | Forest ANSI5 became cyan; ANSI0 equaled background (1:1). | Emit all 16 ANSI values. Keep conventional hue windows, select nearby source hues, derive saturation from the scene, and enforce 4.5:1 against terminal background. |

All existing tests remain, including resource limits, FIFO/corrupt input rejection,
PNG/JPEG determinism, 5K support and single-pixel black/white/RGB. Eight new tests
cover scene hue fidelity and contrast, grayscale, extreme dimensions, outliers,
duplicate merging, hidden RGB, ANSI identities and all desktop adapter formats.

## Algorithm and boundaries

Sampling is <=128 positions on each axis (<=16,384 samples), with no full-size
RGBA conversion. Alpha below 128 remains excluded; other samples receive their
alpha byte as a weight. Up to 12 OKLab centers and 24 Lloyd iterations bound work.
Clusters within .04 OKLab merge; coverage below 0.5% is treated as noise.
Representatives sort by population with explicit numeric tie handling.

The primary score combines population and chroma with a neutral noise floor.
Its source hue is preserved. Secondary selection requires chromatic separation,
not an arbitrary angular gap. For one family, secondary keeps its hue and changes
lightness/chroma. Neutral tint/chroma come from the weighted scene, not a fixed
blue fallback. Dark/light backgrounds use separate ranges influenced by source
lightness; their surfaces preserve ordering. Gamut mapping reduces chroma at fixed
hue/lightness; contrast adjustment moves lightness toward the appropriate extreme.
Status and ANSI colors use conventional identity windows and source hues inside
those windows. **Dark-mode ANSI0 is now readable gray**, not invisible background
black; this is an intentional accessibility tradeoff. Light-mode “bright” ANSI
variants become darker for emphasis/readability.

This reuses the existing [OKLab/OKLCH conversion](https://bottosson.github.io/posts/oklab/).
The [4.5:1 text](https://www.w3.org/WAI/WCAG22/Understanding/contrast-minimum)
and [3:1 UI](https://www.w3.org/WAI/WCAG22/Understanding/non-text-contrast) targets
apply to the documented opaque sRGB pairs. They do not certify a whole desktop:
translucency, compositing, application overrides and colored ANSI backgrounds can
change contrast. ICC/HDR conversion is still unsupported. Tiny features can be
missed by bounded sampling or intentionally dropped; this is not a saliency model.
Same image/settings/toolchain reproduce the same palette; output across algorithm
versions is not guaranteed identical. Existing hand-written palettes and presets
retain their previous validation/fallback behavior.

## Controlled before/after comparison

Each 128×96 fixture uses the four explicit RGB colors and deterministic regions
in `examples/wallpaper_quality.rs`. This table shows dark primary / secondary:

| Scene | Baseline | Improved |
| --- | --- | --- |
| forest | #8BC097 / #64BEE2 | #8DB496 / #BED4C8 |
| brown | #DBA383 / #A1BD76 | #CF9C7F / #FEBF8C |
| sky | #70B7FA / #D09BD6 | #63B3FD / #93D8FD |
| low-contrast | #74BDD4 / #B5A4EA | #ACACAC / #CECECE |
| gray | #74BDD4 / #B5A4EA | #A8A8A8 / #CECECE |
| saturated | #F98F80 / #6FCA69 | #FC8676 / #8CE885 |

Before and after foreground contrast remains strong; improvements target source
fidelity, role coverage, borders and ANSI. For example, grayscale primary chroma
drops from .0799 to effectively zero. Forest's ANSI5 changes from cyan `#1FB8CF`
to recognizable magenta `#BB91BC`. Brown and gray are no longer forced toward
invented green or purple secondaries.

## Actual T470 wallpaper

A read-only `awww query` confirmed eDP-1 displaying
`/home/kuroki/Pictures/wallpapers/foggy_forest.jpg` (5120×2880). Image SHA256:
`c8bff4db842c59b882fae179170af89f75fc044651edadf754333b2278027ce5`.
The daemon, image and desktop configurations were not modified. The image itself
is not redistributed here. Extraction describes the complete image, not an
arbitrary compositor crop.

Representative colors after merging (normalized sampled coverage):

| Representative | Coverage |
| --- | --- |
| #616E82 | 36.40% |
| #4E596A | 11.12% |
| #000201 | 10.24% |
| #424D5B | 9.55% |
| #030907 | 7.71% |
| #353E4A | 7.04% |
| #283039 | 6.48% |
| #1B2228 | 5.68% |
| #0E1413 | 5.09% |
| #000000 | 0.70% |

The measured representatives are cool slate/blue and very dark green shadows. The
new slate primary and subtle green secondary follow those source families;
Rustwood is a deliberately warmer, separately designed palette.

| Role | Old generated dark | Improved dark | Improved light | Existing Rustwood |
| --- | --- | --- | --- | --- |
| background | #090D15 | #090B0D | #EDF0F4 | #17130F |
| surface | #181D25 | #16181B | #DDE0E3 | #241D17 |
| surface_alt | #282E37 | #242729 | #CCD0D3 | #30271F |
| foreground | #EBF2FE | #F0F2F4 | #08090A | #E8DCC8 |
| muted | #A7AEBA | #8C8F93 | #383B3D | #A99B88 |
| primary | #93B3E4 | #9DADC4 | #4C596F | #B98252 |
| secondary | #D899CB | #C5D1CB | #1F2925 | #73533B |
| border | #4A505A | #6F7275 | #707376 | #574436 |
| error | #EB635E | #BC8580 | #5C2D2A | #B85C50 |
| warning | #F7C243 | #E9D6A3 | #675627 | #D6A45C |
| success | #63B067 | #8AB18A | #2E5130 | #84946B |

| Opaque contrast measurement | Old dark | Improved dark | Old light | Improved light | Rustwood |
| --- | --- | --- | --- | --- | --- |
| Foreground/background | 17.28 | 17.57 | 16.99 | 17.43 | 13.64 |
| Minimum text role across all three surfaces | 4.22 | 4.63 | 3.74 | 4.56 | 2.11 |
| Minimum border across all three surfaces | 1.68 | 3.11 | 2.09 | 3.07 | 1.59 |
| Minimum ANSI against terminal background | 1.00 | 4.83 | 1.00 | 6.86 | 1.00 |

“Text role” includes foreground/muted/primary/secondary/error/warning/success;
these stricter generated-role targets are not retroactively imposed on Rustwood.
Rustwood, its active outputs and transaction state remain untouched.

ANSI comparison (fallback values are resolved through the unchanged template
renderer for the old generated palette and Rustwood):

| ANSI | Old generated dark | Improved dark | Improved light | Rustwood |
| --- | --- | --- | --- | --- |
| 0 | #090D15 | #7B7E81 | #16191B | #17130F |
| 1 | #EB635E | #C9918C | #71403C | #B85C50 |
| 2 | #63B067 | #87AD87 | #365A38 | #84946B |
| 3 | #F7C243 | #B2A06F | #5F4E1E | #D6A45C |
| 4 | #78A5EF | #8DA3C4 | #3D506E | #78A5EF |
| 5 | #C58CD5 | #B893BA | #634265 | #99B055 |
| 6 | #49B8B8 | #6BAFB3 | #0D5B5F | #49B8B8 |
| 7 | #EBF2FE | #D1D5D8 | #2C2E31 | #E8DCC8 |
| 8 | #35393F | #ABAEB1 | #4D5053 | #413D3A |
| 9 | #EF7F7B | #F5BAB4 | #4B1E1B | #C57970 |
| 10 | #7FBE82 | #AFD7AF | #143716 | #9AA786 |
| 11 | #F8CD65 | #DCC997 | #392C06 | #DDB479 |
| 12 | #90B5F2 | #B5CCEF | #1C2E49 | #90B5F2 |
| 13 | #CFA1DD | #E2BCE4 | #3F2041 | #ABBE74 |
| 14 | #6AC5C5 | #94D9DC | #013538 | #6AC5C5 |
| 15 | #EFF4FE | #F6F9FC | #050607 | #ECE2D2 |

Additional read-only real-image generation used `autumn_mountain.jpeg` (1920×1080)
and `sea.png` (3840×2160), in both modes. Autumn's dark primary/secondary changed
from `#FA9074 / #B4B76A` to `#FA8463 / #FCBDA8`; the sea changed from
`#76BDD7 / #B9A3E8` to `#9DB2BB / #C5D0D4`. Thus warm terrain retains warm accents,
while a muted sea remains subdued instead of acquiring a purple secondary. All
six real-image CLI outputs passed palette validation; no isolated apply/state
was needed.

## Preview and exact reproduction

Static previews need no app, daemon or GUI framework:
[old dark](wallpaper-evaluation/before-dark.svg),
[improved dark](wallpaper-evaluation/foggy-forest-dark.svg),
[improved light](wallpaper-evaluation/foggy-forest-light.svg),
[Rustwood](wallpaper-evaluation/rustwood.svg).
The candidate TOMLs and representative report are in `docs/wallpaper-evaluation/`.

From this feature worktree, generate a fresh isolated candidate with the existing
CLI (no init, apply, reload or active-config access):

```sh
cargo build --release --locked
preview_dir="$(mktemp -d /tmp/chroma-wallpaper-preview-XXXXXX)"
./target/release/chroma --config-dir "$preview_dir" generate   /home/kuroki/Pictures/wallpapers/foggy_forest.jpg   --mode dark --id foggy-forest-dark --name 'Foggy Forest'
./target/release/chroma --config-dir "$preview_dir" validate foggy-forest-dark
cat "$preview_dir/palettes/foggy-forest-dark.toml"
```

Use `--mode light --id foggy-forest-light` for the other mode. There is no automatic
activation. To reproduce fixtures, representative reports, semantic/ANSI previews,
contrast measurements and rendered adapter samples in a new/empty directory:

```sh
cargo run --locked --example wallpaper_quality -- /tmp/chroma-wallpaper-report-NEW   /home/kuroki/Pictures/wallpapers/foggy_forest.jpg
```

The example refuses a nonempty output directory. Optional fourth argv is an older
report directory containing palette TOMLs, rendered into a `baseline/` comparison.
The eight regression tests do not require the private wallpaper. For a fresh
baseline, build commit `54ebaba` separately and run its unchanged `generate` CLI on
the example's synthetic PNGs and the same real image. Do not change the audit branch.

## Verification actually run

- `cargo fmt --all -- --check`: passed.
- `cargo clippy --workspace --all-targets --locked -- -D warnings`: passed.
- `cargo test --workspace --locked`: **116 passed, 0 failed, 0 ignored**
  (108 stable-main baseline + 8 new generator quality tests).
- `cargo build --release --locked`: passed.
- `git diff --check`: passed.
- Native parser smoke checks: **75 passed** (15 palettes × Niri, GTK3, Fuzzel,
  Kitty and actual Island source parser), plus JSON parsing and SVG XML parsing.
  The 15 comprise 12 controlled dark/light fixtures, two real-wallpaper modes
  and unchanged Rustwood. Temporary XDG roots and disconnected display environment
  were used. No compositor/bar/launcher/Island UI was started.
- All built-in fragments and all six dedicated desktop formats render in the
  portable new tests. Foot/Quickshell lack installed native validators; their
  results are structural tests, not a claim of runtime validation.
- Final comparison checks all 37 snapshotted active config/output/state files for
  identical bytes and modes. No live palette or wallpaper was applied.

## Later live test — separate explicit approval required

No command in this section was run. Preserve the existing private T470 backup
and take a fresh private copy of `.config/chroma`, `.local/state/chroma`, Kitty
and Fuzzel configs before a live test, because they now contain reviewed imports.
Review the exact candidate and rendering for each currently enabled adapter.
`--no-reload` does not suppress a watcher or changes seen by the next invocation.
Niri/Waybar/Island remain unwired; this task does not approve connecting them.

After separate approval, generate only a **new** palette ID with the feature
binary, refusing an existing file (no `--force`). Then use the existing audited
binary for transactional apply/rollback; this feature branch intentionally does
not contain the separate PR #4 transaction fixes:

```sh
feature_bin=/home/kuroki/Projects/chroma/local/wallpaper-generation/target/release/chroma
audited_bin=/home/kuroki/Projects/chroma/target/release/chroma
chroma_config=/home/kuroki/.config/chroma
chroma_state=/home/kuroki/.local/state/chroma
"$feature_bin" --config-dir "$chroma_config" --state-dir "$chroma_state" generate   /home/kuroki/Pictures/wallpapers/foggy_forest.jpg   --mode dark --id foggy-forest-preview --name 'Foggy Forest Preview'
"$audited_bin" --config-dir "$chroma_config" --state-dir "$chroma_state" validate foggy-forest-preview
"$audited_bin" --config-dir "$chroma_config" --state-dir "$chroma_state" apply foggy-forest-preview --dry-run --no-reload
```

Review the dry-run, then only within that approval run:

```sh
"$audited_bin" --config-dir "$chroma_config" --state-dir "$chroma_state" apply foggy-forest-preview --no-reload
"$audited_bin" --config-dir "$chroma_config" --state-dir "$chroma_state" current
```

The user can open new Kitty/Fuzzel instances manually to inspect normal/selected
text, ANSI identities and transparency. Do not restart or signal desktop
processes. Restore once, assuming no intervening transaction, with:

```sh
"$audited_bin" --config-dir "$chroma_config" --state-dir "$chroma_state" rollback --no-reload
"$audited_bin" --config-dir "$chroma_config" --state-dir "$chroma_state" current
```

Expected: Rustwood restored and no pending transaction. On a conflict, stop and
preserve records/edits; do not force overwrite or delete backups. Keep the preview
palette as unreferenced data if desired. Neither PR is authorized for merging.
