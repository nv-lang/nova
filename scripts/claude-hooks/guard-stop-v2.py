# -*- coding: utf-8 -*-
u"""Stop-хук v2 (ЧЕРНОВИК плана 292): остановка требует ДОКАЗАТЕЛЬСТВА, а
продолжение не требует ничего.

НЕ ПОДКЛЮЧЁН. В `.claude/settings.json` по-прежнему стоит `guard-stop.py`.
Этот файл — черновик к решению владельца (план 292 Р.1-Р.5); подключается
шагом Ш.4 того же плана, одной строкой в `settings.json`, и тем же коммитом
снимается v1.

ЗАЧЕМ. Владелец 2026-09-18: «работа по плану постоянно останавливается и в
интеграторе, и в Карине; /flow не помогает, /push-controller сам иногда
останавливается, /goal не помогает; как решить раз и навсегда». Замер по
одиннадцати стенограммам проекта: 1104 конца хода, из них с объявленным
следующим шагом 7, заблокировано v1-хуком 5. Один процент. Остальные
остановки МОЛЧАЛИВЫЕ — в них нет слов, по которым v1 судит.

ЧТО МЕНЯЕТСЯ. v1 ищет обещание в тексте и блокирует нарушенное. v2 не читает
намерений: он требует, чтобы конец хода НЁС ПРИЧИНУ КОДОМ, и проверяет код
against мира. Нет строки — блокируем; код есть — проверяем то, что можно
проверить машиной. Умолчание переворачивается: раньше продолжение держалось
на памяти окна, теперь на памяти держится остановка.

ФОРМА. Последний абзац доклада несёт строку:

    СТОП: очередь-пуста
    СТОП: вопрос
    СТОП: неавторизовано <действие>

Три кода — ровно три законные причины из `/flow`, других нет.

ЧТО ПРОВЕРЯЕТСЯ ПО КАЖДОМУ КОДУ.

* `очередь-пуста` — сверяется со снимком `target/queue.json`, который пишет
  `scripts/tools/queue-snapshot.py` (план 292 Ш.2). Снимка нет или он старше
  получаса — блокируем с требованием обновить; в снимке есть открытые пункты —
  блокируем и называем их. Снимок ВЫВОДИТСЯ командами (три зеркала,
  `--no-merged`, `registry-routes-scan.py`), то есть решение 2026-08-10
  «очередь спрашивается, а не ведётся» в силе: второго дома правды не
  появляется, появляется КЭШ ответа, который машина умеет прочитать в момент
  остановки.
* `вопрос` — в последнем абзаце обязан быть вопросительный знак, и предыдущий
  ход не должен был кончиться тем же кодом без единого вызова инструмента
  между ними. Два вопроса подряд без работы — это не вопрос, это пауза.
* `неавторизовано` — названное действие обязано быть из закрытого списка
  (пуш, тег, удаление, публикация, наружу). Пустое «неавторизовано» не
  доказательство.

ПРЕДОХРАНИТЕЛЬ, И ОН ОБЯЗАТЕЛЕН. Хук, который может запереть окно, однажды
запрёт. Поэтому: `stop_hook_active` пропускается (это делает и v1), а сверх
того считается число блокировок ПОДРЯД в файле состояния. Третья подряд
пропускается, а случай пишется в `target/.stop-guard/escapes.log` — чтобы
побег был виден владельцу, а не растворился в тишине. Счётчик сбрасывается
любым пропуском.

ЦЕНА. Ноль запусков процессов: хук читает стенограмму и два маленьких файла.
Git не зовётся — за это отвечает снимок, который обновляется отдельно.
"""
from __future__ import annotations

import io
import json
import os
import re
import sys
import time

try:
    sys.stdout.reconfigure(encoding="utf-8", errors="replace")
except Exception:
    pass

STOP_RE = re.compile(
    u"^[\\s>*_-]*СТОП:\\s*(очередь-пуста|вопрос|неавторизовано)\\b[ \\t]*(.*)$",
    re.IGNORECASE | re.MULTILINE,
)

