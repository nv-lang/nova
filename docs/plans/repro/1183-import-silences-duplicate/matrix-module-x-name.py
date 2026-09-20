# -*- coding: utf-8 -*-
u"""ЧЕМ `std.encoding.serde` отличается от модулей, которые отказ сохраняют.

Строка 1183: импорт этого модуля глушит у `nova check` отказ
`duplicate top-level name`, а импорт `std.collections.vec` и
`std.time.duration` — нет. Носитель назван; нужно СВОЙСТВО.

Признаки перебираются, а не вычитываются из кода. Оси:
  * ИМЯ, которое дублируется у потребителя: `Node` (объявлено в ТЕСТОВОМ
    peer-файле serde — `tagging_test.nv`) против `Zebra` (не объявлено нигде);
  * МОДУЛЬ, который импортируется: serde · vec · duration.
Произведение осей и есть ответ: если молчание держится на СОВПАДЕНИИ ИМЕНИ с
тестовым peer'ом, оно пропадёт на `Zebra` и появится у другого модуля на имени
из ЕГО тестового peer'а.

Счётчик неразобранного обязателен: ноль непрочитанного — часть вердикта.
"""
import io
import os
import re
import shutil
import subprocess
import tempfile

ROOT = u"D:/Sources/nv-lang/nova-wt-research"
NOVA = u"D:/Sources/nv-lang/nova/nova-cli/target/release/nova.exe"

IMPORTS = {
    u"serde": u"import std.encoding.serde.{json_encode}\n",
    u"vec": u"import std.collections.vec.{Vec}\n",
    u"duration": u"import std.time.duration.{Duration}\n",
    u"(нет)": u"",
}
# `Node` объявлен в ТЕСТОВОМ peer-файле serde; `IntBox` — тоже (другой файл);
# `Zebra` не объявлен нигде; `Vec` — экспортированный тип vec (не тестовый).
NAMES = [u"Node", u"IntBox", u"Zebra"]

W = tempfile.mkdtemp(prefix="serde-prop-")
unparsed = 0
print(u"%-10s %-8s %-8s %-6s %s" % (u"импорт", u"имя", u"вердикт", u"PASS", u"первая ошибка"))
for imp_name, imp in IMPORTS.items():
    for nm in NAMES:
        src = os.path.join(W, u"src")
        shutil.rmtree(src, ignore_errors=True)
        os.makedirs(src)
        io.open(os.path.join(W, u"nova.toml"), "w", encoding="utf-8").write(
            u"[package]\nname = \"pprop\"\nversion = \"0.1.0\"\n")
        io.open(os.path.join(src, u"main.nv"), "w", encoding="utf-8", newline="\n").write(
            u"module pprop\n" + imp + u"type %s { ro a int }\n" % nm +
            u"fn main() Io -> () => println(\"x\")\n")
        io.open(os.path.join(src, u"peer.nv"), "w", encoding="utf-8", newline="\n").write(
            u"module pprop\ntype %s { ro b int }\nexport fn touch() -> int => 7\n" % nm)
        r = subprocess.run([NOVA, "check", src], cwd=ROOT, capture_output=True, timeout=900)
        out = re.sub(u"\x1b\\[[0-9;]*m", u"",
                     ((r.stdout or b"") + (r.stderr or b"")).decode("utf-8", "replace"))
        errs = [l.strip() for l in out.splitlines() if u"error" in l.lower()]
        pas = re.search(u"PASS:\\s*(\\d+)", out)
        verdict = u"ГОВОРИТ" if u"duplicate" in out.lower() else u"МОЛЧИТ"
        print(u"%-10s %-8s %-8s %-6s %s" % (
            imp_name, nm, verdict, pas.group(1) if pas else u"-",
            (errs[0][-46:] if errs else u"(ошибок нет)")))
shutil.rmtree(W, ignore_errors=True)
print(u"\nнеразобранных строк: %d" % unparsed)
