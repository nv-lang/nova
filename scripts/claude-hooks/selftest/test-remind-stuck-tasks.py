# -*- coding: utf-8 -*-
"""Самотест remind-stuck-tasks: застрявшая принятая задача названа, всё прочее — тишина.

Данные плагина подкладываются во временный XDG_DATA_HOME (шов хука), поэтому
настоящие задачи проекта самотест не читает и не трогает.
"""
import io
import json
import os
import shutil
import subprocess
import sys
import tempfile
import time

sys.stdout.reconfigure(encoding="utf-8", errors="replace", newline="\n")

HOOK = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..",
                    "remind-stuck-tasks.py")
fails = 0
cases = 0


def ok(msg):
    global cases
    cases += 1
    print("  ok   %s" % msg)


def bad(msg):
    global fails, cases
    cases += 1
    fails += 1
    print("  ПРОВАЛ %s" % msg)


def write(p, obj):
    os.makedirs(os.path.dirname(p), exist_ok=True)
    io.open(p, "w", encoding="utf-8").write(json.dumps(obj, ensure_ascii=False))


def setup(base, tasks, integrator="ses_INTEG"):
    data = os.path.join(base, "data")
    peers = os.path.join(data, "opencode", "nova-peers")
    write(os.path.join(peers, "roles", "nova_integrator.json"), {"session": integrator, "at": 1})
    for t in tasks:
        write(os.path.join(peers, "tasks", "nova", "%d.json" % t["n"]), t)
    tree = os.path.join(base, "tree")
    os.makedirs(tree, exist_ok=True)
    return data, tree


def run(data, tree, sid="ses_INTEG", fresh=True):
    if fresh:
        try:
            os.remove(os.path.join(tree, "target", ".remind-stuck-tasks"))
        except OSError:
            pass
    env = dict(os.environ)
    env["XDG_DATA_HOME"] = data
    env["CLAUDE_PROJECT_DIR"] = tree
    env.pop("OPENCODE_SESSION_ID", None)
    if sid:
        env["OPENCODE_SESSION_ID"] = sid
    p = subprocess.run([sys.executable, HOOK], input=b"{}", capture_output=True, env=env, cwd=tree)
    out = p.stdout.decode("utf-8", "replace").strip()
    if not out:
        return ""
    try:
        return json.loads(out)["hookSpecificOutput"]["additionalContext"]
    except Exception:
        return "<не JSON: %r>" % out[:80]


now = int(time.time() * 1000)
hour = 3600 * 1000
base = tempfile.mkdtemp(prefix="nova-stuck-")
try:
    data, tree = setup(base, [
        {"n": 9, "status": "accepted", "updated": now - 11 * hour, "reviewer": "ses_REV9"},
        {"n": 15, "status": "accepted", "updated": now - 5 * 60 * 1000, "reviewer": "ses_REV15"},
        {"n": 14, "status": "cleaned", "updated": now - 20 * hour, "reviewer": "ses_REV14"},
        {"n": 18, "status": "running", "updated": now - 20 * hour},
    ])

    # (1) Застрявшая принятая задача названа, с возрастом и приёмщиком.
    ctx = run(data, tree)
    if "#9 " in ctx and "11ч" in ctx and "ses_REV9" in ctx:
        ok("принятая 11 ч назад и не убранная — названа (#9, возраст, приёмщик)")
    else:
        bad("застрявшая #9 не названа: %r" % ctx[:160])

    # (2) Свежая принятая (5 мин) — не шум: приёмщик ещё убирает.
    if "#15" not in ctx:
        ok("принятая 5 мин назад — не названа")
    else:
        bad("свежая #15 названа как застрявшая")

    # (3) Убранная и работающая — не названы.
    if "#14" not in ctx and "#18" not in ctx:
        ok("cleaned и running — не названы")
    else:
        bad("названа задача не в статусе accepted: %r" % ctx[:160])

    # (4) Остуда: повтор подряд молчит.
    if run(data, tree, fresh=False) == "":
        ok("повтор внутри остуды — молчит")
    else:
        bad("остуда не сработала")

    # (5) Не интегратор — молчит (задачи не его забота, напоминание неисполнимо).
    if run(data, tree, sid="ses_OTHER") == "":
        ok("сессия без роли интегратора — молчит")
    else:
        bad("напомнил не интегратору")

    # (6) Окно не во вкладке OpenCode — молчит.
    if run(data, tree, sid=None) == "":
        ok("без OPENCODE_SESSION_ID — молчит")
    else:
        bad("напомнил без OPENCODE_SESSION_ID")

    # (7) Нечего называть — молчит.
    data2, tree2 = setup(os.path.join(base, "b"), [
        {"n": 9, "status": "cleaned", "updated": now - 11 * hour},
    ])
    if run(data2, tree2) == "":
        ok("застрявших нет — молчит")
    else:
        bad("напомнил при отсутствии застрявших")
finally:
    shutil.rmtree(base, ignore_errors=True)

print("самотест remind-stuck-tasks: PASS %d FAIL %d" % (cases - fails, fails))
sys.exit(1 if fails else 0)