# Закрытый список необратимых действий (AGENTS.md «Git», /flow пункт 2).
IRREVERSIBLE = [u"пуш", u"push", u"тег", u"tag", u"удал", u"публик",
                u"наружу", u"force", u"релиз", u"слия", u"merge"]

QUEUE_MAX_AGE_SEC = 30 * 60
MAX_BLOCKS_IN_ROW = 2


def block(reason):
    print(json.dumps({"decision": "block", "reason": reason}, ensure_ascii=False))
    return True


def read_turns(path):
    u"""Возвращает список ходов: [(текст последнего сообщения, были ли вызовы
    инструментов после него)]. Нужен ровно для правила «два вопроса подряд»."""
    turns = []
    if not path or not os.path.exists(path):
        return turns
    cur_text = None
    tools_since = 0
    with io.open(path, encoding="utf-8", errors="replace") as fh:
        for line in fh:
            line = line.strip()
            if not line:
                continue
            try:
                rec = json.loads(line)
            except Exception:
                continue
            t = rec.get("type")
            if t == "assistant":
                msg = rec.get("message") or {}
                parts = msg.get("content") or []
                if isinstance(parts, str):
                    parts = [{"type": "text", "text": parts}]
                buf = []
                for p in parts:
                    if not isinstance(p, dict):
                        continue
                    if p.get("type") == "text":
                        buf.append(p.get("text") or "")
                    elif p.get("type") == "tool_use":
                        tools_since += 1
                if any(b.strip() for b in buf):
                    cur_text = "\n".join(buf)
            elif t == "user":
                if cur_text is not None:
                    turns.append((cur_text, tools_since))
                cur_text = None
                tools_since = 0
    if cur_text is not None:
        turns.append((cur_text, tools_since))
    return turns


def state_path(cwd, session):
    d = os.path.join(cwd or ".", "target", ".stop-guard")
    try:
        os.makedirs(d, exist_ok=True)
    except Exception:
        return None
    return os.path.join(d, "%s.json" % (session or "unknown"))


def load_state(p):
    if not p or not os.path.exists(p):
        return {"blocks": 0}
    try:
        with io.open(p, encoding="utf-8") as fh:
            return json.load(fh)
    except Exception:
        return {"blocks": 0}


def save_state(p, st):
    if not p:
        return
    try:
        with io.open(p, "w", encoding="utf-8") as fh:
            fh.write(json.dumps(st, ensure_ascii=False))
    except Exception:
        pass


def log_escape(cwd, text):
    d = os.path.join(cwd or ".", "target", ".stop-guard")
    try:
        os.makedirs(d, exist_ok=True)
        with io.open(os.path.join(d, "escapes.log"), "a", encoding="utf-8") as fh:
            fh.write(u"%s %s\n" % (time.strftime("%Y-%m-%d %H:%M"), text))
    except Exception:
        pass


