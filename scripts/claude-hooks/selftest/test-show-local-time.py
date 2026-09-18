# -*- coding: utf-8 -*-
"""Самотест show-local-time: время ВЕРНОЕ, подаётся, и остуда не молчит вечно.

Клетки порознь, потому что хук с одной рабочей половиной выглядит исправным
ровно до того дня, когда понадобится вторая.
"""
import io
import json
import os
import subprocess
import sys
import tempfile
import time

sys.stdout.reconfigure(encoding="utf-8", errors="replace", newline="\n")

HOOK = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..",
                    "show-local-time.py")
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


def run(root):
    # cwd = derevo ETOY sessii: otmetka ostudy lichnaya, i hook beret koren
    # ottuda (No1156). Bez cwd kletka merila by otmetku REPOZITORIYA.
    env = dict(os.environ)
    env["CLAUDE_PROJECT_DIR"] = root
    p = subprocess.run([sys.executable, HOOK], input=b"{}",
                       capture_output=True, env=env, cwd=root)
    return p.stdout.decode("utf-8", "replace").strip()


def ctx_of(out):
    if not out:
        return ""
    try:
        return json.loads(out)["hookSpecificOutput"]["additionalContext"]
    except Exception:
        return ""


root = tempfile.mkdtemp(prefix="nova-time-")
try:
    # (1) ВРЕМЯ ВЕРНОЕ. Главная клетка: хук существует ради того, чтобы окно не
    #     писало время из памяти, и сам обязан брать его у машины. Сравниваем с
    #     системным на момент прогона; допускаем разницу в минуту — прогон может
    #     пересечь границу минуты, и это не ошибка хука.
    before = time.strftime("%H:%M")
    ctx = ctx_of(run(root))
    after = time.strftime("%H:%M")
    if ctx and (before in ctx or after in ctx):
        ok(u"время в строке совпадает с системным (%s)" % before)
    else:
        bad(u"время не совпало: системное %s/%s, строка %r"
            % (before, after, ctx[:90]))

    # (2) ОСТУДА МОЛЧИТ. Вторая подача подряд не идёт: иначе строка сыплется
    #     после каждой команды и её перестают читать — износ предупреждения.
    if run(root) == "":
        ok(u"повтор внутри остуды — молчит")
    else:
        bad(u"остуда не сработала: подал второй раз подряд")

    # (3) ОСТУДА НЕ ВЕЧНАЯ. Без этой клетки (2) доказывала бы лишь умение
    #     молчать: хук, замолчавший навсегда, проходит её идеально.
    st = os.path.join(root, "target", ".show-local-time")
    old = time.time() - 10000
    os.utime(st, (old, old))
    if ctx_of(run(root)):
        ok(u"после остуды подаёт снова")
    else:
        bad(u"после истёкшей остуды хук промолчал")

    # (4) НЕТ КАТАЛОГА ДЛЯ ОТМЕТКИ — подаёт, а не падает. Отметка вторична:
    #     потерять подачу времени из-за невозможности записать файл значило бы
    #     променять предмет на бухгалтерию о нём.
    ro_root = tempfile.mkdtemp(prefix="nova-time-ro-")
    io.open(os.path.join(ro_root, "target"), "w").write("not a dir")
    if ctx_of(run(ro_root)):
        ok(u"отметку записать нельзя — время всё равно подано")
    else:
        bad(u"без возможности записать отметку хук промолчал")
finally:
    import shutil
    shutil.rmtree(root, ignore_errors=True)

# --- ДВА ДЕРЕВА: отметка остуды ЛИЧНАЯ (реестр 221.1 №1156) -----------------
#
# Остуда молчит ровно там, где время уже подавали. Если отметка общая на все
# окна, то одно окно глушит часы другому — и увидеть это можно только клеткой
# с ДВУМЯ настоящими деревьями: пока `CLAUDE_PROJECT_DIR` и рабочий каталог
# совпадают, подмена корня ничего не меняет и клетка зелена при любом коде.
import subprocess as _sp
_main = tempfile.mkdtemp(prefix="nova-time-main-")
_mine = tempfile.mkdtemp(prefix="nova-time-mine-")
_sp.run(["git", "init", "-q", _main], capture_output=True)
_sp.run(["git", "init", "-q", _mine], capture_output=True)
try:
    _env = dict(os.environ)
    _env["CLAUDE_PROJECT_DIR"] = _main
    _p = _sp.run([sys.executable, HOOK], input=b"{}",
                 capture_output=True, env=_env, cwd=_mine)
    _out = _p.stdout.decode("utf-8", "replace").strip()
    _stamp_mine = os.path.join(_mine, "target", ".show-local-time")
    _stamp_main = os.path.join(_main, "target", ".show-local-time")
    if ctx_of(_out) and os.path.isfile(_stamp_mine) and not os.path.isfile(_stamp_main):
        ok(u"отметка остуды легла в дерево СЕССИИ, а не в главное")
    else:
        bad(u"отметка не там: своя=%s чужая=%s"
            % (os.path.isfile(_stamp_mine), os.path.isfile(_stamp_main)))
finally:
    shutil.rmtree(_main, ignore_errors=True)
    shutil.rmtree(_mine, ignore_errors=True)

if fails:
    print("селфтест show-local-time: ЕСТЬ ПРОВАЛЫ (%d)" % fails)
    sys.exit(1)
print("селфтест show-local-time: %d/%d ok" % (cases, cases))
