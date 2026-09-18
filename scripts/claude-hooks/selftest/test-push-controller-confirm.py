#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""Selftest: push-controller.md must require confirmed delivery for a push, not
just the SendMessage call.

WHAT THIS PROVES, AND WHAT IT CANNOT. This is a TEXT-level check: it proves the
requirement is WRITTEN, not that any given cycle actually confirmed a push. The
second part can only be judged by reading a real report. The limit is named here
rather than hidden, because a selftest whose reach is unstated gets read as
proving more than it does -- the same class the gate conventions call "a check
that does not measure its subject".

Written by the controller window, applied by the integrator: the controller does
not write into the main tree by its own boundary.
"""
import os
import pathlib
import sys


def find_command_file():
    root = os.environ.get("CLAUDE_PROJECT_DIR")
    if root:
        p = pathlib.Path(root) / ".claude" / "commands" / "push-controller.md"
        if p.exists():
            return p
    here = pathlib.Path(__file__).resolve()
    for parent in here.parents:
        p = parent / ".claude" / "commands" / "push-controller.md"
        if p.exists():
            return p
    raise FileNotFoundError(
        "push-controller.md not found via CLAUDE_PROJECT_DIR or parents")


def main():
    cmd_path = find_command_file()
    raw = cmd_path.read_text(encoding="utf-8")
    # Склеиваем пробелы: требование живёт в абзаце, а перенос строки в нём —
    # дело вёрстки, не смысла. Иначе самотест краснел бы от переформатирования.
    text = " ".join(raw.split())
    checks = [
        ("success: true", "требование ссылается на поле success SendMessage"),
        ("не по факту вызова", "требование отличает вызов от подтверждённой доставки"),
        ("не доставлено", "требование называет исход отказа явно"),
    ]
    missing = [desc for needle, desc in checks if needle not in text]
    if missing:
        print("FAIL: push-controller.md не требует подтверждённой доставки толчка:")
        for desc in missing:
            print("  - %s" % desc)
        return 1
    print("OK: push-controller.md требует подтверждённой доставки толчка "
          "(%d/%d признаков найдено)" % (len(checks), len(checks)))
    return 0


if __name__ == "__main__":
    sys.exit(main())