def check_queue(cwd):
    u"""(ok, причина-отказа). Снимок отсутствует/просрочен/непуст — не ok."""
    p = os.path.join(cwd or ".", "target", "queue.json")
    if not os.path.exists(p):
        return False, (
            u"Код «очередь-пуста» ничем не подтверждён: снимка `target/queue.json` нет. "
            u"Сними его — `python scripts/tools/queue-snapshot.py .` — и либо останови "
            u"ход с доказанным пустым снимком, либо бери из него следующий пункт."
        )
    age = time.time() - os.path.getmtime(p)
    if age > QUEUE_MAX_AGE_SEC:
        return False, (
            u"Снимку очереди %d мин, порог %d. Устаревший снимок доказательством "
            u"не считается: обнови `python scripts/tools/queue-snapshot.py .`."
            % (int(age // 60), QUEUE_MAX_AGE_SEC // 60)
        )
    try:
        with io.open(p, encoding="utf-8") as fh:
            q = json.load(fh)
    except Exception as e:
        return False, u"Снимок очереди не читается (%s) — обнови его." % e
    if q.get("role") == "none":
        return True, u""
    items = q.get("open") or []
    if items:
        head = u"; ".join(unicode_str(i) for i in items[:5])
        more = u" (и ещё %d)" % (len(items) - 5) if len(items) > 5 else u""
        return False, (
            u"Очередь НЕ пуста: %d пункт(ов) — %s%s. Бери следующий сейчас, в этом "
            u"же ходе. Если пункт больше не нужен — сними его в снимке и объясни "
            u"строкой, а не молчанием." % (len(items), head, more)
        )
    return True, u""


def unicode_str(x):
    if isinstance(x, dict):
        return x.get("title") or x.get("id") or json.dumps(x, ensure_ascii=False)
    return u"%s" % x


def main():
    try:
        data = json.loads(sys.stdin.read() or "{}")
    except Exception:
        return 0

    if data.get("stop_hook_active"):
        return 0

    cwd = data.get("cwd") or os.environ.get("CLAUDE_PROJECT_DIR") or "."
    sp = state_path(cwd, data.get("session_id"))
    st = load_state(sp)

    turns = read_turns(data.get("transcript_path") or "")
    if not turns:
        return 0
    text = turns[-1][0]
    tail = text[-900:]

    # Предохранитель: третья блокировка подряд не ставится.
    if st.get("blocks", 0) >= MAX_BLOCKS_IN_ROW:
        st["blocks"] = 0
        save_state(sp, st)
        log_escape(cwd, u"пропуск после %d блокировок подряд" % MAX_BLOCKS_IN_ROW)
        return 0

    m = STOP_RE.search(tail)
    if not m:
        st["blocks"] = st.get("blocks", 0) + 1
        save_state(sp, st)
        block(
            u"Ход окончен без причины. Остановка требует доказательства: последняя "
            u"строка доклада обязана нести код — «СТОП: очередь-пуста» (сверяется со "
            u"снимком `target/queue.json`), «СТОП: вопрос» (в абзаце есть вопрос) или "
            u"«СТОП: неавторизовано <действие>» (пуш, тег, удаление, публикация). "
            u"Если причины нет — значит очередь не пуста: бери следующий пункт сейчас, "
            u"в этом же ходе."
        )
        return 0

    code = m.group(1).lower()
    arg = (m.group(2) or u"").strip()

    ok = True
    reason = u""

    if code == u"очередь-пуста":
        ok, reason = check_queue(cwd)

    elif code == u"вопрос":
        if u"?" not in tail:
            ok, reason = False, (
                u"Код «вопрос» без вопроса: в последнем абзаце нет ни одного знака "
                u"вопроса. Либо задай вопрос, либо это не причина остановки."
            )
        elif len(turns) >= 2:
            prev_text, tools_between = turns[-2][0], turns[-1][1]
            prev = STOP_RE.search(prev_text[-900:])
            if prev and prev.group(1).lower() == u"вопрос" and tools_between == 0:
                ok, reason = False, (
                    u"Второй вопрос подряд без единого действия между ними. Вопрос — "
                    u"причина остановки, когда без ответа работа НЕ идёт; если идёт — "
                    u"делай её, а вопрос задай в конце сделанного."
                )

    elif code == u"неавторизовано":
        low = arg.lower()
        if not any(w in low for w in IRREVERSIBLE):
            ok, reason = False, (
                u"Код «неавторизовано» без названного действия. Назови его словом из "
                u"закрытого списка: пуш, тег, удаление, публикация, слияние. Всё "
                u"остальное авторизовано самой задачей и остановкой не является."
            )

    if ok:
        st["blocks"] = 0
        save_state(sp, st)
        return 0

    st["blocks"] = st.get("blocks", 0) + 1
    save_state(sp, st)
    block(reason)
    return 0


if __name__ == "__main__":
    sys.exit(main())
