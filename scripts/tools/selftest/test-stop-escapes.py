#!/usr/bin/env python
# -*- coding: utf-8 -*-
u"""Самотест чтения побегов (план 292 Ш.5). Клетки ПАРАМИ.

Пара нужна затем же, зачем у хука: читатель, который всегда молчит, и читатель,
который всегда кричит, одинаково бесполезны, и одиночная клетка не отличает их
от верного.
"""

import io
import os
import subprocess
import sys
import tempfile
import time

HERE = os.path.dirname(os.path.abspath(__file__))
TOOL = os.path.join(os.path.dirname(HERE), u"stop-escapes.py")

ok = fail = 0


def run(root, *args):
    r = subprocess.run([sys.executable, TOOL, root] + list(args), capture_output=True)
    return r.returncode, r.stdout.decode("utf-8", "replace")


def make(root, lines):
    d = os.path.join(root, u"target", u".stop-guard")
    os.makedirs(d, exist_ok=True)
    with io.open(os.path.join(d, u"escapes.log"), "w", encoding="utf-8") as fh:
        fh.write(u"\n".join(lines) + u"\n")


def cell(name, predicate):
    global ok, fail
    try:
        verdict, detail = predicate()
    except Exception as e:
        verdict, detail = False, u"исключение: %s: %s" % (type(e).__name__, e)
    if verdict:
        ok += 1
        print(u"  PASS  %-44s %s" % (name, detail))
    else:
        fail += 1
        print(u"  FAIL  %-44s %s" % (name, detail))


def stamp(offset_sec):
    return time.strftime("%Y-%m-%d %H:%M", time.localtime(time.time() + offset_sec))


tmp = tempfile.mkdtemp(prefix="stop-escapes-selftest-")

# --- пара 1: свежий побег виден, старый не виден -------------------------
fresh_dir = os.path.join(tmp, u"fresh")
make(fresh_dir, [u"%s Снимок очереди НЕПОЛЕН (ok=false): зеркало nosuchremote" % stamp(-600)])
old_dir = os.path.join(tmp, u"old")
make(old_dir, [u"%s побег трёхдневной давности" % stamp(-3 * 24 * 3600)])


def c_fresh():
    rc, o = run(fresh_dir)
    return (rc == 1 and u"ПОБЕГИ" in o and u"nosuchremote" in o), u"rc=%d" % rc


def c_old_out_of_window():
    rc, o = run(old_dir)
    return (rc == 0 and u"нет" in o), u"rc=%d (старый за окном суток)" % rc


cell(u"свежий побег виден и rc=1", c_fresh)
cell(u"побег старше суток не виден и rc=0", c_old_out_of_window)


# --- пара 2: тишина не выдумывается, но и не скрывается -------------------
empty_dir = os.path.join(tmp, u"empty")
os.makedirs(empty_dir, exist_ok=True)


def c_no_log_is_not_escape():
    rc, o = run(empty_dir)
    return (rc == 0 and u"нет" in o), u"лога нет -> не побег, rc=%d" % rc


def c_quiet_is_silent_but_not_deaf():
    rc_q, o_q = run(empty_dir, u"--quiet")
    rc_f, o_f = run(fresh_dir, u"--quiet")
    return (rc_q == 0 and o_q.strip() == u"" and rc_f == 1 and u"ПОБЕГИ" in o_f), \
        u"молчит в тишине, говорит при побеге"


cell(u"лога нет — это НЕ побег", c_no_log_is_not_escape)
cell(u"--quiet молчит в тишине, но не при побеге", c_quiet_is_silent_but_not_deaf)


# --- пара 3: строка неизвестной формы не теряется -------------------------
weird_dir = os.path.join(tmp, u"weird")
make(weird_dir, [u"строка без метки времени вообще"])


def c_unparsed_still_counts():
    rc, o = run(weird_dir)
    return (rc == 1 and u"без метки" in o), u"неразобранная строка всё равно побег, rc=%d" % rc


def c_hours_window_moves():
    rc, o = run(old_dir, u"--hours", u"96")
    return (rc == 1), u"окно шире — старый побег виден, rc=%d" % rc


cell(u"строка неизвестной формы НЕ выброшена", c_unparsed_still_counts)
cell(u"--hours расширяет окно", c_hours_window_moves)

print(u"PASS %d  FAIL %d" % (ok, fail))
sys.exit(1 if fail else 0)
