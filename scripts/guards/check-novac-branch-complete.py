# -*- coding: utf-8 -*-
"""scripts/guards/check-novac-branch-complete.py — ветвление в novac обязано
быть ПОЛНЫМ (конвенция П31; указание владельца 2026-08-17: «все ветвления
обязательны, else обязателен, если ветка пуста — обозначь явно, что это
валидная ситуация»).

ПОЧЕМУ. За одну смену 2026-08-17 четыре дефекта родились из одного и того же:
решение принималось в `if`, вторая ветка не называлась, и вход просто
проваливался мимо — тип параметра шире идентификатора, тип возврата не судил
никто, каноническая сумма не разбиралась, объявление метода роняло компилятор.
Ни один не выглядел как ошибка в коде: они выглядели как ОТСУТСТВИЕ кода.

ЧТО СЧИТАЕТСЯ ПОЛНЫМ ВЕТВЛЕНИЕМ — три формы, и все три явные:
  1. `if ... { ... } else { ... }` — обе ветки написаны;
  2. then-ветка кончается ТЕРМИНАТОРОМ (`return`, `continue`, `break`, `throw`,
     `ice(`) — тогда «иначе» это остаток функции или следующая итерация;
  3. `else { }` пустой — законен, но ОБЯЗАН нести комментарий с причиной.

ЧЕГО СТРАЖ НЕ ТРЕБУЕТ: `else` после формы 2. Требовать значило бы добавить 255
пустых скобок (замер 2026-08-17), не сообщающих ничего.

ПОЧЕМУ PYTHON: shell-редакция поднимала ДВА процесса awk на КАЖДЫЙ файл — 4.0с
на 33 файлах, из них счёт по существу занимает четверть секунды (П14). Порядок
находок сохранён: сперва все неполные ветвления по файлам, затем все пустые
`else` — от него зависит, что попадёт в `head -n 15` отказа.

$1 — корень; $2 — override директории (шов самотеста).
"""
import os
import pathlib
import re
import sys

sys.stdout.reconfigure(encoding="utf-8", errors="replace", newline="\n")
sys.stderr.reconfigure(encoding="utf-8", errors="replace", newline="\n")

NAME = "check-novac-branch-complete"

RE_IF = re.compile(r"if [^=]")
RE_ELSE_IF = re.compile(r"\} else if ")
RE_ONELINE = re.compile(r"\{.*\}[ \t]*$")
RE_TERM_ANY = re.compile(r"(return|continue|break|throw|ice\()")
RE_OPEN = re.compile(r"\{[ \t]*$")
RE_TERM_HEAD = re.compile(r"(return|continue|break|throw)([ (]|$)")
RE_ICE = re.compile(r"ice\(")
RE_ELSE_EMPTY = re.compile(r"else[ \t]*\{[ \t]*\}?[ \t]*$")
RE_ELSE_FULL = re.compile(r"else[ \t]*\{.*[^ \t{}].*\}")
RE_BRACES_EMPTY = re.compile(r"\{[ \t]*\}[ \t]*$")
RE_BASE = re.compile(r"^incomplete-branches[^\S\n]+(\d+)")


def trim(s):
    # awk-овский trim: только пробел и таб, как gsub(/^[ \t]+|[ \t]+$/, "").
    return s.strip(" \t")


def indent(s):
    n = 0
    for ch in s:
        if ch not in " \t":
            break
        n += 1
    return n


def read_lines(path):
    # Запись awk = строка без \n, затем sub(/\r$/) снимает РОВНО один \r.
    # Больше — не снимает: именно так дерево с `\r\r\n` и было поймано.
    text = path.read_bytes().decode("utf-8", "replace")
    out = text.split("\n")
    if out and out[-1] == "":
        out.pop()
    return [l[:-1] if l.endswith("\r") else l for l in out]


