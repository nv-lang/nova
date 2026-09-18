# -*- coding: utf-8 -*-
"""check-novac-grammar-kinds — правила `spec/nova.ungrammar` == варианты `NodeKind`.

ЗАЧЕМ. План 274.13 Ш.2: форма языка становится ДАННЫМИ в одном файле. Пока
перечисление видов узлов живёт отдельно от описания их формы, они расходятся
молча — и расходились: вариант `Args` стоял в перечислении, не строился
парсером НИ РАЗУ и был виден только как пустая ветка исчерпывающего разбора.
Этот страж делает такое расхождение КРАСНЫМ, а не заметным при чтении.

ЧТО СУДИТСЯ — три вопроса, и каждый отвечает своим списком, а не общим «ок»:

  1. равенство множеств В ОБЕ СТОРОНЫ: правило без вида и вид без правила
     называются ПОФАМИЛЬНО. Односторонняя проверка выглядела бы проверкой,
     не будучи ею;
  2. каждое ИМЯ, на которое грамматика ссылается, либо определено правилом,
     либо стоит в НАЗВАННОМ списке категорий ниже. Опечатка в ссылке иначе
     читается как «узел, которого нет»;
  3. исключения печатаются ПОИМЕННО даже когда всё зелено: молчаливое
     исключение неотличимо от дыры.

ПОЧЕМУ `LeafNode` ИСКЛЮЧЁН. Он не форма ветви, а вид ЛИСТА (`tree.nv:33`:
«LeafNode is the leaf's own kind — a leaf is NOT an error»). Грамматика
описывает ветви, поэтому правила у него нет и быть не должно. Это
единственное исключение, и оно здесь названо, а не подразумевается.
"""
import io
import os
import re
import sys

sys.stdout.reconfigure(encoding="utf-8", errors="replace", newline="\n")
sys.stderr.reconfigure(encoding="utf-8", errors="replace", newline="\n")

NAME = "check-novac-grammar-kinds"

# Исключения из равенства множеств — ПОИМЕННО, с причиной в одной строке.
KIND_EXCEPTIONS = {
    "LeafNode": "вид ЛИСТА, а не форма ветви (tree.nv:33); грамматика описывает ветви",
}

# Имена, на которые грамматика ссылается НАМЕРЕННО, не будучи узлами.
# Каждое обязано быть объяснено в шапке самой грамматики.
CATEGORIES = {
    "Item": "любая декларация верхнего уровня",
    "Stmt": "любая инструкция",
    "Expr": "любое выражение",
    "Arg": "аргумент вызова: NamedArg либо голое выражение",
    "Any": "любой токен или узел (прогон восстановления)",
    "TypeRefLeaves": "прогон листьев от @type_ref(), своего узла не строит",
    "BalancedRun": "сбалансированный прогон сырых токенов, парсером не разбираемый",
}


def read(path):
    return io.open(path, encoding="utf-8", newline="").read()


def node_kinds(root):
    """Варианты `NodeKind` — из перечисления, а не из памяти.

    Разбор пропускает И `///`, И обычные `//`: между вариантами стоят те и
    другие. Замер 2026-09-18: разбор, знавший только `///`, оборвался на
    первом же `//` и дал 37 видов вместо 59 — число правдоподобное, и
    поймало его лишь НЕЗАВИСИМОЕ число из плана.
    """
    path = os.path.join(root, "novac", "src", "tree", "tree.nv")
    lines = read(path).replace("\r\n", "\n").split("\n")
    start = None
    for i, ln in enumerate(lines):
        if ln.startswith("export type NodeKind enum"):
            start = i
            break
    if start is None:
        return None, path
    kinds = []
    for ln in lines[start + 1:]:
        m = re.match(r"^\s*\|\s*([A-Za-z_][A-Za-z0-9_]*)", ln)
        if m:
            kinds.append(m.group(1))
            continue
        s = ln.strip()
        if s == "" or s.startswith("//"):
            continue
        break
    return kinds, path


def grammar(root):
    """Правила и ссылки грамматики. Комментарии снимаются ДО разбора."""
    path = os.path.join(root, "spec", "nova.ungrammar")
    if not os.path.isfile(path):
        return None, None, path
    body = []
    for ln in read(path).replace("\r\n", "\n").split("\n"):
        body.append(re.sub(r"//.*$", "", ln))
    text = "\n".join(body)
    # Правило: имя в начале строки, затем `=`.
    rules = re.findall(r"^([A-Z][A-Za-z0-9_]*)\s*=", text, re.M)
    # Ссылки: PascalCase-слово вне одинарных кавычек.
    stripped = re.sub(r"'[^'\n]*'", " ", text)
    stripped = re.sub(r"^([A-Z][A-Za-z0-9_]*)\s*=", " ", stripped, flags=re.M)
    refs = set(re.findall(r"\b([A-Z][A-Za-z0-9_]*)\b", stripped))
    return rules, refs, path


def main():
    root = sys.argv[1] if len(sys.argv) > 1 else "."
    kinds, kpath = node_kinds(root)
    if kinds is None:
        sys.stderr.write("%s: FAIL — не нашёл перечисление NodeKind в %s\n" % (NAME, kpath))
        return 1
    rules, refs, gpath = grammar(root)
    if rules is None:
        sys.stderr.write("%s: FAIL — нет файла грамматики %s (план 274.13 Ш.2)\n" % (NAME, gpath))
        return 1

    dup = sorted(set(r for r in rules if rules.count(r) > 1))
    rules_set = set(rules)
    kinds_set = set(kinds)
    expected = kinds_set - set(KIND_EXCEPTIONS)

    kind_no_rule = sorted(expected - rules_set)
    rule_no_kind = sorted(rules_set - kinds_set)
    unknown_refs = sorted(refs - rules_set - set(CATEGORIES))

    print("%s: видов %d, правил %d, исключено поимённо %d (%s)"
          % (NAME, len(kinds), len(rules), len(KIND_EXCEPTIONS),
             ", ".join(sorted(KIND_EXCEPTIONS))))

    bad = False
    if dup:
        sys.stderr.write("%s: FAIL — правило определено дважды: %s\n" % (NAME, ", ".join(dup)))
        bad = True
    if kind_no_rule:
        sys.stderr.write("%s: FAIL — вид `NodeKind` БЕЗ правила грамматики (%d): %s\n"
                         % (NAME, len(kind_no_rule), ", ".join(kind_no_rule)))
        sys.stderr.write("    Либо опиши форму узла в spec/nova.ungrammar, либо сними вид\n")
        sys.stderr.write("    из перечисления: вид, формы которого нет, обещает узел, которого нет.\n")
        bad = True
    if rule_no_kind:
        sys.stderr.write("%s: FAIL — правило грамматики БЕЗ вида `NodeKind` (%d): %s\n"
                         % (NAME, len(rule_no_kind), ", ".join(rule_no_kind)))
        bad = True
    if unknown_refs:
        sys.stderr.write("%s: FAIL — ссылка ни на правило, ни на названную категорию (%d): %s\n"
                         % (NAME, len(unknown_refs), ", ".join(unknown_refs)))
        sys.stderr.write("    Категории названы в самом страже (CATEGORIES) и в шапке грамматики.\n")
        bad = True
    if bad:
        return 1

    print("%s ok: множества правил и видов совпадают пофамильно в обе стороны; "
          "ссылок-категорий %d" % (NAME, len(CATEGORIES)))
    return 0


if __name__ == "__main__":
    sys.exit(main())
