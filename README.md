# Chroma

A local color engine for Linux desktops, written in Rust. Apply one semantic
palette to separate application theme files, or derive a palette from a PNG/JPEG
wallpaper. No daemon, network service, wallpaper setter, or desktop session is
required. MIT licensed; version 0.1 is a tested MVP, not a production-readiness claim.

Chroma **never edits your application's main configuration**. You choose adapters,
review generated files, and add imports yourself. Every shipped adapter starts
disabled. Fonts, geometry, keybindings, layout, animations and opacity are outside
the built-in templates. Fuzzel's inseparable RGBA alpha bytes require explicit local
configuration so palette changes preserve your chosen transparency.

## Build and install on Arch Linux

Requires Linux, Rust/Cargo >= 1.85 and a C linker. Arch's `rust` package supplies
the toolchain; `base-devel` supplies build tools. Install those yourself if needed.
No GTK, Qt, Wayland, image tools or desktop development libraries are build dependencies.

```sh
cargo build --locked --release
cargo install --locked --path . --root "$HOME/.local"
```

Ensure `$HOME/.local/bin` is on PATH. Chroma is not currently published to crates.io
or packaged in an official Arch repository. Cargo needs network access for the
first dependency download; the compiled program has no network operations.

## Try it safely, without connecting a desktop

Run these commands from the repository. All outputs, state and backups stay in
the temporary directory. The Kitty adapter below only generates a text file.

```sh
cargo build --locked --release
chroma_demo=$(mktemp -d)
./target/release/chroma --config-dir "$chroma_demo" init
cat > "$chroma_demo/config.toml" <<'EOF'
[general]
auto_reload = false
[adapters.kitty]
enabled = true
EOF
./target/release/chroma --config-dir "$chroma_demo" validate
./target/release/chroma --config-dir "$chroma_demo" apply rustwood --dry-run
./target/release/chroma --config-dir "$chroma_demo" apply rustwood --no-reload
cat "$chroma_demo/generated/kitty.conf"
./target/release/chroma --config-dir "$chroma_demo" current
./target/release/chroma --config-dir "$chroma_demo" apply nord --no-reload
./target/release/chroma --config-dir "$chroma_demo" rollback --no-reload
printf 'Demo retained at: %s\n' "$chroma_demo"
```

`--no-reload` prevents commands. An application's own file watcher can still react
to writes **if you have manually connected those files to a live application**.
The temporary demo has no imports and cannot affect your running desktop.

## Commands

```sh
chroma init                          # Seed editable files; never overwrite
chroma list                          # IDs and display names
chroma list --names-only             # Stable IDs, one per line
chroma current                       # Last committed palette, including ANSI
chroma apply rustwood
chroma apply                         # Use general.default_palette
chroma apply nord --dry-run          # No files, state, locks, or commands
chroma apply paper --no-reload
chroma generate ~/Pictures/wallpaper.png
chroma generate wallpaper.jpg --mode dark
chroma generate wallpaper.png --mode light --id beach --name 'Beach Light'
chroma generate wallpaper.png --output /tmp/beach.toml
chroma generate wallpaper.png --force # Explicitly replace generated.toml
chroma apply generated
chroma rollback
chroma rollback --no-reload          # Recovery even with a broken config
chroma validate                     # All available palettes and configured templates
chroma validate rustwood
chroma doctor
```

`generate` saves a palette; it never applies a theme or changes wallpaper.
The default generated ID is `generated` and the default mode is `dark`. Repeated
generation refuses to overwrite unless `--force` is present.

`validate` checks palette schema, color formats, contrast, adapter paths and
substitutions. It does not parse every application's language or check output
permissions. Use `apply --dry-run` for enabled destinations, duplicates and
overwrite authorization. Disabled Fuzzel templates are syntax-checked with dummy
alpha bytes; enabling one requires actual values.

`doctor` inspects PATH without invoking applications. It reports configured
adapters, missing config/template/reload files, unsafe destinations and pending
recovery. It cannot verify that you have added an import or that an app supports
hot reload on your installed version. Missing optional programs are informational.

Exit codes: **0** success, **1** operational/validation/diagnostic issue, **2** clap
argument error. `current` with no active palette succeeds and says so.

