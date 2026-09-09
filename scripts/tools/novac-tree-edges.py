# -*- coding: utf-8 -*-
"""Мера волны M3: сколько имён ЧТЕНИЯ ДЕРЕВА импортируют печатающие модули `novac`.

ЗАЧЕМ. Волна M3 плана 274.8 обещает «эмиттер тела читает только IR» и снятие временных рёбер
`emit_c -> tree/lex/names/sem`. Обещание без числа проверить нечем, поэтому число печатает эта
мерка, и критерий К1 волны сформулирован её строкой.

ПРАВИЛО СЧЁТА — МЕХАНИЧЕСКОЕ, И ЭТО ГЛАВНОЕ В ФАЙЛЕ. Имя, импортированное печатающим модулем
из `../tree`, `../lex`, `../names`, `../sem`, считается ЧТЕНИЕМ ДЕРЕВА, если
  (а) это сам тип дерева (`Node`, `NodeKind`, `TokenKind`, `BinOp`, `UnOp`), либо
  (б) его объявление в источнике упоминает любой из этих типов В ПОДПИСИ.
Всё прочее — СЛУЖБА: она законно живёт в `sem` (C-имя типа, тег варианта, строки реестров),
дерева не читает и волной не затрагивается.

Почему правило, а не список: первый замер (2026-09-09) делил имена по списку, составленному
рукой, и дал 73/106. Список сломался на `interp_slices(text str) -> []str` — он стоял в
«чтении дерева», хотя берёт СТРОКУ. Правило, зависящее от чьего-то суждения, в критерий не
годится: его нельзя ни повторить, ни проверить. Механическое правило вернуло `interp_slices` в
службы и втянуло восемь имён, которых в списке не было (`decl_fn_row`, `param_name_at`,
`call_arg_at`, `disj_head_slots`, ...) — все берут или возвращают узлы. Итог: 76, не 73.

МЕРА ИСКЛЮЧАЕТ ТРИ ГРУППЫ, и каждое исключение НАЗВАНО, а не подразумевается:
  * `emit_decls.nv` — печатает объявления ТИПОВ, а IR у нас на функцию (решение M0), поэтому у
    типа записи нет и читать ему больше нечего. Волна про печать ТЕЛА (274.8 §2б, граница 2);
  * четыре имени вокруг СПИСКА функций файла (`file_decls`, `decl_fn_row`, `decl_name_at`,
    `param_name_at`) — это обход объявлений, а не тел (там же);
  * СЛОВАРЬ IR — типы дерева, которые несёт сам IR (см. `ir_vocabulary` ниже). Сегодня он пуст.
Исключённые печатаются отдельными строками: исключение, которого не видно, через месяц
читается как ошибка счёта.

ЧЕТЫРЕ ВОПРОСА ПЕРЕНОСИМОСТИ, отвеченные ДО передачи в общее дерево (замер 2026-09-09: мой
предыдущий инструмент покраснел у интегратора тремя стражами разом, все три — в первых четырёх
строках, все три — допущения, истинные пока файл жил в `scratch/`):
  1. АБСОЛЮТНЫЙ ПУТЬ — нет: корень выводится от расположения файла, `NOVAC_ROOT` перекрывает;
  2. ИМЯ БИНАРЯ — неприменимо, мерка ничего не запускает, только читает файлы;
  3. ПРЕДЕЛ ПО ВРЕМЕНИ — неприменимо: чистое чтение ~180 файлов, десятки миллисекунд;
  4. МОЛЧАНИЕ НА ПУСТОМ ВХОДЕ — ЗАКРЫТО: если печатающих модулей или модуля IR не нашлось,
     мерка ОТКАЗЫВАЕТ (rc=2), а не печатает «0 в 0 файлах». Ноль по пустому входу выглядит как
     успех волны и именно так и был бы прочитан.

Вызов: python scripts/tools/novac-tree-edges.py [--all]
  без флага — мера волны (с исключениями выше);  --all — все имена, без исключений.
"""
import glob
import io
import os
import re
import sys

sys.stdout.reconfigure(encoding="utf-8", errors="replace")

TREE_TYPES = ("Node", "NodeKind", "TokenKind", "BinOp", "UnOp")
RE_IMPORT = re.compile(r"^(?:import|use)\s+\.\./(tree|lex|names|sem)\.\{([^}]*)\}", re.M)

# Excluded from the MEASURE, each for a reason named in the docstring above.
EXCLUDED_FILES = ("emit_decls.nv",)
EXCLUDED_NAMES = ("file_decls", "decl_fn_row", "decl_name_at", "param_name_at")


