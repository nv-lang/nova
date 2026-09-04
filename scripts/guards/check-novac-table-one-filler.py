#!/usr/bin/env python3
# -*- coding: utf-8 -*-
# scripts/guards/check-novac-table-one-filler.py — у таблицы реестра novac ОДИН
# наполнитель, и он назван. План: docs/plans/274.5-read-own-source.md §3-пред62
# (класс «наполнитель и спрашивающий связаны только намерением автора», найден
# трижды за 2026-09-04); реестр 221.1 №TBD. Самотест:
# selftest/test-check-novac-table-one-filler.sh.
"""scripts/guards/check-novac-table-one-filler.py — у КАЖДОЙ таблицы реестра
novac ровно ОДИН модуль-наполнитель.

План: docs/plans/274.5-read-own-source.md §3-пред62; реестр 221.1 №TBD.
Самотест: selftest/test-check-novac-table-one-filler.sh (пять случаев).

ЗАЧЕМ. У таблицы реестра есть НАПОЛНИТЕЛЬ (кто в неё кладёт) и СПРАШИВАЮЩИЙ
(кто у неё спрашивает), и связаны они НИЧЕМ, кроме намерения автора. Разошлись —
и таблица не молчит, а отвечает НЕВЕРНЫМ ОТКАЗОМ: спрашивающий получает
честное «нет такого» о том, что в компиляторе есть. Три носителя одного дня
(2026-09-04):
  1. `harvest` клал в `defs` имя суммы, а варианты — никуда; судья спрашивал
     `defs` и говорил «не вариант» — 249 диагностик;
  2. `harvest_fn` читает только методы, свободная функция не попадала никуда;
     дверь вызова спрашивала `defs` и говорила «не знаю такого вызываемого» —
     361 диагностика с эхом;
  3. у оракула чекер писал в канал, а кодоген спрашивал строку манглированного
     имени.
Ни один из трёх не был отказом механизма — все три были ПРАВИЛЬНОЙ работой
механизма над НЕПОЛНОЙ таблицей. Тест на такое не пишется заранее: он проверяет
поведение, а тут поведение верное; неверен вход.

ПОЧЕМУ СЧИТАЮТСЯ МОДУЛИ, А НЕ ФУНКЦИИ. Два места записи внутри ОДНОГО модуля
(`sem/collect.nv` и `sem/harvest.nv`) читаются рядом и правятся вместе — это
один автор одной таблицы. Два РАЗНЫХ модуля — это два автора, которые про
таблицу договорились когда-то и с тех пор не разговаривают; ровно так и
рождается «наполнил один, спросил другой». Правило поэтому: имя таблицы →
ОДИН каталог `novac/src/<модуль>`, где в неё кладут.

ЧТО СЧИТАЕТ.
  * ИМЕНА ТАБЛИЦ — не список в страже, а ЧТЕНИЕ объявления `export type Ctx`
    в `novac/src/sem/sem.nv`. Список в страже был бы второй копией схемы и
    разошёлся бы с первым же новым полем — тем самым классом, который страж и
    ловит. Поля, которые таблицами не являются (`module_c`, `prims`), просто
    не имеют мест записи и в отчёт не попадают.
  * МЕСТА ЗАПИСИ — вызовы `.add(`, `.put(`, `.push(` на поле-таблице, в любом
    написании получателя: `defs.add(`, `@defs.add(`, `ctx.defs.add(`,
    `out.defs.add(`. Имя таблицы обязано начинаться на границе слова, поэтому
    `payload_types.push(` — запись в `payload_types`, а не в `types`.
  * ГДЕ: `novac/src/**/*.nv`, кроме `*_test.nv` (тест волен набивать свою
    подложку). Комментарий (`//`) не код: строка, ЦИТИРУЮЩАЯ форму, не
    считается.

ЧТО КРАСНИТ.
  1. У таблицы БОЛЬШЕ ОДНОГО модуля-наполнителя — названы таблица, оба модуля
     и все места файл:строка. Это и есть класс.
  2. Общее число мест записи ВЫШЕ базы — храповик вниз: новая запись в реестр
     заводится осознанно, а не «ещё один push по дороге».
  3. МИШЕНЬ ПОТЕРЯНА: не нашлось объявления `Ctx`, ни одного поля или ни одного
     места записи. Ноль — не «чисто», а ослепший страж: главный урок охоты
     guards × К7 того же дня, когда девять стражей из десяти печатали зелёный
     ноль на уехавшем якоре.
  4. Таблица, НАЗВАННАЯ В БАЗЕ, исчезла из объявления `Ctx` — поле
     переименовали, и строка базы с этого мгновения считает пустоту. База
     обязана поехать вместе с полем.

БАЗА: `scripts/guards/novac-table-fillers.baseline` — по строке на таблицу
`<имя>=<число мест>:<модули через запятую>` плюс ключ `total=N`. Это ФОТОГРАФИЯ
дня заведения, а не идеал: строки нужны, чтобы движение было видно поимённо, а
не одним числом. Храповик судит только `total` (вниз) и пункт 4.

Аргументы: $1 — корень репозитория (по умолчанию — репозиторий стража);
$2 — override каталога `novac/src` (шов самотеста); env
NOVAC_TABLE_FILLERS_BASELINE — override базы (шов самотеста).

Вход для гейта — `main()`: run-guards.py исполняет стражей в одном процессе и
зовёт именно её; страж с телом на уровне модуля зелен вручную и красен в гейте.

ПОЧЕМУ PYTHON: старт процесса дороже самой проверки (П14).
"""
import io
import os
import re
import sys

