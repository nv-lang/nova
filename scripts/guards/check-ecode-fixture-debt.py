# -*- coding: utf-8 -*-
"""scripts/guards/check-ecode-fixture-debt.py — новый код `E_*` без фикстуры и
без названного перехватчика.

ДОМ ПРАВИЛА: реестр 221.1 №1187 (решение интегратора 2026-09-20).

ЗАЧЕМ. Диагностический код, объявленный в компиляторе и не закреплённый
негативной фикстурой, — приёмка, держащаяся ни на чём: текст сообщения можно
сломать, и никто не покраснеет. Замер 2026-09-20 (репро
docs/plans/repro/1186-check-vs-test/): объявлено 417 имён, без ЖИВОЙ фикстуры
217. Гасить это разом нельзя, поэтому судится РОСТ, а старое держится базой.

ТРИ СОСТОЯНИЯ, А НЕ ДВА (решение интегратора, дословно). Код бывает не только
«покрыт» и «долг». Если форма, которая должна поднять код, перехватывается
РАНЬШЕ другой проверкой, и у той проверки фикстура ЕСТЬ, — код ЗАТЕНЁН и в долг
не входит. Но только когда перехватчик НАЗВАН: без имени неотличимо «ловится
раньше законно» от «код мёртв, и никто не заметил». Замер, купивший это
правило: из десяти проб три вернули СОСЕДНИЙ код, и у каждого перехватчика
фикстуры есть.

    // nova:shadowed-by E_OTHER_CODE — форма перехватывается раньше
    let code = "E_SHADOWED_ONE";

Пометка читается НА ТОЙ ЖЕ строке или на строке ВПЛОТНУЮ НАД вхождением — как
у `nova:allow`, и по той же причине: пометка, живущая «где-то рядом», начинает
покрывать соседей.

ДВА УТОЧНЕНИЯ ПРЕДИКАТА, оба куплены замером того же дня:

  * ЗНАМЕНАТЕЛЬ — ИСПОЛНИМЫЙ КОД, не всякое вхождение имени. Из 417 имён 39
    живут ТОЛЬКО в комментариях, и часть — обрывки (`E_BANG_`, `E_COERCE_`,
    `E_CODE`). Считать их значило бы первым делом потребовать фикстуру на
    обрывок. Комментарии `//` и `/* */` снимаются до поиска.
  * ПОКРЫТИЕ — ЖИВАЯ СТРОКА ОЖИДАНИЯ (`// EXPECT_COMPILE_ERROR …` в начале
    строки или `nova:expect`), а не упоминание кода: упоминание в историческом
    пояснении фикстуры выглядит как ожидание, и однажды уже дало ложную находку.

БАЗА ИМЕННАЯ. Набор кодов перечислим, а счёт прячет ДВЕ вещи: замену (один код
закрыли, другой завели — число то же) и сужение предиката (перестали
перечислять часть исходников — число упало и прочлось как погашенный долг).
Исчезновение имени разбирается НЕ предикатом стража, иначе он судил бы сам
себя, а независимым поиском идентификатора по дереву:
  * имя не в наборе, а идентификатор в исходниках есть → КРАСНЫЙ, предикат сузился;
  * идентификатора нет вовсе (код удалён) → совет убрать строку из базы;
  * код обзавёлся фикстурой или пометкой → совет опустить базу.

usage: python scripts/guards/check-ecode-fixture-debt.py [КОРЕНЬ] [БАЗА]
env: NOVA_ECODE_DEBT_BASELINE — путь к базе (шов самотеста).
"""
import io
import os
import re
import sys

sys.stdout.reconfigure(encoding="utf-8", errors="replace", newline="\n")
sys.stderr.reconfigure(encoding="utf-8", errors="replace", newline="\n")

NAME = "check-ecode-fixture-debt"
CODE = re.compile(r"\bE_[A-Z][A-Z0-9_]{2,}\b")
SHADOW = re.compile(r"nova:shadowed-by\s+(E_[A-Z][A-Z0-9_]{2,})")
SRC_DIRS = ("compiler-codegen/src",)
FIXTURE_DIRS = ("spec_tests",)
SKIP_DIRS = (".git", "target", "node_modules")


def strip_comments(text):
    """Убрать комментарии Rust, СОХРАНИВ разбиение на строки.

    Строки не склеиваются и не выбрасываются: номер строки нужен, чтобы
    прочитать пометку затенения над вхождением.

    ПРЕДИКАТ НАМЕРЕННО ПРОСТОЙ: комментарием считается СТРОКА, начинающаяся с
    `//`, `///`, `//!`, `/*` или `*` (продолжение блочного комментария). Всё
    прочее — код.

    Почему не разбор литералов. Первая редакция искала `//` и `/*` в тексте как
    есть, и одна `/*` внутри строки-сообщения объявляла комментарием ВСЁ до
    конца файла; число («только в комментариях 143» против 39 у соседнего
    замера) выглядело правдоподобно, и разошедшиеся замеры оказались находкой —
    выборка показала `E_AMBIGUOUS_CALLER_LOC` с живой строкой печати в списке
    «комментарии». Вторая редакция была полным разбором литералов Rust: вдвое
    длиннее самого правила и с собственными краями, которые никто не проверит.
    Взята третья, построчная.

    НАЗВАННАЯ СЛЕПАЯ ЗОНА, и она в безопасную сторону: код, стоящий в хвостовом
    комментарии строки кода (`foo(); // см. E_X`), считается исполнимым. Ошибка
    такого рода добавляет долг, а не прячет его, — и это единственное
    направление, в котором страж имеет право ошибаться.
    """
    out = []
    for line in text.split("\n"):
        s = line.lstrip()
        if s.startswith("//") or s.startswith("/*") or s.startswith("*"):
            out.append("")
        else:
            out.append(line)
    return out


