# -*- coding: utf-8 -*-
"""scripts/guards/check-novac-one-door-export.py — одна операция экспортируется
из ОДНОГО модуля (план 274.1 §2в п.2).

ПОЧЕМУ. Одинаковое имя, экспортированное из двух модулей, зовущий читает как
одну операцию — и получает две реализации, которые расходятся молча. Каждый
класс информации имеет одну дверь-производителя; потребители ЧИТАЮТ результат,
а не перевычисляют.

КЛЮЧ КВАЛИФИЦИРОВАН ПРОСТРАНСТВОМ: у методов разных типов пространства разные,
поэтому `A @len` и `B @len` НЕ сталкиваются, а `A @len` из двух модулей —
сталкиваются, и это ровно две двери одной операции.

ТРИ ЛОВУШКИ, каждая стоила ложного вердикта:
  * `export unsafe fn` — это модификатор двери, а не другая дверь: форма
    приводится к общей, иначе одна и та же операция из двух модулей проходит
    МОЛЧА;
  * приёмник-срез `[]u8` — СВОЙ тип, а не `u8`: маркер снимается ДО вычистки
    дженериков, потому что она видит в `[]` пустую группу и съедает её;
  * повтор ключа ВНУТРИ модуля второй дверью не является: файлы одной папки
    co-equal, это один модуль.

Тесты (`*_test.nv`) дверьми не являются.

ПОЧЕМУ PYTHON: старт процесса дороже самой проверки (П14).

$1 — корень; $2 — override директории (шов самотеста).
"""
import os
import pathlib
import re
import sys

sys.stdout.reconfigure(encoding="utf-8", errors="replace", newline="\n")
sys.stderr.reconfigure(encoding="utf-8", errors="replace", newline="\n")

NAME = "check-novac-one-door-export"
RE_IDENT = re.compile(r"[A-Za-z_][A-Za-z0-9_]*")
RE_GENERIC = re.compile(r"\[[^][]*\]")
RE_SLICE = re.compile(r"^\[\][ \t]*")


def strip_generics(s):
    prev = None
    while s != prev:
        prev = s
        s = RE_GENERIC.sub("", s, count=1)
    return s


def first_ident(s):
    m = RE_IDENT.search(s)
    return m.group(0) if m else ""


def main():
    a = sys.argv
    root = pathlib.Path(a[1] if len(a) > 1 else ".").resolve()
    src = pathlib.Path(a[2].rstrip("/")) if len(a) > 2 else root / "novac" / "src"

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
        print(f"{NAME} ok: судить нечего (нет .nv в {src})")
        return 0

    raw = []                      # (вид, ключ, модуль, файл:строка)
    for f in files:
        rel = str(f.relative_to(src)).replace("\\", "/")
        mod = "main" if "/" not in rel else rel.rsplit("/", 1)[0]
        for n, line in enumerate(f.read_bytes().decode("utf-8", "replace").split("\n"), 1):
            if line.endswith("\r"):
                line = line[:-1]
            line = re.sub(r"^export unsafe fn ", "export fn ", line)
            where = f"{rel}:{n}"

            if line.startswith("export type "):
                nm = first_ident(strip_generics(line[12:]))
                if nm:
                    raw.append(("тип", nm, mod, where))
                continue

            if not line.startswith("export fn "):
                continue
            h = line[10:]
            p = h.find("(")
            if p < 0:
                continue
            head = h[:p].lstrip(" \t")
            sl = ""
            while RE_SLICE.match(head):
                head = RE_SLICE.sub("", head, count=1)
                sl += "[]"
            head = strip_generics(head).strip(" \t")
            if not head:
                continue
            if re.search(r"[ \t]", head):
                tk = re.split(r"[ \t]+", head)
                ty = first_ident(tk[0])
                mn = first_ident(re.sub(r"^@", "", tk[-1]))
                if ty and mn:
                    raw.append(("метод", f"{sl}{ty}@{mn}", mod, where))
            elif "." in head:
                dt = head.split(".")
                ty, mn = first_ident(dt[0]), first_ident(dt[1])
                if ty and mn:
                    raw.append(("статический метод", f"{sl}{ty}.{mn}", mod, where))
            else:
                nm = first_ident(head)
                if nm:
                    raw.append(("свободная функция", f"{sl}{nm}", mod, where))

    if not raw:
        print(f"{NAME}: FAIL — в {src} есть .nv, но ни одного экспорта не разобрано: "
              f"страж потерял мишень (класс №519)", file=sys.stderr)
        print("  Либо форма экспорта в языке изменилась, либо путь не тот. Молчать нельзя: "
              "вечнозелёный страж хуже отсутствующего.", file=sys.stderr)
        return 1

    # Повтор ключа ВНУТРИ модуля второй дверью не является: на модуль остаётся
    # одна запись — первая по сортировке, как её оставлял `sort | awk !seen`.
    doors = []
    seen = set()
    for rec in sorted("\t".join(r) for r in raw):
        kind, key, mod, where = rec.split("\t")
        if (kind, key, mod) in seen:
            continue
        seen.add((kind, key, mod))
        doors.append((kind, key, mod, where))

    by_key = {}
    for kind, key, mod, where in doors:
        by_key.setdefault((kind, key), []).append((mod, where))
    dups = [k for k in sorted(by_key) if len(by_key[k]) > 1]

    if dups:
        out = sys.stderr
        print(f"{NAME}: FAIL — две двери в один класс задачи (план 274.1 §2в п.2): "
              f"одно имя экспортировано из разных модулей", file=out)
        for kind, key in dups:
            print(f"  {kind} {key} — экспортировано из разных модулей:", file=out)
            for mod, where in by_key[(kind, key)]:
                print(f"      модуль {mod} -> {where}", file=out)
        print("  Как чинить: оставь ОДНУ дверь — модуль-производитель, — а второй модуль пусть", file=out)
        print("  ИМПОРТИРУЕТ её и ЧИТАЕТ результат, а не перевычисляет (274.1 §2в: каждый класс", file=out)
        print("  информации имеет одну дверь-производителя, потребители ЧИТАЮТ, а не перевычисляют).", file=out)
        print("  Если операции РАЗНЫЕ — разведи имена так, чтобы разница читалась в имени, а не в", file=out)
        print("  комментарии: одинаковое имя в двух модулях зовущий читает как одну операцию.", file=out)
        print("  Ребро, по которому пойдёт импорт, обязано быть строкой таблицы §3 архитектуры", file=out)
        print("  (check-novac-deps.py): двери сводятся вместе с картой, а не вместо неё.", file=out)
        return 1

    nkeys = len(by_key)
    nmods = len({d[2] for d in doors})
    print(f"{NAME} ok: экспортированных дверей: {nkeys} в {nmods} модулях, вторых дверей: 0")
    return 0


if __name__ == "__main__":
    sys.exit(main())
