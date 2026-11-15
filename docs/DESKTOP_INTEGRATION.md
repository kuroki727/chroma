# Desktop integration in v0.2.0

This release implements a **review bundle with manual desktop activation**. It
does not automate changes to existing desktop configurations. This is a deliberate
scope limit: native includes can trigger file watchers, and Island has its own
parser rather than GTK or Quickshell. `integrate --apply` installs separate files
and disabled adapters only. It does not connect them to the running desktop.

## Preview safely

```sh
chroma integrate --plan
chroma integrate --plan --palette nord
chroma integrate --status
```

Planning reads only the six explicitly known configuration paths under
`$XDG_CONFIG_HOME` (default `~/.config`). It reads Chroma configuration and palettes
as needed. It creates no directories, locks, state, temporary files or processes.
The output contains complete proposed generated files, complete migration
previews, source paths, backup requirements, validation and activation guidance.
No behavior JSONC files are parsed or changed by the integration commands.

For isolated copies, supply both roots explicitly:

```sh
chroma --config-dir /tmp/chroma-review integrate --plan \
  --desktop-dir /tmp/copied-desktop --palette rustwood
```

`--config-dir` requires `--desktop-dir` when planning/installing a bundle so a
temporary test configuration cannot accidentally inspect the live desktop.
Status and rollback need only the original Chroma/state paths.

## Install a review bundle (optional, changes Chroma files only)

After reviewing the plan, `chroma integrate --apply --confirm` installs:

- `generated/desktop-APPLICATION.EXT`: palette output, separate from the source;
- `adapters/desktop-APPLICATION.toml`: application-aware adapter, **disabled**;
- `generated/preview-{niri,kitty,waybar}.EXT`: proposed manual integration;
- a private `$XDG_STATE_HOME/chroma/integration.toml` recovery journal.

Without `--confirm`, installation fails before creating state. Existing unrelated
artifacts are never overwritten. Source changes between planning and installation
are refused. Repeated installation with a complete journal makes no changes.
Re-run a fresh preview after installation by using a separate temporary Chroma
root; the installed operation remains a fixed, reviewable snapshot.

No adapters are enabled automatically. Review the installed adapter, change its
`enabled = false` to `true` yourself, then check:

```sh
chroma validate
chroma apply nord --dry-run
```

The completed bundle journal authorizes the first palette update only if the
generated file still exactly matches the installed snapshot. Subsequent updates
use normal palette transaction ownership. Do not set `allow_overwrite = true`
to bypass a conflict without understanding the local changes.

After the manual connections below, `chroma apply rustwood`, `chroma apply generated`
and `chroma apply nord` update enabled outputs. Until those connections exist,
palette changes affect generated files only. The optional, existing
`scripts/chroma-fuzzel` selector invokes the same palette commands, with cancellation
and option-like IDs tested. Nothing installs it or adds keybindings automatically.

## Supported applications and activation

| Application | Output | Manual connection | Validation |
| --- | --- | --- | --- |
| Niri >=25.11 | Four colors in a native KDL include | Append the preview's final top-level `include` after reviewing a temporary composed config | `niri validate -c TEMP`; required when installing a Niri bundle |
| Waybar / GTK3 CSS | Eleven named palette definitions | Review `preview-waybar.css`; back up the original and accept its color-only migration plus import yourself | Restricted CSS parser; offline GTK parser harness supplied |
| Fuzzel | Full copied INI, ten mapped colors | Use `fuzzel --config=/absolute/path/to/generated/desktop-fuzzel.ini` | Installed `--check-config` when available |
| Kitty | Color-only native include, including ANSI 0–15 | Append preview's native include, then use a normal new terminal or your usual manual reload | Strict known color directives; native parser checked offline on this host |
| Standalone Island | Full copied stylesheet | On the next normal launch, point Island's `-s` at `generated/desktop-island.css` | Restricted literal parser plus actual C++ parser harness |
| Swaylock-effects | Full copied config, supported color states | Correct invalid source syntax first; manually select the generated file only after review | Color syntax; **no safe standalone validator/launch** |

### Niri

Only `layout.border.active-color` → primary, `inactive-color` → border,
`urgent-color` → warning, and `layout.background-color` → background are generated.
The original `layout.border` remains present, preserving its enabled state and
width. Focus rings, gaps, rules, animations and bindings are absent from the color
include. Niri's documented include merging preserves those settings.

The validator runs against a temporary full copy plus generated include; it does
not start or contact the compositor. Existing `include` lines require manual
composed validation because relative path resolution would otherwise change in
the temporary directory. Missing validators fail Niri bundle installation.
Niri's own watcher observes connected includes: **`--no-reload` cannot disable
that watcher**. Chroma never invokes a Niri reload or restart.

