# -*- coding: utf-8 -*-
"""Самотест хука-напоминания о сохранении сессии (план 290 п.3).

ДОКАЗЫВАЕТ ОБЕ СТОРОНЫ, как требует П16: свежая записка — молчит; устаревшая —
говорит; повтор внутри остуды — молчит снова. Третий случай здесь не украшение:
напоминание на каждый вызов инструмента читается как фон, и тогда его не видно
ровно тогда, когда оно нужно.

Работает на ВРЕМЕННОМ дереве и настоящих файлов проекта не трогает.
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
                    "remind-session-save.py")
HANDOFF_REL = os.path.join("docs", "dev", "prompts", "integrator-handoff.md")
CARINA_REL = os.path.join("docs", "dev", "prompts", "carina-handoff.md")


def make_tree(branch):
    """Настоящий git с НАЗВАННОЙ веткой: роль хук берёт именно у git."""
    root = tempfile.mkdtemp(prefix="nova-remind-")
    subprocess.run(["git", "init", "-q", "-b", branch, root],
                   capture_output=True)
    return root

fails = 0
cases = 0


def run(root):
    env = dict(os.environ)
    env["CLAUDE_PROJECT_DIR"] = root
    p = subprocess.run([sys.executable, HOOK], input=b"{}",
                       capture_output=True, env=env)
    return p.stdout.decode("utf-8", "replace").strip(), p.returncode


def ok(msg):
    global cases
    cases += 1
    print("  ok   %s" % msg)


def bad(msg):
    global fails, cases
    cases += 1
    fails += 1
    print("  ПРОВАЛ %s" % msg)


root = make_tree("main")
try:
    path = os.path.join(root, HANDOFF_REL)
    os.makedirs(os.path.dirname(path))
    io.open(path, "w", encoding="utf-8").write(u"# записка\n")

    # (1) свежая записка — молчание
    out, rc = run(root)
    if out == "" and rc == 0:
        ok(u"свежая записка — хук молчит")
    else:
        bad(u"свежая записка, а хук заговорил: %r" % out[:120])

    # (2) записка старше часа — говорит, и НАЗЫВАЕТ возраст
    st = os.stat(path)
    os.utime(path, (st.st_atime, time.time() - 7200))
    out, rc = run(root)
    if not out:
        bad(u"устаревшая записка, а хук промолчал")
    else:
        try:
            ctx = json.loads(out)["hookSpecificOutput"]["additionalContext"]
        except Exception as e:
            ctx = ""
            bad(u"вывод не разбирается как JSON хука: %s" % e)
        if ctx and u"/save" in ctx and u"2ч" in ctx:
            ok(u"устаревшая записка — хук называет возраст и команду")
        elif ctx:
            bad(u"хук заговорил, но без возраста или команды: %r" % ctx[:120])

    # (3) повтор внутри остуды — снова молчание
    out2, _ = run(root)
    if out2 == "":
        ok(u"повтор внутри остуды — молчит (иначе напоминание станет фоном)")
    else:
        bad(u"повтор внутри остуды заговорил снова")

    # (4) записки нет вовсе — молчит (чужой проект, свежий клон)
    os.remove(path)
    out3, rc3 = run(root)
    if out3 == "" and rc3 == 0:
        ok(u"записки нет — хук молчит, а не падает")
    else:
        bad(u"без записки хук отреагировал: rc=%d out=%r" % (rc3, out3[:80]))
    # (4) РОЛЬ: на ветке Карины хук зовёт ЕЁ команду и ЕЁ записку, а не мои.
    #     Без этого случая правка «хук учитывает роль» осталась бы недоказанной:
    #     до неё он звал /save в любом окне, включая то, которому /save не адресована.
    croot = make_tree("p274-novac")
    cpath = os.path.join(croot, CARINA_REL)
    os.makedirs(os.path.dirname(cpath))
    io.open(cpath, "w", encoding="utf-8").write(u"# записка окна\n")
    st = os.stat(cpath)
    os.utime(cpath, (st.st_atime, time.time() - 7200))
    out, rc = run(croot)
    ctx = ""
    if out:
        try:
            ctx = json.loads(out)["hookSpecificOutput"]["additionalContext"]
        except Exception:
            ctx = ""
    if ctx and u"/save" in ctx and u"carina-handoff" in ctx:
        ok(u"ветка Карины — хук зовёт ЕЁ команду и ЕЁ записку")
    else:
        bad(u"на ветке Карины хук назвал не то: %r" % (ctx or out)[:140])
    shutil.rmtree(croot, ignore_errors=True)

    # (5) НЕЗНАКОМАЯ РОЛЬ — молчание, и это решение, а не дыра: у пакетных окон
    #     своя передача (/stop), выдумывать им адресата хук не вправе.
    oroot = make_tree("p999-other")
    opath = os.path.join(oroot, HANDOFF_REL)
    os.makedirs(os.path.dirname(opath))
    io.open(opath, "w", encoding="utf-8").write(u"# чужая записка\n")
    st = os.stat(opath)
    os.utime(opath, (st.st_atime, time.time() - 7200))
    out, rc = run(oroot)
    if out == "" and rc == 0:
        ok(u"незнакомая ветка — молчит, а не зовёт чужую команду")
    else:
        bad(u"на незнакомой ветке хук заговорил: %r" % out[:140])
    shutil.rmtree(oroot, ignore_errors=True)
finally:
    shutil.rmtree(root, ignore_errors=True)

print(u"селфтест remind-session-save: %d/%d ok" % (cases - fails, cases)
      if fails == 0 else u"селфтест remind-session-save: ЕСТЬ ПРОВАЛЫ (%d)" % fails)
sys.exit(1 if fails else 0)