if hasattr(sys.stdout, "reconfigure"):
    sys.stdout.reconfigure(encoding="utf-8", newline="\n")
    sys.stderr.reconfigure(encoding="utf-8", newline="\n")

NAME = "check-novac-table-one-filler"

# `export type Ctx {` ... `}` — блок полей, откуда берутся имена таблиц.
RE_CTX_OPEN = re.compile(r"^\s*export\s+type\s+Ctx\b.*\{\s*$")
# Строка поля: `    defs DefTable /// ...`, `    coerces []CoercePair /// ...`.
RE_FIELD = re.compile(r"^\s+([a-z_][A-Za-z0-9_]*)\s+(\[\]|[A-Za-z_])")

WRITE_VERBS = ("add", "put", "push")


def fail(msg):
    sys.stderr.write("%s: FAIL — %s\n" % (NAME, msg))
    return 1


def ctx_tables(sem_file):
    """Имена полей из объявления `export type Ctx` — ЧТЕНИЕМ, не списком."""
    if not os.path.isfile(sem_file):
        return None
    names = []
    inside = False
    with io.open(sem_file, encoding="utf-8", errors="replace") as f:
        for line in f:
            line = line.rstrip("\r\n")
            if not inside:
                if RE_CTX_OPEN.match(line):
                    inside = True
                continue
            if line.strip() == "}":
                break
            s = line.strip()
            if not s or s.startswith("//"):
                continue
            m = RE_FIELD.match(line)
            if m:
                names.append(m.group(1))
    return names if inside else None


def write_pattern(tables):
    alt = "|".join(sorted((re.escape(t) for t in tables), key=len, reverse=True))
    return re.compile(r"(?<![A-Za-z0-9_])(%s)\s*\.\s*(%s)\s*\(" % (alt, "|".join(WRITE_VERBS)))


def judged_files(src):
    out = []
    for dirpath, dirs, names in os.walk(src):
        dirs.sort()
        for nm in sorted(names):
            if nm.endswith(".nv") and not nm.endswith("_test.nv"):
                out.append(os.path.join(dirpath, nm))
    return out


def module_of(path, src):
    """Модуль = каталог первого уровня под `novac/src`; файл прямо в корне —
    `<root>` (сегодня это `main.nv`, и он такой же наполнитель, как любой)."""
    rel = os.path.relpath(path, src).replace("\\", "/")
    parts = rel.split("/")
    return (parts[0] if len(parts) > 1 else "<root>"), rel


def read_baseline(base_file):
    try:
        text = io.open(base_file, encoding="utf-8", errors="replace").read()
    except IOError:
        return None, None
    total = None
    rows = {}
    for line in text.split("\n"):
        s = line.strip()
        if not s or s.startswith("#"):
            continue
        m = re.match(r"^total=(\d+)$", s)
        if m:
            total = int(m.group(1))
            continue
        m = re.match(r"^([a-z_][A-Za-z0-9_]*)=(\d+):(.*)$", s)
        if m:
            rows[m.group(1)] = (int(m.group(2)), m.group(3).strip())
    return total, rows


