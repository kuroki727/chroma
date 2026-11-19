#!/usr/bin/env python3
"""Offline end-to-end fixture using real Chroma, Island parser and ctl client.

Usage: python3 scripts/check-wallpaper-island.py CHROMA CHECKER ISLAND
No desktop application is launched; the IPC endpoint is a private test socket.
"""
import json
import os
from pathlib import Path
import socket
import struct
import subprocess
import sys
import tempfile
import zlib


def main():
    chroma, checker, island = [str(Path(p).resolve(strict=True)) for p in sys.argv[1:]]
    repo = Path(__file__).resolve().parent.parent
    with tempfile.TemporaryDirectory(prefix="chroma-island-sync-") as temp:
        root = Path(temp)
        config, state, runtime = [root / p for p in ("config", "state", "runtime")]
        runtime.mkdir(mode=0o700)
        env = dict(os.environ, HOME=temp, XDG_CONFIG_HOME=str(root/"xdg"),
                   XDG_STATE_HOME=str(root/"xdg-state"), XDG_RUNTIME_DIR=str(runtime))
        for key in ("WAYLAND_DISPLAY", "DISPLAY", "NIRI_SOCKET"):
            env.pop(key, None)

        def cli(*args, ok=True):
            result = subprocess.run([chroma, "--config-dir", str(config), "--state-dir", str(state), *map(str,args)],
                                    env=env, capture_output=True, text=True, timeout=60)
            if ok and result.returncode:
                raise RuntimeError(result.stdout + result.stderr)
            if not ok:
                assert result.returncode, result.stdout
            return result

        cli("init")
        source = root/"style.css"
        source.write_bytes((repo/"tests/fixtures/desktop/island/style.css").read_bytes())
        alpha = {key:255 for key in ("background","text","prompt","placeholder","input","match","selection","selection_text","selection_match","border")}
        config.joinpath("config.toml").write_text(
            '[general]\nauto_reload=true\n[wallpaper]\nenabled=true\ndebounce_ms=0\n'
            '[adapters.waybar]\nenabled=true\n[adapters.kitty]\nenabled=true\n'
            '[adapters.fuzzel]\nenabled=true\nalpha={' + ','.join(f'{k}={v}' for k,v in alpha.items()) + '}\n'
            '[adapters.gtk]\nenabled=true\ntemplate="templates/gtk.css"\noutput="generated/gtk.css"\n')
        config.joinpath("templates/gtk.css").write_bytes((repo/"templates/waybar.css").read_bytes())
        adapter = config/"adapters/desktop-island.toml"
        adapter.write_text('enabled=true\ndesktop="island"\nisland_readable=true\n'
                           f'config_path={json.dumps(str(source))}\noutput="generated/desktop-island.css"\n'
                           f'validate=[{json.dumps(checker)},"{{file}}"]\n'
                           f'reload=[{json.dumps(island)},"ctl","reload"]\n')
        def chunk(name,data):
            return struct.pack('>I',len(data))+name+data+struct.pack('>I',zlib.crc32(name+data))
        def png(path,color):
            path.write_bytes(b'\x89PNG\r\n\x1a\n'+chunk(b'IHDR',struct.pack('>IIBBBBB',48,32,8,2,0,0,0))+
                             chunk(b'IDAT',zlib.compress((b'\0'+bytes(color)*48)*32))+chunk(b'IEND',b''))
        a,b = root/'a.png',root/'b.png'
        png(a,[40,120,65]);png(b,[190,75,30])
        with socket.socket(socket.AF_UNIX,socket.SOCK_DGRAM) as server:
            server.bind(str(runtime/'island.sock'));server.settimeout(0.1)
            def changed(path):
                cli('wallpaper','changed',path)
                assert server.recv(2001)==b'reload'
                css = config/'generated/desktop-island.css'
                result = subprocess.run([checker,str(css),'--dump'],capture_output=True,check=True)
                assert not result.stderr
                assert len(result.stdout.splitlines())==16
            changed(a)
            before = (state/'state.toml').read_bytes()
            cli('wallpaper','changed',a)
            assert (state/'state.toml').read_bytes()==before
            try:
                server.recv(2001)
                raise AssertionError('duplicate reload')
            except TimeoutError:
                pass
            changed(b)
            cli('rollback')
            assert server.recv(2001)==b'reload'
            before = (state/'state.toml').read_bytes()
            outputs = {p:p.read_bytes() for p in (config/'generated').iterdir()}
            a.write_bytes(b'broken image')
            cli('wallpaper','changed',a,ok=False)
            assert (state/'state.toml').read_bytes()==before
            assert all(p.read_bytes()==data for p,data in outputs.items())
            server.close()
            (runtime/'island.sock').unlink()
            # Actual ctl send failure must roll the entire file transaction back.
            cli('wallpaper','changed',b,ok=False)
            assert (state/'state.toml').read_bytes()==before
            assert all(p.read_bytes()==data for p,data in outputs.items())
        print('PASS five adapters (Waybar/Fuzzel/Kitty/GTK/Island), actual parser/ctl, deduplication, invalid decode, IPC failure recovery and rollback; no live desktop')


if __name__ == '__main__':
    main()