See [Niri include semantics](https://github.com/niri-wm/niri/wiki/Configuration%3A-Include).

### Waybar

Migration uses GTK `@define-color`, `@chroma_NAME`, `@import`, and `alpha()`.
It does not use browser CSS custom properties. Border shorthands replace only
the trailing color token, retaining `2px solid`, for example. Existing transparent
backgrounds, all selectors, fonts, sizes, spacing and declarations are kept.
RGBA alpha is retained; hex alpha is represented as an equivalent GTK alpha
factor to nine decimal places. Multi-selector rules with conflicting semantic
roles, imports/at-rules, nested rules, named colors other than transparent,
inherit/none, and unsupported expressions require manual setup. The parser is
intentionally restricted, not a general GTK CSS parser.

The source-aware Waybar adapter continues generating definitions after the main
stylesheet is migrated; it does not attempt to remigrate it on each palette
change. `config.jsonc`, module geometry, scripts and actions are untouched.
The generated preview is a review artifact, not a file to import wholesale.

See [GTK CSS import and named colors](https://docs.gtk.org/gtk3/css-overview.html).

### Standalone Island

Read-only inspection of the local Island `config.hpp` and `main.cpp` found a
hand-written C++ CSS parser. It recognizes literal hex/RGB/RGBA colors and a fixed
selector/property set. It has **no imports, GTK named colors or CSS variables**.
Its documented `-s`/`--style` switch selects a separate stylesheet. Chroma uses
that mechanism; no Quickshell dependency or architecture change is involved.

Mappings: island background → background, border → border; clock/text/body/value
→ foreground; date/label → muted; title/fill/flash/layout/dnd/tile-on → primary;
track/tile → surface_alt. Only existing recognized color tokens change. Tests
compare the entire non-color byte skeleton, including border width, font sizes,
comments, whitespace, selector names and ordering. Hex alpha bytes and RGBA alpha
spelling survive. No geometry, animation, timing or behavior JSONC is modified.

Optional trusted local adapter configuration may set:

```toml
reload = ["island", "ctl", "reload"]
```

Reload still requires explicitly enabling `general.auto_reload`. This task never
sent it. Ensure the Island instance actually uses the generated `-s` path first.
Replacing the original stylesheet is not required or automated. If you choose
that alternative, explicitly approve it and keep a separate backup yourself.

`scripts/check-island-parser.cpp` compiles against the actual local Island header
and runs its `apply_style`/`parse_color` without starting Island or connecting to
Wayland. It is an optional development harness; Chroma never compiles application
source automatically. The restricted Rust grammar is stricter than Island's
permissive parser and rejects ambiguous input rather than guessing.

### Fuzzel, Kitty and Swaylock

Fuzzel maps background, text, prompt, placeholder, input, match, selection,
selection-text, selection-match and border. Every original final alpha byte is
preserved. Font, terminal, dimensions and border settings survive in the generated
copy. Use the source-aware adapter to avoid hand-maintaining alpha settings.
The original template adapter also supports all ten keys now, with explicit local
alpha values; `init` does not replace an older local template automatically.

Kitty maps background, foreground, cursor, selections and all sixteen ANSI slots.
Other main-config settings remain outside the native color include. Duplicate
recognized keys, incomplete ANSI sets or unsupported color expressions fail closed.

Swaylock maps all four inside states, ring normal/clear/verifying/wrong, text
states, key highlight and backspace highlight. Alpha bytes survive. Transparent
line/separator options and non-color options remain literal. The inspected
installed variant is swaylock-effects 1.7.0.0. Its config uses long-option syntax;
the actual source contains `exec swayidle` and shell continuations, so bundle
planning reports it as **unsupported until manually corrected**. Chroma never
executes it, launches a lock screen, or edits blur/vignette/clock/indicator options.

## Backup, recovery and ownership

Integration and palette journals are separate. The bundle creates private new
files only, so its before snapshots are absent; rollback removes only artifacts
whose contents **and modes** still equal the operation's after snapshot.
The journal is fsynced before writes and marked complete afterward. Interrupted
operations are visible through `--status` and recoverable with:

```sh
chroma integrate --rollback
```

Rollback preserves subsequent edits and retains its journal on conflict. Resolve
edited adapters manually; if palette application changed generated files, run
the corresponding palette rollback first. Both operations use the same advisory
state lock. Rollback never follows symlinks or restores arbitrary journal paths.
External writers do not honor Chroma's lock: comparisons narrow but cannot
eliminate the final check/rename race. Multi-file writes are recoverable, not
globally atomic. Empty artifact directories can remain after rollback.

Manually accepted desktop includes/migrations are **not owned by the bundle**.
Before rolling the bundle back, manually disconnect those imports or restore
your separately backed-up desktop stylesheet. Chroma cannot safely infer whether
you have connected generated files to a live application's watcher.

## Wallpaper bounds

The actual failing JPEG is 5120×2880. The pixel ceiling is now 16,777,216;
the 64 MiB compressed input, 64 MiB decoded-output budget and 16384-pixel side
limit remain enforced before pixel allocation. Native decoded data is thumbnailed
to 128×128 before RGBA conversion. There is no full-size RGBA conversion.
Both PNG and JPEG 5K fixtures are tested. A 16-bit or large RGBA image may still
exceed the byte budget even within the pixel ceiling. Larger images (including
8K RGB) still need external downsampling. Codec working buffers and compressed
copies add overhead; this is not a hard process RSS or CPU-time cap. Streaming
PNG and codec-native JPEG scaling remain future work.

## Deferred work and limitations

- Automatic desktop wiring and its surgical rollback are not implemented. Review
  artifacts, explicit activation and separate user backups are required.
- Qt6ct scheme generation is deferred. The installed package is **qt6ct-kde** and
  its active `.colors` file is a KDE color scheme, not upstream Qt6ct's positional
  QPalette INI format. `Kuroki.colors` and the active selection stay untouched.
- No global GTK or Qt recoloring: GTK3 named colors only affect themes that
  reference them; GTK4/libadwaita, Qt Widgets, Qt Quick and KDE applications have
  different theme mechanisms. No GTK/Qt selection is changed.
- Application parser verification covers the inspected versions. There is no
  general Island schema negotiation or safe Swaylock validator. No visual/live
  reload checks were performed against the running desktop.
- Integration plans expose full proposed styling/configuration text on stdout;
  review it locally before sharing. No private configuration archives are stored
  in the repository; fixtures contain only the scoped CSS styles.
