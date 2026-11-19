# Final integration audit: PR #4, #5 and #9

This is a reviewed installation plan, not permission to merge or alter the
desktop. Audit applications use isolated directories. No live configuration,
wallpaper, desktop process or Island source is changed by the audit/checkers.

## Dependencies and duplicate-free merge

Main was `54ebaba`. #4 (`760346d`) and #5 (`a328220`) are independent changes,
with no overlapping production files. #9 includes copied commits `36ee2f1`
(#4) and `5aa000c` (#5). A regular merge of #9 would retain those duplicate
commits in main's ancestry.

Use **#4 regular merge → #5 regular merge → #9 squash merge**. The squash after
both prerequisites applies only the remaining net changes. Each original
prerequisite patch occurs once in main; copied commits stay on the feature
branch and do not enter main. This needs no force-push, rebase, branch deletion
or rewriting of published history. Do not regular-merge #9 in this history.

`scripts/check-wallpaper-merge.py` rehearses that sequence in a NEW local clone:

```sh
git fetch origin
python3 scripts/check-wallpaper-merge.py --repo . \
  --pr4 origin/audit/v0.2-real-hardware \
  --pr5 origin/feat/wallpaper-palette-quality \
  --pr9 origin/feat/automatic-wallpaper-sync \
  --output-dir /tmp/chroma-merge-review-NEW
```

The output directory must not exist. The checker never pushes, updates source
branches or merges a GitHub PR. Its retained `repo/` clone asserts final-tree
equality with #9, original #4/#5 ancestry, one occurrence per prerequisite patch
and exclusion of copied prerequisite commits. `result.json` records all exact
source heads, `prerequisites_tree`, `final_tree` and the simulated final commit.
Build/test that retained clone with the full CI commands below. A changed source
head or main invalidates the rehearsal; repeat it before proceeding.

Only after explicit merge approval, record these full SHAs, compare them with
the rehearsal and wait for all required checks:

```sh
gh pr checks 4 --repo kuroki727/chroma --watch
gh pr checks 5 --repo kuroki727/chroma --watch
gh pr checks 9 --repo kuroki727/chroma --watch
gh pr view 4 --repo kuroki727/chroma --json headRefOid --jq .headRefOid
gh pr view 5 --repo kuroki727/chroma --json headRefOid --jq .headRefOid
gh pr view 9 --repo kuroki727/chroma --json headRefOid --jq .headRefOid
```

The following are **future commands requiring merge approval**, not audit actions.
Replace SHA4/SHA5/SHA9 with the recorded literal SHAs:

```sh
gh pr ready 4 --repo kuroki727/chroma
gh pr merge 4 --repo kuroki727/chroma --merge --match-head-commit SHA4
gh pr ready 5 --repo kuroki727/chroma
gh pr merge 5 --repo kuroki727/chroma --merge --match-head-commit SHA5
gh pr ready 9 --repo kuroki727/chroma
gh pr merge 9 --repo kuroki727/chroma --squash --match-head-commit SHA9
```

Wait for each merge to complete before continuing; do not enable auto-merge or
delete branches/worktrees. Stop if any head/main/check changes. After #4/#5,
main's files must match `prerequisites_tree`; after #9 they must match
`final_tree`. Compare with `git rev-parse origin/main^{tree}` after fetching.
GitHub merge commit IDs may differ from the simulation's dates/messages; the
tree, ancestry and patch counts are the verification targets.

## Findings corrected in #9

Regressions reproduced against `28da611` before fixes:

- A previously admitted waiting hook undid manual rollback or manual palette
  selection after the writer lock became available.
- A user's deletion of a managed output was automatically recreated.
- The duplicate shortcut could report success with a stale destination snapshot
  if a validator/external writer changed it during validation.

Manual apply, rollback and wallpaper enable/disable now advance a cancellation
epoch in the **existing lock inode**, under its exclusive lock. Hooks capture
that epoch at registration and reject older epochs after acquiring the lock.
Ordinary wallpaper commits do not cancel newer wallpaper events. The inode is
never replaced: a dedicated test checks identity and continued flock exclusion.
Epoch bytes are coordination metadata, outside the version-1 manifest/history.

Automatic hooks preserve missing managed outputs and recheck plan inputs, state,
destinations and modes before deduplication. Emergency rollback never reads the
hook queue or adapter configuration; malformed coordination/config does not
prevent file recovery. New tests cover these boundaries. Avoid concurrent old
and new Chroma binaries: old releases do not advance the cancellation epoch.

#4's recovery/Kitty fixes and #5's generator remain unchanged; they need no
unrelated backport. These integration corrections belong to #9's workflow.

## Current T470 inspection

- `~/.local/bin/chroma` is absent. The repository's development binary is not an
  installed command. `~/.local/bin/wall` is the reviewed 207-byte picker with
  SHA256 `61d38d610b6e137a4aa2a84fa83b4369a7dd9ffe176807dcf2975de06c2f6677`.
  It invokes awww only and has no Chroma hook.
- Live config has `auto_reload=false`, no wallpaper table (sync disabled), and
  enabled Niri, Waybar, Fuzzel and Kitty adapters. A standalone legacy Island
  adapter is also enabled. No pending transaction was present.
- Niri already includes `generated/niri.kdl`, so writes may activate immediately
  even with `--no-reload`. Waybar, Fuzzel and Kitty already import their generated
  fragments. No Waybar style watcher or Kitty remote-control config was found.
- Island's startup declaration has no `-s`: it does not select
  `generated/desktop-island.css`. Its enabled adapter alone establishes no visible
  recoloring. The readable profile from #8 is not incorporated.

These are file observations, not proof of running UI behavior, and can become
stale after user changes. #6–#8 remain independent unmerged PRs; prepared or
separately installed desktop files do not imply their branches were merged.

## Safe installation after separate approval

1. Complete the approved merge sequence, fetch main and verify its exact audited
   tree/CI. Create a fresh detached installation worktree at that main SHA;
   retain existing development/audit worktrees. Run all checks below there.

   ```sh
   git fetch origin
   # Must equal the rehearsal's final_tree before proceeding:
   git rev-parse 'origin/main^{tree}'
   CHROMA_MAIN_SHA=$(git rev-parse origin/main)
   CHROMA_BUILD_DIR="$HOME/Projects/chroma/local/t470-install-$CHROMA_MAIN_SHA"
   git worktree add --detach "$CHROMA_BUILD_DIR" "$CHROMA_MAIN_SHA" && \
     cd "$CHROMA_BUILD_DIR" && \
     cargo fmt --all -- --check && \
     cargo clippy --workspace --all-targets --all-features --locked -- -D warnings && \
     cargo test --workspace --all-targets --all-features --locked && \
     cargo build --release --locked
   ```
2. Stop issuing wallpaper/Chroma commands and avoid concurrent config editors or
   deployers. Confirm sync is disabled and no transaction is pending. Create and
   inspect a private pre-install archive; util-linux flock protects cooperating
   state writers during the snapshot:

   ```sh
   umask 077
   CHROMA_BACKUP=$(mktemp -d "$HOME/chroma-preinstall.XXXXXX") && \
   flock --nonblock --close "$HOME/.local/state/chroma/lock" \
     tar -C "$HOME" -czf "$CHROMA_BACKUP/config-state-wall.tar.gz" \
       .config/chroma .local/state/chroma .local/bin/wall \
       .config/niri/config.kdl .config/waybar/style.css .config/waybar/config.jsonc \
       .config/fuzzel/fuzzel.ini .config/kitty/kitty.conf .config/island/style.css && \
   tar -tzf "$CHROMA_BACKUP/config-state-wall.tar.gz" && \
   sha256sum "$CHROMA_BACKUP/config-state-wall.tar.gz" > "$CHROMA_BACKUP/SHA256SUMS"
   ```

   Check each exit status. Stop on missing/changed paths, unsafe permissions,
   lock failure or incomplete archives. If an installed binary has appeared,
   stop and back it up/review it too; do not overwrite it using this plan.
3. Run the isolated wall checker below against the verified release binary.
4. Install only the binary, leaving synchronization disabled. For the currently
   absent destination, stage in the same directory and refuse unexpected files:

   ```sh
   # From the verified installation worktree:
   test ! -e "$HOME/.local/bin/chroma" && test ! -L "$HOME/.local/bin/chroma" && \
     test ! -e "$HOME/.local/bin/chroma.candidate" && test ! -L "$HOME/.local/bin/chroma.candidate" && \
     install -m 755 target/release/chroma "$HOME/.local/bin/chroma.candidate" && \
     mv --no-clobber -T "$HOME/.local/bin/chroma.candidate" "$HOME/.local/bin/chroma" && \
     cmp target/release/chroma "$HOME/.local/bin/chroma"
   "$HOME/.local/bin/chroma" wallpaper status
   "$HOME/.local/bin/chroma" current
   ```

   Compare the installed hash with the verified build. This requires no init,
   desktop config edits, reload or restart. A no-longer-absent destination needs
   a fresh review, not an overwriting install command.
5. Separately review wall's candidate, mappings and reload policy. Only another
   approval should install that edit and enable sync. `wallpaper enable --dry-run`
   previews Chroma TOML; `enable` writes Chroma config/history. A real hook can
   activate Niri immediately. `changed --dry-run` generates only when enabled.

Binary rollback and theme rollback are different. Retain archives/binaries but
never restore an old state archive over newer history. Recover interrupted
applies with the same config/state paths using `rollback --no-reload`. Each
successful rollback pops one entry, including enable/disable entries. Do not
delete pending state/backups or restore the entire desktop tree.

## Test existing wall without changing it

```sh
python3 scripts/check-wall-hook.py \
  --wall "$HOME/.local/bin/wall" --chroma target/release/chroma \
  --image "$HOME/Pictures/wallpapers/foggy_forest.jpg"
```

The checker accepts only the exact reviewed script, executes copies with mocked
ls/Fuzzel/awww under a temporary HOME/PATH, and runs real Chroma against explicit
temporary config/state. It compares source wall bytes/mode/UID/GID afterward.
No real desktop command or IPC is invoked. To retain a review copy add
`--candidate-output /NEW/path/wall.candidate`; it refuses an existing destination
and never installs it. The candidate adds a post-success `&& chroma wallpaper
changed "$dir/$file"`. Spaces/metacharacters remain literal path characters.

The ten scenarios cover original behavior, cancellation, manager failure, hook
failure, quoting, no-write dry-run, real isolated apply, duplicates, rollback/
reapply and disablement. CI runs the self-contained fixture version:

```sh
python3 scripts/check-wall-hook.py --self-test --chroma target/release/chroma
```

## Verification and daily-use gates

Required on both #9 and the simulated final clone:

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo test --workspace --all-targets --all-features --locked
cargo build --release --locked
python3 scripts/check-wall-hook.py --self-test --chroma target/release/chroma
git diff --check
```

Initial #4/#5/#9 GitHub Actions runs all passed: [#4](https://github.com/kuroki727/chroma/actions/runs/44064457350),
[#5](https://github.com/kuroki727/chroma/actions/runs/44121579170),
[#9 before audit](https://github.com/kuroki727/chroma/actions/runs/44262819987).
The updated Draft PR records the final audited SHAs, test counts and CI links.

Local audit results on 2026-11-18: **150 Rust tests passed**, including 27 hook
tests and the lock-inode regression; formatting/Clippy/release build passed.
The wall checker passed ten scenarios with both its generated fixture and a
copy of the real foggy-forest wallpaper. Five injected ENOSPC cases passed
(first/second output staging, backup, pending manifest, final manifest). Three
new bug regressions were demonstrated failing before their fixes. Final tree
identity and GitHub CI are recorded against exact commits in the updated PR.

Before daily automatic use:

- Serialize the **whole manager + hook**, not only palette writes. Delayed
  callbacks from overlapping wallpaper setters can otherwise arrive in the wrong
  physical wallpaper order. After the wall edit is separately reviewed, use one
  entry point, for example:

  ```sh
  flock --nonblock --close "${XDG_RUNTIME_DIR:?}/chroma-wall-launch.lock" \
    "$HOME/.local/bin/wall"
  ```

  This refuses overlapping launches. Direct awww calls/other managers bypass the
  launcher; do not mix them during the initial trial. Termination/power loss
  still requires checking the actual wallpaper and explicitly reissuing its hook.
- Verify Niri watching, Waybar reload policy and Kitty behavior manually. Fuzzel
  reads its next launch's config. With auto_reload disabled, imports alone do not
  make every already-running application update visibly.
- Keep Island out of the daily trial until its real stylesheet path and active
  tile/text contrast are separately reviewed. #8 remains a separate prerequisite
  for that prepared profile; no imports, variables or restart are assumed.
- Review physical display appearance/contrast, including transparency. Opaque
  palette ratios do not certify every UI mapping or ANSI pair.
- Dark PNG/JPEG only; one global palette, bounded sampling/size. Unsupported
  images may leave a new wallpaper with the previous theme. No daemon replays
  killed/timed-out work; genuinely new hooks after rollback may recolor again.
- Transactions recover files but are not a globally atomic visual switch.
  Edits/deletions cause refusal; conflicts and pending state remain for recovery.
  Uncooperative writers, power loss, ACL/xattr/group ownership and external-command
  effects retain existing limits. Backups have no automatic quota.

**Readiness:** an audited binary may proceed to an explicitly approved staged
installation with sync disabled. Unattended daily recoloring still needs the
launcher, adapter/reload and physical appearance gates above.
