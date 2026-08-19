# -*- coding: utf-8 -*-
"""scripts/guards/check-novac-no-copy-loop.py — коллекция не перекладывается
поэлементно.

ПРАВИЛО. В std есть дверь `Vec[T].append(other AsSlice[T])`. Своя копия
повторяет её реализацию и не получает её правок — это вторая дверь, написанная
циклом.

ЛОВИТ РОВНО ДВЕ ФОРМЫ, обе — чистое перекладывание, где переменная цикла
кладётся в приёмник БЕЗ работы:
  * однострочную `for x in Y { Z.push(x) }`;
  * трёхстрочную: заголовок, одна строка `Z.push(x)`, закрывающая скобка.

НЕ ТРОГАЕТ цикл, который НЕСЁТ работу: `Z.push(f(x))`, условие, накопление —
там цикл законен, и запрет означал бы запрет циклов вообще. Тесты (`*_test.nv`)
вне суда.

ПОЧЕМУ PYTHON: старт процесса дороже самой проверки (П14).

$1 — корень; $2 — override директории (шов самотеста).
"""
import os
import pathlib
import re
import sys

sys.stdout.reconfigure(encoding="utf-8", errors="replace", newline="\n")
sys.stderr.reconfigure(encoding="utf-8", errors="replace", newline="\n")

NAME = "check-novac-no-copy-loop"
RE_ONELINE = re.compile(r"^for ([A-Za-z_][A-Za-z0-9_]*) in .+ \{ "
                        r"[A-Za-z_@][A-Za-z0-9_.]*\.push\(([A-Za-z_][A-Za-z0-9_]*)\) \}$")
RE_HEAD = re.compile(r"^for ([A-Za-z_][A-Za-z0-9_]*) in .+ \{$")
RE_PUSH = re.compile(r"^[A-Za-z_@][A-Za-z0-9_.]*\.push\(([A-Za-z_][A-Za-z0-9_]*)\)$")


def lines_of(path, enc="utf-8"):
    """Строки как их видел awk: хвостовой перевод НЕ даёт лишней записи."""
    out = path.read_bytes().decode(enc, "replace").split("\n")
    if out and out[-1] == "":
        out.pop()
    return out


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
            if nm.endswith(".nv") and not nm.endswith("_test.nv"):
                files.append(pathlib.Path(dirpath) / nm)
    files.sort(key=lambda p: str(p).replace("\\", "/"))

    if not files:
        print(f"{NAME}: FAIL — в {src} нет ни одного .nv: страж потерял мишень", file=sys.stderr)
        return 1

    bad = []
    total = 0
    # Состояние ожидания НЕ сбрасывается на границе файла: так вёл себя единый
    # awk-проход, и вердикт обязан совпасть до буквы.
    state = 0
    pend_var = pend_line = 0
    pend_text = ""
    for f in files:
        rel = str(f.relative_to(src)).replace("\\", "/")
        for n, raw in enumerate(lines_of(f), 1):
            if raw.endswith("\r"):
                raw = raw[:-1]
            line = raw.strip(" \t\v\f")
            total += 1

            m = RE_ONELINE.match(line)
            if m:
                if m.group(1) == m.group(2):
                    bad.append(f"  {rel}:{n} — перекладывание поэлементно: {line[:70]}")
                continue

            m = RE_HEAD.match(line)
            if m:
                pend_var, pend_line, state, pend_text = m.group(1), n, 1, line
                continue

            if state == 1:
                m = RE_PUSH.match(line)
                if m and m.group(1) == pend_var:
                    state = 2
                    continue
                state = 0
                continue

            if state == 2:
                if line == "}":
                    bad.append(f"  {rel}:{pend_line} — перекладывание поэлементно: {pend_text[:70]}")
                state = 0

    if bad:
        print(f"{NAME}: FAIL — коллекция перекладывается поэлементно:", file=sys.stderr)
        for b in bad:
            print(b, file=sys.stderr)
        print("  В std есть дверь: `Vec[T].append(other AsSlice[T])`. Своя копия", file=sys.stderr)
        print("  повторяет её реализацию и не получает её правок.", file=sys.stderr)
        print("  Цикл законен там, где он НЕСЁТ работу: `Z.push(f(x))`, условие,", file=sys.stderr)
        print("  накопление — этого страж не трогает.", file=sys.stderr)
        return 1

    print(f"{NAME} ok: строк .nv: {total}, циклов-перекладываний: 0 (append живёт в std)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
