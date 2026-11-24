# -*- coding: utf-8 -*-
"""Самотест `check-novac-lattice-readers` — КЛЕТКАМИ, а не историями.

ПОЧЕМУ КЛЕТКАМИ (П16: страж не принят, пока самотест не ДОКАЗАЛ, что он
ловит). Дыра живёт в непокрытой клетке произведения осей. Оси здесь две:
  СТОРОНА расхождения — новый читатель без строки / строка без читателя /
  класс M без доминирующей проверки / класс или основание не по форме / базы нет;
  СОСТОЯНИЕ — расходится (ждём красный с именованной причиной) или совпадает
  (ждём зелёный). Зелёные клетки не менее важны красных: страж, краснеющий на
  законной форме (трёхстрочный `if … { return }`, цепочка `else if`, ветка с
  выходом), выдал бы ложные отказы на каждой волне.

КАЖДАЯ КЛЕТКА СТРОИТ СВОЙ КОРЕНЬ во временном каталоге: страж принимает корень
и базу аргументами, поэтому подделываются ровно `novac/src/check/*.nv` и файл
базы. Настоящее дерево самотест НЕ трогает.
"""
import io
import os
import shutil
import subprocess
import sys
import tempfile

sys.stdout.reconfigure(encoding="utf-8", errors="replace", newline="\n")
sys.stderr.reconfigure(encoding="utf-8", errors="replace", newline="\n")

NAME = "selftest check-novac-lattice-readers"
HERE = os.path.dirname(os.path.abspath(__file__))
GUARD = os.path.join(os.path.dirname(HERE), "check-novac-lattice-readers.py")

HEAD = "module novac.check\n\n"
WHY = "the refusal mark stands above and dominates this ask"


def one_line():
    """A one-line comparison with an exit: what follows is behind it."""
    return ("fn Checker mut @one(e Node) -> () {\n"
            "    ro before = @out.len()\n"
            "    @type_expr(e)\n"
            "    if @out.len() > before { return }\n"
            "    ro t = type_of(e, @ctx, @scope, @chan)\n"
            "}\n")


def three_line():
    """The same comparison written on three lines."""
    return ("fn Checker mut @three(e Node) -> () {\n"
            "    ro before = @out.len()\n"
            "    @type_expr(e)\n"
            "    if @out.len() > before {\n"
            "        @bind_poisoned(e, kids)\n"
            "        return\n"
            "    }\n"
            "    ro t = type_of(e, @ctx, @scope, @chan)\n"
            "}\n")


def in_block():
    """The ask stands inside the comparison's own block."""
    return ("fn Checker mut @block(e Node) -> () {\n"
            "    ro before = @out.len()\n"
            "    @type_expr(e)\n"
            "    if @out.len() == before {\n"
            "        ro t = type_of(e, @ctx, @scope, @chan)\n"
            "    } else {\n"
            "        // refused: nothing to read\n"
            "    }\n"
            "}\n")


def chain():
    """An `else if` chain: the comparison opens the ask's block."""
    return ("fn Checker mut @chain(e Node) -> () {\n"
            "    ro before = @out.len()\n"
            "    @type_expr(e)\n"
            "    if e.kind_of() == NodeKind.Lit {\n"
            "        ro a = type_of(e, @ctx, @scope, @chan)\n"
            "    } else if @out.len() == before {\n"
            "        ro b = type_of(e, @ctx, @scope, @chan)\n"
            "    } else {\n"
            "        // refused\n"
            "    }\n"
            "}\n")


def nested_exit():
    """The comparison's block exits through a NESTED branch: only a path that
    reaches the ask leaves it, so it guards nothing below -- and the ask is
    below it. The guard must say so."""
    return ("fn Checker mut @nested(e Node) -> () {\n"
            "    ro before = @out.len()\n"
            "    @type_expr(e)\n"
            "    if @out.len() > before {\n"
            "        if e.kind_of() == NodeKind.Lit {\n"
            "            return\n"
            "        } else {\n"
            "            @bind_poisoned(e, kids)\n"
            "        }\n"
            "    }\n"
            "    ro t = type_of(e, @ctx, @scope, @chan)\n"
            "}\n")


def closed_branch():
    """The comparison closed a branch that does not exit: the ask below is
    NOT behind it."""
    return ("fn Checker mut @closed(e Node) -> () {\n"
            "    ro before = @out.len()\n"
            "    @type_expr(e)\n"
            "    if @out.len() > before {\n"
            "        @bind_poisoned(e, kids)\n"
            "    } else {\n"
            "        // typed\n"
            "    }\n"
            "    ro t = type_of(e, @ctx, @scope, @chan)\n"
            "}\n")


def bare():
    return ("fn Checker mut @bare(e Node) -> () {\n"
            "    ro k = @chan.type_if_known(e.id_of())\n"
            "}\n")


def no_mark():
    return ("fn Checker mut @nomark(e Node) -> () {\n"
            "    @type_expr(e)\n"
            "    ro t = type_of(e, @ctx, @scope, @chan)\n"
            "}\n")


