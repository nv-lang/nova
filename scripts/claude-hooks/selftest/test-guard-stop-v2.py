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


def note(tmp, role="integrator", age_sec=0):
    u"""Ролевая записка — доказательство для кода «смена»."""
    rel = {"integrator": "docs/dev/prompts/integrator-handoff.md",
           "carina": "docs/dev/prompts/carina-handoff.md",
           "controller": "docs/dev/prompts/controller-handoff.md"}[role]
    p = os.path.join(tmp, *rel.split("/"))
    os.makedirs(os.path.dirname(p), exist_ok=True)
    with io.open(p, "w", encoding="utf-8") as fh:
        fh.write(u"# записка\n")
    if age_sec:
        old = time.time() - age_sec
        os.utime(p, (old, old))
    return p


def run(tmp, turns, session="s1", active=False, role="integrator",
        hook=None, env_extra=None):
    u"""role=None — окно без роли: ворота по роли обязаны сделать хук немым."""
    payload = {"transcript_path": transcript(tmp, turns), "cwd": tmp,
               "session_id": session, "stop_hook_active": active}
    env = dict(os.environ)
    if role:
        env["NOVA_WINDOW_ROLE"] = role
    else:
        env.pop("NOVA_WINDOW_ROLE", None)
    if env_extra:
        env.update(env_extra)
    r = subprocess.run([sys.executable, hook or HOOK], input=json.dumps(payload),
                       capture_output=True, text=True, encoding="utf-8", env=env)
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


def c_no_role_silent(tmp):
    # Окно без роли: нет ни ветки, ни NOVA_WINDOW_ROLE. Хук обязан молчать —
    # иначе он спросит код у исследовательского окна, отвечающего на вопрос.
    return run(tmp, [(u"Ответил на вопрос владельца.", 0)], role=None), False


def c_interrupt_passes(tmp):
    queue(tmp, {"role": "integrator", "open": [u"пункт"]})
    return run(tmp, [(u"Начал делать...\n\n[Request interrupted by user]", 0)]), False


def c_shift_with_note(tmp):
    note(tmp)
    return run(tmp, [(u"Смена сдана.\n\nСТОП: смена", 0)]), False


def c_shift_without_note(tmp):
    note(tmp, age_sec=3 * 60 * 60)
    return run(tmp, [(u"Ухожу.\n\nСТОП: смена", 0)]), True


def c_snapshot_failed_passes_with_escape(tmp):
    # ТРЕТЬЕ УСЛОВИЕ ИНТЕГРАТОРА: снимок сам себя объявил неполным —
    # окно пропускается (чужую сеть оно не починит), но побег обязан лечь в лог:
    # «не смог посчитать» не имеет права выглядеть пустой очередью.
    queue(tmp, {"role": "integrator", "ok": False,
                "failed_sources": [u"gitverse: нет сети"], "open": []})
    res = run(tmp, [(u"Всё.\n\nСТОП: очередь-пуста", 0)])
    log = os.path.join(tmp, "target", ".stop-guard", "escapes.log")
    assert os.path.exists(log), u"побег не записан в лог"
    with io.open(log, encoding="utf-8") as fh:
        assert u"gitverse" in fh.read(), u"в логе не назван сломавшийся источник"
    return res, False


def broken_hook(tmp):
    u"""Копия хука с подстроенным падением в первой же строке `main()`.

    Проверяется не источник данных, а САМ скрипт: исключение внутри хука. Это
    отдельный класс, и он опаснее — снимок при отказе источника хотя бы говорит
    `ok: false`, а упавший скрипт не говорит ничего.
    """
    src = io.open(HOOK, encoding="utf-8").read()
    marker = u"def main():"
    assert src.count(marker) == 1, u"маркер инъекции неоднозначен"
    src = src.replace(marker, marker + u"\n    raise RuntimeError(u'подстроено')", 1)
    p = os.path.join(tmp, "hook_boom.py")
    with io.open(p, "w", encoding="utf-8", newline="\n") as fh:
        fh.write(src)
    return p


def c_hook_crash_blocks(tmp):
    # ПРЕДУПРЕЖДЕНИЕ ИНТЕГРАТОРА (2026-09-18): проверять надо не только поломку
    # источника, но и поломку самого скрипта. Упавший хук даёт ненулевой код и
    # пустой stdout — Claude Code пропускает ход, и заметить это нечем.
    res = run(tmp, [(u"Сделал правку.", 0)], hook=broken_hook(tmp))
    assert res and u"ХУК УПАЛ" in (res.get("reason") or u""), u"падение не названо"
    log = os.path.join(tmp, "target", ".stop-guard", "escapes.log")
    assert os.path.exists(log), u"падение не оставило следа в логе"
    return res, True


def c_hook_crash_does_not_lock(tmp):
    # Обратная половина: блокировка от падения не имеет права запереть окно.
    # `stop_hook_active` читается из СЫРОГО входа, то есть до кода, который упал.
    return run(tmp, [(u"Сделал правку.", 0)], active=True,
               hook=broken_hook(tmp)), False


def c_queue_broken_json(tmp):
    d = os.path.join(tmp, "target")
    os.makedirs(d, exist_ok=True)
    with io.open(os.path.join(d, "queue.json"), "w", encoding="utf-8") as fh:
        fh.write(u"{ это не json")
    return run(tmp, [(u"Всё.\n\nСТОП: очередь-пуста", 0)]), True


def c_cyrillic_path_cp1251(tmp):
    # Случай интегратора в чистом виде: кириллица в пути и однобайтовая консоль.
    # `print` кириллицей на cp1251 роняет хук UnicodeEncodeError, поэтому решение
    # печатается байтами. tmp здесь свой — с кириллицей в имени.
    d = tempfile.mkdtemp(prefix=u"stopv2-Кириллица-")
    try:
        res = run(d, [(u"Сделал правку.", 0)],
                  env_extra={"PYTHONIOENCODING": "cp1251"})
        assert res and res.get("decision") == "block", u"хук онемел на cp1251"
        assert u"СТОП" in (res.get("reason") or u""), u"причина нечитаема"
        return res, True
    finally:
        shutil.rmtree(d, ignore_errors=True)


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
    (u"окно без роли — хук нем", c_no_role_silent),
    (u"прерывание владельца пропускается", c_interrupt_passes),
    (u"смена сдана: записка свежая", c_shift_with_note),
    (u"смена объявлена, записка трёхчасовой давности", c_shift_without_note),
    (u"снимок ok=false: пропуск + побег в лог", c_snapshot_failed_passes_with_escape),
    (u"хук упал сам — блок, а не немота", c_hook_crash_blocks),
    (u"хук упал повторно — не запирает окно", c_hook_crash_does_not_lock),
    (u"снимок битый JSON — блок", c_queue_broken_json),
    (u"кириллический путь и cp1251 — хук говорит", c_cyrillic_path_cp1251),
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
