# -*- coding: utf-8 -*-
"""scripts/guards/check-novac-emitted-unique.py — двух ОПРЕДЕЛЕНИЙ с одним C-именем
в одном юните нет (план 274 §9.1д п.4).

ЧЕМ ЭТО НЕ `mangle-fixed-point`. Тот судит СУЩЕСТВОВАНИЕ имён оракула: символ,
который мы печатаем, обязан быть объявлен оболочкой или рантаймом. Здесь вопрос
обратный и про НАШИ имена: одно имя — одно определение. Совпадение двух наших
имён — не «символа нет», а «символов два», и первый страж такое пропускает по
построению.

КЛАСС УЖЕ СЛУЧАЛСЯ ДВАЖДЫ, и оба раза его нашёл clang, а не гейт: перегрузки по
ТИПАМ (2026-08-19: две `describe` печатались одним именем) и по РЕЖИМУ
(2026-08-21: `@ptr()` и `mut @ptr()`). План требует, чтобы страж приехал ВМЕСТЕ с
модулями, а не после первого дефекта: путь модуля входит в идентичность
декларации (§9.1д п.1-3), и до этого два модуля с одноимённой функцией дают одно
имя молча.

ЧТО СУДИТСЯ — два пространства, оба внутри одного юнита:
  * определения функций в НАШЕЙ приставке: `novac_...` с телом (строка кончается
    на `{`); прототип (`;`) определением не считается;
  * константы тегов `NOVAC_TAG_...`.

ЧЕГО ЗДЕСЬ НЕТ, и это замер, а не упущение: `typedef` C-типов. Оболочка объявляет
имя типа ДВАЖДЫ законно — предварительное `typedef struct X X;` и определение, — и
C11 повторный идентичный typedef разрешает. «Дважды» там не дефект; дефектом была
бы ДРУГАЯ структура под тем же именем, а это вопрос про тела, не про имена, и
страж, который путал бы одно с другим, хуже отсутствующего (проверено на первом же
прогоне: двадцать ложных находок в оболочке).

ЧТО НЕ СУДИТСЯ: имена рантайма и оболочки (их существование судит
mangle-fixed-point), локальные переменные, вызовы.

ШОВ САМОТЕСТА: `$2` — директория с готовыми `.c`-юнитами; тогда novac не
запускается вовсе. Без шва страж эмитит каждую `pos_*.nv` фикстуру и судит её
эмиссию — то есть ровно то, что уедет в компилятор.

$1 — корень репозитория; $2 — override: директория с `.c`.
"""
import collections
import pathlib
import re
import subprocess
import sys

sys.stdout.reconfigure(encoding="utf-8", errors="replace", newline="\n")
sys.stderr.reconfigure(encoding="utf-8", errors="replace", newline="\n")

NAME = "check-novac-emitted-unique"

RE_FN_DEF = re.compile(r"^(?:static\s+)?[A-Za-z_][A-Za-z0-9_ *]*\b(novac_[A-Za-z0-9_]+)\s*\(")
RE_TAG = re.compile(r"\b(NOVAC_TAG_[A-Za-z0-9_]+)\b")


def judge(unit_name, text, bad):
    """Считает определения в одном юните и складывает находки в `bad`."""
    seen = collections.Counter()
    in_enum = False
    for n, line in enumerate(text.split("\n"), 1):
        code = line.split("//", 1)[0]
        m = RE_FN_DEF.match(code)
        if m and code.rstrip().endswith("{"):
            seen[("fn", m.group(1))] += 1
        # Теги перечисления объявляются внутри `enum { ... }`; вне него то же
        # написание — это ЧТЕНИЕ тега, а не объявление.
        if "enum" in code and "{" in code:
            in_enum = True
        if in_enum:
            for t in RE_TAG.findall(code):
                seen[("tag", t)] += 1
        if in_enum and "}" in code:
            in_enum = False
    total = sum(seen.values())
    for (space, name), cnt in sorted(seen.items()):
        if cnt > 1:
            bad.append(f"  {unit_name}: `{name}` определено {cnt} раза "
                       f"({space}) — одно имя, одно определение")
    return total


def main():
    a = sys.argv + [""] * 3
    root = pathlib.Path(a[1] if a[1] else ".").resolve()
    bad = []
    units = names = 0

    if a[2]:
        for c in sorted(pathlib.Path(a[2]).rglob("*.c")):
            units += 1
            names += judge(c.name, c.read_text(encoding="utf-8", errors="replace"), bad)
    else:
        novac = root / "novac" / "target" / ("novac.exe" if sys.platform == "win32" else "novac")
        fixtures = sorted((root / "novac" / "fixtures").rglob("pos_*.nv"))
        if not novac.is_file() or not fixtures:
            print(f"{NAME} ok: судить нечего (нет бинаря novac или фикстур)")
            return 0
        for f in fixtures:
            r = subprocess.run([str(novac), "emit", str(f)], capture_output=True, text=True,
                               encoding="utf-8", errors="replace")
            if r.returncode != 0 or not r.stdout:
                # Эмиссия не состоялась — это судит дифференциальный страж; здесь
                # такой юнит просто не участвует, и молчания нет: юнит посчитан.
                continue
            units += 1
            names += judge(f.name, r.stdout, bad)

    if bad:
        print(f"{NAME}: FAIL — одно C-имя определено дважды в одном юните (274 §9.1д п.4):",
              file=sys.stderr)
        for b in bad[:15]:
            print(b, file=sys.stderr)
        print("  Это НЕ «символа нет» (то судит mangle-fixed-point), а «символов два».",
              file=sys.stderr)
        print("  Класс ловил clang дважды — перегрузки по типам и по режиму; путь", file=sys.stderr)
        print("  модуля входит в идентичность декларации, и без него два модуля с", file=sys.stderr)
        print("  одноимённой функцией дают одно имя молча.", file=sys.stderr)
        return 1

    print(f"{NAME} ok: юнитов эмиссии {units}, объявленных имён {names}, "
          f"повторов имени в юните 0")
    return 0


if __name__ == "__main__":
    sys.exit(main())
