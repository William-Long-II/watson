#!/usr/bin/env python3
"""Example Watson script command: text case converter.

Settings → Script Commands → Name "Case", Keyword "case",
Script path pointing at this file. Then type `case Hello World`.
Each row copies its variant to the clipboard when selected.
"""
import json
import re
import sys

text = sys.argv[1] if len(sys.argv) > 1 else ""
words = re.findall(r"[A-Za-z0-9]+", text)

variants = [
    ("UPPER", text.upper()),
    ("lower", text.lower()),
    ("Title", text.title()),
    ("snake_case", "_".join(w.lower() for w in words)),
    ("kebab-case", "-".join(w.lower() for w in words)),
    ("camelCase", words[0].lower() + "".join(w.title() for w in words[1:]) if words else ""),
]

items = [
    {
        "id": label,
        "title": value,
        "subtitle": f"{label} · Enter to copy",
        "icon": "🔤",
        "action": {"type": "copy_clipboard", "content": value},
    }
    for label, value in variants
    if value
]

print(json.dumps({"version": 1, "items": items}))
