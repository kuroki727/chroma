# Security and recovery

Chroma v0.1 is a local user utility, not a privilege boundary or a sandbox.
Run it as your desktop user, never through sudo. All shipped adapters are disabled.

## Trust boundaries

- **Palettes and wallpapers are data.** Palette TOML accepts a strict schema,
  validated opaque sRGB colors and metadata. Unknown fields are rejected. Metadata
  is not substituted into templates, paths or commands. Wallpapers are decoded
  locally; there are no services or network requests.
- **Local config, adapters and templates are trusted code/configuration.** They
  determine destinations and reload argv. Chroma's template language only
  substitutes colors/locally configured alpha and does not evaluate code. However,
  generated QML or other application configuration can have effects when consumed
  by that application. Review downloaded adapters/templates before using them.
- **State and backups are trusted recovery data.** They must be user-owned, in
  private directories, with no group/other file access. Structural validation
  detects malformed snapshots, duplicate paths, inconsistent transaction endpoints
  and invalid palettes/modes. It is not a checksum, signature or tamper-proof log.
  Another process with your user privileges can change your data or configuration.

## Managed files and paths

The first replacement of an existing unmanaged output requires an explicit
`allow_overwrite = true` in local adapter config. Later replacements compare bytes
and ordinary mode bits with the committed managed snapshot. Leave this option
false during normal use. It also authorizes replacing manual edits when true.

An application's diagnostic `config_path` is never written. Templates and main
configuration paths declared by any adapter cannot be used as output destinations.
Chroma's own config inputs and state are protected from adapter outputs. A custom
adapter can intentionally target other user files; Chroma cannot recognize every
application's main config or infer whether a custom template is colors-only.

Path tokens are limited to those in the README. No shell/environment evaluation
occurs. Managed destinations must be absolute file paths without parent traversal.
The Linux filesystem layer opens every directory component with `O_NOFOLLOW` and
performs leaf operations relative to held directory descriptors. Public leaf
operations reject slashes, absolute paths, `.` and `..`. Destination symlinks,
hardlinks, other owners, nonregular files and special permission bits are refused.

Output parents must be user-owned, owner-writable/searchable and not group/world
writable. State/transaction directories must additionally be private (`0700`);
state and backup files must not grant group/other permissions. Existing unsafe
permissions cause an error, not a silent chmod. Newly created directories are
fsynced in their parents before use; new records/outputs default to `0600`.

Ordinary output mode bits survive replacement. Group ownership, ACLs, extended
attributes, timestamps and security labels do not. Use ordinary user theme files.
Symlinked dotfile trees require explicit real destination paths.

## Transaction guarantees

1. Validate palette, templates, destinations and overwrite authorization.
2. Render and fsync every staged file in its destination directory.
3. Serialize/validate and durably save a complete transaction backup.
4. Durably record a pending transaction before any output replacement.
5. Compare each current target with its before snapshot immediately before
   replacement; use no-replace rename for previously absent files.
6. Replace all outputs, then optionally run configured reloads.
7. Commit the active palette/head and clear the pending record.

An ordinary error after replacement attempts file/state restoration and reports
incomplete restoration explicitly. Rollback validates the whole backup and its
relationship to the manifest before restoring files. Files already at their
before snapshot are skipped, matching after snapshots are restored, and conflicts
are retained for the user. Files created by the transaction are removed. Repeated
successful rollback walks backward through the history, including repeated
applications of the same palette.

There is **no global atomicity**: watchers can observe intermediate files, and
different applications can display different palettes during a transaction.
File/directory fsync and same-directory rename improve durability; actual kernel,
storage and filesystem failure behavior is not exhaustively tested.

The scoped advisory lock coordinates Chroma operations sharing one state directory.
It explicitly unlocks on scope exit, even if another thread forks while it is held.
External writers ignore this lock. Rechecking after staging narrows the edit race,
but Linux has no general atomic compare-content-and-rename primitive: an external
write between the final comparison and rename/unlink can still race with Chroma.
Do not concurrently edit managed outputs or run independent Chroma state directories
against the same destinations. Identical bytes/modes cannot reveal a replacement
by a different writer. Chroma does not defend against same-user malicious processes.

## Reload commands

Reload argv is taken only from current trusted local config, never palettes or
backups. Chroma launches an executable directly, without shell interpolation.
Direct shell executable names are rejected as a convenience, not as a security
barrier: other interpreters, wrappers or executable scripts can do anything the
user can do. Absolute executable paths avoid PATH ambiguity.

Reload stdin/stdout/stderr are disconnected; errors report the executable and exit
status/timeout, not its arguments or captured output. Do not embed credentials in
reload argv. A timeout kills/reaps only Chroma's direct child. Descendants and
external side effects cannot be undone. File restoration is attempted after apply
reload failure, but restored files do not guarantee restored application state.
`--no-reload` prevents commands, not an application's own existing file watcher.

