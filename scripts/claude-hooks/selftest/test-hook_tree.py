# -*- coding: utf-8 -*-
"""Самотест общего ответа «в каком дереве работает ЭТО окно» (`hook_tree.py`).

ДОКАЗЫВАЕТ ОБЕ СТОРОНЫ, как требует П16: дерево сессии выбирается ВМЕСТО
главного; а когда git ответить не может, хук не немеет, а возвращает прежний
корень. Третья клетка — про то, ради чего вся эта машинерия: путь читается
БАЙТАМИ, и не-латинский каталог не превращается в мусор.

Почему клетки строятся на ДВУХ настоящих деревьях: пока `CLAUDE_PROJECT_DIR` и
рабочий каталог — один каталог, подмена корня ничего не меняет, и клетка зелена
при ЛЮБОМ коде. Именно поэтому дефект №1156 прожил незамеченным.
"""
import io
import os
import shutil
import subprocess
import sys
import tempfile

sys.stdout.reconfigure(encoding="utf-8", errors="replace", newline="\n")

HOOKS = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))

fails = 0
cases = 0


def ok(msg):
    global cases
    cases += 1
    print("  ok   %s" % msg)


def bad(msg):
    global fails, cases
    cases += 1
    fails += 1
    print("  ПРОВАЛ %s" % msg)


def ask(cwd, project_dir):
    """Спросить `session_root()` в отдельном процессе — как это делает хук."""
    code = (
        "import sys; sys.path.insert(0, %r)\n"
        "from hook_tree import session_root\n"
        # БАЙТАМИ И ЯВНО В UTF-8. Первая редакция писала `sys.stdout.write`, и
        # дочерний процесс кодировал путь кодировкой КОНСОЛИ: не-латинское имя
        # приезжало мусором, клетка краснела, а ХУК БЫЛ ПРАВ. То есть измеритель
        # воспроизвёл ровно тот дефект, который проверяет, — и отличить это от
        # настоящей поломки можно было только прочитав, ЧТО именно он вернул.
        "sys.stdout.buffer.write(session_root().encode('utf-8'))\n" % HOOKS
    )
    env = dict(os.environ)
    env["CLAUDE_PROJECT_DIR"] = project_dir
    p = subprocess.run([sys.executable, "-c", code],
                       capture_output=True, env=env, cwd=cwd)
    return p.stdout.decode("utf-8", "replace").strip()


def same(a, b):
    """Windows отдаёт то короткое имя каталога, то длинное — сравниваем по факту."""
    try:
        return os.path.samefile(a, b)
    except OSError:
        return os.path.normcase(os.path.normpath(a)) == os.path.normcase(os.path.normpath(b))


main_tree = tempfile.mkdtemp(prefix="nova-htree-main-")
mine = tempfile.mkdtemp(prefix="nova-htree-mine-")
plain = tempfile.mkdtemp(prefix="nova-htree-plain-")   # НЕ git-дерево
try:
    for t in (main_tree, mine):
        subprocess.run(["git", "init", "-q", t], capture_output=True)

    # (1) Я работаю в СВОЁМ дереве, а переменная указывает на главное.
    got = ask(mine, main_tree)
    if got and same(got, mine):
        ok(u"дерево сессии выбрано вместо главного")
    else:
        bad(u"взято не моё дерево: %r (ждали %r)" % (got, mine))

    # (2) ОБРАТНАЯ сторона: git ответить не может — прежний корень, не пустота.
    #     Без этой клетки первая проходила бы и у хука, который всегда молчит.
    got = ask(plain, main_tree)
    if got and same(got, main_tree):
        ok(u"вне git-дерева — прежний корень, хук не немеет")
    else:
        bad(u"запасной путь не сработал: %r (ждали %r)" % (got, main_tree))

    # (3) ПУТЬ ЧИТАЕТСЯ БАЙТАМИ. Каталог с не-латинским именем — ровно тот
    #     случай, на котором первая редакция фикса молча уходила в чужое
    #     дерево: `text=True` декодировал вывод git кодировкой консоли.
    nonlatin = os.path.join(tempfile.mkdtemp(prefix="nova-htree-nl-"), u"дерево")
    os.makedirs(nonlatin)
    subprocess.run(["git", "init", "-q", nonlatin], capture_output=True)
    got = ask(nonlatin, main_tree)
    if got and same(got, nonlatin):
        ok(u"не-латинский путь не портится — дерево найдено")
    else:
        bad(u"не-латинский путь потерян: %r (ждали %r)" % (got, nonlatin))
    shutil.rmtree(os.path.dirname(nonlatin), ignore_errors=True)
finally:
    for t in (main_tree, mine, plain):
        shutil.rmtree(t, ignore_errors=True)

if fails:
    print("селфтест hook_tree: ЕСТЬ ПРОВАЛЫ (%d)" % fails)
    sys.exit(1)
print("селфтест hook_tree: %d/%d ok" % (cases, cases))
