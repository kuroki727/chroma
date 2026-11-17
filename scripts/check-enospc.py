#!/usr/bin/env python3
"""Inject ENOSPC into isolated CLI writes; requires Linux, cc and dynamic libc.

LD_PRELOAD is confined to each tested child. This does not fill a filesystem or
simulate power loss, rename failures, storage firmware or actual disk exhaustion.
"""
import argparse
import os
from pathlib import Path
import subprocess
import tempfile

C_SOURCE = r'''#define _GNU_SOURCE
#include <dlfcn.h>
#include <errno.h>
#include <limits.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>

ssize_t write(int fd, const void *buf, size_t n) {
    static ssize_t (*real_write)(int, const void *, size_t);
    static unsigned matching;
    static int injected;
    if (!real_write) real_write = dlsym(RTLD_NEXT, "write");
    const char *root = getenv("CHROMA_FAULT_ROOT");
    const char *suffix = getenv("CHROMA_FAULT_SUFFIX");
    const char *nth = getenv("CHROMA_FAULT_NTH");
    char descriptor[64], path[PATH_MAX];
    snprintf(descriptor, sizeof descriptor, "/proc/self/fd/%d", fd);
    ssize_t size = readlink(descriptor, path, sizeof path - 1);
    if (size > 0 && root && suffix && nth && !injected) {
        path[size] = 0;
        if (!strncmp(path, root, strlen(root)) && strstr(path, suffix) &&
            ++matching == strtoul(nth, NULL, 10)) {
            injected = 1;
            errno = ENOSPC;
            return -1;
        }
    }
    return real_write(fd, buf, n);
}
'''


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--chroma", type=Path, required=True)
    args = parser.parse_args()
    binary = str(args.chroma.absolute())
    with tempfile.TemporaryDirectory(prefix="chroma-fault-library-") as build:
        source = Path(build) / "enospc.c"
        library = Path(build) / "enospc.so"
        source.write_text(C_SOURCE)
        subprocess.run(["cc", "-shared", "-fPIC", str(source), "-o", str(library), "-ldl"],
                       check=True, timeout=30)
        cases = [("/generated/.chroma-", 1, "first output staging"),
                 ("/generated/.chroma-", 2, "second output staging"),
                 ("/transactions/.chroma-", 1, "backup staging"),
                 ("/state/.chroma-", 1, "pending manifest staging"),
                 ("/state/.chroma-", 2, "final manifest after both commits")]
        for suffix, nth, label in cases:
            with tempfile.TemporaryDirectory(prefix="chroma-enospc-") as directory:
                cfg = Path(directory) / "config"
                (cfg / "templates").mkdir(parents=True)
                (cfg / "generated").mkdir()
                (cfg / "templates/test").write_text("{{ background }}")
                (cfg / "config.toml").write_text("".join(
                    f"[adapters.{name}]\nenabled=true\ntemplate='templates/test'\n"
                    f"output='generated/{name}'\n" for name in ("a", "b")))

                def run(arguments, env=None):
                    return subprocess.run([binary, "--config-dir", str(cfg), *arguments],
                                          capture_output=True, text=True, env=env, timeout=10)

                assert run(["apply", "nord", "--no-reload"]).returncode == 0
                before = {p: p.read_bytes() for p in (cfg / "generated/a",
                          cfg / "generated/b", cfg / "state/state.toml")}
                env = dict(os.environ, LD_PRELOAD=str(library), CHROMA_FAULT_ROOT=directory,
                           CHROMA_FAULT_SUFFIX=suffix, CHROMA_FAULT_NTH=str(nth))
                result = run(["apply", "rustwood", "--no-reload"], env)
                assert result.returncode != 0, (label, result.stdout, result.stderr)
                assert "No space left on device" in result.stderr, (label, result.stderr)
                assert before == {p: p.read_bytes() for p in before}, (label, result.stderr)
                assert len(list((cfg / "generated").iterdir())) == 2
                assert run(["apply", "paper", "--no-reload"]).returncode == 0
                assert run(["rollback", "--no-reload"]).returncode == 0
                assert before == {p: p.read_bytes() for p in before}
                print("PASS injected ENOSPC:", label)


if __name__ == "__main__":
    main()
