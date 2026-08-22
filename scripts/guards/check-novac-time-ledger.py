# -*- coding: utf-8 -*-
"""scripts/guards/check-novac-time-ledger.py — доля времени 274/221 берётся из
леджера, а не из памяти (274 §1.4).

ПРОВЕРЯЕТ ТРИ ВЕЩИ:
 (а) ПОКРЫТИЕ: у каждой даты, когда трогали `novac/**`, есть строка в леджере.
     Начало покрытия — МИНИМАЛЬНАЯ дата таблицы, а не первая строка файла
     (адверсарная проверка 274.3/F4: при пересортировке строк `head -1` отдавал
     позднюю дату, и все коммиты раньше неё переставали судиться молча).
 (б) ФОРМАТ: строка, ПОХОЖАЯ на запись (начинается с `|` и несёт дату), но не
     разобранная строгим форматом, — красный, а не тихий пропуск: не вошедшая в
     сумму строка прячет переполнение дня.
 (в) АРИФМЕТИКА: сумма долей за дату <= 1.00. Доля — часть ОДНОГО рабочего дня,
     а не длительность сессии и не «сколько сделано».

Кодовые заборы ``` — иллюстрация формата, а не данные: пример таблицы в заборе
однажды покрасил гейт.

ПОЧЕМУ PYTHON: shell-редакция поднимала awk, пять grep, sort и по процессу на
каждую дату истории — 1.4с, из которых работой был один `git log` (П14).

$1 — корень репозитория.
env NOVA_TL_DATES — самотестовая дверь: подменяет список дат коммитов. ГРОМКАЯ
(274.3/F4): молчаливая подмена половины правила неотличима от работающей
проверки.
"""
import os
import pathlib
import re
import subprocess
import sys

sys.stdout.reconfigure(encoding="utf-8", errors="replace", newline="\n")
sys.stderr.reconfigure(encoding="utf-8", errors="replace", newline="\n")

NAME = "check-novac-time-ledger"
RE_ROW = re.compile(r"^\| (20[0-9][0-9]-[0-9][0-9]-[0-9][0-9]) \|")
RE_DATE = re.compile(r"20[0-9][0-9]-[0-9][0-9]-[0-9][0-9]")
RE_SHARE = re.compile(r"^[0-9]+(\.[0-9]+)?$")


