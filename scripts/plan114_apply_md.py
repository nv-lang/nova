#!/usr/bin/env python3
"""Apply plan114_rewrite.py к docs/ + spec/ исключая history/."""
import subprocess
import sys
from pathlib import Path

files = []
for d in (Path('docs'), Path('spec')):
    if not d.exists():
        continue
    for f in d.rglob('*.md'):
        s = str(f).replace('\\', '/')
        if 'history' in s:
            continue
        files.append(str(f))

print(f"Files to process: {len(files)}")
result = subprocess.run(
    [sys.executable, 'scripts/plan114_rewrite.py', '--ext', '.md'] + files,
    capture_output=True, text=True,
)
print(result.stdout)
if result.returncode != 0:
    sys.stderr.write(result.stderr)
    sys.exit(result.returncode)
