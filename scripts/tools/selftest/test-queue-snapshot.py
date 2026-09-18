#!/usr/bin/env python
# -*- coding: utf-8 -*-
u"""Самотест снимка очереди (план 292 Ш.2).

Форма — та же, что у самотеста хука: клетки ПАРАМИ, и в каждой паре одна ждёт
отказа, другая успеха. Одинокая клетка «ждём отказа» ничего не доказывает: она
зеленеет и когда скрипт отказывает ВСЕГДА.

Клетки, которых требует приёмка Ш.2:
  * недостижимый источник      -> ok=false, имя источника названо, open НЕ пуст;
  * незапускаемая команда      -> ok=false, формулировка ЯВНО другая;
  * роль carina                -> ok=false с названной причиной, не пустая очередь;
  * два прогона подряд         -> одинаковое содержимое, кроме `at`/`elapsed_sec`;
  * подмена одного источника   -> меняется РОВНО его поле;
  * сборка `open`              -> из всех трёх источников, а не из одного;
  * сломан САМ снимок          -> громко, а не «роль none, ворота открыты».

Зеркала в клетках, где они не предмет, сведены к одному (`origin`): полный опрос
трёх стоит ~12 с на прогон, и самотест из восьми прогонов встал бы в полторы
минуты ради числа, которое клетка не проверяет.
"""

import io
import json
import os
import shutil
import subprocess
import sys
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
TOOLS = os.path.dirname(HERE)
TREE = os.path.dirname(os.path.dirname(TOOLS))
SCRIPT = os.path.join(TOOLS, u"queue-snapshot.py")

sys.path.insert(0, TOOLS)
snapmod = __import__("queue-snapshot".replace("-", "_")) if False else None

# Модуль зовётся с дефисом — обычный import его не возьмёт; грузим по пути.
try:
    import importlib.util as _ilu
    _spec = _ilu.spec_from_file_location("queue_snapshot", SCRIPT)
    snapmod = _ilu.module_from_spec(_spec)
    _spec.loader.exec_module(snapmod)
except Exception as e:  # pragma: no cover
    print(u"FAIL: не загрузился модуль снимка: %s" % e)
    sys.exit(1)

ok_count = 0
fail_count = 0


def run_snapshot(env_extra, tree=None):
    u"""Прогон скрипта в отдельном процессе. Снимок пишется в ВРЕМЕННЫЙ файл:
    самотест не имеет права затирать настоящий `target/queue.json` окна."""
    fd, out = tempfile.mkstemp(suffix=".json", prefix="queue-selftest-")
    os.close(fd)
    env = dict(os.environ)
    env["NOVA_QUEUE_OUT"] = out
    env.setdefault("NOVA_QUEUE_MIRRORS", "origin")
    env.update(env_extra)
    p = subprocess.Popen(
        [sys.executable, SCRIPT, tree or TREE],
        stdout=subprocess.PIPE, stderr=subprocess.PIPE, env=env, cwd=TREE)
    so, se = p.communicate(timeout=90)
    data = None
    try:
        with io.open(out, encoding="utf-8") as fh:
            data = json.load(fh)
    except Exception:
        pass
    try:
        os.unlink(out)
    except Exception:
        pass
    return {
        "rc": p.returncode,
        "stdout": (so or b"").decode("utf-8", "replace"),
        "stderr": (se or b"").decode("utf-8", "replace"),
        "data": data,
    }


def cell(name, predicate):
    global ok_count, fail_count
    try:
        verdict, detail = predicate()
    except Exception as e:
        verdict, detail = False, u"исключение в клетке: %s: %s" % (type(e).__name__, e)
    if verdict:
        ok_count += 1
        print(u"  PASS  %-46s %s" % (name, detail))
    else:
        fail_count += 1
        print(u"  FAIL  %-46s %s" % (name, detail))


def joined(data, key="failed_sources"):
    return u" | ".join(data.get(key) or [])