def ir_vocabulary(root):
    """Tree types the IR ITSELF carries -- IR vocabulary, not tree reading.

    WHY THIS EXISTS, AND WHY IT IS COMPUTED RATHER THAN LISTED. A printer must name the
    operator it prints. Today `BinOp`/`UnOp` are declared in `tree/tree.nv` and the IR carries
    NO operators at all (measured 2026-09-09: zero mentions in `lower/ir.nv`) -- which is
    precisely why wave M3 exists. The moment M3 puts the operator into the IR's `Rvalue`, a
    printer naming `BinOp` is reading the IR's OWN vocabulary, whatever module declares the
    enum. A measure that still counted it would make its own criterion unreachable: no printer
    can print `a + b` without naming the operator.

    So the set is READ FROM `lower/ir.nv` instead of being written down. Today it is empty and
    changes nothing; after M3 it fills by itself. This is the same day's normaliser lesson
    applied in advance: an assumption that is true now and left unwritten becomes false
    silently, so the rule is made to depend on the tree rather than on my memory of it.

    `NOVAC_IR_FILE` overrides the path -- that is how the exclusion is probed without editing
    a source file.
    """
    p = os.environ.get("NOVAC_IR_FILE") or os.path.join(root, "novac/src/lower/ir.nv")
    p = p.replace("\\", "/")
    if not os.path.isfile(p):
        sys.stderr.write(
            "novac-tree-edges: REFUSED -- no IR module at %s; the measure cannot tell IR\n"
            "vocabulary from tree reading, and guessing would fake a zero.\n" % p)
        return None
    text = io.open(p, encoding="utf-8", errors="replace").read()
    return tuple(t for t in TREE_TYPES if re.search(r"\b%s\b" % t, text))


def ir_carries_nodes(root):
    """Where the IR carries a TREE NODE as its own payload -- the other half of the measure.

    MEASURING ONLY THE PRINTERS WOULD BE GAMEABLE, and the probe of 2026-09-09 showed it in one
    run: the printer-side count fell 71 -> 62 the moment `ir_vocabulary` noticed that
    `lower/ir.nv` mentions `Node`. It does, and not by accident -- the IR carries raw tree nodes
    as payload today: `Operand.Tree(Node)`, `Rvalue.Expr(Node)`, `StoreStmt.target/src`,
    `Cond.Expr(Node)`, `RangeTerm.lo/hi`, `LoopTerm.While(Node)`, `Lit(Node)`,
    `SwitchArm.guard Option[Node]`. The module's own doc calls it what it is: "the M1 bridge:
    printed from the tree until M3".

    So while that bridge stands, a printer reading nodes is reading the IR's own vocabulary and
    the printer-side number cannot fall on its own. The two halves are ONE criterion: the IR
    stops handing out nodes, AND the printers stop naming tree readers. Reported together, and
    K1 is met only when both are zero.

    Not counted: the import line itself (lowering reads the tree by definition -- that edge is
    permanent), and doc comments, which say `Node` while carrying nothing.
    """
    p = os.environ.get("NOVAC_IR_FILE") or os.path.join(root, "novac/src/lower/ir.nv")
    p = p.replace("\\", "/")
    if not os.path.isfile(p):
        return None
    sites = []
    for i, line in enumerate(io.open(p, encoding="utf-8", errors="replace").read().splitlines(), 1):
        code = line.split("///")[0].split("//")[0]
        if re.match(r"\s*(import|use)\s", code):
            continue
        for _ in re.finditer(r"\bNode\b", code):
            sites.append((i, line.strip()[:76]))
    return sites


def repo_root():
    env = os.environ.get("NOVAC_ROOT")
    if env:
        return env
    # scripts/tools/<this file> -> two levels up
    return os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))


def read(p):
    return io.open(p, encoding="utf-8", errors="replace").read()


def declaration_of(name, srcs):
    """The declaration line of `name` outside the printers, if the tree spells one."""
    pats = (
        re.compile(r"^export fn (?:\w+ (?:mut )?)?@?%s\b.*$" % re.escape(name), re.M),
        re.compile(r"^export fn %s\b.*$" % re.escape(name), re.M),
        re.compile(r"^export type %s\b.*$" % re.escape(name), re.M),
    )
    for p, text in srcs.items():
        if "/emit_c/" in p:
            continue
        for pat in pats:
            m = pat.search(text)
            if m:
                return m.group(0)
    return None


