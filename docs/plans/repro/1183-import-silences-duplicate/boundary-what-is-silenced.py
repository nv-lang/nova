# -*- coding: utf-8 -*-
u"""ГРАНИЦА свойства 1183: выключается ОДНА проверка или весь обход peer-файлов.

Свойство названо: отказ `duplicate top-level name` пропадает, когда дублируемое
имя объявлено в импортированном модуле. Не измерено главное следствие — МАСШТАБ
молчания. Если молчит только дубль имён, это одна дырка; если молчат ЛЮБЫЕ
диагностики peer-файла, то у потребителя не проверяется целый файл, и это уже
другой разговор.

Три вопроса, на каждый — своя клетка:
  1. в peer-файле ДРУГАЯ ошибка (не дубль) — видна ли она при том же импорте;
  2. файлов ТРИ, а не два — меняется ли картина;
  3. что говорит `nova test` там, где `check` молчит.

Контроль к каждой: то же самое БЕЗ импорта. Без него «молчит» неотличимо от
«ошибки нет».
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

W = tempfile.mkdtemp(prefix="b1183-")


def run(files, mode=u"check"):
    src = os.path.join(W, u"src")
    shutil.rmtree(src, ignore_errors=True)
    os.makedirs(src)
    io.open(os.path.join(W, u"nova.toml"), "w", encoding="utf-8").write(
        u"[package]\nname = \"pb\"\nversion = \"0.1.0\"\n")
    for name, text in files.items():
        io.open(os.path.join(src, name), "w", encoding="utf-8", newline="\n").write(text)
    args = [NOVA, mode, src]
    r = subprocess.run(args, cwd=ROOT, capture_output=True, timeout=900)
    out = re.sub(u"\x1b\\[[0-9;]*m", u"",
                 ((r.stdout or b"") + (r.stderr or b"")).decode("utf-8", "replace"))
    errs = [l.strip() for l in out.splitlines() if u"error" in l.lower()]
    pas = re.search(u"PASS:\\s*(\\d+)", out)
    return r.returncode, (pas.group(1) if pas else u"-"), (errs[0][-58:] if errs else u"(ошибок нет)")


MAIN = u"module pb\n%stype Duration { ro a int }\nfn main() Io -> () => println(\"x\")\n"
# Peer с ДРУГОЙ бедой: обращение к необъявленному имени.
PEER_OTHER = u"module pb\nexport fn broken() -> int => no_such_name_here()\n"
PEER_DUP = u"module pb\ntype Duration { ro b int }\nexport fn touch() -> int => 7\n"

print(u"%-46s %-4s %-6s %s" % (u"случай", u"rc", u"PASS", u"первая ошибка"))
for label, imp, peers in (
        (u"1. чужая ошибка в peer, БЕЗ импорта", u"", {u"peer.nv": PEER_OTHER}),
        (u"1. чужая ошибка в peer, С импортом", IMP, {u"peer.nv": PEER_OTHER}),
        (u"2. ТРИ файла, дубль, БЕЗ импорта", u"",
         {u"peer.nv": PEER_DUP, u"peer2.nv": u"module pb\nexport fn t2() -> int => 1\n"}),
        (u"2. ТРИ файла, дубль, С импортом", IMP,
         {u"peer.nv": PEER_DUP, u"peer2.nv": u"module pb\nexport fn t2() -> int => 1\n"}),
):
    files = {u"main.nv": MAIN % imp}
    files.update(peers)
    rc, pas, err = run(files)
    print(u"%-46s %-4d %-6s %s" % (label, rc, pas, err))

print()
for label, imp in ((u"3. `nova test`, дубль, БЕЗ импорта", u""),
                   (u"3. `nova test`, дубль, С импортом", IMP)):
    rc, pas, err = run({u"main.nv": MAIN % imp, u"peer.nv": PEER_DUP}, mode=u"test")
    print(u"%-46s %-4d %-6s %s" % (label, rc, pas, err))
shutil.rmtree(W, ignore_errors=True)
