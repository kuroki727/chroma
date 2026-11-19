#!/usr/bin/env python3
"""Simulate #4 merge, #5 merge, #9 squash in a NEW isolated local clone.

No GitHub operation, push, source-branch update, history rewrite or desktop write.
The retained output clone can be built/tested and explicitly published for CI.
"""
import argparse
import json
from pathlib import Path
import subprocess


def git(repo, *args, data=None):
    command = ['git', '-c', 'core.hooksPath=/dev/null', '-c', 'commit.gpgsign=false',
               '-c', 'user.name=Chroma Integration Audit', '-c', 'user.email=audit@local.invalid',
               '-C', str(repo), *args]
    result = subprocess.run(command, input=data, text=True, capture_output=True, timeout=60)
    if result.returncode:
        raise RuntimeError(f'git {args[0]} failed:\n{result.stdout}\n{result.stderr}')
    return result.stdout.strip()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--repo', type=Path, default=Path('.'))
    parser.add_argument('--main', default='origin/main')
    parser.add_argument('--pr4', default='audit/v0.2-real-hardware')
    parser.add_argument('--pr5', default='feat/wallpaper-palette-quality')
    parser.add_argument('--pr9', default='feat/automatic-wallpaper-sync')
    parser.add_argument('--output-dir', type=Path, required=True, help='Must NOT already exist')
    args = parser.parse_args()
    repo = args.repo.resolve(strict=True)
    refs = {name: git(repo, 'rev-parse', '--verify', value + '^{commit}')
            for name, value in [('main', args.main), ('pr4', args.pr4), ('pr5', args.pr5), ('pr9', args.pr9)]}
    destination = args.output_dir.absolute()
    # Exclusive directory creation protects existing checkouts and review files.
    destination.mkdir(mode=0o700)
    clone = destination / 'repo'
    git(repo, 'clone', '--no-hardlinks', '--no-checkout', str(repo), str(clone))
    git(clone, 'checkout', '-b', 'audit/pr4-pr5-pr9-final', refs['main'])
    git(clone, 'merge', '--no-ff', '-m', 'Audit simulation: merge PR #4', refs['pr4'])
    git(clone, 'merge', '--no-ff', '-m', 'Audit simulation: merge PR #5', refs['pr5'])
    prerequisites_tree = git(clone, 'rev-parse', 'HEAD^{tree}')
    git(clone, 'merge', '--squash', refs['pr9'])
    git(clone, 'commit', '-m', 'Audit simulation: squash PR #9 after prerequisites')
    tree = git(clone, 'rev-parse', 'HEAD^{tree}')
    assert tree == git(clone, 'rev-parse', refs['pr9'] + '^{tree}'), 'simulated final source differs from reviewed #9'
    reachable = set(git(clone, 'rev-list', refs['main'] + '..HEAD').splitlines())
    assert refs['pr4'] in reachable and refs['pr5'] in reachable

    def patch_id(commit):
        diff = git(clone, 'show', '--format=', '--no-ext-diff', commit)
        value = git(clone, 'patch-id', '--stable', data=diff + '\n')
        return value.split()[0] if value else None

    original = {name: patch_id(refs[name]) for name in ['pr4', 'pr5']}
    counts = dict.fromkeys(original, 0)
    for commit in git(clone, 'rev-list', '--no-merges', refs['main'] + '..HEAD').splitlines():
        identity = patch_id(commit)
        for name, expected in original.items():
            if identity == expected:
                counts[name] += 1
    assert counts == {'pr4': 1, 'pr5': 1}, f'duplicate prerequisite patches in simulated main: {counts}'
    copies = [commit for commit in git(clone, 'rev-list', '--no-merges', refs['main'] + '..' + refs['pr9']).splitlines()
              if commit not in refs.values() and patch_id(commit) in original.values()]
    assert not reachable.intersection(copies), 'copied prerequisite commits leaked into simulated main'
    report = {'source_heads': refs, 'merge_order': ['#4 --merge', '#5 --merge', '#9 --squash'],
              'prerequisites_tree': prerequisites_tree, 'final_tree': tree,
              'final_commit': git(clone, 'rev-parse', 'HEAD'),
              'prerequisite_patch_counts': counts, 'copied_commits_excluded': copies,
              'clone': str(clone), 'github_prs_merged': False}
    (destination / 'result.json').write_text(json.dumps(report, indent=2) + '\n')
    print(json.dumps(report, indent=2))


if __name__ == '__main__':
    main()
