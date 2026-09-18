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
    d = R_CARINA["data"]
    if d is None:
        return False, u"снимок не написан"
    return (d["ok"] is False and u"carina" in joined(d)
            and bool(d["open"])), u"ok=%s, open=%d" % (d["ok"], len(d["open"]))


def c_integrator():
    d = R_LIVE["data"]
    return (d["role"] == u"integrator" and d["ok"] is True), u"role=%s" % d["role"]


cell(u"роль carina: ok=false с причиной, не пустая очередь", c_carina)
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


print(u"PASS %d  FAIL %d" % (ok_count, fail_count))
sys.exit(1 if fail_count else 0)
