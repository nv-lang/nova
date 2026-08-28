# -*- coding: utf-8 -*-
u"""Самотест `scripts/claude-hooks/inject-after-compact.py` (план 276 шаг 6).

ЗАЧЕМ. Этот хук подаёт правила режима в КАЖДОЕ окно после КАЖДОГО сжатия
контекста. Он был написан и подключён, а проверен одним ручным запуском —
наблюдение 3 замера плана 276. Страж из шага 1 судит ТРУБУ (список существует,
инжектор его читает, объём под потолком), но не ПОВЕДЕНИЕ: снятие YAML-шапки,
подстановку `$ARGUMENTS`, пропуск комментариев, `--add`, `--list` и — самое
важное — что пропавший файл виден В САМОМ ВПРЫСКЕ, а не только в stderr.

Подложное дерево подставляется через `CLAUDE_PROJECT_DIR` — тот же шов, которым
пользуется сам хук.

Запуск: `python scripts/claude-hooks/selftest/test-inject-after-compact.py`
"""
from __future__ import annotations

import io
import os
import shutil
import subprocess
import sys
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
HOOK = os.path.join(HERE, "..", "inject-after-compact.py")

fails = 0


def ok(name):
    print(u"  ok   %s" % name)


def bad(name, detail):
    global fails
    fails += 1
    sys.stderr.write(u"  FAIL %s: %s\n" % (name, detail))


def mk_tree():
    root = tempfile.mkdtemp(prefix="nova-inject-selftest-")
    os.makedirs(os.path.join(root, ".claude", "commands"))
    io.open(os.path.join(root, ".claude", "commands", "a.md"), "w",
            encoding="utf-8", newline="\n").write(
        u"---\ndescription: shapka\n---\n\nТело A с $ARGUMENTS внутри.\n")
    io.open(os.path.join(root, ".claude", "commands", "b.md"), "w",
            encoding="utf-8", newline="\n").write(u"Тело B без шапки.\n")
    io.open(os.path.join(root, ".claude", "after-compact.list"), "w",
            encoding="utf-8", newline="\n").write(
        u"# comment line\n\n.claude/commands/a.md\n.claude/commands/b.md\n")
    return root


def run(root, *args):
    env = dict(os.environ)
    env["CLAUDE_PROJECT_DIR"] = root
    p = subprocess.run([sys.executable, HOOK] + list(args),
                       stdout=subprocess.PIPE, stderr=subprocess.PIPE, env=env)
    return (p.returncode,
            p.stdout.decode("utf-8", "replace"),
            p.stderr.decode("utf-8", "replace"))


# ── впрыск: тела обоих файлов, шапка снята, $ARGUMENTS подставлен ───────────
root = mk_tree()
rc, out, _ = run(root)
if rc == 0 and u"Тело A" in out and u"Тело B" in out:
    ok(u"впрыск несёт тела обоих файлов списка")
else:
    bad(u"впрыск обязан нести тела", "rc=%s out=%r" % (rc, out[:160]))

if u"description: shapka" not in out:
    ok(u"YAML-шапка снята")
else:
    bad(u"шапка обязана сниматься", out[:160])

if u"$ARGUMENTS" not in out and u"(текущая очередь)" in out:
    ok(u"$ARGUMENTS заменён, а не оставлен как есть")
else:
    bad(u"$ARGUMENTS обязан заменяться", out[:200])

if u"# comment line" not in out:
    ok(u"строки-комментарии списка не считаются путями")
else:
    bad(u"комментарий списка попал во впрыск", out[:160])

# ── пропавший файл виден В САМОМ ВПРЫСКЕ, а не только в stderr ─────────────
os.unlink(os.path.join(root, ".claude", "commands", "b.md"))
rc, out, err = run(root)
if u"b.md" in out and rc == 0:
    ok(u"пропавший файл назван В САМОМ впрыске (stderr в контекст не попадает)")
else:
    bad(u"молчание о пропавшем файле читается как успех — класс №770",
        "rc=%s out=%r" % (rc, out[-200:]))
if u"b.md" in err:
    ok(u"и в stderr — для журнала хука")
else:
    bad(u"stderr тоже обязан назвать пропажу", err[:160])

# ── --list: таблица с байтами и отметкой отсутствия ────────────────────────
rc, out, _ = run(root, "--list")
if rc == 0 and u"a.md" in out and (u"НЕТ ФАЙЛА" in out or u"NET" in out):
    ok(u"--list показывает и байты, и отсутствующий файл")
else:
    bad(u"--list обязан показывать состояние списка", "rc=%s out=%r" % (rc, out[:200]))

shutil.rmtree(root, ignore_errors=True)

# ── --add: новый путь, дубль, несуществующий, вне репозитория ──────────────
root = mk_tree()
io.open(os.path.join(root, ".claude", "commands", "c.md"), "w",
        encoding="utf-8", newline="\n").write(u"Тело C.\n")
rc, out, _ = run(root, "--add", ".claude/commands/c.md")
listed = io.open(os.path.join(root, ".claude", "after-compact.list"),
                 encoding="utf-8").read()
if rc == 0 and ".claude/commands/c.md" in listed:
    ok(u"--add вписывает новый путь в список")
else:
    bad(u"--add обязан добавлять путь", "rc=%s list=%r" % (rc, listed))

before = listed
rc, out, _ = run(root, "--add", ".claude/commands/c.md")
after = io.open(os.path.join(root, ".claude", "after-compact.list"),
                encoding="utf-8").read()
if rc == 0 and after == before:
    ok(u"--add дубля не плодит вторую строку")
else:
    bad(u"дубль обязан быть замечен, а не дописан", "rc=%s" % rc)

rc, out, err = run(root, "--add", ".claude/commands/no-such.md")
if rc != 0:
    ok(u"--add несуществующего файла — отказ")
else:
    bad(u"несуществующий путь обязан отвергаться", "rc=%s" % rc)

rc, out, err = run(root, "--add", os.path.join(tempfile.gettempdir(), "outside.md"))
if rc != 0:
    ok(u"--add файла вне репозитория — отказ")
else:
    bad(u"файл вне репозитория хук читать не может", "rc=%s" % rc)

# ── пустой/отсутствующий список — не падение ───────────────────────────────
os.unlink(os.path.join(root, ".claude", "after-compact.list"))
rc, out, _ = run(root)
if rc == 0:
    ok(u"списка нет — впрыск не падает")
else:
    bad(u"отсутствие списка не должно ронять хук", "rc=%s" % rc)

rc, out, _ = run(root, "--list")
if rc == 0:
    ok(u"--list без списка не падает")
else:
    bad(u"--list обязан пережить отсутствие списка", "rc=%s" % rc)

# ── неизвестный аргумент — честный отказ с подсказкой ──────────────────────
rc, out, err = run(root, "--nonsense")
if rc == 2 and "usage" in (out + err):
    ok(u"неизвестный аргумент — отказ с подсказкой")
else:
    bad(u"неизвестный аргумент обязан давать usage", "rc=%s" % rc)

shutil.rmtree(root, ignore_errors=True)

print(u"самотест inject-after-compact: PASS %d FAIL %d" % (14 - fails, fails))
sys.exit(1 if fails else 0)
