"""Real native/parser/transaction/activation roundtrip, entirely in a temp fixture.

Usage: python3 scripts/test-island-native.py CHROMA RENDERER NATIVE_CHECKER
"""
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import tomllib

spec=importlib.util.spec_from_file_location("guard",Path(__file__).with_name("island-candidate.py"))
guard=importlib.util.module_from_spec(spec);spec.loader.exec_module(guard)
chroma,renderer,checker=map(lambda p:str(Path(p).absolute()),sys.argv[1:])
with tempfile.TemporaryDirectory(prefix="chroma-island-native-") as temp:
    base=Path(temp);config=base/'chroma';state=base/'state'
    def cli(*args):subprocess.run([chroma,'--config-dir',str(config),'--state-dir',str(state),*args],check=True,capture_output=True)
    cli('init')
    source=base/'style.css';source.write_bytes((Path(__file__).parent.parent/'tests/fixtures/desktop/island/style.css').read_bytes())
    adapter=config/'adapters/desktop-island.toml';adapter.parent.mkdir(exist_ok=True,mode=0o700)
    original=f'enabled=true\ndesktop="island"\nconfig_path={json.dumps(str(source))}\noutput="generated/desktop-island.css"\n'.encode()
    adapter.write_bytes(original);adapter.chmod(0o600)
    cli('apply','rustwood','--no-reload')
    old_state=tomllib.loads((state/'state.toml').read_text());old_head=old_state['head']
    # The transition guard is intentionally tied to one unchanged palette ID.
    # Use an isolated alias containing the existing Rustwood palette.
    (config/'palettes/boats-preview.toml').write_bytes((config/'palettes/rustwood.toml').read_bytes())
    cli('apply','boats-preview','--no-reload');old_head=tomllib.loads((state/'state.toml').read_text())['head']
    generated=config/'generated/desktop-island.css'
    review=base/'review';review.mkdir(mode=0o700)
    for directory in ('adapter','startup'):(review/directory).mkdir(mode=0o700)
    css=subprocess.check_output([renderer,str(source),str(config/'palettes/boats-preview.toml')])
    def save(path,data,mode=0o600):
        p=review/path;p.write_bytes(data);p.chmod(mode);return p
    native=save('checker',Path(checker).read_bytes(),0o700);css_path=save('profile.css',css)
    updated=original+b'island_readable=true\n';save('adapter/original',original);save('adapter/candidate',updated)
    niri=base/'niri/config.kdl';niri.parent.mkdir();oldn=b'spawn-at-startup "/home/kuroki/.local/bin/island"\n';niri.write_bytes(oldn);niri.chmod(0o644)
    newn=oldn.rstrip()+b' "-s" "/home/kuroki/.config/chroma/generated/desktop-island.css"\n';save('startup/original',oldn);save('startup/candidate',newn)
    guards=[source,niri,generated,config/'config.toml',state/'state.toml',*list((state/'transactions').iterdir())]
    plan={'version':1,'kind':'adapter','active':str(adapter),'original':guard.metadata(adapter),'candidate_sha256':guard.digest(updated),'guards':{str(p):guard.metadata(p) for p in guards},'checker':str(native),'checker_sha256':guard.digest(native.read_bytes()),'css_candidate':str(css_path),'css_sha256':guard.digest(css),'generated':str(generated),'state':str(state/'state.toml'),'old_head':old_head,'startup_dir':str(review/'startup'),'startup_active':str(niri),'startup_candidate_sha256':guard.digest(newn)}
    manifest=save('adapter/manifest.json',json.dumps(plan).encode());pin=guard.digest(manifest.read_bytes())
    guard.run('check',review/'adapter',pin);guard.run('install',review/'adapter',pin)
    cli('apply','boats-preview','--no-reload');assert generated.read_bytes()==css
    guard.prepare_startup(review/'adapter',pin)
    startup_pin=guard.digest((review/'startup/manifest.json').read_bytes())
    guard.run('check',review/'startup',startup_pin);guard.run('install',review/'startup',startup_pin)
    try:guard.run('reverse',review/'adapter',pin);raise AssertionError('unsafe reversal accepted')
    except RuntimeError:pass
    guard.run('reverse',review/'startup',startup_pin);guard.run('reverse',review/'adapter',pin)
    assert niri.read_bytes()==oldn and adapter.read_bytes()==original
    assert source.read_bytes()==(Path(__file__).parent.parent/'tests/fixtures/desktop/island/style.css').read_bytes()
    print('PASS native two-stage configure/apply/prepare/connect/reverse fixture; no desktop processes launched')