def main():
    root = pathlib.Path(sys.argv[1] if len(sys.argv) > 1 else ".").resolve()
    ledger = root / "docs" / "dev" / "novac-time-ledger.md"
    seam = os.environ.get("NOVA_TL_DATES", "")

    if not (root / "novac").is_dir() and not seam and not ledger.is_file():
        print(f"{NAME} ok: судить нечего (novac ещё нет)")
        return 0
    if not ledger.is_file():
        print(f"{NAME}: FAIL — леджера {ledger} нет, а novac есть (274 §1.4)", file=sys.stderr)
        return 1

    lines = ledger.read_text(encoding="utf-8", errors="replace").replace("\r", "").split("\n")

    row_dates = sorted(m.group(1) for m in (RE_ROW.match(l) for l in lines) if m)
    if not row_dates:
        print(f"{NAME}: FAIL — в леджере нет ни одной строки с датой", file=sys.stderr)
        return 1
    start = row_dates[0]

    if seam:
        print(f"{NAME}: ВНИМАНИЕ — покрытие дат ПОДМЕНЕНО через NOVA_TL_DATES (самотест): {seam}")
        dates = seam.split()
    else:
        # Неглубокий клон (CI с fetch-depth 1) не имеет истории файлов: `git log
        # -- novac` приписывает ВСЁ граничному коммиту. Без истории судить нечего
        # — и это красный: «нет данных» отличимо от «всё покрыто» только отказом.
        gd = subprocess.run(["git", "-C", str(root), "rev-parse", "--git-dir"],
                            capture_output=True).stdout.decode("utf-8", "replace").strip()
        if gd and (pathlib.Path(gd) / "shallow").exists():
            print(f"{NAME}: FAIL — неглубокий клон (shallow): истории novac/** нет, "
                  f"даты коммитов не восстановить; нужен fetch-depth 0", file=sys.stderr)
            return 1
        out = subprocess.run(["git", "-C", str(root), "log", "--format=%as", "--", "novac"],
                             capture_output=True).stdout.decode("utf-8", "replace")
        dates = sorted(set(out.split()))

    bad = 0
    have = {d for d in row_dates}

    # (а) покрытие
    for d in dates:
        if d < start:
            continue
        if d not in have:
            print(f"{NAME}: FAIL — коммит в novac/** от {d} без строки в леджере", file=sys.stderr)
            print("  Одна строка в конце сессии: дата · класс · доля · что (274 §1.4).", file=sys.stderr)
            bad = 1

    # (б,в) формат и арифметика
    unparsed, badval = [], []
    total, count, order = {}, {}, []
    fence = False
    for n, line in enumerate(lines, 1):
        if line.startswith("```"):
            fence = not fence
            continue
        if fence:
            continue
        m = RE_ROW.match(line)
        if not m:
            if line.startswith("|") and RE_DATE.search(line):
                unparsed.append((n, line[:60]))
            continue
        f = line.split("|")
        date = re.sub(r"[ \t]", "", f[1])
        share = re.sub(r"[ \t]", "", f[3]) if len(f) > 3 else ""
        share = re.sub(r"^~", "", share)
        if not RE_SHARE.match(share):
            badval.append((date, share if share else "<пусто>"))
            continue
        if date not in count:
            order.append(date)
        total[date] = total.get(date, 0.0) + float(share)
        count[date] = count.get(date, 0) + 1

    if unparsed:
        print(f"{NAME}: FAIL — строка с датой не разобрана "
              f"(274.3/F4: тихо пропущенная строка не входит в сумму дня):", file=sys.stderr)
        for n, rest in unparsed:
            print(f"  строка {n}: {rest}", file=sys.stderr)
        print("  Формат записи строгий: «| ГГГГ-ММ-ДД | класс | доля | что |» — по одному", file=sys.stderr)
        print("  пробелу вокруг разделителей (иначе арифметика дня считается не по всем", file=sys.stderr)
        print("  строкам, а метрика §1.4 остаётся недействительной незаметно).", file=sys.stderr)
        bad = 1

    if badval:
        print(f"{NAME}: FAIL — доля не число (колонка 3 таблицы леджера):", file=sys.stderr)
        for d, v in badval:
            print(f"  {d}: доля «{v}»", file=sys.stderr)
        print("  Формат доли: 0.4 или ~0.4 — десятичная доля рабочего дня (274 §1.4).", file=sys.stderr)
        bad = 1

    over = [(d, total[d], count[d]) for d in order if total[d] > 1.0 + 1e-9]
    if over:
        print(f"{NAME}: FAIL — сумма долей за дату больше 1.0 (274 §1.4, ревью 274.3/F4):", file=sys.stderr)
        for d, s, c in over:
            print(f"  {d}: сумма {s:.2f} при {c} строках (потолок 1.00)", file=sys.stderr)
        print("  Как чинить: доля — часть ОДНОГО рабочего дня, а не длительность сессии", file=sys.stderr)
        print("  и не «сколько сделано». Пересчитать строки этой даты пропорционально до", file=sys.stderr)
        print("  суммы <= 1.00 (шаг 0.05, минимум 0.05 на строку — строк НЕ терять) и", file=sys.stderr)
        print("  отметить пересчёт примечанием над таблицей, как это сделано 2026-08-15", file=sys.stderr)
        print("  (274.3/F4). Метрика «274 против 221» дня 30 делится на день.", file=sys.stderr)
        bad = 1

    if bad:
        return 1

    n_rows = sum(1 for l in lines if l.startswith("| 20"))
    maxs = max(total.values()) if total else 0.0
    print(f"{NAME} ok: даты коммитов novac покрыты; строк {n_rows}, дат {len(order)}, "
          f"максимум суммы долей за дату {maxs:.2f} (потолок 1.00)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
