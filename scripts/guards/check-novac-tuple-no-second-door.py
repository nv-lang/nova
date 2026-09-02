# -*- coding: utf-8 -*-
"""scripts/guards/check-novac-tuple-no-second-door.py — у кортежа НЕТ второй
бухгалтерии: он записывается и строится теми же дверями, что запись.

ПРАВИЛО (владелец, 2026-09-02). Кортеж — это ЗАПИСЬ НА СТЕКЕ. Спека говорит то
же самое дословно: «**tuple** — `type X(типы)` — stack-allocated value type
(позиционные поля `.0`/`.1`)» и «**named tuple** — `type X(name1 T1, name2 T2)`
— stack-allocated value type (именованные поля `.name`)» (02-types.md, D215),
поля несут `priv`/`mut` (07-modules.md, D215 Tuple form), а конструктор берёт
значения по умолчанию РОВНО как функция: `type Complex(re f64 = 0.0, im f64 =
0.0)`, приёмка «`type X(f T = expr)` принимается парсером ✅» (02-types.md).

Значит у кортежа те же способности, что у записи: именованные поля, режимы
полей, видимость полей, умолчания конструктора, а завтра методы и перегруженные
операторы. ВТОРОЕ ОКНО для всего этого — двойная бухгалтерия: каждую новую
способность придётся учить дважды, и второй раз забудут. Замер, вызвавший
правило: `type Pair(x int, y int)` + `Pair(1, 2)` + `p.x` — оракул собирает и
печатает `1`, novac отказывает («unknown field»), потому что кортежная дорога
не заведена в путь записи.

ЧТО СТРАЖ СУДИТ — ровно эмиссию и суд, то есть места, где кортеж и запись
делают ОДНО И ТО ЖЕ:
  A. дверь с `tuple` в ИМЕНИ внутри `emit_c/` — второе окно эмиссии
     (typedef, конструктор, деструктуризация: всё это умеет путь записи);
  B. ветка по `TkTuple`/`is_tuple` внутри `emit_c/` — та же развилка, вид
     сбоку: эмиттер не должен знать, кортеж перед ним или запись;
  C. СВОЙ механизм умолчаний у кортежа: `default`/`slot`-двери рядом со словом
     tuple вне `sem/` — умолчания это D102, одна механика на функции, методы и
     конструкторы, а не копия для кортежей.

ЧЕГО СТРАЖ НЕ ЗАПРЕЩАЕТ, и это названо, а не забыто:
  * `sem/mangle.nv` — СПЕЛЛИНГ имени (`mono_tuple_name`, D123 length-prefixed):
    это схема ИМЕНИ, а не второе окно; путь записи берёт имя из декларации,
    кортеж — из мангла, и обе стороны дальше идут одной дверью;
  * `types/` — СТРУКТУРНОЕ тождество (`head_id` = арность, интернирование двух
    равных кортежей в одну строку). Это D123: тождество кортежа структурное,
    тождество записи номинальное. Слить ИХ нельзя и не нужно — правило про
    бухгалтерию, а не про ключ таблицы типов;
  * `parse/` — грамматика формы `(a, b)`.

ХРАПОВИК. На день заведения второе окно СУЩЕСТВУЕТ (`emit_c/emit_tuple.nv`),
поэтому база — замер, а не ноль: число может только ПАДАТЬ, цель — 0, и рост
красный в тот же миг. Так новое второе окно не заводится уже сегодня, а старое
обязано уходить. База — `novac-tuple-doors.baseline`, снижение фиксируется
правкой базы в том же слиянии (как у корпуса и у долгов подмножества).

$1 — корень; $2 — override сканируемой директории (шов самотеста).
"""
import pathlib
import re
import sys

sys.stdout.reconfigure(encoding="utf-8", errors="replace", newline="\n")
sys.stderr.reconfigure(encoding="utf-8", errors="replace", newline="\n")

NAME = "check-novac-tuple-no-second-door"
BASE_FILE = "novac-tuple-doors.baseline"

# Дверь = объявление функции, чьё ИМЯ говорит про кортеж.
# БЕЗ границ слова, и это не небрежность: `tuple` живёт ВНУТРИ идентификатора
# (`@emit_tuple_lit`), а подчёркивание -- словесный символ, так что `\btuple\b`
# там не совпадает. Дыру нашёл первый же прогон стража по живому дереву -- он
# насчитал НОЛЬ дверей при трёх существующих, то есть был бы зелёным враньём.
RE_DOOR = re.compile(r"^\s*(export\s+)?fn\s+[^(\n]*tuple", re.IGNORECASE)
# Развилка по виду типа внутри эмиссии.
RE_BRANCH = re.compile(r"\bTkTuple\b|\bis_tuple\s*\(")
# Своя механика умолчаний рядом со словом tuple.
RE_DEFAULTS = re.compile(r"tuple[^\n]*(default|slot_is_required|param_default)"
                         r"|(default|slot_is_required|param_default)[^\n]*tuple",
                         re.IGNORECASE)


