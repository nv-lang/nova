# -*- coding: utf-8 -*-
"""scripts/guards/check-novac-lattice-readers.py — каждый читатель решётки
типов записан в перечне, и у тех, кому положена проверка отказа, она стоит.

ПОЧЕМУ (реестр 221.1, строка 984; класс №918). Решётка (`type_of.nv`) отвечает
на вопрос «какой у узла тип» и на нерешённый узел падает внутренним отказом —
так задумано: молчаливый `int` вместо ответа стоил бы дороже (№652). Поэтому
КАЖДЫЙ, кто спрашивает решётку, обязан спрашивать только после того, как узел
решён или отказан, — то есть после проверки «был ли отказ с моей отметки»
(`@out.len() > before`). Правило писали там, где о нём вспоминали: при
закрытии №918 его поставили на одну дверь, а читателем оказалась другая.
Эта строка — второй носитель того же класса, третьего быть не должно.

ЧТО СУДИТСЯ:

  1. ДВУСТОРОННЯЯ СВЕРКА С ПЕРЕЧНЕМ. Каждое обращение к `type_of(` или
     `type_if_known(` в `novac/src/check/*.nv` (тесты не в счёт) стоит в
     `novac-lattice-readers.baseline`, и каждая строка базы есть в исходнике.
     В счёт идёт и СТРОГАЯ ДВЕРЬ КАНАЛА — `chan.type_of(id)` (CheckOut
     `@type_of`, sem/channel.nv): она падает внутренним отказом на узле без
     записи, ровно как решётка, поэтому новый голый читатель через неё
     краснит так же.
     Новый читатель без строки — красный: он не может «выпасть из перечня
     молча». Односторонняя проверка была бы проверкой, не будучи ею.
  2. КЛАСС И ОСНОВАНИЕ У КАЖДОЙ СТРОКИ. Класс — одна из шести букв:
       M  проверка отказа стоит выше в этой же функции (проверяется
          механизмом: см. п. 3);
       C  проверку ставит не это место: либо вызывающий, прежде чем звать
          эту функцию, либо каждая ветка выше, через которую сюда приходят;
       S  узел — литерал или конструкция, чей тип дан самим узлом;
       K  чтение канала `type_if_known`: отсутствие — законный ответ, и
          потребитель его разбирает;
       T  проход потока владения: идёт после того, как файл протипизирован;
       R  рука самой решётки: под-узел спрашивается после того, как обход
          записал этот узел или отказал выше.
     Основание — не меньше пяти слов: класс без слов нельзя проверить
     чтением, а перечень без доказательства — просто список.
  3. КЛАСС M ПРОВЕРЯЕТСЯ МЕХАНИЗМОМ. Выше обращения в той же функции должна
     стоять сверка с `@out.len()`, причем ДОМИНИРУЮЩАЯ: либо свёртка стоит в
     блоке, который к обращению ещё не закрылся, либо её ветка заканчивается
     выходом (`return`/`continue`/`break`), и обращение идёт после неё.
     Сверка из закрытой нетерминальной ветки не считается.

ПОЧЕМУ PYTHON: один проход по 34 файлам вместо grep/awk на каждый (П14).

$1 — корень репозитория; $2 — override базы (шов самотеста);
       NOVA_LR_CHECK_DIR — override каталога check (шов самотеста).
"""
import os
import pathlib
import re
import sys

sys.stdout.reconfigure(encoding="utf-8", errors="replace", newline="\n")
sys.stderr.reconfigure(encoding="utf-8", errors="replace", newline="\n")

NAME = "check-novac-lattice-readers"
CLASSES = ("M", "C", "S", "K", "T", "R")
MIN_REASON_WORDS = 5

# The lattice itself and the channel's STRICT door are one class of reader:
# `chan.type_of(id)` (CheckOut `@type_of`, sem/channel.nv) dies on a node
# with no recorded decision exactly like the lattice does, so a `.type_of(`
# ask is inventoried too (registry 984, second opinion). `@type_of(` stays
# out: that is the lattice's own method name, judged at its call sites.
ANY = re.compile(r"(?<![\w@])type_of\(|\btype_if_known\(")
FN = re.compile(r"^(?:export\s+)?fn\s+(?:\w+\s+)?(?:mut\s+)?(@?\w+)\s*(?:\(|<)")
# A COMPARISON with the mark, not its creation: `ro before = @out.len()` makes
# the mark and guards nothing (the selftest caught the guard counting it).
MARK = re.compile(r"@out\.len\(\)\s*(?:==|!=|<=|>=|<|>)|(?:==|!=|<=|>=|<|>)\s*@out\.len\(\)")
EXIT = re.compile(r"^(?:return|continue|break)\b|^ice_at\(")


def code_of(line):
    """The line without its comment tail."""
    cut = line.find("//")
    return line if cut < 0 else line[:cut]


def inventory(base):
    """Every lattice ask as (file, fn, ordinal, kind, line, guarded), where
    `guarded` says a dominating refusal-mark comparison stands above it in the
    same function."""
    rows = []
    for p in sorted(base.glob("*.nv")):
        if p.name.endswith("_test.nv"):
            continue
        lines = p.read_text(encoding="utf-8", errors="replace").replace("\r", "").split("\n")
        code = [code_of(l) for l in lines]
        fn, ordinal = "(top)", 0
        depth = 0
        start_depth, end_depth = [], []
        marks = []  # line indexes of mark comparisons in the current function
        for i, c in enumerate(code):
            start_depth.append(depth)
            m = FN.match(c.strip())
            if m:
                # A definition line names a function; it asks nothing.
                fn, ordinal, marks = m.group(1), 0, []
            else:
                for hit in ANY.finditer(c):
                    ordinal += 1
                    hd = depth + c[:hit.start()].count("{") - c[:hit.start()].count("}")
                    kind = "type_if_known" if c[hit.start():].startswith("type_if_known") else "type_of"
                    guarded = any(dominates(mi, i, hd, code, start_depth, end_depth, marks) for mi in reversed(marks))
                    rows.append((p.name, fn, ordinal, kind, i + 1, guarded))
            depth += c.count("{") - c.count("}")
            end_depth.append(depth)
            if not m and MARK.search(c):
                marks.append(i)
    return rows