## Configuration and XDG directories

```text
$XDG_CONFIG_HOME/chroma/          (default ~/.config/chroma)
  config.toml
  palettes/*.toml
  templates/*
  adapters/*.toml
  generated/*                    (default adapter outputs)
$XDG_STATE_HOME/chroma/           (default ~/.local/state/chroma)
  lock
  state.toml
  transactions/tx-*.toml          (durable before/after snapshots)
$XDG_CACHE_HOME/chroma/           (reserved; MVP creates no cache)
```

Unset/empty/relative XDG variables use the standard home fallback. Paths are not
based on a fixed username. An absolute `HOME` is required. Global
`--config-dir PATH` places state at `PATH/state` and cache at `PATH/cache`, making
tests and demos isolated. `--state-dir PATH` overrides this. Keep the same state
directory when rolling back. Do not share state between unrelated installations.

`init` creates private directories and editable copies of presets/templates. It
keeps existing files, and creates no application imports or reload commands.
If you upgrade, compare existing templates with repository templates manually.

Example configuration:

```toml
[general]
default_palette = "rustwood"
auto_reload = false

[adapters.waybar]
enabled = true
# Default template: templates/waybar.css, falling back to embedded template.
# Default output: generated/waybar.css under Chroma's config directory.
# Only opt in after checking your Waybar's signal action:
# reload = ["pkill", "-USR2", "-x", "waybar"]
reload_timeout_ms = 3000

[adapters.niri]
enabled = true

[adapters.dynamic_island]
enabled = false
template = "templates/dynamic-island.txt"
output = "generated/dynamic-island.theme"
```

See [examples/config.toml](examples/config.toml) for every shipped adapter.
Standalone files `adapters/NAME.toml` contain the fields of one adapter without
a surrounding table. Defining the same name there and in `config.toml` is an
error. Unknown config and palette keys are errors, including misspellings.

Adapter path strings accept `{config_dir}`, `{state_dir}`, `{home}`,
`{xdg_config_home}`, and a leading `~/`. Other relative paths resolve under
Chroma's config directory. No shell expansions, commands, environment substitution
or parent traversal are supported. Destinations' parent directories must exist.
Outputs inside Chroma config are restricted to `generated/`; state is protected.

## Palettes and ANSI derivation

Built-ins: `rustwood`, `gruvbox-dark`, `nord`, `catppuccin-mocha`, `tokyo-night`,
and `paper`. Rustwood and Paper are original warm dark/light palettes.
Third-party semantic mappings are Chroma's; their source colors and ANSI values
follow upstream themes. See [palette attribution](THIRD_PARTY_NOTICES.md).

Edit `palettes/rustwood.toml` after `init`, or add a new `palettes/my-theme.toml`.
User files override embedded presets by filename ID. An invalid override is
reported, never silently replaced by an embedded preset.

```toml
name = "Rustwood"
author = "Kuroki"              # Optional

[colors]
background = "#17130F"
surface = "#241D17"
surface_alt = "#30271F"
foreground = "#E8DCC8"
muted = "#A99B88"
primary = "#B98252"
secondary = "#73533B"
border = "#574436"
error = "#B85C50"
warning = "#D6A45C"
success = "#84946B"

[ansi]                         # Optional; partial overrides supported
"4" = "#7299B0"
"12" = "#9ABBD0"
```

All eleven semantic fields are required. Colors are exactly `#RRGGBB`, case
insensitive, serialized uppercase. Palette names are printable, nonempty and at
most 120 bytes. IDs are 1–80 lowercase ASCII letters, digits, `_` or `-`.
Foreground/background relative sRGB luminance contrast must be >= 4.5:1.
Palette data cannot specify commands, adapter paths, templates or alpha bytes.

Missing ANSI entries use these deterministic defaults:

| Index | Default |
| --- | --- |
| 0 | background |
| 1 | error |
| 2 | success |
| 3 | warning |
| 4 | OKLCH blue, hue 260°, chroma .12 |
| 5 | OKLCH magenta, primary hue + 60°, chroma .12 |
| 6 | OKLCH cyan, hue 195°, chroma .10 |
| 7 | foreground |
| 8–15 | Corresponding 0–7 mixed 18% toward white for dark backgrounds, black for light |

