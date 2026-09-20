#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""Selftest: push-controller.md must push EVERY stalled session, with no
verbal exception left standing.

WHY THIS EXISTS. Until 2026-09-20 the file carved two exceptions ("frozen
with a stated condition", "queue empty, said in words") out of the blanket
push rule. Applying either exception required the controller to read the
full text of a session's last turn looking for a stop code or a phrase --
i.e. to look at WHAT it stopped on, the exact thing the surrounding rule
forbids. The owner named this directly: asked why a 10-minute-silent window
was not pushed, got "her own word, legitimate hold" as the answer, and
pointed out that the reading had crept back in through the exception door.
The fix removes the exceptions rather than tightening them, for the same
reason the cause-analysis was removed in 2026-09-18: every added condition
is one more chance to stay silent, and the owner has judged that a stalled
window costs more than a spurious push.

WHAT THIS PROVES, AND WHAT IT CANNOT. Text-level check only: it proves the
rule is WRITTEN as unconditional, not that any given cycle actually pushed
without reading. The second part is only visible in a real report. Named
here rather than hidden, per the same convention as
test-push-controller-confirm.py.

Written by the controller window, applied by the integrator: the controller
does not write into the main tree by its own boundary.
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
    text = " ".join(raw.split())

    # Требование должно быть написано словами "ВСЕГДА" / "без исключений" —
    # присутствие заголовка доказывает, что раздел не переименован обратно.
    required = [
        ("ВСЕГДА, БЕЗ ИСКЛЮЧЕНИЙ", "заголовок раздела снят с условия"),
        ("Исключений больше нет", "явно объявлено, что исключений нет"),
    ]
    missing = [desc for needle, desc in required if needle not in text]

    # Старые формулировки исключений не имеют права остаться ЖИВЫМ правилом:
    # они могут упоминаться только в историческом контексте (тот же абзац,
    # что объявляет их снятыми), а не как отдельное активное условие
    # "Не толкать только когда...".
    forbidden = [
        ("**Не толкать** только когда", "старое условие «не толкать» осталось активным"),
        ("Статус, отличный от «встал» и «стоит законно»",
         "статус «стоит законно» всё ещё в словаре"),
    ]
    leftover = [desc for needle, desc in forbidden if needle in text]

    if missing or leftover:
        print("FAIL: push-controller.md не закрывает исключения из толчка:")
        for desc in missing:
            print("  - отсутствует: %s" % desc)
        for desc in leftover:
            print("  - осталось активным: %s" % desc)
        return 1
    print("OK: push-controller.md толкает без исключений "
          "(%d/%d требований, %d/%d старых формулировок снято)"
          % (len(required) - len(missing), len(required),
             len(forbidden) - len(leftover), len(forbidden)))
    return 0


if __name__ == "__main__":
    sys.exit(main())
