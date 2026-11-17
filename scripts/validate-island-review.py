"""Measure actual native-parser color fields. No desktop connections or writes."""
import json
import subprocess
import sys


def native(checker, path):
    result = subprocess.run([checker, path, "--dump"], capture_output=True, text=True, check=True)
    if result.stderr:
        raise RuntimeError(result.stderr)
    data = {p[0]: list(map(float, p[1:])) for line in result.stdout.splitlines() if (p := line.split())}
    if len(data) != 16 or any(values[3] != 1 for key, values in data.items() if key != "sizes"):
        raise RuntimeError("expected all 15 opaque color fields and size metadata")
    return data


def hover(rgb):
    return [(round((x * .94 + .06) * 255) / 255) for x in rgb[:3]] + [1]


def contrast(a, b):
    def lum(rgb):
        return sum(w * (x / 12.92 if x <= .04045 else ((x + .055) / 1.055) ** 2.4) for w, x in zip((.2126, .7152, .0722), rgb))
    lo, hi = sorted((lum(a), lum(b)))
    return (hi + .05) / (lo + .05)


def evaluate(original, data):
    if data["sizes"] != original["sizes"]:
        raise RuntimeError("font sizes or border width changed")
    surfaces = [data[k] for k in ("background", "tile", "tile-on")] + [hover(data[k]) for k in ("tile", "tile-on")]
    rows = {}
    for text in ("time", "date", "text", "title", "body", "label", "value", "flash"):
        rows[text] = min(contrast(data[text], bg) for bg in surfaces)
        if rows[text] < 4.5:
            raise RuntimeError(f"text contrast failed: {text}")
    rows["danger text"] = min(contrast(data["text"], bg) for bg in (data["dnd"], hover(data["dnd"])))
    if rows["danger text"] < 4.5:
        raise RuntimeError("danger text contrast failed")
    for key in ("border", "dnd", "track"):
        rows[key + "/background"] = contrast(data[key], data["background"])
    for key in ("fill", "title", "label"):
        rows[key + "/track"] = contrast(data[key], data["track"])
    if min(value for key, value in rows.items() if "/" in key) < 3:
        raise RuntimeError("indicator contrast failed")
    return {"colors": {k: "#" + "".join(f"{round(x * 255):02X}" for x in v[:3]) for k, v in data.items() if k != "sizes"}, "contrast": rows}


if __name__ == "__main__":
    checker, source, *candidates = sys.argv[1:]
    original = native(checker, source)
    print(json.dumps({path: evaluate(original, native(checker, path)) for path in candidates}, indent=2))