def scan_incomplete(rel, lines, bad):
    """Первый проход: ветвление без else и без терминатора. Возвращает число
    ветвлений в файле (для итоговой цифры «ветвлений N»)."""
    total = 0
    n = len(lines)
    for i in range(1, n + 1):
        s = trim(lines[i - 1])
        if not RE_IF.match(s) and not RE_ELSE_IF.match(s):
            continue
        if s.startswith("//"):
            continue
        total += 1
        if RE_ONELINE.search(s):
            if "else" in s:
                continue
            if RE_TERM_ANY.search(s):
                continue
            bad.append(f"  {rel}:{i} — ветвление без else и без терминатора: {s[:72]}")
            continue
        if not RE_OPEN.search(s):
            continue
        ind = indent(lines[i - 1])
        term = False
        j = n + 1
        for k in range(i + 1, n + 1):
            cur = lines[k - 1]
            t = trim(cur)
            if t.startswith("}") and indent(cur) == ind:
                j = k
                break
            if RE_TERM_HEAD.match(t) or RE_ICE.search(t):
                term = True
        closer = trim(lines[j - 1]) if j <= n else ""
        if closer.startswith("} else"):
            continue
        if term:
            continue
        bad.append(f"  {rel}:{i} — ветвление без else и без терминатора: {s[:72]}")
    return total


def scan_empty_else(rel, lines, bad):
    """Второй проход: пустой else обязан нести причину."""
    n = len(lines)
    for i in range(1, n + 1):
        s = trim(lines[i - 1])
        if not RE_ELSE_EMPTY.search(s):
            continue
        if RE_ELSE_FULL.search(s):
            continue
        body = trim(lines[i]) if i + 1 <= n else ""
        prev = trim(lines[i - 2]) if i >= 2 else ""
        if RE_BRACES_EMPTY.search(s):
            if prev.startswith("//") or "//" in s:
                continue
            bad.append(f"  {rel}:{i} — пустой else без причины: {s[:60]}")
            continue
        if body.startswith("}"):
            if prev.startswith("//"):
                continue
            bad.append(f"  {rel}:{i} — пустой else без причины: {s[:60]}")


def main():
    a = sys.argv
    root = pathlib.Path(a[1] if len(a) > 1 else ".").resolve()
    src = pathlib.Path(a[2]) if len(a) > 2 else root / "novac" / "src"

    if not src.is_dir():
        print(f"{NAME} ok: судить нечего (нет {src})")
        return 0

    files = []
    for dirpath, _dirs, names in os.walk(src):
        for nm in names:
            if nm.endswith(".nv"):
                files.append(pathlib.Path(dirpath) / nm)
    # `find | sort` под LC_ALL=C: порядок находок обязан быть воспроизводим,
    # иначе `head -n 15` в отказе показывал бы каждый раз разные строки.
    files.sort(key=lambda p: str(p).replace("\\", "/"))

    if not files:
        print(f"{NAME}: FAIL — в {src} нет ни одного .nv: страж потерял мишень", file=sys.stderr)
        return 1

    bad = []
    total = 0
    cache = []
    for f in files:
        rel = str(f.relative_to(src)).replace("\\", "/")
        lines = read_lines(f)
        cache.append((rel, lines))
        total += scan_incomplete(rel, lines, bad)
    for rel, lines in cache:
        scan_empty_else(rel, lines, bad)

    base_file = root / "scripts" / "guards" / "novac-branch.baseline"
    base = None
    if base_file.is_file():
        for line in base_file.read_text(encoding="utf-8", errors="replace").replace("\r", "").split("\n"):
            m = RE_BASE.match(line)
            if m:
                base = int(m.group(1))
                break
    if base is None:
        print(f"{NAME}: FAIL — нет базы {base_file}: судить нечем, а нечем != зелено", file=sys.stderr)
        return 1

    n = len(bad)
    if n > base:
        print(f"{NAME}: FAIL — неполных ветвлений {n}, в базе {base} — РОСТ (П31)", file=sys.stderr)
        for b in bad[:15]:
            print(b, file=sys.stderr)
        if n > 15:
            print(f"  ... и ещё {n - 15}", file=sys.stderr)
        print("  Полное ветвление — одно из трёх: else с телом; терминатор в", file=sys.stderr)
        print("  then-ветке (return/continue/break/throw/ice); пустой else С", file=sys.stderr)
        print("  КОММЕНТАРИЕМ, объясняющим, почему ничего не происходит.", file=sys.stderr)
        print("  Несколько if об одном предмете — это match (П31 п.2): он", file=sys.stderr)
        print("  исчерпаем по конструкции, и компилятор сам не даст пропустить.", file=sys.stderr)
        return 1

    if n < base:
        print(f"{NAME}: FAIL — неполных ветвлений {n}, в базе {base} — ПРОГРЕСС без опускания базы", file=sys.stderr)
        print(f"  Опусти число в {base_file} ТЕМ ЖЕ коммитом (§10.4).", file=sys.stderr)
        return 1

    print(f"{NAME} ok: ветвлений {total}, неполных {n} (== база; храповик на убывание)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
