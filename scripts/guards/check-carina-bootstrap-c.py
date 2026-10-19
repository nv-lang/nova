#!/usr/bin/env python3
"""Guard: run the published Carina B1 against the managed raw-C baseline.

Stable interface for CI/hooks (task 106):
  python scripts/guards/check-carina-bootstrap-c.py <source-tree> --asset <published-zip-or-novac.exe> --work-dir <scratchpad>
The release asset and source tree are explicit; missing or mismatched inputs fail.
"""

from __future__ import annotations

import argparse
from pathlib import Path
import subprocess
import sys


ROOT = Path(__file__).resolve().parents[2]
VERIFIER = ROOT / "scripts/tools/verify-carina-bootstrap-c.py"


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("source_tree", type=Path)
    parser.add_argument("--asset", type=Path, required=True,
                        help="explicit published ZIP or its shipped novac.exe")
    parser.add_argument("--work-dir", type=Path, required=True,
                        help="existing scratchpad directory for temporary files")
    args = parser.parse_args()
    result = subprocess.run(
        [sys.executable, str(VERIFIER), str(args.source_tree.resolve()),
         "--asset", str(args.asset.resolve()), "--work-dir", str(args.work_dir.resolve())],
        check=False,
    )
    return result.returncode


if __name__ == "__main__":
    raise SystemExit(main())