def main():
    root = os.path.abspath(sys.argv[1] if len(sys.argv) > 1
                           else os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", ".."))
    src = os.path.abspath(sys.argv[2]) if len(sys.argv) > 2 else os.path.join(root, "novac", "src")
    base_file = os.environ.get("NOVAC_TABLE_FILLERS_BASELINE",
                               os.path.join(os.path.dirname(os.path.abspath(__file__)),
                                            "novac-table-fillers.baseline"))

    sem_file = os.path.join(src, "sem", "sem.nv")
    tables = ctx_tables(sem_file)
    if tables is None:
        return fail("в %s нет объявления `export type Ctx {` — мишень потеряна: "
                    "имена таблиц читаются ОТТУДА, а не хранятся в страже" % sem_file)
    if not tables:
        return fail("объявление `export type Ctx` в %s не дало НИ ОДНОГО поля — "
                    "мишень потеряна (форма объявления изменилась), а не «таблиц 0»" % sem_file)

    files = judged_files(src)
    if not files:
        return fail("под судом ни одного файла .nv в %s — мишень потеряна, а не «мест записи 0»" % src)

    pat = write_pattern(tables)
    sites = {}          # таблица -> [(модуль, файл:строка, код)]
    total = 0
    for p in files:
        mod, rel = module_of(p, src)
        with io.open(p, encoding="utf-8", errors="replace") as f:
            for n, line in enumerate(f, 1):
                code = line.split("//", 1)[0]
                for m in pat.finditer(code):
                    sites.setdefault(m.group(1), []).append((mod, "%s:%d" % (rel, n), code.strip()[:100]))
                    total += 1

    if total == 0:
        sys.stderr.write("%s: FAIL — мест записи в таблицы реестра не найдено НИ ОДНОГО "
                         "(полей Ctx: %d, файлов .nv: %d).\n" % (NAME, len(tables), len(files)))
        sys.stderr.write("  Ноль — не «чисто», а потерянная мишень: либо `.add(`/`.put(`/`.push(`\n")
        sys.stderr.write("  больше не то, чем наполняют реестр, либо страж смотрит не туда.\n")
        return 1

    # --- 1. КЛАСС: у таблицы больше одного модуля-наполнителя ------------------
    many = []
    for t in sorted(sites):
        mods = sorted(set(m for m, _, _ in sites[t]))
        if len(mods) > 1:
            many.append((t, mods))
    if many:
        sys.stderr.write("%s: FAIL — у таблицы реестра БОЛЬШЕ ОДНОГО модуля-наполнителя "
                         "(274.5 §3-пред62):\n" % NAME)
        for t, mods in many:
            sys.stderr.write("  %s — наполняют модули: %s\n" % (t, ", ".join(mods)))
            for mod, addr, code in sites[t]:
                sys.stderr.write("      %s [%s]: %s\n" % (addr, mod, code))
        sys.stderr.write("  Наполнитель и спрашивающий связаны только намерением автора. Два\n")
        sys.stderr.write("  модуля-наполнителя — это два автора, и таблица разойдётся молча:\n")
        sys.stderr.write("  не отказом механизма, а ВЕРНЫМ отказом над НЕПОЛНОЙ таблицей.\n")
        sys.stderr.write("  Законно одно из двух: свести запись в один модуль (дверь-наполнитель),\n")
        sys.stderr.write("  либо, если это осознанное разделение, — назвать его в базе %s.\n" % base_file)
        return 1

    # --- 2. якорь базы и храповик ----------------------------------------------
    base_total, base_rows = read_baseline(base_file)
    if base_total is None:
        return fail("нет базы %s (ключ total=N) — храповик судить нечем" % base_file)

    gone = [t for t in sorted(base_rows) if t not in tables]
    if gone:
        sys.stderr.write("%s: FAIL — таблицы, названные в базе, исчезли из `export type Ctx` "
                         "(%s):\n" % (NAME, sem_file))
        for t in gone:
            sys.stderr.write("    %s — строка базы с этого мгновения считает пустоту\n" % t)
        sys.stderr.write("  Поле переименовали — база обязана поехать вместе с ним, иначе\n")
        sys.stderr.write("  страж зелен ровно потому, что ослеп.\n")
        return 1

    if total > base_total:
        sys.stderr.write("%s: FAIL — мест записи в таблицы реестра: %d, база %d. Новая запись в\n"
                         "реестр заводится осознанно; сегодняшнее состояние построчно:\n"
                         % (NAME, total, base_total))
        for t in sorted(sites):
            mods = sorted(set(m for m, _, _ in sites[t]))
            sys.stderr.write("    %s=%d:%s\n" % (t, len(sites[t]), ",".join(mods)))
        sys.stderr.write("    total=%d\n" % total)
        return 1

    if total < base_total:
        print("%s: долг СНИЗИЛСЯ (%d < базы %d) — опусти total в %s со строкой летописи"
              % (NAME, total, base_total, base_file))

    print("%s ok: таблиц Ctx %d, из них наполняемых %d, мест записи %d (база %d) — "
          "у каждой таблицы ровно один модуль-наполнитель"
          % (NAME, len(tables), len(sites), total, base_total))
    return 0


if __name__ == "__main__":
    sys.exit(main())