# --------------------------------------------------------------------------
# пара 1 — недостижимый источник
# --------------------------------------------------------------------------

R_FAKE_MIRROR = run_snapshot({"NOVA_WINDOW_ROLE": "integrator",
                              "NOVA_QUEUE_MIRRORS": "origin,nosuchremote"})
R_LIVE = run_snapshot({"NOVA_WINDOW_ROLE": "integrator"})


def c_fake_mirror():
    d = R_FAKE_MIRROR["data"]
    if d is None:
        return False, u"снимок не написан"
    bad = joined(d)
    return (d["ok"] is False and u"nosuchremote" in bad
            and snapmod.FAIL_RAN in bad and bool(d["open"])), \
        u"ok=%s, open=%d" % (d["ok"], len(d["open"]))


def c_live_mirror():
    d = R_LIVE["data"]
    return (d is not None and d["ok"] is True
            and u"nosuchremote" not in joined(d)), \
        u"ok=%s, failed=%d" % (d["ok"], len(d["failed_sources"]))


cell(u"недостижимое зеркало: ok=false и оно названо", c_fake_mirror)
cell(u"живые зеркала: ok=true, отказов нет", c_live_mirror)


# --------------------------------------------------------------------------
# пара 2 — команда не запустилась (это ДРУГАЯ беда, чем отказ источника)
# --------------------------------------------------------------------------

R_NO_EXE = run_snapshot({"NOVA_WINDOW_ROLE": "integrator",
                         "NOVA_QUEUE_PYTHON": "no-such-python-executable"})


def c_not_launched():
    d = R_NO_EXE["data"]
    if d is None:
        return False, u"снимок не написан"
    bad = joined(d)
    return (d["ok"] is False and snapmod.FAIL_LAUNCH in bad
            and bool(d["open"])), u"формулировка: %s" % bad[:60]


def c_wordings_differ():
    a = joined(R_NO_EXE["data"])
    b = joined(R_FAKE_MIRROR["data"])
    return (snapmod.FAIL_LAUNCH in a and snapmod.FAIL_LAUNCH not in b
            and snapmod.FAIL_RAN in b), u"две беды записаны по-разному"


cell(u"незапускаемая команда: ok=false, своя формулировка", c_not_launched)
cell(u"две беды различимы в логе", c_wordings_differ)


# --------------------------------------------------------------------------
# пара 3 — роль без выводимой очереди
# --------------------------------------------------------------------------

R_CARINA = run_snapshot({"NOVA_WINDOW_ROLE": "carina"})


def c_carina():
    u"""У Карины ЕСТЬ источник очереди (с 2026-09-19), и он её собственный.

    До этой правки клетка требовала `ok: false` — и была зелёной ровно
    потому, что код `СТОП: очередь-пуста` был для Карины недоказуем по
    построению. Клетка проверяла форму отказа вместо предмета.

    Числа открытых подпланов здесь НЕТ намеренно: оно меняется от её
    работы, и клетка покраснела бы на успехе. Проверяется, что очередь
    СОБРАНА из своего источника, а не из моей.
    """
    d = R_CARINA["data"]
    if d is None:
        return False, u"снимок не написан"
    src = (d.get("sources") or {}).get("carina_subplans")
    mine = [k for k in (d.get("sources") or {})
            if k in ("mirrors_behind", "unmerged_branches", "blockers")]
    return (d["ok"] is True and src is not None and not mine
            and src.get("value") == len(src.get("plans") or [])), \
        u"ok=%s, подпланов открыто %s, чужих источников %d" % (
            d["ok"], (src or {}).get("value"), len(mine))


def c_integrator():
    d = R_LIVE["data"]
    return (d["role"] == u"integrator" and d["ok"] is True), u"role=%s" % d["role"]


cell(u"роль carina: очередь из СВОЕГО источника", c_carina)
cell(u"роль integrator: очередь выводится", c_integrator)