def read_base(guards_dir):
    """База храповика: одно число `doors=N`. Нет файла — судить не по чему."""
    p = guards_dir / BASE_FILE
    if not p.is_file():
        return None
    for raw in p.read_bytes().decode("utf-8", "replace").split("\n"):
        line = raw.strip()
        if line.startswith("doors="):
            try:
                return int(line[len("doors="):].strip())
            except ValueError:
                return None
    return None


def main():
    root = pathlib.Path(sys.argv[1] if len(sys.argv) > 1 else ".").resolve()
    src = pathlib.Path(sys.argv[2]).resolve() if len(sys.argv) > 2 else root / "novac" / "src"
    guards_dir = pathlib.Path(__file__).resolve().parent

    if not src.is_dir():
        print(f"{NAME} ok: судить нечего — нет {src}")
        return 0

    base = read_base(guards_dir)
    if base is None:
        print(f"{NAME}: FAIL — нет базы храповика {BASE_FILE} (или в ней нет строки "
              f"`doors=N`): без базы страж не может отличить рост от снижения и "
              f"молча пропустил бы второе окно.", file=sys.stderr)
        return 1

    hits = {"A": [], "B": [], "C": []}
    nfiles = 0
    for f in sorted(src.rglob("*.nv")):
        rel = f.relative_to(src).as_posix()
        nfiles += 1
        in_emit = rel.startswith("emit_c/")
        in_sem = rel.startswith("sem/")
        text = f.read_bytes().decode("utf-8", "replace")
        for n, raw in enumerate(text.split("\n"), 1):
            # Комментарий не код: правило судит объявления и ветки, а проза о
            # кортежах законна и нужна (иначе страж запретил бы объяснять себя).
            stripped = raw.lstrip()
            if stripped.startswith("//") or stripped.startswith("///"):
                continue
            if in_emit and RE_DOOR.search(raw):
                hits["A"].append((rel, n, raw.strip()))
            if in_emit and RE_BRANCH.search(raw):
                hits["B"].append((rel, n, raw.strip()))
            if not in_sem and RE_DEFAULTS.search(raw):
                hits["C"].append((rel, n, raw.strip()))

    doors = len(hits["A"]) + len(hits["B"]) + len(hits["C"])

    if doors > base:
        print(f"{NAME}: FAIL — вторая бухгалтерия кортежей ВЫРОСЛА: {doors} > база {base}.",
              file=sys.stderr)
        for tag, note in (("A", "дверь эмиссии с `tuple` в имени — путь записи это уже умеет"),
                          ("B", "ветка по виду TkTuple/is_tuple в эмиссии — эмиттер не должен различать"),
                          ("C", "свой механизм умолчаний у кортежа — умолчания это D102, одна механика")):
            cur = ""
            for rel, n, line in sorted(hits[tag]):
                if rel != cur:
                    cur = rel
                    print(f"  {rel} — {note}:", file=sys.stderr)
                print(f"      {n}: {line[:110]}", file=sys.stderr)
        print("  Кортеж — ЗАПИСЬ НА СТЕКЕ (D215: именованные поля, priv/mut, умолчания", file=sys.stderr)
        print("  конструктора как у функции). Веди его теми же дверями, что запись; кортежу", file=sys.stderr)
        print("  своё — только СПЕЛЛИНГ имени (sem/mangle) и СТРУКТУРНОЕ тождество (types/).", file=sys.stderr)
        return 1

    if doors < base:
        print(f"{NAME}: FAIL — вторая бухгалтерия кортежей СНИЗИЛАСЬ ({doors} < база {base}), "
              f"а база не опущена. Опусти `doors={doors}` в {BASE_FILE} тем же слиянием: "
              f"иначе следующий рост до прежней цифры пройдёт молча.", file=sys.stderr)
        return 1

    goal = " — ЦЕЛЬ 0" if doors > 0 else " (цель достигнута)"
    print(f"{NAME} ok: файлов .nv: {nfiles}, дверей второй бухгалтерии кортежей: "
          f"{doors} (== база {base}){goal}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
