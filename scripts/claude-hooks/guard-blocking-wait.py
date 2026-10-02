#!/usr/bin/env python3
"""guard-blocking-wait.py — PreToolUse-хук Claude Code (Bash + PowerShell):
не даёт окну занять ход ЦИКЛОМ ОЖИДАНИЯ на переднем плане.

ПОЧЕМУ. Окно интегратора — точка, куда владелец приносит работу облачных сессий и
соседей. Цикл `for i in $(seq 1 27); do grep … && break; sleep 20; done` держит ход
до девяти минут: владелец не может вставить слово, не прервав его, а прерывание
отменяет и само ожидание. 2026-10-01 владелец трижды прерывал такие циклы; правило
«долгое ждать фоном» записали в память — и 2026-10-02 окно поставило такой цикл
больше десяти раз за день, пока владелец не спросил «почему ты блокируешь себя этой
задачей?» и не сказал: «правила не работают к сожалению». Соблюдение зависело от
того, вспомнит ли агент правило в моменте; этот хук от этого не зависит.
Отдельный триггер того дня: Stop-хук отказывал «второй вопрос подряд без действия»,
и ожидание подставлялось как «действие». Ожидание — не работа.

ЧТО ДЕЛАТЬ ВМЕСТО. Долгое — запускать с `run_in_background`: о завершении придёт
уведомление, ход отдаётся коротким докладом. Нужен ответ прямо сейчас — команда,
которая сама ждёт результата (`git push`, `cargo build`, прогон стража), а не опрос.

ЧТО ПРОВЕРЯЕТ (только передний план — `run_in_background` не судится):
  1. `sleep`/`Start-Sleep` внутри цикла (`for`, `while`, `until`, `foreach`) —
     это опрос: отказ при любом числе секунд;
  2. одиночный `sleep N` / `Start-Sleep N` с N >= 60 — отказ.
Клапан — пометка в самой команде `# blocking-wait-ok: <причина>` (как
`# index-verified:` у guard-git): осознанное короткое ожидание, видимое в логе.

План: docs/plans/231-bug-cycle-exit.md, трек Д п.6 («правила из памяти переезжают в
перехватчик»); родня — guard-shell-nonascii.py (тот же путь: правило из памяти,
не сработавшее трижды, стало перехватчиком).
"""
import json
import re
import sys

try:
    sys.stdout.reconfigure(encoding="utf-8", errors="replace")
    sys.stderr.reconfigure(encoding="utf-8", errors="replace")
except Exception:
    pass

VALVE = re.compile(r"#\s*blocking-wait-ok:\s*\S")
LOOP = re.compile(r"(?i)(^|[\s;&|({])(for|while|until|foreach)\b")
SLEEP = re.compile(r"(?i)(^|[\s;&|({])(sleep|start-sleep)(\s+-(s|seconds))?\s+(\d+(\.\d+)?)")
LONG = 60


def verdict(cmd: str, background: bool):
    """None — пропустить; иначе текст отказа."""
    if background or not cmd or VALVE.search(cmd):
        return None
    sleeps = [float(m.group(5)) for m in SLEEP.finditer(cmd)]
    if not sleeps:
        return None
    if LOOP.search(cmd):
        return "цикл ожидания: `sleep` внутри цикла на переднем плане (опрос)"
    if max(sleeps) >= LONG:
        return "одиночный `sleep %g` на переднем плане (>= %d с)" % (max(sleeps), LONG)
    return None


def main() -> int:
    try:
        data = json.loads(sys.stdin.read() or "{}")
        ti = data.get("tool_input") or {}
        cmd = ti.get("command") or ""
        bg = bool(ti.get("run_in_background"))
    except Exception:
        return 0
    why = verdict(cmd, bg)
    if why is None:
        return 0
    sys.stderr.write(
        "ЗАПРЕЩЕНО: %s.\n" % why
        + "  ПОЧЕМУ: ход занят ожиданием — владелец не может вставить слово, не прервав\n"
        + "  его (2026-10-02: «почему ты блокируешь себя этой задачей?»; правило в памяти\n"
        + "  не сработало ни разу за день). Ожидание — не работа и не «действие» для Stop-хука.\n"
        + "  КАК НАДО: запусти долгое с run_in_background — уведомление придёт само, —\n"
        + "  и отдай ход коротким докладом; либо делай настоящую независимую работу.\n"
        + "  Осознанное короткое ожидание — пометкой в команде `# blocking-wait-ok: <причина>`.\n"
    )
    return 2


if __name__ == "__main__":
    sys.exit(main())
