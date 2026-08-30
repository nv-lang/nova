# -*- coding: utf-8 -*-
"""scripts/guards/check-novac-required-token-door.py — токен, который грамматика
ТРЕБУЕТ, берётся обязательной дверью (#809, #815).

ПОЧЕМУ. У парсера две двери на «взять токен». `@push_if` означает «токена может
не быть, и это законный исход» — верно для висячей запятой, для `unsafe` у
`extern` и для `...` у вариадика. `@push_expected` означает «ничего другого
здесь стоять не может, и эта форма без него — не она».

ЦЕНА ОШИБКИ ЗАМЕРЕНА ДВАЖДЫ, а не воображена:
  * #809 — все шестнадцать ЗАКРЫВАЮЩИХ скобок шли необязательной дверью, и
    `fn main() {` без `}` компилировался с кодом 0 и БЕЗ единой диагностики;
  * #815 — та же дверь на других видах токена: тринадцать проб охоты
    (parse × К2), где novac принимал молча, а оракул отказывал. Имя типа, поля,
    варианта, метода, привязки; `=` у константы и привязки; `{` у `match`;
    `=>` у плеча; `(` у сигнатуры; `in` у цикла.

ЭТОТ СТРАЖ СМЕНИЛ ИМЯ (2026-08-30). Он звался `check-novac-closer-mandatory` и
судил только скобки; когда решение «говорить ли о пропаже» уехало в чекер, две
двери схлопнулись в одну, и старое имя стало ложью — оно называло частный
случай общего правила. Старый счётчик при этом печатал ноль и выглядел
измерением: тот самый класс doc-truth, ради которого стражи и заводятся.

ЧТО СУДИТСЯ: `@push_if(<что-то>, TokenKind.<вид>)` в `.nv` под `novac/src`, где
вид входит в список ОБЯЗАТЕЛЬНЫХ ниже. Ноль — норма и база.

ЧТО НЕ СУДИТСЯ: `Comma`, `KwUnsafe`, `Ellipsis` — они правда необязательны, и
это установлено чтением каждого места, а не умолчанием: `extern "C" unsafe fn`
законна и без `unsafe`, а вариадиком является не всякий параметр.

СУДЯТСЯ ТОЛЬКО ЖИВЫЕ СТРОКИ: комментарий, ЦИТИРУЮЩИЙ снятую форму, законен —
без этого страж стирал бы историю класса вместе с самим классом.

ПОЧЕМУ PYTHON: старт процесса дороже самой проверки (П14).

$1 — корень; $2 — override директории (шов самотеста).
"""
import os
import pathlib
import re
import sys

sys.stdout.reconfigure(encoding="utf-8", errors="replace", newline="\n")
sys.stderr.reconfigure(encoding="utf-8", errors="replace", newline="\n")

NAME = "check-novac-required-token-door"

# Виды токенов, которые грамматика требует в каждом месте, где novac их берёт.
# Список выведен ЧТЕНИЕМ всех двадцати не-закрывающих вызовов 2026-08-30, а не
# догадкой: восемнадцать оказались обязательными, два — нет.
REQUIRED = ("RParen", "RBracket", "RBrace", "Ident", "Assign",
            "FatArrow", "LBrace", "LParen", "StrLit", "KwIn")

RE_OPTIONAL_DOOR = re.compile(
    r"@push_if\([^()]*,\s*TokenKind\.(" + "|".join(REQUIRED) + r")\s*\)")
RE_MANDATORY = re.compile(r"@push_expected\(")


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
        print(f"{NAME}: FAIL — в {src} нет ни одного .nv: страж потерял мишень", file=sys.stderr)
        return 1

    bad = []
    mandatory = 0
    for f in files:
        rel = str(f.relative_to(src)).replace("\\", "/")
        lines = f.read_bytes().decode("utf-8", "replace").split("\n")
        if lines and lines[-1] == "":
            lines.pop()
        for n, line in enumerate(lines, 1):
            if line.endswith("\r"):
                line = line[:-1]
            s = line.lstrip(" \t\v\f")
            # комментарий или док — не судим: история класса там законна
            if s.startswith("//"):
                continue
            m = RE_OPTIONAL_DOOR.search(line)
            if m:
                bad.append(f"  {rel}:{n} — `{m.group(1)}` взят необязательной дверью: {s[:70]}")
            # считаются ВЫЗОВЫ, а не объявление самой двери: иначе число в
            # вердикте на единицу больше правды
            if not s.startswith("fn "):
                mandatory += len(RE_MANDATORY.findall(line))

    if mandatory == 0:
        print(f"{NAME}: FAIL — обязательная дверь `@push_expected` не зовётся НИ РАЗУ: "
              f"либо её переименовали и страж ослеп, либо парсер потерял правило", file=sys.stderr)
        print("  Ноль вызовов — не «чисто», а потерянная мишень: страж, считающий", file=sys.stderr)
        print("  несуществующее имя, печатает ноль и выглядит замером.", file=sys.stderr)
        return 1

    if bad:
        print(f"{NAME}: FAIL — токен, требуемый грамматикой, взят дверью для НЕОБЯЗАТЕЛЬНОГО (#809/#815):",
              file=sys.stderr)
        for b in bad:
            print(b, file=sys.stderr)
        print("  `@push_if` означает «токена может не быть, и это законно» — верно", file=sys.stderr)
        print("  для запятой, для `unsafe` у extern и для `...` у вариадика.", file=sys.stderr)
        print("  Обязательные идут через `@push_expected`: он сажает пробел, а", file=sys.stderr)
        print("  чекер решает один раз на файл, стоит ли о нём говорить.", file=sys.stderr)
        return 1

    print(f"{NAME} ok: файлов .nv: {len(files)}, требуемых токенов через обязательную дверь: "
          f"{mandatory}, через необязательную: 0")
    return 0


if __name__ == "__main__":
    sys.exit(main())
