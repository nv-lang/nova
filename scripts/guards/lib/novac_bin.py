"""scripts/guards/lib/novac_bin.py -- the python half of the door `novac_bin` (lib/novac.sh).

Registry 221.1 #1607 (docs/plans/221.1-bug-sweep.md): which file is Carina is decided in ONE
place. NOVAC from the environment first; else the NEWER of novac/target/novac.exe and
novac/target/novac (a Linux build writes `novac`, and a stale `novac.exe` used to win by
name); else the build path of this platform. Keep it in step with lib/novac.sh.
"""
import os
import pathlib


def novac_bin_out(root):
    """Where Carina is BUILT: `novac.exe` on Windows, `novac` elsewhere."""
    name = "novac.exe" if os.name == "nt" else "novac"
    return pathlib.Path(root) / "novac" / "target" / name


def novac_bin(root):
    """The Carina binary to RUN."""
    env = os.environ.get("NOVAC")
    if env:
        return pathlib.Path(env)
    target = pathlib.Path(root) / "novac" / "target"
    found = [p for p in (target / "novac.exe", target / "novac") if p.is_file()]
    if len(found) == 2:
        return max(found, key=lambda p: p.stat().st_mtime)
    if found:
        return found[0]
    return novac_bin_out(root)