def commented():
    """A mention in a comment and in a definition line ask nothing."""
    return ("// type_of(x) in a comment is not a reader\n"
            "fn Checker @type_of(e Node) -> TyId {\n"
            "    no_ty()\n"
            "}\n")


def build(check_text, baseline):
    root = tempfile.mkdtemp(prefix="lattice-readers-")
    check = os.path.join(root, "novac", "src", "check")
    os.makedirs(check)
    io.open(os.path.join(check, "a.nv"), "w", encoding="utf-8", newline="\n").write(HEAD + check_text)
    io.open(os.path.join(check, "x_test.nv"), "w", encoding="utf-8", newline="\n").write(HEAD + no_mark())
    bpath = os.path.join(root, "novac-lattice-readers.baseline")
    if baseline is not None:
        io.open(bpath, "w", encoding="utf-8", newline="\n").write(baseline)
    return root, bpath


def run(root, bpath):
    args = [sys.executable, GUARD, root]
    if bpath:
        args.append(bpath)
    out = subprocess.run(args, capture_output=True, text=True, encoding="utf-8", errors="replace")
    return out.returncode, (out.stdout or "") + (out.stderr or "")


def row(fn, o, kind, cls="M", reason=WHY):
    return "a.nv\t%s\t%d\t%s\t%s\t%s\n" % (fn, o, kind, cls, reason)


CELLS = []


def cell(name, text, baseline, want_rc, needle):
    CELLS.append((name, text, baseline, want_rc, needle))


# ---- ЗЕЛЁНЫЕ: законные формы, на которых страж обязан молчать --------------
cell("green: one-line comparison with exit", one_line(), row("@one", 1, "type_of"), 0, "ok:")
cell("green: comparison on three lines", three_line(), row("@three", 1, "type_of"), 0, "ok:")
cell("green: ask inside the comparison's block", in_block(), row("@block", 1, "type_of"), 0, "ok:")
cell("green: else-if chain", chain(), row("@chain", 1, "type_of", "S") + row("@chain", 2, "type_of"), 0, "ok:")
cell("green: comments and definition lines are not readers", commented(), "", 0, "ok: читателей 0")
cell("green: a test peer is not judged", bare(), row("@bare", 1, "type_if_known", "K"), 0, "ok:")
cell("green: every class with a full reason is accepted",
     one_line() + three_line() + in_block() + bare(),
     "# a comment line\n" + row("@one", 1, "type_of", "C") + row("@three", 1, "type_of", "S")
     + row("@block", 1, "type_of", "R") + row("@bare", 1, "type_if_known", "T"), 0, "ok:")

# ---- КРАСНЫЕ: каждая сторона расхождения названа ----------------------------
cell("red: a new reader without a baseline row", one_line() + no_mark(),
     row("@one", 1, "type_of"), 1, "НОВЫЙ ЧИТАТЕЛЬ без строки в базе: a.nv:11 — @nomark #1 (type_of)")
cell("red: a second reader in the same function is a new row",
     one_line().replace("    ro t = type_of(e, @ctx, @scope, @chan)\n",
                        "    ro t = type_of(e, @ctx, @scope, @chan)\n    ro u = type_of(e, @ctx, @scope, @chan)\n"),
     row("@one", 1, "type_of"), 1, "@one #2 (type_of)")
cell("red: a baseline row whose reader is gone", one_line(),
     row("@one", 1, "type_of") + row("@gone", 1, "type_of"), 1, "больше не соответствует исходнику")
cell("red: class M with no comparison at all", no_mark(), row("@nomark", 1, "type_of"), 1,
     "класс M без доминирующей проверки отказа: a.nv:5")
cell("red: class M behind a closed branch that does not exit", closed_branch(), row("@closed", 1, "type_of"), 1,
     "класс M без доминирующей проверки отказа")
cell("red: class M behind a block that exits only through a nested branch", nested_exit(),
     row("@nested", 1, "type_of"), 1, "класс M без доминирующей проверки отказа")
cell("red: unknown class", one_line(), row("@one", 1, "type_of", "X"), 1, "неизвестный класс 'X'")
cell("red: reason shorter than five words", one_line(), row("@one", 1, "type_of", "M", "mark above"), 1,
     "основание короче 5 слов")
cell("red: a row with the wrong number of fields", one_line(), "a.nv\t@one\t1\ttype_of\tM\n", 1,
     "ожидается 6 полей")
cell("red: a duplicate row", one_line(), row("@one", 1, "type_of") + row("@one", 1, "type_of"), 1, "повтор записи")
cell("red: no baseline file", one_line(), None, 1, "нет базы")


def main():
    failed = 0
    for name, text, baseline, want_rc, needle in CELLS:
        root, bpath = build(text, baseline)
        try:
            rc, out = run(root, bpath)
        finally:
            shutil.rmtree(root, ignore_errors=True)
        good = rc == want_rc and needle in out
        if not good:
            failed += 1
        print("%s  %s" % ("ok  " if good else "FAIL", name))
        if not good:
            print("      want rc=%d and %r; got rc=%d" % (want_rc, needle, rc))
            for line in out.strip().split("\n")[:6]:
                print("      | " + line)
    print("%s: %s" % (NAME, "ok" if not failed else "FAIL"))
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main())