The synthesized chromatic entries use OKLab lightness .72 for dark and .45 for
light palettes, reducing chroma to fit sRGB. “Light” here means background relative
luminance > .4. This is a practical fallback, not a reproduction of every named
theme's terminal scheme. Four third-party presets explicitly supply all 16 values.
Override them for your terminal's contrast needs; ANSI combinations have no universal
contrast guarantee.

## Wallpaper extraction

PNG and JPEG decoding happens in Rust with `image`, with only those codecs enabled.
The generator samples a thumbnail bounded by 128×128 pixels, ignores pixels with
alpha < 128, and clusters in OKLab using weighted Lloyd k-means. Initialization
chooses the most frequent color then farthest points; sorting and tie handling
are deterministic. It uses at most 12 clusters and 24 iterations.

Cluster population and chroma select the primary hue. Secondary hues must be
separated by >=45°, with a +75° fallback for single-hue/monochrome images. Layer
lightnesses are separated, neutral surfaces are tinted, status colors retain
conventional hues with distinct lightnesses, and gamut mapping reduces chroma.
Foreground/muted text stay readable against generated surfaces; accents are
refined to >=3:1 against them. Flat black, white, grayscale, red, green and blue
fixtures are covered. Identical input/settings reproduce the same TOML on the
same toolchain; floating-point behavior across future algorithm versions is not
a stable palette format guarantee.

Limits: actual compressed input <=64 MiB, dimensions <=16384 per side, at most
8,388,608 pixels (including 3840×2160), and <=64 MiB decoded output. These checks
precede pixel decoding. FIFO/device inputs are rejected without blocking. The
image is decoded before thumbnailing, so large images still cost decode time and
memory. Codec-internal allocations and compressed buffers add overhead; this is
not a hard process memory cap. Resize larger wallpapers offline. Fully transparent
images are rejected. Embedded ICC
profiles/HDR and wide-gamut color management are not implemented; pixels are treated
as sRGB. Thresholded transparency is an approximation.

## Template language and custom adapters

Templates are plain UTF-8 text, with color substitutions only:

| Expression | Example |
| --- | --- |
| `{{ background }}` or `{{ background.hex }}` | `#17130F` |
| `{{ background.hex_raw }}` | `17130F` |
| `{{ background.rgb }}` | `rgb(23, 19, 15)` |
| `{{ background.rgb_raw }}` | `23, 19, 15` |
| `{{ background.r }}`, `.g`, `.b` | `23`, `19`, `15` |
| `{{ ansi0 }}` through `{{ ansi15 }}` | Resolved terminal colors |
| `{{ alpha.KEY }}` | Local adapter's fixed alpha byte, e.g. `DD` |

Escape literal `{{` as `{{{{` and `}}` as `}}}}`. Ordinary single braces remain
unchanged, including JSON, CSS and QML braces. Unknown variables, formats and
unbalanced delimiters fail with context and line numbers. There are no expressions,
loops, filesystem includes, script execution or palette metadata substitutions.

For example, `templates/custom.css`:

```css
@define-color background {{ background }};
@define-color foreground {{ foreground }};
@define-color accent {{ primary }};
```

`adapters/custom.toml`:

```toml
enabled = true
config_path = "{xdg_config_home}/custom/config"  # Diagnostics only; never edited
template = "templates/custom.css"
output = "{xdg_config_home}/custom/chroma-colors.css"
allow_overwrite = false
# reload = ["/absolute/path/to/custom-control", "reload-colors"]
reload_timeout_ms = 3000
```

Create the output parent yourself. The first apply refuses any pre-existing file
unless **you explicitly set `allow_overwrite = true`** in trusted local config.
Subsequent applies can replace an unedited managed output. Manual edits or changed
permissions trigger the same refusal. Leave the overwrite option false for normal
use. Every replacement is backed up even when explicitly authorized.

Reload argv comes exclusively from your local adapter configuration. No shell is
used, shell executables are rejected, and a timeout kills/reaps the direct child.
An explicit executable script may be used when necessary; review it like any
other program. Set `general.auto_reload = true` only when you want these commands
to run. Chroma does not sandbox trusted reload programs or undo their side effects.

