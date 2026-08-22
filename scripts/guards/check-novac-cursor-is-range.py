# -*- coding: utf-8 -*-
"""scripts/guards/check-novac-cursor-is-range.py — курсор, который всегда шагает
на ОДИН, это диапазон (П32, вопрос владельца 2026-08-22).

ЗАЧЕМ. «`mut j = 2` / `while j < pat.len()` — почему не `for j in 2..pat.len()`?»
Причины не нашлось: шесть таких циклов в компиляторе продвигались ровно на один и
ровно один раз за оборот. Что даёт ручной курсор взамен: лишний `mut`, лишнюю
строку и НОВЫЙ класс ошибки — «забыл продвинуть». Причём код это уже знал: рядом с
одним из них стоял комментарий-ПРЕДУПРЕЖДЕНИЕ «обход не может `continue`: продвигает
его `j += 1` ниже». Предупреждение об опасности хуже формы без опасности —
в `for` над диапазоном `continue` продвигает по построению.

ПРАВИЛО. В `novac/src/**/*.nv` запрещена связка: `mut <i> = <нечто>` + `while <i> <
…` + РОВНО ОДИН безусловный `<i> += 1` в теле цикла. Это `for <i> in <нечто>..<…>`.

ЧТО ЗАКОННО и НЕ судится:
  * шаг не единица или шаг УСЛОВНЫЙ: `i += 2` при экранировании, две ветки с
    разным шагом — там шаг несёт информацию, и диапазон её потеряет (живой случай
    в `emit_c`, помечен комментарием);
  * `while` без счётчика вовсе — обход цепочки (`r = rows[r].next_row`), фикспойнт
    (`while grew`), чтение до конца;
  * два и более продвижения одного курсора: это уже машина состояний;
  * граница через `<=`, а не `<`: диапазон полуоткрыт, и переписывание потребовало
    бы `+ 1` в границе — арифметика в границе читается хуже честного `while`.
    Первый прогон поймал стража на себе самом: регулярка разобрала `<=` как `<`
    и предложила бессмысленное `0..= n`.

СЧИТАЕТ страж по ТЕЛУ цикла, а не по окну строк: `+= 1` внутри `if` — условный, и
такой цикл зелёный.

$1 — корень; $2 — override директории (шов самотеста).
"""
import os
import pathlib
import re
import sys

sys.stdout.reconfigure(encoding="utf-8", errors="replace", newline="\n")
sys.stderr.reconfigure(encoding="utf-8", errors="replace", newline="\n")

NAME = "check-novac-cursor-is-range"

RE_MUT = re.compile(r"^\s*mut\s+([a-z_][A-Za-z0-9_]*)\s*=\s*(.+?)\s*$")
RE_WHILE = re.compile(r"^\s*while\s+([a-z_][A-Za-z0-9_]*)\s*<(?!=)\s*(.+?)\s*\{\s*$")
RE_STEP = re.compile(r"^\s*([a-z_][A-Za-z0-9_]*)\s*\+=\s*(\d+)\s*$")


def body_of(lines, start):
    """Строки тела цикла, открытого на `start`, и их отступы."""
    depth = 0
    out = []
    for k in range(start, len(lines)):
        line = lines[k]
        code = line.split("//", 1)[0]
        if k > start:
            out.append(line)
        depth += code.count("{") - code.count("}")
        if k > start and depth <= 0:
            return out[:-1]
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
            if nm.endswith(".nv"):
                files.append(pathlib.Path(dirpath) / nm)
    files.sort(key=lambda p: str(p).replace("\\", "/"))

    if not files:
        print(f"{NAME}: FAIL — в {src} нет ни одного .nv: страж потерял мишень (класс №519)",
              file=sys.stderr)
        return 1

    bad = []
    loops = 0
    for f in files:
        rel = str(f.relative_to(src)).replace("\\", "/")
        lines = f.read_bytes().decode("utf-8", "replace").replace("\r", "").split("\n")
        for n, line in enumerate(lines):
            mw = RE_WHILE.match(line.split("//", 1)[0])
            if not mw:
                continue
            loops += 1
            var, bound = mw.group(1), mw.group(2)

            # инициализация: ближайший `mut <var> = ...` выше по файлу
            init = None
            for k in range(n - 1, max(-1, n - 6), -1):
                mi = RE_MUT.match(lines[k].split("//", 1)[0])
                if mi and mi.group(1) == var:
                    init = mi.group(2)
                    break
            if init is None:
                continue                      # курсор не отсюда: не наш случай

            body = body_of(lines, n)
            steps = []
            for bl in body:
                ms = RE_STEP.match(bl.split("//", 1)[0])
                if ms and ms.group(1) == var:
                    # безусловный = отступ ровно на уровне тела (одна ступень)
                    indent = len(bl) - len(bl.lstrip())
                    steps.append((int(ms.group(2)), indent))
            if len(steps) != 1:
                continue                      # ноль, два и более — машина состояний
            step, indent = steps[0]
            if step != 1:
                continue                      # шаг несёт информацию
            base = len(line) - len(line.lstrip())
            if indent != base + 4:
                continue                      # `+= 1` вложен в условие: условный

            bad.append(f"  {rel}:{n + 1}: `while {var} < {bound}` с одним безусловным "
                       f"`{var} += 1` — это `for {var} in {init}..{bound}`")

    if bad:
        print(f"{NAME}: FAIL — курсор, шагающий всегда на один, написан вручную (П32):",
              file=sys.stderr)
        for b in bad:
            print(b, file=sys.stderr)
        print("  Ручной курсор даёт лишний `mut`, лишнюю строку и НОВЫЙ класс ошибки —", file=sys.stderr)
        print("  «забыл продвинуть». Диапазон его удаляет: `continue` продвигает сам.", file=sys.stderr)
        print("  Законен `while` там, где шаг НЕ единица или условный (тогда шаг несёт", file=sys.stderr)
        print("  информацию), и там, где курсора нет вовсе — обход цепочки, фикспойнт.", file=sys.stderr)
        return 1

    print(f"{NAME} ok: файлов .nv: {len(files)}, циклов `while`: {loops}, "
          f"ручных курсоров с шагом один: 0")
    return 0


if __name__ == "__main__":
    sys.exit(main())
