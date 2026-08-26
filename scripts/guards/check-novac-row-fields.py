# -*- coding: utf-8 -*-
"""scripts/guards/check-novac-row-fields.py — состав полей строки реестра
объявлен решением в плане (§10.3в), и представление не допускает состояний,
запрещённых языком (П22/П23).

ТРИ ПРАВИЛА, и все три — об одном классе «поле завелось само»:
  A. поле `value`-записи ↔ строка таблицы §10.3в, в обе стороны: поле без
     записи — решение, которого не принимали; запись без поля — протухший
     реестр (класс №519);
  B. строка хранит СПИСОК парой `*_off`/`*_cnt` и рядом отдельное поле на один
     его элемент (`recv_*`, `self_*`, `first_*`) — это производная: элемент
     читается из списка функцией, а поле рядом со списком с ним расходится;
  C. булево поле на строке, которая хранится СПИСКОМ, обязано нести в §10.3в
     пометку «[на элемент]» — иначе бит, который по инварианту может держать
     лишь ОДИН элемент, положен на каждый, и представление начинает допускать
     «получателей два» (живой случай 2026-08-16: `ParamDef.is_recv`).

РЕЕСТРЫ ЖИВУТ В ЧЕТЫРЁХ ФАЙЛАХ: sem/sem.nv (декларации), sem/channel.nv
(канал чекера), sem/coerce.nv (пары D429) и types/types.nv (интернер). Пока
страж читал два, параллельные векторы 1:1 в канале он не видел вовсе; а список
файлов вместо ПАПКИ дал вторую щель — 2026-08-21 строка `CoercePair` уехала из
sem.nv в со-равный файл того же модуля, и её поля вышли из-под суда БЕЗ единой
правки правила. Список остаётся списком (папка `sem` держит и не-реестровые
файлы, mangle.nv и slots.nv), но новый файл реестра обязан прийти сюда — и это
единственный способ его завести.

СТРОКИ реестра — это `value`-записи (так документировано у TypeDef); контейнеры
(Ctx, *Table, Scope) — обычные записи, их состав судит §10.3б, а не эта таблица.

ПОЧЕМУ PYTHON: shell-редакция поднимала три awk, шесть `tr`, `comm`, и в правиле
C ещё по два `grep` на КАЖДОЕ булево поле — 1.4с (П14).

$1 — корень; $2 — override ОДНОГО файла (шов самотеста); $3 — override плана.
"""
import pathlib
import re
import sys

sys.stdout.reconfigure(encoding="utf-8", errors="replace", newline="\n")
sys.stderr.reconfigure(encoding="utf-8", errors="replace", newline="\n")

NAME = "check-novac-row-fields"

RE_VALUE = re.compile(r"^export type ([A-Z][A-Za-z0-9_]*) value \{")
RE_PLAIN = re.compile(r"^export type [A-Z][A-Za-z0-9_]* \{")
RE_FIELD = re.compile(r"^[ \t\v\f][ \t\v\f]*[a-z_][A-Za-z0-9_]*[ \t\v\f]")
RE_BOOL = re.compile(r"^[ \t\v\f][ \t\v\f]*[a-z_][A-Za-z0-9_]*[ \t\v\f][ \t\v\f]*bool([ \t\v\f]|$)")
RE_PLAN_SEC = re.compile(r"^#+ .*10\.3в")
RE_HEAD = re.compile(r"^#+ ")
RE_PLAN_ROW = re.compile(r"^\|[ \t]*`([A-Z][A-Za-z0-9_]*)`[ \t]*\|")
RE_TICKED = re.compile(r"`([a-z_][A-Za-z0-9_]*)`")
RE_SLICE = re.compile(r"\[\]([A-Z][A-Za-z0-9_]*)")


def head_field(line):
    return re.split(r"\s", line.strip(" \t"), maxsplit=1)[0]


