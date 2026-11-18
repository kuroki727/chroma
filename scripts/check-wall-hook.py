#!/usr/bin/env python3
"""Test a copy of the inspected wall picker with mocked desktop commands.

Never executes the live script, awww or Fuzzel. Only the exact inspected script
is accepted; a changed script requires a new review before executing a copy.
The real Chroma binary receives explicit temporary HOME/config/state paths.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import stat
import struct
import subprocess
import tempfile
import zlib

EXPECTED = b'''#!/bin/sh
dir="$HOME/Pictures/wallpapers"
file=$(ls "$dir" | fuzzel --dmenu --lines 10 --width 40 --prompt "> ")
[ -n "$file" ] || exit 0
awww img "$dir/$file" --transition-type fade --transition-duration 1
'''
CANDIDATE = EXPECTED.rstrip(b'\n') + b' &&\n    chroma wallpaper changed "$dir/$file"\n'


def snapshot(path):
    fd = os.open(path, os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK)
    with os.fdopen(fd, 'rb') as source:
        info = os.fstat(source.fileno())
        if not stat.S_ISREG(info.st_mode) or info.st_size > 65536:
            raise RuntimeError('wall must be a regular script <=64 KiB')
        data = source.read(65537)
    if len(data) > 65536:
        raise RuntimeError('wall grew beyond 64 KiB')
    return data, stat.S_IMODE(info.st_mode), info.st_uid, info.st_gid


def png(path):
    def chunk(name, data):
        return struct.pack('>I', len(data)) + name + data + struct.pack('>I', zlib.crc32(name + data))
    path.write_bytes(b'\x89PNG\r\n\x1a\n' +
                     chunk(b'IHDR', struct.pack('>IIBBBBB', 1, 1, 8, 2, 0, 0, 0)) +
                     chunk(b'IDAT', zlib.compress(b'\0\x30\x70\x50')) + chunk(b'IEND', b''))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    source = parser.add_mutually_exclusive_group(required=True)
    source.add_argument('--wall', type=Path)
    source.add_argument('--self-test', action='store_true')
    parser.add_argument('--chroma', type=Path, required=True)
    parser.add_argument('--image', type=Path, help='Read/copy this image instead of the tiny PNG fixture')
    parser.add_argument('--candidate-output', type=Path, help='Create a NEW review copy; never overwrite')
    args = parser.parse_args()
    original = snapshot(args.wall) if args.wall else (EXPECTED, 0o755, os.getuid(), os.getgid())
    if original[0] != EXPECTED:
        raise RuntimeError('wall differs from the reviewed 207-byte script; inspect it before extending this checker')
    binary = args.chroma.resolve(strict=True)
    with tempfile.TemporaryDirectory(prefix='chroma-wall-hook-') as directory:
        root = Path(directory)
        cfg = root / 'config'
        cfg.mkdir(mode=0o700)
        (cfg / 'generated').mkdir(mode=0o700)
        (cfg / 'config.toml').write_text('[general]\nauto_reload=false\n[wallpaper]\nenabled=true\ndebounce_ms=0\n[adapters.kitty]\nenabled=true\n[adapters.foot]\nenabled=true\n')
        image = root / 'Pictures/wallpapers/image with spaces;literal.png'
        image.parent.mkdir(parents=True)
        if args.image:
            image.write_bytes(args.image.read_bytes())
        else:
            png(image)
        bin_dir = root / 'bin'
        bin_dir.mkdir(mode=0o700)
        mocks = {
            'ls': 'printf "%s\\n" "$CHROMA_CHECK_SELECTION"',
            'fuzzel': 'printf "%s\\n" "$CHROMA_CHECK_SELECTION"',
            'awww': 'printf "%s\\n" manager "$@" >> "$CHROMA_CHECK_LOG"\nexit "$CHROMA_CHECK_MANAGER_STATUS"',
            'chroma': 'printf "%s\\n" hook "$@" >> "$CHROMA_CHECK_LOG"\nif [ "$CHROMA_CHECK_REAL" = 1 ]; then\nexec "$CHROMA_CHECK_BINARY" --config-dir "$CHROMA_CHECK_CONFIG" "$@"\nfi\nexit "$CHROMA_CHECK_HOOK_STATUS"',
        }
        for name, text in mocks.items():
            path = bin_dir / name
            path.write_text('#!/bin/sh\n' + text + '\n')
            path.chmod(0o700)
        original_copy = root / 'wall-original'
        candidate = root / 'wall-candidate'
        original_copy.write_bytes(original[0])
        candidate.write_bytes(CANDIDATE)
        env = dict(os.environ, HOME=str(root), PATH=str(bin_dir),
                   XDG_CONFIG_HOME=str(root / 'xdg'), XDG_STATE_HOME=str(root / 'state'),
                   XDG_CACHE_HOME=str(root / 'cache'), CHROMA_CHECK_BINARY=str(binary),
                   CHROMA_CHECK_CONFIG=str(cfg), CHROMA_CHECK_SELECTION=image.name,
                   CHROMA_CHECK_LOG=str(root / 'calls'), CHROMA_CHECK_MANAGER_STATUS='0',
                   CHROMA_CHECK_HOOK_STATUS='0', CHROMA_CHECK_REAL='0')
        for key in ('WAYLAND_DISPLAY', 'DISPLAY', 'NIRI_SOCKET'):
            env.pop(key, None)

        def cli(*argv):
            result = subprocess.run([str(binary), '--config-dir', str(cfg), *map(str, argv)],
                                    env=env, capture_output=True, text=True, timeout=30)
            if result.returncode:
                raise RuntimeError(result.stdout + result.stderr)
            return result

        def exercise(script=candidate, **settings):
            log = root / 'calls'
            log.write_bytes(b'')
            result = subprocess.run(['/bin/sh', str(script)], env=dict(env, **settings),
                                    capture_output=True, text=True, timeout=30)
            return result.returncode, log.read_text().splitlines()

        checks = []
        code, calls = exercise(original_copy)
        assert code == 0 and 'hook' not in calls
        checks.append('original picker invokes manager only')
        code, calls = exercise(CHROMA_CHECK_SELECTION='')
        assert code == 0 and not calls
        checks.append('cancelled picker invokes neither manager nor hook')
        code, calls = exercise(CHROMA_CHECK_MANAGER_STATUS='7')
        assert code == 7 and 'hook' not in calls
        checks.append('failed manager never invokes hook')
        code, calls = exercise(CHROMA_CHECK_HOOK_STATUS='9')
        assert code == 9 and calls.index('manager') < calls.index('hook')
        assert calls[-1] == str(image)
        checks.append('hook failure is visible and image path stays one argument')
        cli('wallpaper', 'changed', image, '--dry-run')
        assert not (cfg / 'state').exists()
        assert not list((cfg / 'generated').iterdir())
        checks.append('real hook dry-run writes no persistent state')
        code, _ = exercise(CHROMA_CHECK_REAL='1')
        assert code == 0
        manifest = (cfg / 'state/state.toml').read_bytes()
        outputs = {p.name: p.read_bytes() for p in (cfg / 'generated').iterdir()}
        assert len(outputs) == 2
        checks.append('candidate applies real Chroma only to isolated outputs')
        code, _ = exercise(CHROMA_CHECK_REAL='1')
        assert code == 0 and manifest == (cfg / 'state/state.toml').read_bytes()
        checks.append('repeat preserves transaction manifest')
        code, calls = exercise(CHROMA_CHECK_REAL='1', CHROMA_CHECK_MANAGER_STATUS='7')
        assert code == 7 and 'hook' not in calls
        assert manifest == (cfg / 'state/state.toml').read_bytes()
        checks.append('manager failure preserves the previous palette')
        cli('rollback', '--no-reload')
        assert not list((cfg / 'generated').iterdir())
        code, _ = exercise(CHROMA_CHECK_REAL='1')
        assert code == 0 and outputs == {p.name: p.read_bytes() for p in (cfg / 'generated').iterdir()}
        checks.append('rollback and reapply reproduce exact bytes')
        cli('wallpaper', 'disable')
        disabled = (cfg / 'state/state.toml').read_bytes()
        code, _ = exercise(CHROMA_CHECK_REAL='1')
        assert code == 0 and disabled == (cfg / 'state/state.toml').read_bytes()
        checks.append('disabled hook changes no theme or history')
        if args.wall and snapshot(args.wall) != original:
            raise RuntimeError('source wall changed during read-only test; review again')
        if args.candidate_output:
            with args.candidate_output.open('xb') as output:
                output.write(CANDIDATE)
        print(json.dumps({'passed': len(checks), 'checks': checks,
                          'wall_sha256': hashlib.sha256(original[0]).hexdigest(),
                          'live_wall_unchanged': bool(args.wall),
                          'desktop_commands': 'mocked; no desktop IPC'}, indent=2))


if __name__ == '__main__':
    main()