# --------------------------------------------------------------------------
# пара 4 — воспроизводимость и изоляция источников
# --------------------------------------------------------------------------

R_AGAIN = run_snapshot({"NOVA_WINDOW_ROLE": "integrator"})


def strip_volatile(d):
    x = dict(d)
    x.pop(u"at", None)
    x.pop(u"elapsed_sec", None)
    return json.dumps(x, ensure_ascii=False, sort_keys=True)


def c_reproducible():
    return (strip_volatile(R_LIVE["data"]) == strip_volatile(R_AGAIN["data"])), \
        u"два прогона совпали, кроме at/elapsed_sec"


def c_isolation():
    a, b = R_LIVE["data"][u"sources"], R_FAKE_MIRROR["data"][u"sources"]
    same_others = (a[u"blockers"][u"value"] == b[u"blockers"][u"value"]
                   and a[u"unmerged_branches"][u"value"] == b[u"unmerged_branches"][u"value"])
    changed_own = a[u"mirrors_behind"] != b[u"mirrors_behind"]
    return (same_others and changed_own), u"сменилось только зеркальное поле"


cell(u"воспроизводимость: два прогона совпали", c_reproducible)
cell(u"изоляция: подмена источника меняет своё поле", c_isolation)


# --------------------------------------------------------------------------
# пара 5 — правило сборки `open`
# --------------------------------------------------------------------------

SYNTH = {
    u"mirrors_behind": {u"value": 2, u"behind": [u"gitverse", u"sourcecraft"]},
    u"unmerged_branches": {u"value": 1, u"branches": [u"p999-probe"]},
    u"blockers": {u"value": 2, u"numbers": [11, 22]},
}


def c_open_from_queue_sources():
    items = snapmod.build_open(SYNTH)
    text = u" | ".join(items)
    return (len(items) == 2 and u"gitverse" in text and u"sourcecraft" in text
            and u"p999-probe" in text), u"пунктов %d из обоих очередных" % len(items)


def c_blockers_not_a_queue():
    u"""Вердикт интегратора 2026-09-18, сверено по плану 274: поле `БЛОКИРУЕТ
    ТЕГ` осталось от мёртвого события, и как ОЧЕРЕДЬ счёт лжив — «73 открытых
    пункта» прочтётся как работа, которой нет. Как храповик он остаётся в
    `sources`, и эта клетка стережёт ровно границу между двумя предметами."""
    only_blockers = {
        u"mirrors_behind": {u"value": 0, u"behind": []},
        u"unmerged_branches": {u"value": 0, u"branches": []},
        u"blockers": {u"value": 73, u"numbers": [11, 22]},
    }
    items = snapmod.build_open(only_blockers)
    return (items == []), u"73 блокера не стали очередью"


cell(u"open собран из ОБОИХ очередных источников", c_open_from_queue_sources)
cell(u"blockers в open НЕ попадает (храповик, не очередь)", c_blockers_not_a_queue)


# --------------------------------------------------------------------------
# пара 6 — сломан САМ снимок (предупреждение интегратора 2026-09-18)
# --------------------------------------------------------------------------

R_NO_TREE = run_snapshot({"NOVA_WINDOW_ROLE": ""},
                         tree=os.path.join(TREE, u"no-such-tree-xyz"))


def c_broken_self():
    d = R_NO_TREE["data"]
    if d is None:
        return False, u"снимок не написан вовсе — беда НЕМАЯ"
    return (d["ok"] is False and snapmod.FAIL_SELF in joined(d)
            and d["role"] != u"none" and bool(d["open"])), \
        u"role=%s, ok=%s" % (d["role"], d["ok"])


def c_real_tree():
    d = R_LIVE["data"]
    return (d["role"] != u"unknown"
            and snapmod.FAIL_SELF not in joined(d)), u"настоящее дерево прочитано"


