#!/usr/bin/env python3
"""Extract English message IDs; --check fails if the committed catalog is stale."""
import json
import re
import sys
from pathlib import Path

root = Path(__file__).resolve().parents[1]
messages = set()
for source in (root / "src").rglob("*.rs"):
    text = source.read_text().split("#[cfg(test)]", 1)[0]
    for match in re.finditer(r'(?:\bt!|\btrf!|crate::t!)\(\s*("(?:\\.|[^"\\])*")', text):
        messages.add(json.loads(match.group(1)))
# These static data tables are translated at their UI boundary.
for source in [root / "src/featured.rs", root / "src/recovery.rs"]:
    for value in re.findall(r'"(?:\\.|[^"\\])*"', source.read_text().split("#[cfg(test)]", 1)[0]):
        value = json.loads(value)
        if value and value[0].isupper():
            messages.add(value)
path = root / "locales/en.json"
# The source catalog doubles as the English translation.
expected = json.dumps({key: key for key in sorted(messages)}, ensure_ascii=False, indent=2) + "\n"
if "--check" in sys.argv:
    if not path.exists() or path.read_text() != expected:
        sys.exit("English catalog is stale; run python3 scripts/update-translations.py")
else:
    path.write_text(expected)
