#!/usr/bin/env python3
"""Guard a reviewed Island adapter or future Niri startup change.

Never applies palettes, launches Island or sends IPC. A Niri write triggers its
native watcher, but spawn-at-startup runs only on a later compositor startup.

check is read-only. install and reverse require separate user authorization.
The private review directory contains manifest.json, original and candidate.
"""

import argparse
import contextlib
import fcntl
import hashlib
import json
import os
from pathlib import Path
import secrets
import stat
import subprocess
import tomllib
import re


def require(condition, message):
    if not condition:
        raise RuntimeError(message)


def digest(data):
    return hashlib.sha256(data).hexdigest()


def absolute(path):
    path = Path(path)
    require(path.is_absolute() and ".." not in path.parts, "absolute path without traversal required")
    return path


@contextlib.contextmanager
def directory(path):
    path = absolute(path)
    fd = os.open("/", os.O_RDONLY | os.O_DIRECTORY | os.O_CLOEXEC)
    try:
        for part in path.parts[1:]:
            child = os.open(part, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW | os.O_CLOEXEC, dir_fd=fd)
            os.close(fd)
            fd = child
        yield fd
    finally:
        os.close(fd)


def read_regular(path, private=False):
    path = absolute(path)
    with directory(path.parent) as parent:
        fd = os.open(path.name, os.O_RDONLY | os.O_NOFOLLOW | os.O_CLOEXEC | os.O_NONBLOCK, dir_fd=parent)
        with os.fdopen(fd, "rb") as stream:
            info = os.fstat(stream.fileno())
            mode = stat.S_IMODE(info.st_mode)
            require(stat.S_ISREG(info.st_mode) and info.st_nlink == 1, f"not a single-link regular file: {path}")
            require((info.st_uid, info.st_gid) == (os.getuid(), os.getgid()), f"unexpected ownership: {path}")
            require(not mode & 0o022 and not mode & 0o7000, f"unsafe mode: {path}")
            require(not private or mode == 0o600, f"private file must be 0600: {path}")
            require(not os.listxattr(stream.fileno()), f"extended attributes/ACLs require manual preservation: {path}")
            require(info.st_size <= 4 * 1024 * 1024, f"oversized input: {path}")
            data = stream.read(4 * 1024 * 1024 + 1)
            require(len(data) == info.st_size, f"input changed while reading: {path}")
            return data, mode


def metadata(path, private=False):
    data, mode = read_regular(path, private)
    return {"sha256": digest(data), "mode": mode}