def dominates(mi, i, hd, code, start_depth, end_depth, marks):
    """Does the mark comparison on line `mi` guard the ask on line `i`?"""
    md = start_depth[mi]
    body = end_depth[mi]
    if not code[mi].rstrip().endswith("{"):
        # A one-line comparison (`if @out.len() > before { return }`): what
        # follows at the same depth or deeper is behind it.
        lo = min(md, body)
    else:
        # A comparison that opens a block: the ask must stand inside it, or
        # after it when the block ends in an exit.
        lo = body
        d, k = body, mi + 1
        while k < i and d >= body:
            if d == body and EXIT.match(code[k].strip()):
                lo = md
                break
            d = end_depth[k]
            k += 1
    if hd < lo:
        return False
    for k in range(mi, i):
        if end_depth[k] < lo:
            return False
    return True


def read_baseline(path):
    rows, bad = [], []
    for n, raw in enumerate(path.read_text(encoding="utf-8", errors="replace").replace("\r", "").split("\n"), 1):
        if not raw.strip() or raw.lstrip().startswith("#"):
            continue
        parts = raw.split("\t")
        if len(parts) != 6:
            bad.append("база, строка %d: ожидается 6 полей через табуляцию, найдено %d" % (n, len(parts)))
            continue
        rows.append((n, parts))
    return rows, bad


def main():
    root = pathlib.Path(sys.argv[1] if len(sys.argv) > 1 else ".").resolve()
    base = pathlib.Path(os.environ.get("NOVA_LR_CHECK_DIR") or root / "novac" / "src" / "check")
    bpath = pathlib.Path(sys.argv[2]) if len(sys.argv) > 2 else root / "scripts" / "guards" / "novac-lattice-readers.baseline"
    if not base.is_dir():
        print("%s ok: судить нечего (нет %s)" % (NAME, base))
        return 0
    if not bpath.is_file():
        sys.stderr.write("%s: FAIL — нет базы %s, а читатели есть: перечень сверять не с чем\n" % (NAME, bpath))
        return 1
    src_rows = inventory(base)
    base_rows, problems = read_baseline(bpath)

    by_key = {}
    for n, (f, fn, o, kind, cls, reason) in base_rows:
        try:
            o = int(o)
        except ValueError:
            problems.append("база, строка %d: порядковый номер не число: %r" % (n, o))
            continue
        key = (f, fn, o, kind)
        if key in by_key:
            problems.append("база, строка %d: повтор записи %s" % (n, " :: ".join(map(str, key))))
            continue
        by_key[key] = (n, cls, reason)
        if cls not in CLASSES:
            problems.append("база, строка %d: неизвестный класс %r (нужен один из %s)" % (n, cls, ", ".join(CLASSES)))
        if len(reason.split()) < MIN_REASON_WORDS:
            problems.append("база, строка %d: основание короче %d слов" % (n, MIN_REASON_WORDS))

    src_keys = {(f, fn, o, k) for (f, fn, o, k, _l, _g) in src_rows}
    for f, fn, o, k in sorted(src_keys - set(by_key)):
        line = next(l for (a, b, c, d, l, _g) in src_rows if (a, b, c, d) == (f, fn, o, k))
        problems.append("НОВЫЙ ЧИТАТЕЛЬ без строки в базе: %s:%d — %s #%d (%s): добавь запись с классом и основанием"
                        % (f, line, fn, o, k))
    for f, fn, o, k in sorted(set(by_key) - src_keys):
        problems.append("строка базы %d больше не соответствует исходнику (читатель удалён или переехал): %s :: %s #%d (%s)"
                        % (by_key[(f, fn, o, k)][0], f, fn, o, k))
    for (f, fn, o, k, line, guarded) in src_rows:
        row = by_key.get((f, fn, o, k))
        if row and row[1] == "M" and not guarded:
            problems.append("класс M без доминирующей проверки отказа: %s:%d — %s #%d (%s)" % (f, line, fn, o, k))

    if problems:
        sys.stderr.write("%s: FAIL — перечень читателей решётки разошёлся (%d):\n" % (NAME, len(problems)))
        for p in problems:
            sys.stderr.write("  %s\n" % p)
        sys.stderr.write("  Читатель спрашивает решётку только после решения узла или после\n")
        sys.stderr.write("  проверки отказа; новый читатель получает строку в базе тем же коммитом.\n")
        return 1
    counts = {c: sum(1 for r in base_rows if r[1][4] == c) for c in CLASSES}
    print("%s ok: читателей %d (type_of %d, type_if_known %d) в %d файлах, все в базе; классы %s" % (
        NAME, len(src_rows),
        sum(1 for r in src_rows if r[3] == "type_of"),
        sum(1 for r in src_rows if r[3] == "type_if_known"),
        len({r[0] for r in src_rows}),
        ", ".join("%s=%d" % (c, counts[c]) for c in CLASSES)))
    return 0


if __name__ == "__main__":
    sys.exit(main())
