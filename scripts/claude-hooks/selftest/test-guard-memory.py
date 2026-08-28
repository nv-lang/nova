# -*- coding: utf-8 -*-
u"""Самотест `scripts/claude-hooks/guard-memory.py` (план 276 шаг 6).

ЗАЧЕМ. Хук блокирует запись feedback-заметки памяти без поля `enforcement:` —
правило 231.2 §3: «инцидент класса „забыли правило" кончается не заметкой, а
строкой о МЕХАНИЗМЕ». Сам хук до 2026-08-29 самотеста не имел, и в его же шапке
это было записано честно: «Самотеста в scripts/guards/selftest/ ПОКА НЕТ».
Хук без теста ломается так же молча, как страж без теста, — и заметнее не
становится, потому что руками его никто не запускает.

ЧТО ПРОВЕРЯЕТ — ОБЕ СТОРОНЫ:
  * заметка без `enforcement:` → блок (код 2) с причиной;
  * заметка с полем → пропуск;
  * немашинная форма поля → пропуск (она законна);
  * файл ВНЕ `memory/feedback-*.md` → не судится вовсе;
  * оба разделителя пути (`/` и `\\`) — на Windows приходит второй;
  * мусор на входе → пропуск, а не падение (хук не имеет права ронять запись).

Запуск: `python scripts/claude-hooks/selftest/test-guard-memory.py`
"""
from __future__ import annotations

import json
import os
import subprocess
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
HOOK = os.path.join(HERE, "..", "guard-memory.py")

fails = 0


def ok(name):
    print(u"  ok   %s" % name)


def bad(name, detail):
    global fails
    fails += 1
    sys.stderr.write(u"  FAIL %s: %s\n" % (name, detail))


def run(path, content, raw=None):
    payload = raw if raw is not None else json.dumps(
        {"tool_input": {"file_path": path, "content": content}}, ensure_ascii=False)
    p = subprocess.run([sys.executable, HOOK],
                       input=payload.encode("utf-8"),
                       stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    return p.returncode, p.stderr.decode("utf-8", "replace")


NOTE_NO_ENF = u"---\nname: feedback-x\n---\n\nПравило без механизма.\n"
NOTE_ENF = NOTE_NO_ENF + u"\nenforcement: guard-stop.py, шаг гейта loop\n"
NOTE_ENF_MANUAL = NOTE_NO_ENF + u"\nenforcement: немашинное — судит человек\n"

rc, err = run("memory/feedback-something.md", NOTE_NO_ENF)
if rc == 2 and "enforcement" in err:
    ok(u"feedback-заметка без enforcement — блок, и поле названо")
else:
    bad(u"заметка без enforcement обязана блокироваться", "rc=%s err=%r" % (rc, err[:120]))

rc, _ = run("memory/feedback-something.md", NOTE_ENF)
if rc == 0:
    ok(u"с полем enforcement — пропуск")
else:
    bad(u"ложный блок при наличии поля", "rc=%s" % rc)

rc, _ = run("memory/feedback-something.md", NOTE_ENF_MANUAL)
if rc == 0:
    ok(u"немашинная форма поля законна — пропуск")
else:
    bad(u"honest 'немашинное' обязано проходить", "rc=%s" % rc)

rc, _ = run(u"memory\\feedback-windows-path.md", NOTE_NO_ENF)
if rc == 2:
    ok(u"windows-разделитель пути тоже судится")
else:
    bad(u"путь с обратными слешами обязан судиться (на Windows он такой и приходит)",
        "rc=%s" % rc)

rc, _ = run("memory/reference-something.md", NOTE_NO_ENF)
if rc == 0:
    ok(u"reference-заметка не судится — правило только про feedback")
else:
    bad(u"не-feedback файл не должен блокироваться", "rc=%s" % rc)

rc, _ = run("docs/dev/feedback-note.md", NOTE_NO_ENF)
if rc == 0:
    ok(u"файл вне memory/ не судится")
else:
    bad(u"вне memory/ правило не действует", "rc=%s" % rc)

rc, _ = run(None, None, raw=u"not a json at all")
if rc == 0:
    ok(u"мусор на входе — пропуск, а не падение")
else:
    bad(u"хук не имеет права ронять запись на плохом входе", "rc=%s" % rc)

rc, _ = run(None, None, raw=u"")
if rc == 0:
    ok(u"пустой вход — пропуск")
else:
    bad(u"пустой вход не должен блокировать", "rc=%s" % rc)

print(u"самотест guard-memory: PASS %d FAIL %d" % (8 - fails, fails))
sys.exit(1 if fails else 0)
