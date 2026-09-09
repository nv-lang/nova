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
`call_arg_at`, `disj_head_slots`, ...) - все берут или возвращают узлы. Итог: 76, не 73.

МЕРА ИСКЛЮЧАЕТ ДВЕ ГРУППЫ, и оба исключения НАЗВАНЫ, а не подразумеваются (план 274.8 §2б,
границы 2 и 3):
  * `emit_decls.nv` — печатает объявления ТИПОВ, а IR у нас на функцию (решение M0), поэтому у
    типа записи нет и читать ему больше нечего. Волна про печать ТЕЛА;
  * четыре имени вокруг СПИСКА функций файла (`file_decls`, `decl_fn_row`, `decl_name_at`,
    `param_name_at`) - это обход объявлений, а не тел.
Исключённые печатаются отдельной строкой: исключение, которого не видно, через месяц читается
как ошибка счёта.

ЧЕТЫРЕ ВОПРОСА ПЕРЕНОСИМОСТИ, отвеченные ДО передачи в общее дерево (замер 2026-09-09: мой
предыдущий инструмент покраснел у интегратора тремя стражами разом, все три - в первых четырёх
строках, все три - допущения, истинные пока файл жил в `scratch/`):
  1. АБСОЛЮТНЫЙ ПУТЬ - нет: корень выводится от расположения файла, `NOVAC_ROOT` перекрывает;
  2. ИМЯ БИНАРЯ - неприменимо, мерка ничего не запускает, только читает файлы;
  3. ПРЕДЕЛ ПО ВРЕМЕНИ - неприменимо: чистое чтение ~180 файлов, десятки миллисекунд;
  4. МОЛЧАНИЕ НА ПУСТОМ ВХОДЕ - ЗАКРЫТО: если печатающих модулей не нашлось, мерка ОТКАЗЫВАЕТ
     (rc=2), а не печатает «0 в 0 файлах». Ноль по пустому входу выглядит как успех волны и
     именно так и был бы прочитан.

Вызов: python scripts/tools/novac-tree-edges.py [--all]
  без флага - мера волны (с исключениями выше);  --all - все имена, без исключений.
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
    srcs = {}
    for p in (glob.glob(os.path.join(root, "novac/src/*.nv").replace("\\", "/"))
              + glob.glob(os.path.join(root, "novac/src/*/*.nv").replace("\\", "/"))):
        srcs[p.replace("\\", "/")] = read(p)

    tree, svc, excluded, unresolved = {}, {}, {}, {}
    for p in printers:
        base = p.replace("\\", "/").split("/")[-1]
        for m in RE_IMPORT.finditer(read(p)):
            for name in [x.strip() for x in m.group(2).split(",") if x.strip()]:
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

    scope = "ALL names" if show_all else "the M3 measure (declarations of types and the file's function list excluded)"
    print("novac-tree-edges: %s" % scope)
    print("M3 measure -- tree-reading names in the body printers: %d in %d file(s)"
          % (tot(tree), len(tree)))
    print("not the wave -- service names (C types, mangling, rows): %d in %d file(s)"
          % (tot(svc), len(svc)))
    if not show_all:
        print("excluded by a NAMED boundary (274.8 section 2b): %d in %d file(s)%s"
              % (tot(excluded), len(excluded),
                 " -- " + "; ".join("%s: %s" % (k, " ".join(sorted(v))) for k, v in sorted(excluded.items()))
                 if excluded else ""))
    if unresolved:
        print("UNRESOLVED -- no declaration found, the rule cannot judge: %d" % tot(unresolved))
        for k in sorted(unresolved):
            print("    %-24s %s" % (k, " ".join(sorted(unresolved[k]))))
    print("")
    print("tree-reading, per printing module:")
    if not tree:
        print("  (none -- criterion K1 of wave M3 is met)")
    for k in sorted(tree, key=lambda k: (-len(tree[k]), k)):
        print("  %-24s %2d  %s" % (k, len(tree[k]), " ".join(sorted(tree[k]))))
    return 0


if __name__ == "__main__":
    sys.exit(main())