def main():
    root = repo_root()
    show_all = "--all" in sys.argv
    printers = sorted(glob.glob(os.path.join(root, "novac/src/emit_c/*.nv").replace("\\", "/")))
    if not printers:
        sys.stderr.write(
            "novac-tree-edges: REFUSED -- no printing modules found under %s/novac/src/emit_c/.\n"
            "Nothing was measured; a zero here would read as the wave being finished.\n"
            "Run from the novac repository, or set NOVAC_ROOT.\n" % root)
        return 2
    vocab = ir_vocabulary(root)
    if vocab is None:
        return 2
    srcs = {}
    for p in (glob.glob(os.path.join(root, "novac/src/*.nv").replace("\\", "/"))
              + glob.glob(os.path.join(root, "novac/src/*/*.nv").replace("\\", "/"))):
        srcs[p.replace("\\", "/")] = read(p)

    tree, svc, excluded, ir_named, unresolved = {}, {}, {}, {}, {}
    for p in printers:
        base = p.replace("\\", "/").split("/")[-1]
        for m in RE_IMPORT.finditer(read(p)):
            for name in [x.strip() for x in m.group(2).split(",") if x.strip()]:
                if name in vocab and not show_all:
                    ir_named.setdefault(base, set()).add(name)
                    continue
                if name in TREE_TYPES:
                    is_tree = True
                else:
                    sig = declaration_of(name, srcs)
                    if sig is None:
                        unresolved.setdefault(base, set()).add(name)
                        continue
                    is_tree = any(re.search(r"\b%s\b" % t, sig) for t in TREE_TYPES)
                if not is_tree:
                    svc.setdefault(base, set()).add(name)
                elif not show_all and (base in EXCLUDED_FILES or name in EXCLUDED_NAMES):
                    excluded.setdefault(base, set()).add(name)
                else:
                    tree.setdefault(base, set()).add(name)

    def tot(d):
        return sum(len(v) for v in d.values())

    def spell(d):
        return "; ".join("%s: %s" % (k, " ".join(sorted(v))) for k, v in sorted(d.items()))

    scope = "ALL names" if show_all else "the M3 measure (type declarations, the file's function list and IR vocabulary excluded)"
    print("novac-tree-edges: %s" % scope)
    print("IR vocabulary (tree types the IR itself carries): %s"
          % (", ".join(vocab) if vocab else "none -- the IR carries no tree type yet"))
    sites = ir_carries_nodes(root)
    n_sites = len(sites) if sites is not None else -1
    print("HALF A -- the IR hands out tree nodes at %d site(s) in lower/ir.nv (the M1 bridge)"
          % n_sites)
    print("HALF B -- tree-reading names in the body printers: %d in %d file(s)"
          % (tot(tree), len(tree)))
    print("K1 of wave M3: %s" % ("MET -- both halves are zero" if n_sites == 0 and not tree
                                 else "not met -- both halves must reach zero, and while half A"
                                      " stands a printer reading nodes is reading the IR's own"
                                      " vocabulary"))
    print("not the wave -- service names (C types, mangling, rows): %d in %d file(s)"
          % (tot(svc), len(svc)))
    if not show_all:
        print("excluded by a NAMED boundary (274.8 section 2b): %d in %d file(s)%s"
              % (tot(excluded), len(excluded), " -- " + spell(excluded) if excluded else ""))
        print("excluded as IR vocabulary: %d in %d file(s)%s"
              % (tot(ir_named), len(ir_named), " -- " + spell(ir_named) if ir_named else ""))
    if unresolved:
        print("UNRESOLVED -- no declaration found, the rule cannot judge: %d" % tot(unresolved))
        for k in sorted(unresolved):
            print("    %-24s %s" % (k, " ".join(sorted(unresolved[k]))))
    if sites:
        print("")
        print("HALF A in detail -- every place lower/ir.nv still names a tree node:")
        for i, line in sites:
            print("  ir.nv:%-4d %s" % (i, line))
        print("  (the last few are the BUILDER'S DOORS: they take a node because the data stores")
        print("   one, so they lose it with the data. The lowering pass keeps reading the tree --")
        print("   that edge is permanent; what must go is the IR CARRYING the tree.)")
    print("")
    print("tree-reading, per printing module:")
    if not tree:
        print("  (none -- criterion K1 of wave M3 is met)")
    for k in sorted(tree, key=lambda k: (-len(tree[k]), k)):
        print("  %-24s %2d  %s" % (k, len(tree[k]), " ".join(sorted(tree[k]))))
    return 0


if __name__ == "__main__":
    sys.exit(main())
