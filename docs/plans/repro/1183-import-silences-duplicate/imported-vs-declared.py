# -*- coding: utf-8 -*-
u"""Различаем «имя ИМПОРТИРОВАНО» и «имя ОБЪЯВЛЕНО в импортированном модуле».

Импортируется одно имя, дублируется другое из того же модуля — и наоборот.
Если молчит только на импортированном, признак — импорт; если и на соседних
именах модуля, признак — принадлежность модулю.
"""
import io
import os
import re
import shutil
import subprocess
import tempfile

ROOT = u"D:/Sources/nv-lang/nova-wt-research"
NOVA = u"D:/Sources/nv-lang/nova/nova-cli/target/release/nova.exe"
IMP = u"import std.time.duration.{Duration}\n"
CASES = [(u"import Duration, dup DurationParts", u"DurationParts"),
         (u"import Duration, dup Monotonic", u"Monotonic"),
         (u"import Duration, dup Duration", u"Duration"),
         (u"import Duration, dup Zebra", u"Zebra")]

W = tempfile.mkdtemp(prefix="prop4-")
for label, nm in CASES:
    src = os.path.join(W, u"src")
    shutil.rmtree(src, ignore_errors=True)
    os.makedirs(src)
    io.open(os.path.join(W, u"nova.toml"), "w", encoding="utf-8").write(
        u"[package]\nname = \"pp4\"\nversion = \"0.1.0\"\n")
    io.open(os.path.join(src, u"main.nv"), "w", encoding="utf-8", newline="\n").write(
        u"module pp4\n" + IMP + u"type %s { ro a int }\n" % nm +
        u"fn main() Io -> () => println(\"x\")\n")
    io.open(os.path.join(src, u"peer.nv"), "w", encoding="utf-8", newline="\n").write(
        u"module pp4\ntype %s { ro b int }\nexport fn touch() -> int => 7\n" % nm)
    r = subprocess.run([NOVA, "check", src], cwd=ROOT, capture_output=True, timeout=900)
    out = re.sub(u"\x1b\\[[0-9;]*m", u"",
                 ((r.stdout or b"") + (r.stderr or b"")).decode("utf-8", "replace"))
    errs = [l.strip() for l in out.splitlines() if u"error" in l.lower()]
    pas = re.search(u"PASS:\\s*(\\d+)", out)
    print(u"%-36s %-8s PASS=%s %s" % (
        label, u"ГОВОРИТ" if u"duplicate" in out.lower() else u"МОЛЧИТ",
        pas.group(1) if pas else u"-", (errs[0][-42:] if errs else u"(ошибок нет)")))
shutil.rmtree(W, ignore_errors=True)
