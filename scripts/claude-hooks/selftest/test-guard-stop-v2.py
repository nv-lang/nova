# -*- coding: utf-8 -*-
u"""Самотест черновика `guard-stop-v2.py` (план 292).

ОБА НАПРАВЛЕНИЯ, и это здесь не формальность: хук, который умеет только
пропускать, неотличим от отсутствующего, а хук, который умеет только
блокировать, снимут первым же днём. Поэтому в каждой паре клеток одна ждёт
блокировки, другая — пропуска на почти том же входе.
"""
import io
import json
import os
import shutil
import subprocess
import sys
import tempfile
import time

HOOK = os.path.join(os.path.dirname(os.path.dirname(os.path.abspath(__file__))),
                    "guard-stop-v2.py")
if not os.path.exists(HOOK):
    sys.stderr.write("selftest: hook not found %s\n" % HOOK)
    sys.exit(1)


def transcript(tmp, turns):
    u"""turns: список (текст, число tool_use ПОСЛЕ него до конца хода)."""
    p = os.path.join(tmp, "t.jsonl")
    with io.open(p, "w", encoding="utf-8") as fh:
        for text, tools in turns:
            fh.write(json.dumps({"type": "assistant", "message": {
                "content": [{"type": "text", "text": text}]}}, ensure_ascii=False) + "\n")
            for _ in range(tools):
                fh.write(json.dumps({"type": "assistant", "message": {
                    "content": [{"type": "tool_use", "name": "Bash", "input": {}}]}},
                    ensure_ascii=False) + "\n")
            fh.write(json.dumps({"type": "user", "message": {"content": "ok"}},
                                ensure_ascii=False) + "\n")
    return p


def queue(tmp, payload, age_sec=0):
    d = os.path.join(tmp, "target")
    os.makedirs(d, exist_ok=True)
    p = os.path.join(d, "queue.json")
    with io.open(p, "w", encoding="utf-8") as fh:
        fh.write(json.dumps(payload, ensure_ascii=False))
    if age_sec:
        old = time.time() - age_sec
        os.utime(p, (old, old))
    return p


def run(tmp, turns, session="s1", active=False):
    payload = {"transcript_path": transcript(tmp, turns), "cwd": tmp,
               "session_id": session, "stop_hook_active": active}
    r = subprocess.run([sys.executable, HOOK], input=json.dumps(payload),
                       capture_output=True, text=True, encoding="utf-8")
    out = (r.stdout or "").strip()
    if not out:
        return None
    try:
        return json.loads(out)
    except Exception:
        return {"decision": "?", "reason": out}


CASES = []


def case(name, fn):
    CASES.append((name, fn))


def c_no_code(tmp):
    queue(tmp, {"role": "integrator", "open": []})
    return run(tmp, [(u"Сделал правку, всё зелено.", 0)]), True


def c_empty_queue_ok(tmp):
    queue(tmp, {"role": "integrator", "open": []})
    return run(tmp, [(u"Всё закрыто.\n\nСТОП: очередь-пуста", 0)]), False


def c_empty_queue_lies(tmp):
    queue(tmp, {"role": "integrator", "open": [u"274.13 Ш.1 пин этапа", u"№1147"]})
    return run(tmp, [(u"На сегодня всё.\n\nСТОП: очередь-пуста", 0)]), True


def c_queue_missing(tmp):
    return run(tmp, [(u"Готово.\n\nСТОП: очередь-пуста", 0)]), True


def c_queue_stale(tmp):
    queue(tmp, {"role": "integrator", "open": []}, age_sec=60 * 60)
    return run(tmp, [(u"Готово.\n\nСТОП: очередь-пуста", 0)]), True


def c_question_ok(tmp):
    return run(tmp, [(u"Две развилки, какую берём?\n\nСТОП: вопрос", 0)]), False


def c_question_without_question(tmp):
    return run(tmp, [(u"Жду указаний.\n\nСТОП: вопрос", 0)]), True


def c_question_twice(tmp):
    turns = [(u"Первый вопрос: брать A или B?\n\nСТОП: вопрос", 0),
             (u"Ещё один вопрос: а C?\n\nСТОП: вопрос", 0)]
    return run(tmp, turns), True


def c_question_twice_with_work(tmp):
    # Работа принадлежит ТОМУ ходу, который кончается вторым вопросом, а не
    # предыдущему. Первая редакция клетки ставила вызовы в первый ход и падала:
    # ошибка была в фикстуре, не в хуке (записано в план 292 Ш.4 как урок).
    turns = [(u"Первый вопрос: A или B?\n\nСТОП: вопрос", 0),
             (u"Сделал замер. Теперь вопрос: а C?\n\nСТОП: вопрос", 3)]
    return run(tmp, turns), False


def c_irreversible_named(tmp):
    return run(tmp, [(u"Гейт зелёный.\n\nСТОП: неавторизовано пуш в main", 0)]), False


def c_irreversible_bare(tmp):
    return run(tmp, [(u"Дальше нельзя.\n\nСТОП: неавторизовано", 0)]), True


def c_active_passes(tmp):
    return run(tmp, [(u"Без кода.", 0)], active=True), False


def c_escape_after_two(tmp):
    queue(tmp, {"role": "integrator", "open": [u"пункт"]})
    t = [(u"Всё.\n\nСТОП: очередь-пуста", 0)]
    a = run(tmp, t, session="esc")
    b = run(tmp, t, session="esc")
    c = run(tmp, t, session="esc")
    assert a and b, "первые две обязаны блокировать"
    return c, False


for n, f in [
    (u"нет кода остановки", c_no_code),
    (u"очередь-пуста, снимок пуст", c_empty_queue_ok),
    (u"очередь-пуста, а в снимке два пункта", c_empty_queue_lies),
    (u"очередь-пуста, снимка нет", c_queue_missing),
    (u"очередь-пуста, снимок просрочен", c_queue_stale),
    (u"вопрос со знаком вопроса", c_question_ok),
    (u"вопрос без знака вопроса", c_question_without_question),
    (u"два вопроса подряд без работы", c_question_twice),
    (u"два вопроса, но между ними работа", c_question_twice_with_work),
    (u"неавторизовано: пуш назван", c_irreversible_named),
    (u"неавторизовано без действия", c_irreversible_bare),
    (u"stop_hook_active пропускается", c_active_passes),
    (u"третья блокировка подряд пропускается", c_escape_after_two),
]:
    case(n, f)

ok = 0
fail = 0
for name, fn in CASES:
    tmp = tempfile.mkdtemp(prefix="stopv2-")
    try:
        res, want_block = fn(tmp)
        got_block = bool(res and res.get("decision") == "block")
        if got_block == want_block:
            ok += 1
            print(u"  PASS  %-42s %s" % (name, u"блок" if got_block else u"пропуск"))
        else:
            fail += 1
            print(u"  FAIL  %-42s ждали %s, получили %s%s" % (
                name, u"блок" if want_block else u"пропуск",
                u"блок" if got_block else u"пропуск",
                u" | " + (res or {}).get("reason", u"")[:80] if res else u""))
    finally:
        shutil.rmtree(tmp, ignore_errors=True)

print(u"PASS %d  FAIL %d" % (ok, fail))
sys.exit(1 if fail else 0)