def walk(root, subs, suffix):
    for sub in subs:
        base = os.path.join(root, sub)
        if not os.path.isdir(base):
            continue
        for dirpath, dirs, names in os.walk(base):
            dirs[:] = [d for d in dirs if d not in SKIP_DIRS]
            for n in names:
                if n.endswith(suffix):
                    yield os.path.join(dirpath, n)


def scan_sources(root):
    """(исполнимые коды, затенённые с именем перехватчика, все вхождения имени)."""
    executable, shadowed, anywhere = {}, {}, set()
    for p in walk(root, SRC_DIRS, ".rs"):
        raw = io.open(p, encoding="utf-8", errors="replace").read()
        rel = os.path.relpath(p, root).replace(os.sep, "/")
        raw_lines = raw.split("\n")
        for m in CODE.finditer(raw):
            anywhere.add(m.group(0))
        code_lines = strip_comments(raw)
        for n, line in enumerate(code_lines):
            for m in CODE.finditer(line):
                c = m.group(0)
                executable.setdefault(c, f"{rel}:{n + 1}")
                near = [raw_lines[n]]
                if n > 0:
                    near.append(raw_lines[n - 1])
                for cand in near:
                    sm = SHADOW.search(cand)
                    if sm:
                        shadowed.setdefault(c, sm.group(1))
                        break
    return executable, shadowed, anywhere


def scan_fixtures(root):
    fixtured = set()
    for p in walk(root, FIXTURE_DIRS, ".nv"):
        for line in io.open(p, encoding="utf-8", errors="replace"):
            s = line.strip()
            if s.startswith("// EXPECT_COMPILE_ERROR") or "nova:expect" in s:
                for m in CODE.finditer(s):
                    fixtured.add(m.group(0))
    return fixtured


def read_baseline(path):
    if not os.path.isfile(path):
        return None, set()
    count, names = None, set()
    for line in io.open(path, encoding="utf-8", errors="replace"):
        s = line.split("#", 1)[0].strip()
        if not s:
            continue
        if s.startswith("debt=") and count is None:
            try:
                count = int(s.split("=", 1)[1])
            except ValueError:
                count = None
        elif s.startswith("debt_code="):
            names.add(s.split("=", 1)[1].strip())
    return count, names


def main():
    root = os.path.abspath(sys.argv[1] if len(sys.argv) > 1 else ".")
    base = (sys.argv[2] if len(sys.argv) > 2
            else os.environ.get("NOVA_ECODE_DEBT_BASELINE")
            or os.path.join(root, "scripts", "guards", "ecode-fixture-debt.baseline"))

    executable, shadowed, anywhere = scan_sources(root)
    if not executable:
        print(f"{NAME}: FAIL — в {', '.join(SRC_DIRS)} не найдено ни одного кода E_* в "
              f"ИСПОЛНИМОМ коде: пустая мишень читается как чистота, но ею не является",
              file=sys.stderr)
        return 1

    fixtured = scan_fixtures(root)
    comment_only = sorted(anywhere - set(executable))
    debt = sorted(c for c in executable if c not in fixtured and c not in shadowed)

    print(f"{NAME}: кодов в исполнимом коде {len(executable)}, только в комментариях "
          f"{len(comment_only)}, с живой фикстурой {len(set(executable) & fixtured)}, "
          f"затенённых с именем перехватчика {len(shadowed)}, ДОЛГ {len(debt)}")

    base_count, base_names = read_baseline(base)
    if base_count is None:
        print(f"{NAME}: FAIL — нет базы {base} или в ней нет строки `debt=<N>`",
              file=sys.stderr)
        return 1
    if base_count != len(base_names):
        print(f"{NAME}: FAIL — база противоречит себе: debt={base_count}, "
              f"а имён {len(base_names)}", file=sys.stderr)
        return 1

    problems = 0
    new = sorted(set(debt) - base_names)
    if new:
        print(f"{NAME}: НОВЫЕ коды без фикстуры и без названного перехватчика ({len(new)}):",
              file=sys.stderr)
        for c in new[:12]:
            print(f"    {c} — {executable[c]}", file=sys.stderr)
        print("    Заведи негативную фикстуру со строкой ожидания либо, если форма",
              file=sys.stderr)
        print("    перехватывается раньше, пометь вхождение: `nova:shadowed-by E_ДРУГОЙ`",
              file=sys.stderr)
        print("    (перехватчик обязан иметь фикстуру сам).", file=sys.stderr)
        problems += 1

    vanished = sorted(n for n in base_names
                      if n not in executable and n in anywhere)
    if vanished:
        print(f"{NAME}: ИМЯ ПРОПАЛО ИЗ НАБОРА, А ИДЕНТИФИКАТОР В ДЕРЕВЕ ЕСТЬ — "
              f"предикат сузился:", file=sys.stderr)
        for c in vanished[:12]:
            print(f"    {c}", file=sys.stderr)
        print("    Долг, переставший считаться, читается как погашенный. Это не он.",
              file=sys.stderr)
        problems += 1

    paid = sorted(base_names - set(debt) - set(vanished))
    if paid:
        print(f"{NAME}: долг СНИЗИЛСЯ ({len(paid)}): {', '.join(paid[:8])}"
              f" — убери эти строки из {base} ТОЙ ЖЕ правкой и поправь debt=")

    if problems:
        print(f"{NAME}: FAIL", file=sys.stderr)
        return 1

    print(f"{NAME} ok: новых кодов без фикстуры нет (долг {len(debt)} при базе {base_count})")
    return 0


if __name__ == "__main__":
    sys.exit(main())