cell(u"нет дерева: громко, а не «роль none»", c_broken_self)
cell(u"настоящее дерево: снимок не считает себя сломанным", c_real_tree)



# ------------------------------------------- согласие с хуком о «чьё это окно»
# Снимок и Stop-страж отвечают на ОДИН вопрос. Разойдясь, они дают окну снимок
# ЧУЖОЙ очереди при молчащем страже — замерено на себе 2026-09-19 02:06, когда
# хук уже говорил `none`, а снимок в том же дереве — `integrator`.

def _tree_with_card(owner_sid):
    tmp = tempfile.mkdtemp(prefix="queue-card-")
    g = os.path.join(tmp, ".git")
    os.makedirs(g, exist_ok=True)
    with io.open(os.path.join(g, "HEAD"), "w", encoding="utf-8") as fh:
        fh.write(u"ref: refs/heads/main\n")
    with io.open(os.path.join(g, "nova-session-integrator.card"), "w",
                 encoding="utf-8") as fh:
        fh.write(u"role=integrator\nsession_id=%s\n" % owner_sid)
    return tmp


def _hook_role(tree, sid):
    u"""Роль ГЛАЗАМИ ХУКА — его собственной функцией, а не пересказом."""
    hook = os.path.join(TREE, "scripts", "claude-hooks", "guard-stop-v2.py")
    src = io.open(hook, encoding="utf-8").read().replace(
        'if __name__ == "__main__":', "if False:")
    ns = {"__name__": "probe"}
    old = os.environ.get("CLAUDE_CODE_SESSION_ID")
    os.environ["CLAUDE_CODE_SESSION_ID"] = sid
    os.environ.pop("NOVA_WINDOW_ROLE", None)
    try:
        exec(compile(src, "probe", "exec"), ns)
        return ns["detect_role"](tree)
    finally:
        if old is None:
            os.environ.pop("CLAUDE_CODE_SESSION_ID", None)
        else:
            os.environ["CLAUDE_CODE_SESSION_ID"] = old


def c_agrees_foreign_card():
    tree = _tree_with_card(u"CHUZHOY")
    old = os.environ.get("CLAUDE_CODE_SESSION_ID")
    os.environ["CLAUDE_CODE_SESSION_ID"] = u"MOY"
    os.environ.pop("NOVA_WINDOW_ROLE", None)
    try:
        mine = snapmod.detect_role(tree)
    finally:
        if old is None:
            os.environ.pop("CLAUDE_CODE_SESSION_ID", None)
        else:
            os.environ["CLAUDE_CODE_SESSION_ID"] = old
    theirs = _hook_role(tree, u"MOY")
    shutil.rmtree(tree, ignore_errors=True)
    return (mine == theirs == u"none"), u"снимок=%s, хук=%s" % (mine, theirs)


def c_agrees_own_card():
    tree = _tree_with_card(u"MOY")
    old = os.environ.get("CLAUDE_CODE_SESSION_ID")
    os.environ["CLAUDE_CODE_SESSION_ID"] = u"MOY"
    os.environ.pop("NOVA_WINDOW_ROLE", None)
    try:
        mine = snapmod.detect_role(tree)
    finally:
        if old is None:
            os.environ.pop("CLAUDE_CODE_SESSION_ID", None)
        else:
            os.environ["CLAUDE_CODE_SESSION_ID"] = old
    theirs = _hook_role(tree, u"MOY")
    shutil.rmtree(tree, ignore_errors=True)
    return (mine == theirs == u"integrator"), u"снимок=%s, хук=%s" % (mine, theirs)


cell(u"визитка чужая: снимок и хук говорят `none`", c_agrees_foreign_card)
cell(u"визитка моя: оба говорят `integrator`", c_agrees_own_card)

# ------------------------------------- подпланы Карины: обе стороны и проза
# Источник читает ФАЙЛЫ, поэтому клетки строят дерево из подложных планов —
# так обе стороны видны на одном коде, а не на двух состояниях репозитория.

