# -*- coding: utf-8 -*-
u"""Самотест stop-хука `scripts/claude-hooks/guard-stop.py`.

ЗАЧЕМ ОН ЕСТЬ. До 2026-08-29 у хуков среды агента самотестов не было ВОВСЕ,
хотя они есть почти у каждого стража: хук подачи правил после сжатия был
написан и подключён, проверенный одним ручным запуском (замер плана 276,
наблюдение 3). Хук, который молча перестал работать, неотличим от хука,
которому нечего сказать, — а ошибается он в сторону «пропустить», то есть
тихо.

ЧТО ПРОВЕРЯЕТ — ОБЕ СТОРОНЫ КАЖДОГО УСЛОВИЯ:
  условие 1 — объявлен следующий шаг и не сделан → блок; законная остановка
              (вопрос владельцу) → пропуск; `stop_hook_active` → пропуск;
  условие 2 — работа агентов названа, строки «Модели агентов:» нет → блок;
              строка есть → пропуск; агентов не называли → пропуск.

Запуск: `python scripts/claude-hooks/selftest/test-guard-stop.py`
"""
from __future__ import annotations

import io
import json
import os
import subprocess
import sys
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
HOOK = os.path.join(HERE, "..", "guard-stop.py")

fails = 0


def ok(name):
    print(u"  ok   %s" % name)


def bad(name, detail):
    global fails
    fails += 1
    sys.stderr.write(u"  FAIL %s: %s\n" % (name, detail))


def transcript(text):
    """Стенограмма из одного сообщения ассистента — как её пишет харнесс."""
    fd, path = tempfile.mkstemp(suffix=".jsonl")
    os.close(fd)
    rec = {"type": "assistant",
           "message": {"content": [{"type": "text", "text": text}]}}
    with io.open(path, "w", encoding="utf-8", newline="\n") as fh:
        fh.write(json.dumps(rec, ensure_ascii=False) + "\n")
    return path


def run(text, stop_hook_active=False):
    path = transcript(text)
    try:
        payload = {"transcript_path": path}
        if stop_hook_active:
            payload["stop_hook_active"] = True
        p = subprocess.run([sys.executable, HOOK],
                           input=json.dumps(payload).encode("utf-8"),
                           stdout=subprocess.PIPE, stderr=subprocess.PIPE)
        out = p.stdout.decode("utf-8", "replace").strip()
        if not out:
            return None
        return json.loads(out)
    finally:
        os.unlink(path)


def blocked(res):
    return bool(res) and res.get("decision") == "block"


# ── условие 1: объявленный и несделанный следующий шаг ───────────────────────
r = run(u"Сделал правку, гейт зелёный. Продолжаю: следующим беру строку реестра.")
if blocked(r):
    ok(u"объявлен следующий шаг и не сделан — блок")
else:
    bad(u"условие 1 не сработало", repr(r))

r = run(u"Правка внесена, гейт зелёный. Очередь пуста.")
if not blocked(r):
    ok(u"обычный конец хода без обещания — пропуск")
else:
    bad(u"ложный блок на обычном конце хода", repr(r))

r = run(u"Продолжаю после вашего слова: какой путь выбираем?")
if not blocked(r):
    ok(u"вопрос владельцу — законная остановка, пропуск")
else:
    bad(u"вопрос владельцу не должен блокироваться", repr(r))

r = run(u"Продолжаю: следующий шаг — гейт.", stop_hook_active=True)
if not blocked(r):
    ok(u"stop_hook_active — пропуск (защита от зацикливания)")
else:
    bad(u"зацикливание: хук блокирует уже продолженный ход", repr(r))

# ── условие 2: работа агентов без названных моделей ─────────────────────────
r = run(u"Инвентарь готов: делегировал перечисление, агент вернул одиннадцать строк. "
        u"Очередь пуста.")
if blocked(r) and u"Модели агентов" in (r.get("reason") or ""):
    ok(u"агенты были, строки моделей нет — блок, и сказано какой строки")
else:
    bad(u"условие 2 не сработало", repr(r))

r = run(u"Инвентарь готов: делегировал перечисление, агент вернул одиннадцать строк.\n\n"
        u"Модели агентов: sonnet (инвентарь). Очередь пуста.")
if not blocked(r):
    ok(u"строка моделей на месте — пропуск")
else:
    bad(u"ложный блок при наличии строки моделей", repr(r))

r = run(u"Правило про агентов записано в delegate.md; сам никого не пускал. Очередь пуста.")
if not blocked(r):
    ok(u"упоминание агентов без их запуска — пропуск (узкая граница держит)")
else:
    bad(u"ложный блок на обсуждении правил про агентов", repr(r))

r = run(u"Отчёт.\n\nМодели агентов: haiku (сверка цитат).\n\n"
        u"Дальше беру следующий пункт плана.")
if blocked(r) and u"Модели агентов" not in (r.get("reason") or ""):
    ok(u"строка моделей есть, но обещание не выполнено — блок по условию 1")
else:
    bad(u"условия обязаны судить независимо", repr(r))

# ── границы ─────────────────────────────────────────────────────────────────
r = run(u"")
if not blocked(r):
    ok(u"пустое сообщение — пропуск, а не падение")
else:
    bad(u"пустое сообщение не должно блокировать", repr(r))

print(u"самотест guard-stop: PASS %d FAIL %d" % (9 - fails, fails))
sys.exit(1 if fails else 0)
