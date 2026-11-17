#!/usr/bin/env python3
"""Guard an explicitly reviewed Waybar stylesheet. Never reloads applications.

check is read-only. install and reverse require separate user authorization.
The private review directory contains manifest.json, original.css and candidate.css.
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
        require(active.name == "style.css" and active.parent.name == "waybar", "not a Waybar stylesheet")
        original, _ = read_regular(review / "original.css", private=True)
        candidate, _ = read_regular(review / "candidate.css", private=True)
        require(digest(original) == plan["original"]["sha256"], "original backup changed")
        require(digest(candidate) == plan["candidate_sha256"], "candidate changed")
        mode = plan["original"]["mode"]
        require(mode & 0o022 == 0 and mode & 0o7000 == 0, "unsafe original mode")
        expected = plan["candidate_sha256"] if action == "reverse" else plan["original"]["sha256"]
        payload = original if action == "reverse" else candidate

        def check():
            require(metadata(active) == {"sha256": expected, "mode": mode}, "active stylesheet changed; preserve edits and review manually")
            if action != "reverse":
                for path, expected_meta in plan["guards"].items():
                    require(metadata(path) == expected_meta, f"review dependency changed: {path}")

        check()
        if action == "check":
            print("PASS hashes, ownership, permissions and dependencies; no files changed")
            return
        with directory(active.parent) as parent:
            parent_info = os.fstat(parent)
            require((parent_info.st_uid, parent_info.st_gid) == (os.getuid(), os.getgid()), "unexpected Waybar directory ownership")
            require(not parent_info.st_mode & 0o022, "Waybar directory is writable by others")
            staged = None
            try:
                name = ".chroma-waybar-" + secrets.token_hex(12) + ".tmp"
                fd = os.open(name, os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW | os.O_CLOEXEC, 0o600, dir_fd=parent)
                staged = name
                with os.fdopen(fd, "wb") as stream:
                    stream.write(payload)
                    stream.flush()
                    os.fchmod(stream.fileno(), mode)
                    os.fsync(stream.fileno())
                with directory(active.parent) as reopened:
                    again = os.fstat(reopened)
                    require((parent_info.st_dev, parent_info.st_ino) == (again.st_dev, again.st_ino), "Waybar directory changed")
                check()
                os.replace(staged, active.name, src_dir_fd=parent, dst_dir_fd=parent)
                staged = None
                os.fsync(parent)
                require(metadata(active) == {"sha256": digest(payload), "mode": mode}, "post-replacement verification failed; do not retry blindly")
                print(f"PASS {action}: atomic replacement, original mode/ownership retained; no process commands")
            finally:
                if staged is not None:
                    os.unlink(staged, dir_fd=parent)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("action", choices=("check", "install", "reverse"))
    parser.add_argument("--review-dir", required=True)
    parser.add_argument("--manifest-sha256", required=True)
    args = parser.parse_args()
    try:
        run(args.action, args.review_dir, args.manifest_sha256)
    except (OSError, RuntimeError, ValueError, KeyError, TypeError) as error:
        parser.exit(1, f"STOP: {error}\n")


if __name__ == "__main__":
    main()