def main():
    a = sys.argv + [""] * 4
    root = pathlib.Path(a[1] if a[1] else ".").resolve()
    if a[2]:
        sem_files = [pathlib.Path(a[2])]
    else:
        s = root / "novac" / "src"
        # `protocols.nv` появился 2026-08-23, когда sem.nv перешагнул тысячу
        # строк и решение 12 разрезало его по смыслу. Файл в списке, потому что
        # реестр §10.3в судит ПОЛЯ СТРОК, а не файлы: строка, уехавшая в
        # со-равный файл того же модуля, осталась той же строкой.
        # `binding.nv` — тем же днём и по тому же решению: `ParamDef`, `ArgSpec`,
        # `ParamMode` и `ArgClass` уехали туда из sem.nv, когда тот снова
        # перешагнул тысячу строк. Разрез по СМЫСЛУ (что такое параметр, что
        # такое аргумент, и может ли один заполнить другой), а поля остались
        # теми же полями — и страж сразу сказал, что перестал их видеть.
        # `defs.nv` -- 2026-08-26, the same decision-12 cut: the NAME registry
        # (Def, DefTable) moved out of sem.nv when it crossed a thousand lines
        # again. The rows are the same rows; the guard said it stopped seeing
        # them within minutes of the cut, which is this list working as meant.
        sem_files = [s / "sem" / "sem.nv", s / "sem" / "channel.nv", s / "sem" / "coerce.nv",
                     s / "sem" / "callables.nv", s / "sem" / "protocols.nv",
                     s / "sem" / "binding.nv", s / "sem" / "defs.nv",
                     # `interop.nv` -- 2026-08-26, sub-plan L: the interop surface
                     # is a registry table of Ctx like the others, and its rows are
                     # judged like the others.
                     s / "sem" / "interop.nv",
                     s / "types" / "types.nv"]
    sem = pathlib.Path(a[2]) if a[2] else root / "novac" / "src" / "sem" / "sem.nv"
    plan = pathlib.Path(a[3]) if a[3] else root / "docs" / "plans" / "274-novac-self-hosted-compiler.md"

    if not sem.is_file():
        print(f"{NAME} ok: судить нечего (нет {sem})")
        return 0
    if not plan.is_file():
        print(f"{NAME}: FAIL — нет плана {plan}, состав полей не с чем сверить", file=sys.stderr)
        return 1

    # Файлы читаются ОДНИМ потоком, как их склеивал `cat`: запись, не закрытая
    # к концу файла, продолжается в следующем — так же, как в shell-редакции.
    lines = []
    blob = []
    for f in sem_files:
        if f.is_file():
            text = f.read_bytes().decode("utf-8", "replace").replace("\r", "")
            lines.extend(text.split("\n"))
            blob.append(text)
    blob = "\n".join(blob)

    src = set()
    bools = set()
    rec = ""
    for line in lines:
        m = RE_VALUE.match(line)
        if m:
            rec = m.group(1)
            continue
        if RE_PLAIN.match(line):
            rec = ""
            continue
        if rec and line.startswith("}"):
            rec = ""
        if rec and RE_FIELD.match(line):
            src.add((rec, head_field(line)))
        if rec and RE_BOOL.match(line):
            bools.add((rec, head_field(line)))

    plan_lines = plan.read_bytes().decode("utf-8", "replace").replace("\r", "").split("\n")
    pln = set()
    inb = False
    for line in plan_lines:
        if RE_PLAN_SEC.match(line):
            inb = True
            continue
        if inb and RE_HEAD.match(line):
            inb = False
        if not inb:
            continue
        m = RE_PLAN_ROW.match(line)
        if not m:
            continue
        rest = line.split("`", 2)[2] if line.count("`") >= 2 else ""
        # во второй ячейке может стоять несколько полей через пробел
        cell = rest.split("|")[1] if rest.count("|") >= 1 else ""
        for f in RE_TICKED.finditer(cell):
            pln.add((m.group(1), f.group(1)))

    if not src:
        print(f"{NAME}: FAIL — в {sem} не нашлось ни одной записи-строки: "
              f"страж потерял мишень (класс №519)", file=sys.stderr)
        return 1
    if not pln:
        print(f"{NAME}: FAIL — таблица §10.3в пуста или переименована: "
              f"сверять не с чем, а молчать нельзя (класс №519)", file=sys.stderr)
        return 1

    missing = sorted(f"{r} {f}" for r, f in src - pln)
    stale = sorted(f"{r} {f}" for r, f in pln - src)
    if missing:
        print(f"{NAME}: FAIL — поле строки заведено без записи в §10.3в (П22):", file=sys.stderr)
        for m in missing:
            print(f"  {m}", file=sys.stderr)
        print("  Впиши поле в §10.3в и ответь там на вопрос «почему это ПОЛЕ, а не функция", file=sys.stderr)
        print("  от уже хранимого». Производное значение выражается функцией: получатель —", file=sys.stderr)
        print("  не поле, а params[param_off], и читает его одна FnTable.recv_of.", file=sys.stderr)
        return 1
    if stale:
        print(f"{NAME}: FAIL — строка §10.3в без поля в коде (протухшая запись, класс №519):", file=sys.stderr)
        for s in stale:
            print(f"  {s}", file=sys.stderr)
        return 1

    # --- (B) выделенный элемент общего списка --------------------------------
    by_rec = {}
    for r, f in src:
        by_rec.setdefault(r, []).append(f)
    smell = []
    for r in sorted(by_rec):
        fs = by_rec[r]
        # Список назван первой частью пары (params_off -> params); поле-подозреваемый
        # должно именовать выделенный элемент ЭТОГО списка: recv_id рядом с
        # param_off/param_cnt — да; head_id рядом с arg_off/arg_cnt — НЕТ, голова
        # терма не элемент его аргументов (ложняк 2026-08-16 на TyRow интернера).
        if not (any("_off" in f for f in fs) and any("_cnt" in f for f in fs)):
            continue
        for f in sorted(fs):
            if re.match(r"^(recv|self|first)", f):
                smell.append(f"  {r}.{f} — выделенный элемент общего списка рядом с парой *_off/*_cnt")
    if smell:
        print(f"{NAME}: FAIL — строка хранит общий список И отдельное поле на один его элемент (П22):", file=sys.stderr)
        for s in smell:
            print(s, file=sys.stderr)
        print("  Это производная: элемент читается из списка функцией (образец: FnTable.recv_of", file=sys.stderr)
        print("  возвращает params[param_off] у метода). Поле рядом со списком расходится с ним.", file=sys.stderr)
        return 1

    # --- (C) булево поле на строке-ЭЛЕМЕНТЕ списка (П23) ---------------------
    element_rows = set(RE_SLICE.findall(blob))
    plan_text = plan_lines
    untagged = []
    for r, f in sorted(bools):
        if r not in element_rows:
            continue
        row = ""
        for line in plan_text:
            if f"`{r}`" in line and f"`{f}`" in line:
                row = line
                break
        if "[на элемент]" not in row:
            untagged.append(f"{r}.{f}")
    if untagged:
        print(f"{NAME}: FAIL — булево поле на строке-элементе списка без пометки «[на элемент]» (П23):", file=sys.stderr)
        for u in untagged:
            print(f"  {u}", file=sys.stderr)
        print("  Если свойство по инварианту может держать лишь ОДИН элемент — бит принадлежит", file=sys.stderr)
        print("  владельцу списка, а не элементу: иначе представление допускает запрещённые", file=sys.stderr)
        print("  состояния (образец: has_recv на FnDef, а не is_recv на каждом параметре).", file=sys.stderr)
        print("  Если бит правда различается по элементам — впиши «[на элемент]» в §10.3в.", file=sys.stderr)
        return 1

    print(f"{NAME} ok: полей строк реестра: {len(src)}, все объявлены в §10.3в, "
          f"протухших записей: 0, выделенных элементов списка: 0")
    return 0


if __name__ == "__main__":
    sys.exit(main())