## Desktop setup (manual and opt-in)

Generate files before adding imports to avoid importing missing files. Default
paths below assume `~/.config/chroma`; use your real absolute path when XDG differs.

| Adapter | Generated file | Setup and runtime behavior |
| --- | --- | --- |
| Niri >=25.11 | `generated/niri.kdl` | Top-level merged include; Niri watches included files |
| Waybar | `generated/waybar.css` | CSS import and explicit use of named colors; optional CSS watcher/reload command |
| Fuzzel >=1.10 | `generated/fuzzel.ini` | INI include plus explicit alpha bytes; next invocation |
| Kitty | `generated/kitty.conf` | Native include; new process or manual config reload |
| Foot >=1.26 | `generated/foot.ini` | Native include, `colors-dark` default slot; new process |
| Quickshell | `generated/colors.json` | Add the FileView consumer; no shell reload command required |
| Dynamic Island | User-specified | Generic adapter works; real syntax/consumer/reload must be supplied |
| GTK / Qt | No automatic adapter | See limitations below |

### Niri

After enabling and applying the adapter, add **at the top level**, after any
conflicting layout colors:

```kdl
include "/absolute/path/to/chroma/generated/niri.kdl"
```

The template only sets focus-ring/border colors; it does not turn borders on or
change width. Niri merges included layout properties and watches included files
from version 25.11. Older versions have no supported automatic integration here.
Validate the composed config using `niri validate -c /path/to/config.kdl` before
adopting it. No restart command is configured. See
[upstream include semantics](https://github.com/niri-wm/niri/wiki/Configuration%3A-Include)
and [layout colors](https://github.com/niri-wm/niri/wiki/Configuration%3A-Layout).

### Waybar

At the start of your existing stylesheet, import the generated definitions and
use them in the selectors you choose:

```css
@import url("/absolute/path/to/chroma/generated/waybar.css");
window#waybar {
    background-color: @chroma_background;
    color: @chroma_foreground;
}
#workspaces button.active { color: @chroma_primary; }
```

No selectors, dimensions, margins or opacity are imposed by Chroma. You can opt
into Waybar's `reload_style_on_change` in its own JSON config. Alternatively add
the commented reload argv from the example and enable `auto_reload`; SIGUSR2
defaults to reload but can be reconfigured by Waybar. Chroma never sends it by
default. See [CSS import example](https://github.com/Alexays/Waybar/blob/master/man/waybar-styles.5.scd.in)
and [Waybar configuration](https://github.com/Alexays/Waybar/wiki/Configuration).

### Fuzzel

RGBA alpha is part of every Fuzzel color. Copy the existing alpha suffix of each
affected property into your **local** adapter configuration, as a decimal byte:

```toml
[adapters.fuzzel]
enabled = true
[adapters.fuzzel.alpha]
background = 221       # DD; use YOUR existing alpha, not this illustrative value
text = 255            # FF
match = 255
selection = 255
selection_text = 255
selection_match = 255
border = 255
```

All seven values are required by the default template. You can use a reduced
custom template to manage fewer properties. The palette never controls alpha.
Then add in the main/default section of `fuzzel.ini` (before `[colors]`):

```ini
include=/absolute/path/to/chroma/generated/fuzzel.ini
```

Ensure later color entries do not override your imported values. Check the result
with `fuzzel --check-config --config /path/to/fuzzel.ini` if your version supports
that flag. Fuzzel picks up changes on its next invocation; no reload command.
The installed `fuzzel.ini(5)` manual and
[upstream integration instructions](https://github.com/catppuccin/fuzzel#usage)
document includes and RGBA encoding.

### Kitty and Foot

Kitty (`kitty.conf`):

```text
include /absolute/path/to/chroma/generated/kitty.conf
```

Launch a new terminal or manually use Kitty's config reload binding. Chroma does
not enable remote control or send signals. See [Kitty configuration](https://sw.kovidgoyal.net/kitty/conf/).

Foot (`foot.ini`, main/default section):

```ini
include=/absolute/path/to/chroma/generated/foot.ini
```

The supplied template targets Foot >=1.26's `colors-dark` default slot (also when
the Chroma palette is light). If you have set `initial-color-theme=light`, adapt
the template to `colors-light`. Older Foot versions can use a local template with
`[colors]` instead. Existing Foot/foot-server processes are not recolored. See
the [upstream manual](https://codeberg.org/dnkl/foot/src/branch/master/doc/foot.ini.5.scd);
the same man-page source was inspected via its
[source mirror](https://github.com/clearcmos/foot/blob/main/doc/foot.ini.5.scd).

### Quickshell and custom Dynamic Island

Quickshell has no universal theme file. The adapter generates JSON and
[examples/quickshell/ChromaColors.qml](examples/quickshell/ChromaColors.qml) uses
`Quickshell.Io.FileView` with `watchChanges` and `onFileChanged: this.reload()`.
Copy/adapt the component, instantiate it with your palette path, and bind your
own properties, for example `color: chromaColors.colors.background`. Generate the
JSON before using it. The example's small fallback is for startup/read failures;
add every role your shell needs. This does not replace your shell, `qmldir`, or
singletons, and it does not invoke `qs reload`. The
[FileView documentation](https://quickshell.org/docs/v0.2.1/types/Quickshell.Io/FileView/)
defines this watcher pattern.

Dynamic Island's configuration syntax is intentionally **not assumed**.
[The example adapter](examples/dynamic-island-adapter.toml) and illustrative plain
text template show the contract: diagnostic `config_path`, user-defined
`template`, separate `output`, and optional argv `reload`. Replace the example
template with the real format and arrange for your app to consume the generated
theme. Chroma does not inspect or overwrite your existing Island config. If an
app has no include/theme support, create a separate complete config from an
explicitly maintained local template and point the app to it yourself. Chroma
cannot merge arbitrary configuration languages or preserve unknown properties
inside a file it has been asked to replace.

### GTK and Qt limitations

There is no built-in global GTK/libadwaita or Qt recoloring adapter in this MVP.
GTK CSS definitions only affect widgets/themes that consume those names; modern
libadwaita apps need not honor arbitrary overrides. Qt Widgets, Qt Quick, platform
themes and desktop portals expose different mechanisms. Use a custom adapter
for a known application's supported theme file, with manual integration. Chroma
does not replace `gtk.css`, GTK settings, `qt5ct`/`qt6ct` files or portal settings.

## Optional Fuzzel selector

```sh
install -Dm755 scripts/chroma-fuzzel "$HOME/.local/bin/chroma-fuzzel"
chroma-fuzzel
# Isolated config also supported:
chroma-fuzzel --config-dir /path/to/demo
```

It passes validated palette IDs through `fuzzel --dmenu`, then invokes `chroma
apply`. Canceling exits without applying anything. This helper is separate from
the Rust application. An optional Niri binding you can add yourself:

```kdl
binds {
    Mod+Shift+C { spawn "chroma-fuzzel"; }
}
```

## Transactions, rollback and recovery

Apply validates the palette and every enabled template, renders all outputs,
checks destinations, ownership and overwrite authorization, stages every file
in its destination directory, writes durable backups, and records a pending
transaction **before replacing any output**. Each file is replaced by same-directory
rename. Files and directories are fsynced. Reload commands run only after all
outputs are replaced. The active palette is committed after reload success.
Recovery validates the
whole transaction structure and its relationship to the manifest before writes;
malformed but parseable records are rejected.

Normal write/reload failure attempts to restore all previous files and state.
If reloads already ran, Chroma also attempts to reload the restored files using
the current trusted config, even when templates have since been removed. Reload
programs' external side effects cannot be
rolled back. Applications' own watchers may observe intermediate files.

`rollback` restores the latest transaction's before snapshots and previous
palette. Newly created files are removed. Repeated successful rollback walks
the history. Interrupted apply/rollback keeps a pending recovery record; new
applies refuse to proceed until recovery. `current` reports pending recovery
rather than claiming an active palette is reliable.

```sh
chroma rollback --no-reload
# For an isolated installation, keep the same overrides:
chroma --config-dir /path/to/demo rollback --no-reload
```

Emergency `--no-reload` recovery does not parse local adapter config or templates.
External edits are preserved: rollback refuses conflicting files and reports
an incomplete recovery, retaining the journal for retry. Save your edits elsewhere,
restore the expected managed bytes/permissions from the transaction's `after`
snapshot, then retry. Before snapshots are also available as byte arrays in the
TOML backup for manual recovery. Back up the state directory before manual surgery.
If only application reload fails after a successful rollback, files/state have
already been restored; run your application's reload manually instead of calling
rollback again and popping another history entry.

Safety boundaries and limits:

- All directory components are opened descriptor-relative with `O_NOFOLLOW`.
  Destination symlinks, hardlinks, special files and special permission bits are
  refused. Parent directories must be user-owned and not group/world writable.
  Symlink-based dotfile directories must use their real, explicitly chosen paths.
- Ordinary Unix permission bits are preserved. New outputs/backups are private
  (`0600`), directories `0700`. Existing state/transaction directories must also
  be private; recovery records cannot grant group/other access. Ownership groups,
  ACLs, xattrs, timestamps and
  SELinux labels are not preserved by replacement; use only ordinary user theme files.
- Advisory locking serializes Chroma mutations sharing a state directory. External
  writers do not honor that lock; snapshots are compared before replacement, but
  there is no atomic compare-and-replace with another uncooperative writer.
- Multi-file changes are recoverable, not globally atomic. Crash recovery is manual
  via `rollback`. A crash before a journal is written can leave harmless temporary
  `.chroma-*.tmp` files. No startup daemon or background cleanup runs.
- State/configuration are trusted user-owned data, not a security boundary against
  another process running as the same user or root. Reload programs may launch
  descendants; the timeout bounds the direct child, not arbitrary process trees.
- Managed input files and each serialized state/transaction are capped at 4 MiB.
  Transaction snapshots duplicate data; unusually large themes can reach that limit.
  Backups are retained without automatic garbage collection. Archive/reset state
  only when you no longer need rollback; never delete a pending transaction.
- Default adapters require manual imports. Color rendering is tested; an actual
  user's desktop appearance and live reload behavior require session testing.

## Architecture and development

`main` handles exit codes, `cli` orchestrates commands, `config` resolves XDG paths
and trusted adapters, `palette` owns validated data and deterministic generation,
`adapters` renders plain text and executes bounded argv, `engine` handles planning,
journaling and recovery, and `safe_fs` centralizes descriptor-relative operations.
Errors carry operation, adapter and path context using `anyhow`. The library
forbids unsafe Rust; Linux filesystem primitives are supplied by `rustix`.

The build uses clap, serde/TOML, image (PNG/JPEG only), anyhow and rustix.
`tempfile` is test-only. There is no embedded interpreter or GUI toolkit.

```sh
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
cargo build --release
```

`Cargo.lock` is included. Add `--locked --offline` after fetching dependencies for
a reproducible offline check. Tests create fixtures exclusively under temporary
directories and remove display variables for CLI subprocesses. They cover schema,
colors, unknown variables, both generation modes and codecs, determinism, missing
inputs, unsafe destinations, backup/permissions, history, partial recovery,
external conflicts, locks, reload errors/timeouts, config overrides, alpha
preservation and emergency recovery. No test restarts desktop applications.

Audit details, trust boundaries and lifecycle: [security](docs/SECURITY.md),
[architecture](docs/ARCHITECTURE.md), [audit findings](docs/AUDIT.md), and
[validation results](VALIDATION.md).

## Roadmap

- More varied image fixtures, perceptual evaluations and color-vision checks.
- Explicit backup inspection/export/pruning commands and schema migrations.
- Better recovery inspection and stronger coordination with external file writers.
- Optional application parsers/validators and version-specific capability checks.
- Validate the Quickshell example and each reload workflow in disposable sessions.
- Investigate supported GTK/libadwaita and Qt theme backends independently.
- Add verified integrations for notification daemons, editors and other terminals.
- Arch packaging, man pages, completions, release artifacts and CI MSRV coverage.

Contributions should extend templates first and keep the transaction engine
independent of a particular window manager. Existing user configurations and
desktop behavior must remain under explicit user control.
