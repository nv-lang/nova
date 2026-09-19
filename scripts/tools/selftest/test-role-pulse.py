#!/usr/bin/env python
# -*- coding: utf-8 -*-
u"""Самотест пульса (план 292 Ш.6). Клетки ПАРАМИ.

Приёмка плана дословно: «порог занижен до минуты на пробу — зов приходит;
возвращён — молчит». Обе половины здесь, и обе обязательны: зов, который
приходит всегда, будет снят на второй день, а не приходящий никогда —
неотличим от выключенного.
"""

import io
import os
import subprocess
import sys
import tempfile
import time

HERE = os.path.dirname(os.path.abspath(__file__))
TOOLS = os.path.dirname(HERE)
TOOL = os.path.join(TOOLS, u"role-pulse.py")
NOTES = [
    os.path.join(u"docs", u"dev", u"prompts", u"integrator-handoff.md"),
    os.path.join(u"docs", u"dev", u"prompts", u"carina-handoff.md"),
    os.path.join(u"docs", u"dev", u"prompts", u"controller-handoff.md"),
]

ok = fail = 0


def cell(name, predicate):
    global ok, fail
    try:
        verdict, detail = predicate()
    except Exception as e:
        verdict, detail = False, u"исключение: %s: %s" % (type(e).__name__, e)
    if verdict:
        ok += 1
        print(u"  PASS  %-46s %s" % (name, detail))
    else:
        fail += 1
        print(u"  FAIL  %-46s %s" % (name, detail))


def run(root, max_age=None):
    env = dict(os.environ)
    if max_age is not None:
        env["NOVA_PULSE_MAX_AGE_SEC"] = str(max_age)
    else:
        env.pop("NOVA_PULSE_MAX_AGE_SEC", None)
    r = subprocess.run([sys.executable, TOOL, root], capture_output=True, env=env)
    return r.returncode, r.stdout.decode("utf-8", "replace")


def make_tree(ages):
    u"""ages: возраст каждой записки в секундах (None — записки нет)."""
    root = tempfile.mkdtemp(prefix="nova-pulse-")
    os.makedirs(os.path.join(root, u"docs", u"dev", u"prompts"), exist_ok=True)
    for rel, age in zip(NOTES, ages):
        if age is None:
            continue
        p = os.path.join(root, rel)
        io.open(p, "w", encoding="utf-8").write(u"# записка\n")
        t = time.time() - age
        os.utime(p, (t, t))
    return root


FRESH = make_tree([30, 30, 30])
STALE = make_tree([30, 7200, 30])
NONE_AT_ALL = make_tree([None, None, None])


# --- пара 1: порог в минуту зовёт, порог по умолчанию молчит --------------
def c_low_threshold_calls():
    rc, o = run(make_tree([30, 120, 30]), max_age=60)
    return (rc == 1 and u"ВЛАДЕЛЬЦУ" in o and u"carina" in o), u"rc=%d" % rc


def c_restored_threshold_silent():
    rc, o = run(make_tree([30, 120, 30]))
    return (rc == 0 and u"ВЛАДЕЛЬЦУ" not in o), u"rc=%d, порог вернули — молчит" % rc


cell(u"порог в минуту: зов приходит и называет роль", c_low_threshold_calls)
cell(u"порог возвращён: молчит (приёмка Ш.6 дословно)", c_restored_threshold_silent)


# --- пара 2: молчащая роль названа, живые не оболганы ---------------------
def c_stale_named():
    rc, o = run(STALE)
    return (rc == 1 and u"carina" in o and u"2ч" in o), u"rc=%d" % rc


def c_fresh_all_quiet():
    rc, o = run(FRESH)
    return (rc == 0 and u"пульс: все роли в пределах порога" in o), u"rc=%d" % rc


cell(u"молчащая роль названа с возрастом", c_stale_named)
cell(u"все свежие — зова нет, но ответ ЕСТЬ", c_fresh_all_quiet)


# --- пара 3: отсутствие записки не есть смерть роли -----------------------
def c_missing_is_not_death():
    u"""Записки нет вовсе — роль просто не поднималась в этом дереве. Считать
    это смертью значит звать владельца на пустом месте каждый час."""
    rc, o = run(NONE_AT_ALL)
    return (rc == 0 and u"отсутствует 3" in o), u"rc=%d" % rc


def c_missing_vs_stale_differ():
    rc_m, o_m = run(NONE_AT_ALL)
    rc_s, o_s = run(STALE)
    return (rc_m == 0 and rc_s == 1), u"нет записки != записка протухла"


cell(u"записки нет — это НЕ смерть роли", c_missing_is_not_death)
cell(u"отсутствие и протухание различаются", c_missing_vs_stale_differ)


# --- пара 4: список ролей не разошёлся с напоминанием ---------------------
def c_roles_match_reminder():
    u"""Два файла держат список ролей. Разойдясь, они дадут пульс по роли, о
    которой напоминание молчит, — и наоборот; поймать это должен самотест, а
    не человек при чтении."""
    rem = io.open(os.path.join(TOOLS, u"..", u"claude-hooks", u"remind-session-save.py"),
                  encoding="utf-8").read()
    missing = [rel for rel in NOTES if os.path.basename(rel) not in rem]
    return (not missing), u"все три записки названы и там: %s" % (u"да" if not missing else missing)


def c_tool_lists_three():
    rc, o = run(FRESH)
    return (u"живых записок 3" in o), u"пульс знает ровно три роли"


cell(u"список ролей совпадает с напоминанием", c_roles_match_reminder)
cell(u"пульс знает три роли", c_tool_lists_three)

print(u"PASS %d  FAIL %d" % (ok, fail))
sys.exit(1 if fail else 0)