def run(action, review, manifest_sha256):
    require(os.getuid() != 0 and os.getuid() == os.geteuid(), "run as the desktop user without sudo")
    review = absolute(review)
    with directory(review) as review_fd:
        info = os.fstat(review_fd)
        require((info.st_uid, info.st_gid) == (os.getuid(), os.getgid()), "review ownership changed")
        require(stat.S_IMODE(info.st_mode) == 0o700, "review directory must be 0700")
        # Serializes this tool's operations on the same bundle without creating a lock file.
        fcntl.flock(review_fd, fcntl.LOCK_EX | fcntl.LOCK_NB)
        manifest_bytes, _ = read_regular(review / "manifest.json", private=True)
        require(digest(manifest_bytes) == manifest_sha256, "manifest changed; obtain a new review")
        plan = json.loads(manifest_bytes)
        require(plan["version"] == 1, "unsupported manifest")
        active = absolute(plan["active"])
        require((plan["kind"] == "adapter" and active.name == "desktop-island.toml" and active.parent.name == "adapters") or (plan["kind"] == "startup" and active.name == "config.kdl" and active.parent.name == "niri"), "unexpected integration target")
        original, _ = read_regular(review / "original", private=True)
        candidate, _ = read_regular(review / "candidate", private=True)
        require(digest(original) == plan["original"]["sha256"], "original backup changed")
        require(digest(candidate) == plan["candidate_sha256"], "candidate changed")
        if plan["kind"] == "adapter":
            old = tomllib.loads(original.decode())
            new = tomllib.loads(candidate.decode())
            require(not old.get("island_readable", False) and new == dict(old, island_readable=True) and new.get("desktop") == "island", "unexpected adapter changes")
        else:
            line = b'spawn-at-startup "/home/kuroki/.local/bin/island"'
            updated = line + b' "-s" "/home/kuroki/.config/chroma/generated/desktop-island.css"'
            require(original.count(line) == 1 and candidate == original.replace(line, updated, 1), "unexpected startup changes")
        mode = plan["original"]["mode"]
        require(mode & 0o022 == 0 and mode & 0o7000 == 0, "unsafe original mode")
        expected = plan["candidate_sha256"] if action == "reverse" else plan["original"]["sha256"]
        payload = original if action == "reverse" else candidate

        def check():
            require(metadata(active) == {"sha256": expected, "mode": mode}, "active target changed; preserve edits and review manually")
            if action == "reverse" and plan["kind"] == "adapter" and "startup_active" in plan:
                path=plan["startup_active"]
                require(metadata(path)==plan["guards"][path], "restore reviewed startup configuration before reversing the adapter")
            if action != "reverse":
                for path, expected_meta in plan["guards"].items():
                    require(metadata(path) == expected_meta, f"review dependency changed: {path}")

        def native_check():
            checker = absolute(plan["checker"])
            require(digest(read_regular(checker)[0]) == plan["checker_sha256"], "native checker changed")
            css = absolute(plan["css_candidate"])
            require(digest(read_regular(css, private=True)[0]) == plan["css_sha256"], "CSS candidate changed")
            result = subprocess.run([str(checker), str(css)], capture_output=True, timeout=10)
            require(result.returncode == 0 and not result.stderr, "actual Island parser rejected candidate or emitted diagnostics")
            if plan["kind"] == "startup":
                if action != "reverse":
                    require(digest(read_regular(absolute(plan["generated"]))[0]) == plan["css_sha256"], "generated CSS is not the reviewed profile")
                result = subprocess.run(["/usr/bin/niri", "validate", "-c", str(review / ("original" if action == "reverse" else "candidate"))], capture_output=True, timeout=10)
                require(result.returncode == 0, "native Niri startup candidate rejected")

        check()
        native_check()
        check()
        if action == "check":
            print("PASS hashes, ownership, permissions and native candidate parser; no live files changed")
            return
        with directory(active.parent) as parent:
            parent_info = os.fstat(parent)
            require((parent_info.st_uid, parent_info.st_gid) == (os.getuid(), os.getgid()), "unexpected integration directory ownership")
            require(not parent_info.st_mode & 0o022, "integration directory is writable by others")
            staged = None
            try:
                name = ".chroma-island-" + secrets.token_hex(12) + ".tmp"
                fd = os.open(name, os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW | os.O_CLOEXEC, 0o600, dir_fd=parent)
                staged = name
                with os.fdopen(fd, "wb") as stream:
                    stream.write(payload)
                    stream.flush()
                    os.fchmod(stream.fileno(), mode)
                    os.fsync(stream.fileno())
                with directory(active.parent) as reopened:
                    again = os.fstat(reopened)
                    require((parent_info.st_dev, parent_info.st_ino) == (again.st_dev, again.st_ino), "integration directory changed")
                check()
                os.replace(staged, active.name, src_dir_fd=parent, dst_dir_fd=parent)
                staged = None
                os.fsync(parent)
                require(metadata(active) == {"sha256": digest(payload), "mode": mode}, "post-replacement verification failed; do not retry blindly")
                print(f"PASS {action}: atomic replacement, original mode/ownership retained; no process commands")
            finally:
                if staged is not None:
                    os.unlink(staged, dir_fd=parent)


