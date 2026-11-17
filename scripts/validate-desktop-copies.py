#!/usr/bin/env python3
"""Offline T470 parser checks. Writes only to a private temporary directory.

Requires the five inspected desktop files, native validators, GTK3 development
files, and the standalone Island source. Never launches a desktop application.
"""
import argparse
import hashlib
import os
from pathlib import Path
import re
import shlex
import stat
import subprocess
import tempfile

REPO = Path(__file__).resolve().parent.parent
FILES = ("niri/config.kdl", "waybar/style.css", "fuzzel/fuzzel.ini",
         "kitty/kitty.conf", "island/style.css")


def snapshot(path):
    for component in (path, *path.parents):
        if component.is_symlink():
            raise RuntimeError(f"symlink requires explicit review: {component}")
    info = path.stat()
    if not stat.S_ISREG(info.st_mode) or info.st_size > 4 * 1024 * 1024:
        raise RuntimeError(f"expected a regular file <=4 MiB: {path}")
    data = path.read_bytes()
    return data, stat.S_IMODE(info.st_mode), info.st_uid, info.st_gid


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--chroma", type=Path, default=REPO / "target/release/chroma")
    parser.add_argument("--desktop-dir", type=Path, required=True,
                        help="explicit root confirmed by read-only inspection")
    parser.add_argument("--island-source", type=Path, required=True,
                        help="directory containing the inspected config.hpp")
    args = parser.parse_args()
    chroma = str(args.chroma.absolute())
    desktop = args.desktop_dir.absolute()
    originals = {name: snapshot(desktop / name) for name in FILES}
    # Native parsers can follow includes. Refuse all possible chains before
    # invoking them, including escaped KDL spellings; raw regex strings in the
    # inspected Niri copy are safe only when no includes/escapes are present.
    niri = originals["niri/config.kdl"][0].decode()
    if "include" in niri or "\\" in niri:
        raise RuntimeError("Niri include/escape requires manual composed validation")
    kitty = originals["kitty/kitty.conf"][0].decode()
    if re.search(r"\b(?:include|globinclude|envinclude|geninclude)\b", kitty):
        raise RuntimeError("Kitty include chain requires manual composition")
    with tempfile.TemporaryDirectory(prefix="chroma-t470-parsers-") as directory:
        root = Path(directory)
        env = os.environ.copy()
        for key in ("WAYLAND_DISPLAY", "DISPLAY", "NIRI_SOCKET"):
            env.pop(key, None)
        for key, name in (("XDG_CONFIG_HOME", "xdg-config"),
                          ("XDG_STATE_HOME", "xdg-state"),
                          ("XDG_CACHE_HOME", "xdg-cache")):
            (root / name).mkdir(mode=0o700)
            env[key] = str(root / name)

        def run(argv):
            result = subprocess.run(list(map(str, argv)), env=env, text=True,
                                    capture_output=True, timeout=30)
            if result.returncode:
                raise RuntimeError(f"{argv[0]} failed ({result.returncode}):\n"
                                   f"{result.stdout}\n{result.stderr}")
            return result.stdout

        cfg = root / "chroma"
        copies = root / "desktop"
        # Bundle Niri's simple colors separately; compose them with the full,
        # inspected Niri config below. The bundled scanner deliberately refuses
        # raw KDL strings and is not bypassed or weakened in production.
        for name, (data, *_rest) in originals.items():
            if name.startswith("niri/"):
                continue
            target = copies / name
            target.parent.mkdir(parents=True, mode=0o700, exist_ok=True)
            target.write_bytes(data)
        command = [chroma, "--config-dir", cfg]
        run(command + ["init"])
        run(command + ["integrate", "--apply", "--confirm", "--desktop-dir", copies])
        adapters = {p: p.read_bytes() for p in (cfg / "adapters").glob("desktop-*.toml")}
        for adapter in adapters:
            adapter.write_text(adapter.read_text().replace("enabled = false", "enabled = true"))
        (cfg / "config.toml").write_text("[adapters.niri]\nenabled = true\n")
        gtk = root / "check-gtk"
        flags = shlex.split(run(["pkg-config", "--cflags", "--libs", "gtk+-3.0"]))
        run(["cc", REPO / "scripts/check-gtk-css.c", "-o", gtk, *flags])
        island = root / "check-island"
        run(["c++", "-std=c++17", "-I", args.island_source.absolute(),
             REPO / "scripts/check-island-parser.cpp", "-o", island])
        generated = cfg / "generated"
        baseline = {p: p.read_bytes() for p in generated.iterdir()}
        ids = run(command + ["list", "--names-only"]).splitlines()
        if len(ids) != 6:
            raise RuntimeError(f"expected six presets, found {ids}")
        for palette in ids:
            run(command + ["apply", palette, "--dry-run", "--no-reload"])
            run(command + ["apply", palette, "--no-reload"])
            after = {p: p.read_bytes() for p in generated.iterdir()}
            run(command + ["apply", palette, "--no-reload"])
            assert after == {p: p.read_bytes() for p in generated.iterdir()}
            composed = root / "niri.kdl"
            composed.write_text(niri + f'\ninclude "{generated / "niri.kdl"}"\n')
            run(["niri", "validate", "-c", composed])
            run([gtk, generated / "preview-waybar.css"])
            run(["fuzzel", "--check-config", "--config",
                 generated / "desktop-fuzzel.ini", "--log-no-syslog"])
            composed = root / "kitty.conf"
            composed.write_text(kitty + f'\ninclude {generated / "desktop-kitty.conf"}\n')
            run(["kitty", "+runpy",
                 "import runpy,sys; sys.argv=sys.argv[1:]; "
                 "runpy.run_path(sys.argv[0],run_name='__main__')",
                 REPO / "scripts/check-kitty-config.py", composed])
            run([island, generated / "desktop-island.css"])
            run(command + ["rollback", "--no-reload"])
            run(command + ["rollback", "--no-reload"])
            assert baseline == {p: p.read_bytes() for p in generated.iterdir()}
            assert not (generated / "niri.kdl").exists()
            print(f"PASS {palette}: five native parsers, repeat apply, two rollbacks")
        # Restore this harness's explicit enable edits before undoing the bundle;
        # its journal correctly refuses to discard edited adapter configuration.
        for adapter, data in adapters.items():
            adapter.write_bytes(data)
        run(command + ["integrate", "--rollback"])
        assert not list(generated.iterdir())
    assert originals == {name: snapshot(desktop / name) for name in FILES}
    print("PASS source bytes, modes and ownership unchanged; temporary bundle removed")
    print("GTK3 CSS checked via GTK provider; no GTK settings adapter or visual check")
    print("Island source parser SHA256:", hashlib.sha256(
        (args.island_source / "config.hpp").read_bytes()).hexdigest())


if __name__ == "__main__":
    main()