Rollback reloads resolve destinations without requiring templates to remain
present. If a reload fails after successful rollback, the error explicitly says
files were restored. Run the application's reload manually; another rollback would
pop the next history entry.

## Untrusted input limits

Wallpaper opening is nonblocking so FIFOs/devices are rejected without waiting
for a writer. Only regular PNG/JPEG inputs are accepted. The compressed input is
read through a 64 MiB bound before decoding, rather than trusting its initial file
length. Headers must advertise nonzero dimensions, at most 16384 per side,
**16,777,216 pixels**, and at most **64 MiB decoded output**. This supports 5120×2880 RGB wallpapers;
larger wallpapers need offline resizing. Sampling is bounded to 128×128 and
clustering to 12 centers/24 iterations.

These are input/output limits, **not a hard process RSS limit**. Compressed buffers,
codec metadata, progressive JPEG coefficients and allocator overhead consume more
memory. The `image` decoder allocation setting is non-strict for some codecs.
No parser fuzz campaign, OS memory sandbox or hard CPU deadline was added.

Managed text inputs and serialized state/transaction files are limited to 4 MiB.
Snapshots duplicate data and can reach this serialized limit sooner than the raw
file size. Backup retention is unbounded and needs manual archival. TOML/schema
errors fail closed; CLI operational errors escape terminal control characters.
Text parse diagnostics can still include the offending line, so local configuration
should not contain secrets.

## Recovery procedure

1. Stop concurrent editing of managed outputs. Keep the state directory intact.
2. Run `chroma rollback --no-reload` with the same `--config-dir`/`--state-dir`
   used for apply. This path bypasses broken adapter configuration/templates.
3. On an external edit conflict, save that edit elsewhere. Inspect the relevant
   `transactions/tx-*.toml` before/after snapshots privately. Restore the expected
   managed bytes and permissions before retrying; Chroma will not guess which edit
   you wanted to keep.
4. On corrupt state/backup, preserve a private copy and restore a known-good
   record or recover individual files manually. Do not delete pending records to
   force a new apply. Validation cannot reconstruct lost/corrupt snapshots.
5. On unsafe state permissions, verify ownership/location first, then explicitly
   restore private directory/file modes (`0700`/`0600`). Do not chmod theme outputs
   merely to satisfy state-record policy.

An interrupted apply/rollback retains a pending record and blocks new applies.
Temporary `.chroma-*.tmp` files or unreferenced completed backups can remain after
a crash; no background process cleans them up. Never remove a referenced backup
while recovery is pending.


## v0.2 integration bundles

See [desktop integration](DESKTOP_INTEGRATION.md) for the separate private journal,
fixed artifact allowlist, exact first-apply ownership, disabled defaults and
conflict-preserving rollback. Integration never modifies desktop source files.
Native Niri/Fuzzel validators operate only on temporary copies with a three-second
direct-child timeout. Island/GTK offline harnesses never launch desktop processes.
Manual imports can connect generated files to application watchers; disconnect
those imports before removing bundles. No Quickshell dependency was added.


## v0.2 adapter setup

`adapters detect` reads conventional application files without following imports,
invoking applications or creating state. It rejects unsafe roots and reports
unsupported syntax per application. Its limited CSS/INI inspection does not
establish complete semantic validity or installed-version support.

`adapters enable/disable` changes only Chroma TOML. Guided enablement requires
standard generated fragment destinations; custom formats/outputs remain manual.
Setup shares the palette transaction writer, lock, pending record, backup schema
and rollback history. The `chroma-setup` recovery entry permits exactly Chroma's
config.toml or standalone adapter TOMLs beneath its adapters/ directory. Ordinary
adapter outputs cannot use that exception to replace Chroma input files. Setup
never reloads applications. Its dry-run never stages, locks or writes state.

Enabling is separate from manual desktop wiring. Generated colors cannot override
unrelated application settings by themselves; connecting native imports and
choosing which existing colors to replace requires user review and separate
application backups. Configuration edits outside Chroma after setup may block a
later rollback; the expected snapshots remain in private transaction records.


## Final v0.2 audit hardening

Configuration loading records main/standalone TOML snapshots and descriptor-read
adapter directory membership. Planning records the templates, inspected sources,
selected local palette and integration journal actually used. Inputs are
rechecked before replacement, including intervening mode changes and file
creation/removal. Output directories are reopened with no-follow and compared
with held device/inode identities; swapped paths fail instead of committing to
an obsolete directory. The final kernel compare/rename window with an
uncooperative same-user writer remains a documented limitation.

Manifest replacement compares the expected transaction state before staging and
rechecks its snapshot before rename. External state changes are preserved, even
when that prevents automatic recovery from clearing a pending record. Durable
backups remain available for manual resolution. Rollback retains the descriptor
used for target inspection. No new transaction system or on-disk schema was added.

Dry-run and commit validate the same serialized backup payload and snapshot size
limits. Review output escapes terminal control sequences in source text. Native
review validation refuses unchecked include chains; conservative KDL cases need
manual composed validation rather than following source-specified paths.
