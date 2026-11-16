"""Offline native parser check; run via kitty +runpy, without opening a window."""
import sys
from kitty.config import parse_config

errors = []
with open(sys.argv[1], encoding="utf-8") as source:
    parse_config(source, accumulate_bad_lines=errors, allow_geninclude=False)
for error in errors:
    print(error, file=sys.stderr)
raise SystemExit(bool(errors))