def prepare_startup(review, manifest_sha256):
    """Only after a separately approved Chroma transaction; writes private metadata."""
    review = absolute(review)
    with directory(review) as fd:
        info = os.fstat(fd)
        require(stat.S_IMODE(info.st_mode) == 0o700 and (info.st_uid, info.st_gid) == (os.getuid(), os.getgid()), "unsafe adapter review directory")
    raw, _ = read_regular(review / "manifest.json", private=True)
    require(digest(raw) == manifest_sha256, "adapter manifest changed")
    plan = json.loads(raw)
    require(plan["kind"] == "adapter", "expected adapter-stage manifest")
    adapter = absolute(plan["active"])
    require(metadata(adapter) == {"sha256": plan["candidate_sha256"], "mode": plan["original"]["mode"]}, "reviewed adapter is not installed")
    generated = absolute(plan["generated"])
    require(digest(read_regular(generated)[0]) == plan["css_sha256"], "transactionally generated profile is not ready; do not copy it manually")
    state_path = absolute(plan["state"])
    state = tomllib.loads(read_regular(state_path, private=True)[0].decode())
    require(state.get("pending") is None and state["current"]["id"] == "boats-preview", "unexpected palette/pending transaction")
    new_head = state["head"]
    require(new_head != plan["old_head"] and re.fullmatch(r"tx-[0-9]+-[0-9]+", new_head), "new reviewed transaction required")
    tx_path = state_path.parent / "transactions" / (new_head + ".toml")
    transaction = tomllib.loads(read_regular(tx_path, private=True)[0].decode())
    require(transaction["parent"] == plan["old_head"], "expected exactly one new transaction")
    previous = tomllib.loads(read_regular(state_path.parent / "transactions" / (plan["old_head"] + ".toml"), private=True)[0].decode())
    require(transaction["before"] == previous["after"], "transaction does not start from the reviewed previous state")
    require(transaction["before"]["id"] == transaction["after"]["id"] == "boats-preview", "palette changed")
    require(transaction["before"]["palette"] == transaction["after"]["palette"] and transaction["after"] == state["current"], "palette/state snapshot changed unexpectedly")
    require(any(e["path"] == str(generated) and e["before"] != e["after"] for e in transaction["entries"]), "Island transaction entry missing")
    for path, snapshot in transaction["before"]["files"].items():
        require({"sha256": digest(bytes(snapshot["bytes"])), "mode": snapshot["mode"]} == plan["guards"][path], "unexpected transaction starting output")
    for path, snapshot in state["current"]["files"].items():
        data, mode = read_regular(absolute(path))
        require(data == bytes(snapshot["bytes"]) and mode == snapshot["mode"], "current state/output mismatch")
    for entry in transaction["entries"]:
        require(entry["before"] == entry["after"] or (entry["path"] == str(generated) and digest(bytes(entry["after"]["bytes"])) == plan["css_sha256"]), "unrelated output changed")
    fixed = {p: meta for p, meta in plan["guards"].items() if p not in (str(generated), str(state_path))}
    for path, expected in fixed.items():
        require(metadata(path) == expected, f"fixed dependency changed: {path}")
    expected_names = {Path(p).name for p in fixed if Path(p).parent == tx_path.parent} | {tx_path.name}
    require({p.name for p in tx_path.parent.iterdir()} == expected_names, "transaction inventory changed")
    startup = absolute(plan["startup_dir"])
    with directory(startup) as fd:
        info = os.fstat(fd)
        require(stat.S_IMODE(info.st_mode) == 0o700 and (info.st_uid, info.st_gid) == (os.getuid(), os.getgid()), "unsafe startup review directory")
    original, _ = read_regular(startup / "original", private=True)
    candidate, _ = read_regular(startup / "candidate", private=True)
    require(digest(original) == fixed[plan["startup_active"]]["sha256"], "startup original changed")
    require(digest(candidate) == plan["startup_candidate_sha256"], "startup candidate changed")
    guards = {**fixed, str(adapter): metadata(adapter), str(generated): metadata(generated), str(state_path): metadata(state_path), str(tx_path): metadata(tx_path)}
    active_meta = guards.pop(plan["startup_active"])
    result = {**plan, "kind": "startup", "active": plan["startup_active"], "original": active_meta, "candidate_sha256": digest(candidate), "guards": guards}
    data = json.dumps(result, sort_keys=True, indent=2).encode() + b"\n"
    with directory(startup) as fd:
        file = os.open("manifest.json", os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW, 0o600, dir_fd=fd)
        with os.fdopen(file, "wb") as stream:
            stream.write(data); stream.flush(); os.fsync(stream.fileno())
        os.fsync(fd)
    print("Prepared private startup manifest", digest(data), "REVIEW THIS NEW HASH BEFORE APPROVING STARTUP INSTALLATION")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("action", choices=("check", "install", "reverse", "prepare-startup"))
    parser.add_argument("--review-dir", required=True)
    parser.add_argument("--manifest-sha256", required=True)
    args = parser.parse_args()
    try:
        if args.action == "prepare-startup":
            prepare_startup(args.review_dir, args.manifest_sha256)
        else:
            run(args.action, args.review_dir, args.manifest_sha256)
    except (OSError, RuntimeError, ValueError, KeyError, TypeError, subprocess.SubprocessError) as error:
        parser.exit(1, f"STOP: {error}\n")


if __name__ == "__main__":
    main()
