# -*- coding: utf-8 -*-
u"""Импорт `std.encoding.serde` ГЛУШИТ отказ `duplicate top-level name` у `check`.

Проба выросла из задания интегратора «найди форму, на которой компилятор ГОВОРИТ
про столкновение». Форма нашлась (см. `matrix.py`), а вместе с ней — случай, где
та же форма МОЛЧИТ.

Переменная одна: строка `import` в `main.nv`. Всё остальное — та же пара
peer-файлов одного модуля, каждый объявляет `type Node`.

Почему в наборе ТРИ разных импорта, а не один: без них нельзя отличить «глушит
любой импорт» от «глушит ЭТОТ». Ответ оказался вторым, и он куда уже.
"""
import io
import os
import re
import shutil
import subprocess
import sys
import tempfile

ROOT = os.environ.get("NOVA_TREE", u"D:/Sources/nv-lang/nova-wt-research")
NOVA = os.environ.get("NOVA", u"D:/Sources/nv-lang/nova/nova-cli/target/release/nova.exe")

PEER = u"module p699d\ntype Node { ro b int }\nexport fn touch() -> int => 7\n"
VARIANTS = [
    (u"без импорта", u""),
    (u"import std.collections.vec", u"import std.collections.vec.{Vec}\n"),
    (u"import std.time.duration", u"import std.time.duration.{Duration}\n"),
    (u"import std.encoding.serde", u"import std.encoding.serde.{json_encode}\n"),
]

W = tempfile.mkdtemp(prefix="p699-silence-")
print(u"nova: %s" % NOVA)
print(u"%-28s %-4s %-6s %s" % (u"вариант", u"rc", u"PASS", u"первая строка ошибки"))
for label, imp in VARIANTS:
    src = os.path.join(W, u"src")
    shutil.rmtree(src, ignore_errors=True)
    os.makedirs(src)
    io.open(os.path.join(W, u"nova.toml"), "w", encoding="utf-8").write(
        u"[package]\nname = \"p699d\"\nversion = \"0.1.0\"\n")
    io.open(os.path.join(src, u"main.nv"), "w", encoding="utf-8", newline="\n").write(
        u"module p699d\n" + imp + u"type Node { ro a int }\n"
        u"fn main() Io -> () => println(\"x\")\n")
    io.open(os.path.join(src, u"peer.nv"), "w", encoding="utf-8", newline="\n").write(PEER)
    r = subprocess.run([NOVA, "check", src], cwd=ROOT, capture_output=True, timeout=900)
    out = re.sub(u"\x1b\\[[0-9;]*m", u"",
                 ((r.stdout or b"") + (r.stderr or b"")).decode("utf-8", "replace"))
    errs = [l.strip() for l in out.splitlines() if u"error" in l.lower()]
    pas = re.search(u"PASS:\\s*(\\d+)", out)
    print(u"%-28s %-4d %-6s %s" % (label, r.returncode,
                                   pas.group(1) if pas else u"-",
                                   (errs[0][-60:] if errs else u"(ошибок нет)")))

# Та же пара файлов, тот же импорт — но СБОРКОЙ, а не проверкой.
src = os.path.join(W, u"src")
r = subprocess.run([NOVA, "build", os.path.join(src, u"main.nv"),
                    u"-o", os.path.join(W, u"a.exe")],
                   cwd=ROOT, capture_output=True, timeout=900)
out = re.sub(u"\x1b\\[[0-9;]*m", u"",
             ((r.stdout or b"") + (r.stderr or b"")).decode("utf-8", "replace"))
errs = [l.strip() for l in out.splitlines() if u"error" in l.lower()]
print(u"\n%-28s %-4d %-6s %s" % (u"тот же случай, BUILD", r.returncode, u"-",
                                 (errs[0][-60:] if errs else u"(ошибок нет)")))
shutil.rmtree(W, ignore_errors=True)
