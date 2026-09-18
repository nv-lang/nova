# -*- coding: utf-8 -*-
"""Самотест `check-novac-grammar-kinds` — КЛЕТКАМИ, а не историями.

ПОЧЕМУ КЛЕТКАМИ. Дыра живёт в непокрытой клетке произведения осей, а не в
списке случаев, которые пришли в голову. Оси здесь две: СТОРОНА расхождения
(вид без правила / правило без вида / ссылка в никуда / дубль правила) и
СОСТОЯНИЕ (совпадает / расходится). Зелёная клетка объясняется так же
дотошно, как красная: «страж промолчал» — это утверждение о предмете.

КАЖДАЯ КЛЕТКА СТРОИТ СВОЙ КОРЕНЬ во временном каталоге: страж принимает
корень аргументом, поэтому подделывать надо ровно два файла —
`novac/src/tree/tree.nv` (перечисление) и `spec/nova.ungrammar` (правила).
Настоящее дерево самотест НЕ трогает: проверка, портящая предмет, которым
судит, не проверка.
"""
import io
import os
import shutil
import subprocess
import sys
import tempfile

sys.stdout.reconfigure(encoding="utf-8", errors="replace", newline="\n")
sys.stderr.reconfigure(encoding="utf-8", errors="replace", newline="\n")

NAME = "selftest check-novac-grammar-kinds"
HERE = os.path.dirname(os.path.abspath(__file__))
GUARD = os.path.join(os.path.dirname(HERE), "check-novac-grammar-kinds.py")


def build_root(kinds, rules_text):
    """Корень с двумя подделанными файлами и ничем больше."""
    root = tempfile.mkdtemp(prefix="grammar-kinds-")
    tree = os.path.join(root, "novac", "src", "tree")
    os.makedirs(tree)
    os.makedirs(os.path.join(root, "spec"))
    body = ["export type NodeKind enum"]
    for k in kinds:
        body.append("    | %s" % k)
        # Между вариантами намеренно стоят ОБЫЧНЫЕ `//`-комментарии: именно
        # на них 2026-09-18 оборвался первый разбор и дал 37 видов из 59.
        body.append("    // a comment between the variants, on purpose")
    body.append("")
    body.append("export type BinOp enum")
    body.append("    | Add")
    io.open(os.path.join(tree, "tree.nv"), "w", encoding="utf-8",
            newline="\n").write("\n".join(body) + "\n")
    io.open(os.path.join(root, "spec", "nova.ungrammar"), "w",
            encoding="utf-8", newline="\n").write(rules_text)
    return root


def run(root):
    out = subprocess.run([sys.executable, GUARD, root],
                         capture_output=True, text=True, encoding="utf-8",
                         errors="replace")
    return out.returncode, (out.stdout or "") + (out.stderr or "")


CELLS = []


def cell(title, kinds, rules, want_code, want_text):
    CELLS.append((title, kinds, rules, want_code, want_text))


# --- ось «совпадает» -------------------------------------------------------
cell("everything matches, LeafNode excluded by name",
     ["LeafNode", "File", "Block"],
     "File = 'x'\n\nBlock = 'y'\n",
     0, "ok")

cell("a named category may be referenced without a rule",
     ["LeafNode", "File"],
     "File = Item* Stmt* Expr Arg Any TypeRefLeaves BalancedRun\n",
     0, "ok")

cell("a quoted token that looks like a rule name is NOT a reference",
     ["LeafNode", "File"],
     "File = 'Block' 'Expr'\n",
     0, "ok")

cell("a comment mentioning a missing node is not a reference",
     ["LeafNode", "File"],
     "// see Block and DepthErr for the recovery shape\nFile = 'x'\n",
     0, "ok")

# --- ось «расходится» ------------------------------------------------------
cell("KIND WITHOUT A RULE is named by name (the Args case of 2026-09-18)",
     ["LeafNode", "File", "Args"],
     "File = 'x'\n",
     1, "Args")

cell("RULE WITHOUT A KIND is named by name",
     ["LeafNode", "File"],
     "File = 'x'\n\nGhost = 'y'\n",
     1, "Ghost")

cell("a reference to nothing is caught (a typo reads as a node that is not)",
     ["LeafNode", "File"],
     "File = Blcok\n",
     1, "Blcok"),

cell("a rule defined twice is caught",
     ["LeafNode", "File", "Block"],
     "File = 'x'\n\nBlock = 'y'\n\nBlock = 'z'\n",
     1, "Block")

# Исключение снимает ТРЕБОВАНИЕ правила, а не ПРАВО на него: `LeafNode` —
# настоящий вид, поэтому написанное правило для него законно и не делает
# «правило без вида». Клетка стоит здесь, чтобы исключение не превратилось
# молча в запрет.
cell("an excluded kind may still carry a rule: the exception drops the demand, not the right",
     ["LeafNode", "File"],
     "File = 'x'\n\nLeafNode = 'tok'\n",
     0, "ok")

# --- ось «предмета нет» ----------------------------------------------------
cell("a missing grammar file fails loudly, not silently",
     ["LeafNode", "File"],
     None,
     1, "нет файла грамматики")


def main():
    ok = 0
    for title, kinds, rules, want_code, want_text in CELLS:
        root = build_root(kinds, rules if rules is not None else "")
        if rules is None:
            os.remove(os.path.join(root, "spec", "nova.ungrammar"))
        code, text = run(root)
        shutil.rmtree(root, ignore_errors=True)
        if code != want_code or want_text not in text:
            sys.stderr.write("%s: FAIL — клетка «%s»\n" % (NAME, title))
            sys.stderr.write("    ждали код %d и текст %r, получили код %d\n"
                             % (want_code, want_text, code))
            sys.stderr.write("    вывод: %s\n" % text.strip().replace("\n", " | "))
            return 1
        print("  ok: %s" % title)
        ok += 1
    print("%s: OK (%d клеток)" % (NAME, ok))
    return 0


if __name__ == "__main__":
    sys.exit(main())
