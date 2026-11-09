"""Checks the shape of the code under src/, as Rune and Hex check theirs:
  - a source file holds at most 800 lines, not counting a `#[cfg(test)] mod tests` block at
    its end; a test file (tests.rs, *_tests.rs) at most 1,000;
  - `x.rs` beside two or more `x_*.rs`, or three or more `x_*.rs` alone, belong in `x/`;
  - a folder holds at most 60 source files.
Prints each file or folder over, and exits 1. Usage: python3 -I scripts/code-shape.py [folder under the app, default src]"""

import re
import sys
from collections import defaultdict
from pathlib import Path

SOURCE, TESTS, FOLDER = 800, 1000, 60
root = Path(__file__).resolve().parent.parent / (sys.argv[1] if len(sys.argv) > 1 else "src")
bad = []


def is_test(path):
    return path.name == "tests.rs" or path.name.endswith("_tests.rs")


def code_lines(path):
    lines = path.read_text().splitlines()
    for i, line in enumerate(lines):
        if line.startswith("#[cfg(test)]") and i + 1 < len(lines) and re.match(r"mod \w+ \{", lines[i + 1]):
            return i
    return len(lines)


folders = defaultdict(list)
for path in sorted(root.rglob("*.rs")):
    folders[path.parent].append(path)
    rel = path.relative_to(root.parent)
    if is_test(path):
        n = len(path.read_text().splitlines())
        if n > TESTS:
            bad.append(f"{rel}: {n} lines; a test file holds at most {TESTS}")
    else:
        n = code_lines(path)
        if n > SOURCE:
            bad.append(f"{rel}: {n} lines of code; at most {SOURCE}. Make it a folder, one file per part (mod.rs keeps the state and `pub use`s each part)")

for folder, paths in folders.items():
    sources = [p for p in paths if not is_test(p)]
    if len(sources) > FOLDER:
        bad.append(f"{folder.relative_to(root.parent)}/: {len(sources)} source files; at most {FOLDER}")
    stems = {p.stem for p in sources}
    family = defaultdict(list)
    for p in sources:
        if "_" in p.stem:
            family[p.stem.split("_")[0]].append(p.name)
    for base, members in family.items():
        if len(members) >= 3 or (len(members) >= 2 and (base in stems or (folder / base).is_dir())):
            bad.append(f"{folder.relative_to(root.parent)}/: {', '.join(sorted(members))} belong in {base}/")

for line in bad:
    print(line)
sys.exit(1 if bad else 0)
