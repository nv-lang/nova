# -*- coding: utf-8 -*-
"""scripts/guards/check-novac-no-silent-skip.py — ветка прохода канала решает
что-то, а не уходит молча.

ПОЧЕМУ. Читатели канала ТОТАЛЬНЫ: пропущенный узел всплывёт как «no type
recorded» из другого модуля и проходом позже — там, где причины уже не видно.
Решение принимается здесь: запись в канал, `@refuse(...)` если это ошибка
пользователя, `ice(...)` если инвариант, либо явная пометка
`// SILENT-OK: <почему молчание верно>`.

ОКНО РЕШЕНИЯ — три предыдущие ЗНАЧАЩИЕ строки плюс своя: решение часто стоит
строкой выше (`@refuse(...)` и следом `return`), а пометка SILENT-OK бывает
многострочной прозой.

Комментарий веткой не считается: слово «return» в прозе — не решение.

ПОЧЕМУ PYTHON: старт процесса дороже самой проверки (П14).

$1 — корень; $2 — override директории (по умолчанию novac/src/check).
"""
import os
import pathlib
import re
import sys

sys.stdout.reconfigure(encoding="utf-8", errors="replace", newline="\n")
sys.stderr.reconfigure(encoding="utf-8", errors="replace", newline="\n")

NAME = "check-novac-no-silent-skip"
RE_WALK = re.compile(r"^fn Checker mut @type_[a-z_]*\(")
RE_FN = re.compile(r"^fn |^export fn ")
RE_RETURN = re.compile(r"(^|[^a-zA-Z_])return([^a-zA-Z_]|$)")
# `@out.len() > <метка>` — тоже РЕШЕНИЕ: выход стоит сразу за отказом, поданным
# одним вызовом глубже. Сравнение с МЕТКОЙ, а не с нулём, появилось 2026-08-20:
# файловый счётчик отвечал «отказали ли ЭТОТ оператор» результатом обо всём
# файле, и после первого отказа `@type_bind` пропускал `@scope.bind` -- каждое
# дальнейшее имя выходило «unknown name». Обе формы здесь законны, но новая
# обязательна для нового кода, и от возврата старой держит фикстура каскада.
RE_DECISION = re.compile(r"@refuse\(|@report_|ice\(|@out\.len\(\) > (?:0|[a-z_][a-z_0-9]*)")


def main():
    a = sys.argv
    root = pathlib.Path(a[1] if len(a) > 1 else ".").resolve()
    src = pathlib.Path(a[2]) if len(a) > 2 else root / "novac" / "src" / "check"

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
    nfn = nret = 0
    inwalk = False
    hist = []
    for f in files:
        rel = str(f.relative_to(src)).replace("\\", "/")
        for n, raw in enumerate(f.read_bytes().decode("utf-8", "replace").split("\n"), 1):
            if raw.endswith("\r"):
                raw = raw[:-1]
            if RE_WALK.match(raw):
                inwalk = True
                nfn += 1
                hist = []
                continue
            if RE_FN.match(raw):
                inwalk = False
            if not inwalk:
                continue
            body = raw.lstrip(" \t\v\f")
            if not body:
                continue
            if body.startswith("//"):
                hist.append(body)
                continue
            if RE_RETURN.search(body):
                nret += 1
                window = hist[-3:]
                okay = "SILENT-OK:" in raw or any("SILENT-OK:" in h for h in window)
                cand = " ".join([raw] + list(reversed(window)))
                if RE_DECISION.search(cand):
                    okay = True
                if not okay:
                    bad.append(f"  {rel}:{n} — молчаливый выход в проходе канала: {body}")
            hist.append(body)

    if nfn == 0:
        print(f"{NAME}: FAIL — не найдено ни одной функции прохода канала "
              f"(fn Checker mut @type_*): страж потерял мишень (класс №519)", file=sys.stderr)
        return 1

    if bad:
        print(f"{NAME}: FAIL — ветка прохода канала не решила ничего "
              f"(ни записи, ни отказа, ни ice):", file=sys.stderr)
        for b in bad:
            print(b, file=sys.stderr)
        print("  Читатели канала ТОТАЛЬНЫ: пропущенный узел всплывёт как «no type recorded»", file=sys.stderr)
        print("  из другого модуля и проходом позже. Реши здесь: @refuse(...) если это", file=sys.stderr)
        print("  ошибка пользователя, ice(...) если инвариант, либо пометь строкой", file=sys.stderr)
        print("  '// SILENT-OK: <почему молчание верно>' (минимум четыре слова).", file=sys.stderr)
        return 1

    print(f"{NAME} ok: функций прохода канала {nfn}, выходов {nret} — "
          f"у каждого решение (запись, отказ, ice или названная причина)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
