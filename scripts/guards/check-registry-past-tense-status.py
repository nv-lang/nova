# -*- coding: utf-8 -*-
"""scripts/guards/check-registry-past-tense-status.py — закрытие, написанное
ПРОЗОЙ вместо поля, невидимо сканеру и раздувает счёт блокеров тега.

ЗАЧЕМ (реестр 221.1 №452, найдено 2026-08-23). Строка №452 была закрыта ЗАМЕРОМ
14 августа и записала это фразой «Статус был: ЗАКРЫТ 2026-08-14». Сканер
`registry-routes-scan.py` ищет поле `**Статус:**`; не найдя его, он считает
строку ОТКРЫТОЙ. Итог: двенадцать дней в счёте блокеров релиза стояло то, что
блокером не было. При разборе выяснилось, что носитель не один: **23 строки**
несут ту же фразу без поля, и девять из них сидят в списке открытых блокеров.

ПОЧЕМУ ЭТО ДОРОГО, а не косметика: заглавное число релиза — то, по чему решают
«готовы ли». Число, завышенное фразеологией, врёт в ту сторону, где работа
кажется больше, чем есть, и решение «ещё рано» принимается без основания.
Обратная ошибка того же класса уже описана в шапке реестра: четыре БЛОКИРУЮЩИЕ
строки стояли OPEN при уже слитом фиксе.

ЧТО ПРОВЕРЯЕТ: строка реестра, где встречается «Статус был:», обязана нести и
поле `**Статус:**`. Одно другому не мешает — история пишется прозой, а живое
состояние объявляется полем, которое читают машины.

ЧЕГО НЕ ЛОВИТ (сказано честно): ПРАВДИВОСТЬ закрытия. Что строка закрыта верно,
доказывает замер, а не форма записи; страж следит лишь за тем, чтобы вердикт
было ВИДНО. Проверка правдивости — работа окна, и по №452 она делалась заново
(15 прогонов из 15) ПЕРЕД тем, как трогать формулировку.

ХРАПОВИК: число только вниз. База — `registry-past-tense.baseline`.

ИСПОЛЬЗОВАНИЕ:
  python scripts/guards/check-registry-past-tense-status.py [КОРЕНЬ]
  python scripts/guards/check-registry-past-tense-status.py --selftest
"""
import io
import pathlib
import re
import sys

sys.stdout.reconfigure(encoding="utf-8", errors="replace", newline="\n")
sys.stderr.reconfigure(encoding="utf-8", errors="replace", newline="\n")

NAME = "check-registry-past-tense-status"
PAST = "Статус был:"
FIELD = "**Статус:**"
RE_ROW = re.compile(r"^\| (\d+) \|")
RE_BASE = re.compile(r"^past-tense-status[^\S\n]+(\d+)")


def scan(text):
    """Rows that state a closure in prose and never declare it as a field."""
    bad = []
    for line in text.replace("\r\n", "\n").split("\n"):
        m = RE_ROW.match(line)
        if not m:
            continue
        if PAST in line and FIELD not in line:
            bad.append(m.group(1))
    return bad


def main():
    a = sys.argv
    if len(a) > 1 and a[1] == "--selftest":
        return selftest()

    root = pathlib.Path(a[1] if len(a) > 1 else ".").resolve()
    reg = root / "docs" / "plans" / "221.1-bug-sweep.md"
    base_file = root / "scripts" / "guards" / "registry-past-tense.baseline"

    if not reg.is_file():
        print(f"{NAME}: FAIL — нет реестра {reg}: страж потерял мишень (класс №519)",
              file=sys.stderr)
        return 1

    bad = scan(reg.read_text(encoding="utf-8", errors="replace"))
    n = len(bad)

    if not base_file.is_file():
        print(f"{NAME}: FAIL — нет базы {base_file}: судить нечем, а нечем != зелено",
              file=sys.stderr)
        return 1

    base = None
    for line in base_file.read_text(encoding="utf-8", errors="replace").replace("\r", "").split("\n"):
        mm = RE_BASE.match(line)
        if mm:
            base = int(mm.group(1))
    if base is None:
        print(f"{NAME}: FAIL — в {base_file} нет строки `past-tense-status <число>`",
              file=sys.stderr)
        return 1

    if n > base:
        print(f"{NAME}: FAIL — строк с закрытием ПРОЗОЙ без поля {n} > базы {base}:",
              file=sys.stderr)
        for r in bad[:15]:
            print(f"  №{r}", file=sys.stderr)
        if n > 15:
            print(f"  ... и ещё {n - 15}", file=sys.stderr)
        print("  Закрытие, написанное «Статус был: ЗАКРЫТ», сканер НЕ читает — он ищет",
              file=sys.stderr)
        print("  поле `**Статус:**`. Такая строка вечно числится открытым блокером.",
              file=sys.stderr)
        print("  Объяви состояние полем; прозу-историю оставь рядом, она не мешает.",
              file=sys.stderr)
        return 1

    if n < base:
        print(f"{NAME}: FAIL — {n} < базы {base} — ПРОГРЕСС без опускания базы", file=sys.stderr)
        print(f"  Опусти число в {base_file} ТЕМ ЖЕ коммитом.", file=sys.stderr)
        return 1

    print(f"{NAME} ok: закрытий прозой без поля {n} (== база); новых не прибавилось")
    return 0


def selftest():
    """Обе стороны: ловит прозу без поля, не ложнит на прозе С полем."""
    bad_row = "| 999 | К1 | текст. Статус был: ЗАКРЫТ 2026-08-14 (волна). |"
    good_row = "| 998 | К1 | текст. **Статус:** ЗАКРЫТ 2026-08-14. Статус был: ЗАКРЫТ. |"
    plain_row = "| 997 | К1 | текст. **Статус:** ОТКРЫТ. |"

    hits = scan(bad_row)
    if hits != ["999"]:
        print(f"  FAIL: проза без поля не поймана (получено {hits})", file=sys.stderr)
        return 1
    print("  ok: ловит закрытие прозой без поля")

    hits = scan(good_row)
    if hits:
        print(f"  FAIL: ложняк на прозе С полем (получено {hits})", file=sys.stderr)
        return 1
    print("  ok: проза рядом с полем — не нарушение")

    hits = scan(plain_row)
    if hits:
        print(f"  FAIL: ложняк на обычной строке (получено {hits})", file=sys.stderr)
        return 1
    print("  ok: обычная строка не краснит")

    hits = scan("не строка реестра, Статус был: ЗАКРЫТ")
    if hits:
        print(f"  FAIL: текст вне таблицы засчитан строкой (получено {hits})", file=sys.stderr)
        return 1
    print("  ok: текст вне таблицы не считается строкой")

    print(f"{NAME} selftest: ALL OK")
    return 0


if __name__ == "__main__":
    sys.exit(main())
