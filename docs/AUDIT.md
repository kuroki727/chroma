# T470 v0.2 stabilization audit

The 2026-11-17 audit independently verified the baseline and fixed missing-parent
rollback and indented Kitty directive handling. See [T470 validation](T470_VALIDATION.md)
for 114-test results, isolated native/error checks, environment boundaries and
the pending live validation gate.

# Final v0.2 audit

The complete PR #3 diff, inherited integration work, confirmed defects and final
verification are recorded in [VALIDATION.md](../VALIDATION.md#final-pr-3-audit--v02-2026-11-16).
The fixes preserve the original transaction system and version-1 recovery schema.

---

# v0.1 pre-merge audit

Baseline: branch `feat/initial-implementation`, commit `05cc8b1`. The working tree
was clean. All original 42 tests, formatting, strict Clippy and release build were
independently rerun and passed before changes. No active desktop files were read
or modified by fixtures; no live reload, package installation or sudo was used.

## Confirmed defects and corrections

| Finding | Impact / evidence | Correction |
| --- | --- | --- |
| Structurally corrupt recovery data was accepted | An empty but valid TOML transaction made rollback succeed, clear active tracking and leave generated files behind; reproduced by a failing regression | Validate entries, snapshot modes/paths, after maps, IDs and manifest endpoints before recovery writes |
| Rollback reload depended on template rendering | Removing a template prevented the configured reload after files/state had already been restored; error did not clearly describe committed restoration | Resolve output paths separately and aggregate errors under an explicit files-restored message |
| Lock could survive operation scope in a forked child | A rapid sequential apply test failed with EAGAIN during parallel subprocess tests; closing the parent fd need not release a shared open-file-description lock | Scoped explicit unlock; deterministic regression duplicates the descriptor to simulate inheritance |
| Wallpaper FIFO opening blocked | An isolated CLI child timed out before the regular-file check | Nonblocking open followed by regular-file validation |
| Compressed-size check trusted metadata; decoder allocation cap was overstated | JPEG wrapper reads to EOF; image documents some allocation limits as non-strict | Bound actual reads, freeze input, check pixel/output budgets before decode; document that RSS is not capped |
| Public filesystem leaf methods accepted traversal/absolute names | Direct library callers could escape a held directory using leaf strings, although CLI callers supplied file names | Validate single leaf components in read/check/commit paths |
| Restoration had a staging-to-rename edit window | Rollback read the managed snapshot, staged/fsynced previous contents, then replaced without checking bytes again | Recheck after staging, immediately before replacement; residual compare/rename race explicitly documented |
| New directory entries were not fsynced in parents | Code review identified a durability gap when a new state root was created before replacing files | Fsync successful mkdir in its parent before continuing; physical power loss was not tested |
| Template line accounting was quadratic in variable count | Each substitution rescanned the entire preceding template | Incremental accounting; many-variable exact-line regression |
| Fuzzel selection could be parsed as a CLI flag | Valid palette IDs may begin with `-`; selecting `--help` printed help instead of applying | Add argv delimiter; mocked-menu end-to-end regression |
| Invalid palette diagnostics emitted terminal controls | A malformed TOML fixture placed raw OSC/ESC and BEL bytes in captured stderr; reproduced by a failing regression | Escape control characters in CLI operational errors while preserving multiline context |

State privacy checks were also tightened: existing state/backup directories must
be private and records must not grant group/other access. Fresh records already
had safe defaults; this is hardening of permissive existing locations, not a
claim of a remote exploit against default installations. The rustix minimum was
set to 1.1.6, already present in Cargo.lock, to exclude the affected 1.x releases
of [RUSTSEC-2026-0486](https://rustsec.org/advisories/RUSTSEC-2026-0486.html).
Chroma does not use the affected xattr functions.

## Regression coverage

New checks cover actual apply failure injection after one/two/three replacements,
backup creation failure, inconsistent/empty/duplicate recovery records, unsafe
record permissions, missing-template reload, rapid/repeated applies and rollback,
fork-inherited locks, leaf traversal, edits during staging, FIFO rejection,
sparse oversized inputs, huge PNG headers, valid 8,388,608-pixel input, deterministic
dark/bright/grayscale/low-saturation gradients, XDG/explicit state paths, template
line accounting and option-like selector IDs. Existing shell-injection, symlink,
hardlink, reload timeout/failure and external-edit tests remain enabled.

## Nonblocking limitations and future work

- External writers are not coordinated; final compare-to-rename/unlink races remain.
- Multi-file updates and reloads are not globally atomic; process side effects,
  ACLs/xattrs, security labels and group ownership are outside restoration guarantees.
- Physical disk/full-filesystem/power faults and a parser fuzz campaign were not run.
  Fault injection exercises the transaction state machine, not storage firmware.
- No full cargo-audit scan: the tool is unavailable and shell networking is blocked.
  Manual RustSec/source review is narrower than a complete advisory database scan.
- Foot and Quickshell are not available for runtime validation; Dynamic Island's
  actual format is unknown. All integrations stay opt-in. Global GTK/Qt theming,
  automatic backup pruning and MSRV CI remain future work.
- Public floating-point color helpers assume finite, meaningful lightness/chroma
  input; CLI palette/image paths produce bounded finite values. No stable external
  library API guarantee is made for v0.1.

Final commands, test counts and native parser results are in
[VALIDATION.md](../VALIDATION.md). Review readiness applies to the documented MVP,
not production readiness or a certification of the surrounding desktop.
