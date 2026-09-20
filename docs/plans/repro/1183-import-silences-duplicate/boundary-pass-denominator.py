# -*- coding: utf-8 -*-
u"""Знаменатель для `PASS: N` и добор третьего режима.

`PASS: 1` само по себе не значит «проверен один файл»: надо знать, сколько
печатает ЧИСТЫЙ пакет того же размера. Без этой клетки число читается как
догадка. Плюс `nova test` меряется отдельно и с контролем.
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
W = tempfile.mkdtemp(prefix="b1183b-")


def run(files, mode=u"check"):
    src = os.path.join(W, u"src")
    shutil.rmtree(src, ignore_errors=True)
    os.makedirs(src)
    io.open(os.path.join(W, u"nova.toml"), "w", encoding="utf-8").write(
        u"[package]\nname = \"pb\"\nversion = \"0.1.0\"\n")
    for name, text in files.items():
        io.open(os.path.join(src, name), "w", encoding="utf-8", newline="\n").write(text)
    r = subprocess.run([NOVA, mode, src], cwd=ROOT, capture_output=True, timeout=900)
    out = re.sub(u"\x1b\\[[0-9;]*m", u"",
                 ((r.stdout or b"") + (r.stderr or b"")).decode("utf-8", "replace"))
    errs = [l.strip() for l in out.splitlines() if u"error" in l.lower()]
    pas = re.search(u"PASS:\\s*(\\d+)", out)
    fail = re.search(u"FAIL:\\s*(\\d+)", out)
    return (r.returncode, pas.group(1) if pas else u"-",
            fail.group(1) if fail else u"-", (errs[0][-52:] if errs else u"(ошибок нет)"))


def main_txt(imp, ty=u"Zebra"):
    return u"module pb\n%stype %s { ro a int }\nfn main() Io -> () => println(\"x\")\n" % (imp, ty)


CLEAN2 = {u"main.nv": main_txt(u""), u"peer.nv": u"module pb\nexport fn t() -> int => 1\n"}
CLEAN2I = {u"main.nv": main_txt(IMP), u"peer.nv": u"module pb\nexport fn t() -> int => 1\n"}
CLEAN3 = dict(CLEAN2); CLEAN3[u"peer2.nv"] = u"module pb\nexport fn t2() -> int => 2\n"
CLEAN3I = dict(CLEAN2I); CLEAN3I[u"peer2.nv"] = u"module pb\nexport fn t2() -> int => 2\n"
DUP3I = {u"main.nv": main_txt(IMP, u"Duration"),
         u"peer.nv": u"module pb\ntype Duration { ro b int }\nexport fn t() -> int => 1\n",
         u"peer2.nv": u"module pb\nexport fn t2() -> int => 2\n"}

print(u"%-44s %-4s %-6s %-6s %s" % (u"случай (файлов)", u"rc", u"PASS", u"FAIL", u"первая ошибка"))
for label, files in ((u"ЧИСТЫЙ пакет 2 файла, без импорта", CLEAN2),
                     (u"ЧИСТЫЙ пакет 2 файла, с импортом", CLEAN2I),
                     (u"ЧИСТЫЙ пакет 3 файла, без импорта", CLEAN3),
                     (u"ЧИСТЫЙ пакет 3 файла, с импортом", CLEAN3I),
                     (u"ДУБЛЬ, 3 файла, с импортом", DUP3I)):
    rc, p, f, e = run(files)
    print(u"%-44s %-4d %-6s %-6s %s" % (label, rc, p, f, e))

print()
DUP2 = {u"main.nv": main_txt(u"", u"Duration"),
        u"peer.nv": u"module pb\ntype Duration { ro b int }\nexport fn t() -> int => 1\n"}
DUP2I = {u"main.nv": main_txt(IMP, u"Duration"),
         u"peer.nv": u"module pb\ntype Duration { ro b int }\nexport fn t() -> int => 1\n"}
for label, files in ((u"`nova test`: дубль, БЕЗ импорта", DUP2),
                     (u"`nova test`: дубль, С импортом", DUP2I),
                     (u"`nova test`: чистый, без импорта", CLEAN2)):
    rc, p, f, e = run(files, mode=u"test")
    print(u"%-44s %-4d %-6s %-6s %s" % (label, rc, p, f, e))
shutil.rmtree(W, ignore_errors=True)