def _tree_with_subplans(statuses):
    u"""statuses: словарь номер -> текст статус-строки. Неназванные номера
    получают закрытый статус, чтобы клетка мерила ровно то, что назвала."""
    tmp = tempfile.mkdtemp(prefix="queue-subplans-")
    plans = os.path.join(tmp, "docs", "plans")
    os.makedirs(plans)
    for n in range(1, 10):
        num = u"274.%d" % n
        st = statuses.get(num, u"ЗАКРЫТ 2026-09-19, влит.")
        with io.open(os.path.join(plans, u"%s-fake.md" % num), "w",
                     encoding="utf-8") as fh:
            fh.write(u"# подплан %s\n\n**Статус:** %s\n" % (num, st))
    return tmp


def _subplans(statuses):
    tree = _tree_with_subplans(statuses)
    st = snapmod.State()
    try:
        return snapmod.source_carina_subplans(tree, st), st.failed
    finally:
        shutil.rmtree(tree, ignore_errors=True)


def c_subplans_all_closed():
    src, failed = _subplans({})
    return (src["value"] == 0 and not src["plans"] and not failed), \
        u"value=%s, отказов %d" % (src["value"], len(failed))


def c_subplans_one_open():
    src, failed = _subplans({u"274.5": u"🚧 В РАБОТЕ с 2026-08-29."})
    return (src["value"] == 1 and src["plans"] == [u"274.5"] and not failed), \
        u"value=%s, %s" % (src["value"], src["plans"])


def c_subplans_prose_is_not_a_verdict():
    u"""КИЛСВИТЧ на купленный дефект (2026-09-19): слово «закрыта» в ПРОЗЕ
    статус-строки не закрывает подплан. Настоящая строка 274.8 читалась как
    ЗАКРЫТ, потому что вердикт искался вхождением, а не первым словом."""
    src, failed = _subplans({
        u"274.8": u"🚧 В РАБОТЕ — слито M0; (3а) закрыта обеими половинами",
    })
    return (u"274.8" in src["plans"]), u"открытые: %s" % src["plans"]


def c_subplans_missing_is_loud():
    u"""Пропавший подплан — ОТКАЗ, а не молчаливое «работы меньше»."""
    tree = _tree_with_subplans({})
    plans = os.path.join(tree, "docs", "plans")
    os.unlink(os.path.join(plans, u"274.3-fake.md"))
    st = snapmod.State()
    try:
        snapmod.source_carina_subplans(tree, st)
    finally:
        shutil.rmtree(tree, ignore_errors=True)
    return (len(st.failed) == 1 and u"274.3" in st.failed[0]), \
        u"отказов %d: %s" % (len(st.failed), st.failed[:1])


def c_subplans_no_status_line_is_loud():
    tree = _tree_with_subplans({})
    path = os.path.join(tree, "docs", "plans", u"274.6-fake.md")
    with io.open(path, "w", encoding="utf-8") as fh:
        fh.write(u"# подплан без статуса\n")
    st = snapmod.State()
    try:
        snapmod.source_carina_subplans(tree, st)
    finally:
        shutil.rmtree(tree, ignore_errors=True)
    return (len(st.failed) == 1 and u"274.6" in st.failed[0]), \
        u"отказов %d" % len(st.failed)


cell(u"подпланы: все закрыты — очередь ПУСТА", c_subplans_all_closed)
cell(u"подпланы: один открыт — очередь непуста", c_subplans_one_open)
cell(u"подпланы: «закрыта» в прозе НЕ закрывает", c_subplans_prose_is_not_a_verdict)
cell(u"подпланы: пропавший файл — громкий отказ", c_subplans_missing_is_loud)
cell(u"подпланы: нет строки статуса — громкий отказ", c_subplans_no_status_line_is_loud)


print(u"PASS %d  FAIL %d" % (ok_count, fail_count))
sys.exit(1 if fail_count else 0)
