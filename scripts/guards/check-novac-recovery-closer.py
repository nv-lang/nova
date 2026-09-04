# -*- coding: utf-8 -*-
"""scripts/guards/check-novac-recovery-closer.py — место восстановления парсера
не съедает ЗАКРЫВАЮЩИЙ токен объемлющей формы (274.3/F15).

ПОЧЕМУ. Восстановление берёт ОДИН токен и ставит один `Err`. Но закрывающий
токен принадлежит объемлющей форме: съев его, внутренняя форма оставляет
внешнюю без её `}` — и «одна опечатка утаскивает остаток файла», ровно как
сказано над таблицей `TERMINATORS` в `parse.nv`.

ЗАЧЕМ СТРАЖ, КОГДА ЕСТЬ ТЕСТ. Держатель
`refusal_test.nv` / «recovery does not swallow a closing token» проверяет
ПОВЕДЕНИЕ двух нынешних форм (закрыватель в позиции аргумента и инициализатора)
и краснеет, если правило снять. Он НЕ мешает завтрашнему месту восстановления
быть написанным без проверки: новый цикл со своим `@take()` пройдёт мимо него
молча. Вопрос владельца 2026-09-04 был именно про это — «кто это
контролирует», — и честный ответ в тот момент был «никто»: один вызов
предиката, три границы циклов и комментарий.

ЧТО СУДИТСЯ. Каждая живая строка `.nv` под `novac/src/parse`, где узел ошибки
строится вокруг ВЗЯТОГО токена — `NodeKind.Err, []Node.of(@take())` и его
написания. Такое место законно ровно в двух случаях, и оба обязаны быть видны
РЯДОМ (в окне WINDOW строк выше):
  * стоит проверка `is_terminator(` — место само отказывается есть закрыватель;
  * либо стоит метка `// RECOVERY-BOUNDED: <причина, не короче пяти слов>` —
    место ограничено циклом, чьё условие не пускает закрыватель дальше, и
    причина названа словами.

ПОЧЕМУ МЕТКА, А НЕ РАЗБОР УСЛОВИЯ ЦИКЛА. Условие живёт выше по тексту и бывает
составным (`!@at_eof() && @peek() != TokenKind.RBrace`); разбирать его
регуляркой значит завести вторую, неточную реализацию правила языка внутри
стража — то самое, что этот проект и убирает. Метка стоит дёшево и делает
решение автора ЧИТАЕМЫМ: он обязан назвать, какой закрыватель его цикл
стережёт.

ЧТО НЕ СУДИТСЯ: комментарии (строка, ЦИТИРУЮЩАЯ форму, законна — без этого
страж стирал бы историю класса), и файлы вне `novac/src/parse`: восстановление
живёт только там, и расширение области сделало бы страж дороже без находок.

БАЗА НЕ ЗАВОДИТСЯ: правило абсолютное, все нынешние места ему отвечают
(замерено 2026-09-04 — четыре места, одно с предикатом, три с меткой).

ПОЧЕМУ PYTHON: старт процесса дороже самой проверки (П14).

$1 — корень; $2 — override директории (шов самотеста).
"""
import os
import pathlib
import re
import sys

sys.stdout.reconfigure(encoding="utf-8", errors="replace", newline="\n")
sys.stderr.reconfigure(encoding="utf-8", errors="replace", newline="\n")

NAME = "check-novac-recovery-closer"

# Узел ошибки, построенный вокруг ВЗЯТОГО токена: именно он может съесть чужой
# закрыватель. `@error_node(...)` сюда не входит — он ставит пробел нулевой
# ширины и ничего не берёт.
RE_EATING = re.compile(r"NodeKind\.Err\s*,\s*\[\]Node\.of\(\s*@take\(\)\s*\)")
RE_PREDICATE = re.compile(r"is_terminator\s*\(")
RE_MARK = re.compile(r"//\s*RECOVERY-BOUNDED:\s*(\S+(?:\s+\S+){4,})")

WINDOW = 12


def main():
    a = sys.argv
    root = pathlib.Path(a[1] if len(a) > 1 else ".").resolve()
    src = pathlib.Path(a[2]) if len(a) > 2 else root / "novac" / "src" / "parse"

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
    sites = 0
    by_predicate = 0
    by_mark = 0
    for f in files:
        rel = str(f.relative_to(src)).replace("\\", "/")
        lines = f.read_bytes().decode("utf-8", "replace").split("\n")
        if lines and lines[-1] == "":
            lines.pop()
        for n, raw in enumerate(lines, 1):
            line = raw[:-1] if raw.endswith("\r") else raw
            s = line.lstrip(" \t\v\f")
            if s.startswith("//"):
                continue
            if not RE_EATING.search(line):
                continue
            sites += 1
            lo = max(0, n - 1 - WINDOW)
            window = "\n".join(lines[lo:n])
            if RE_PREDICATE.search(window):
                by_predicate += 1
            elif RE_MARK.search(window):
                by_mark += 1
            else:
                bad.append(f"  {rel}:{n} — берёт токен без защиты от закрывателя: {s[:70]}")

    if sites == 0:
        print(f"{NAME}: FAIL — мест восстановления не найдено НИ ОДНОГО: "
              f"либо форма переименована и страж ослеп, либо парсер потерял восстановление",
              file=sys.stderr)
        print("  Ноль — не «чисто», а потерянная мишень: страж, считающий", file=sys.stderr)
        print("  несуществующую форму, печатает ноль и выглядит замером.", file=sys.stderr)
        return 1

    if bad:
        print(f"{NAME}: FAIL — восстановление может съесть ЗАКРЫВАЮЩИЙ токен объемлющей формы (274.3/F15):",
              file=sys.stderr)
        for b in bad:
            print(b, file=sys.stderr)
        print("  Съеденный закрыватель оставляет внешнюю форму без её скобки, и", file=sys.stderr)
        print("  одна опечатка утаскивает остаток файла. Законны два ответа:", file=sys.stderr)
        print("    * проверка `is_terminator(@peek())` перед взятием токена;", file=sys.stderr)
        print("    * метка `// RECOVERY-BOUNDED: <причина>` (не короче пяти слов),", file=sys.stderr)
        print("      если место ограничено циклом — назови, какой закрыватель он стережёт.", file=sys.stderr)
        return 1

    print(f"{NAME} ok: мест восстановления {sites} — с предикатом {by_predicate}, "
          f"ограниченных циклом с меткой {by_mark}, без защиты 0")
    return 0


if __name__ == "__main__":
    sys.exit(main())
