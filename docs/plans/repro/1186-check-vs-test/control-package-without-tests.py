# -*- coding: utf-8 -*-
u"""КОНТРОЛЬ к сравнению `check`/`test`: а был ли в пакете хоть один тест.

Первый прогон дал `test` молчащим на ШЕСТИ бедах подряд, включая
СИНТАКСИЧЕСКУЮ ОШИБКУ. Это слишком хорошо, чтобы быть правдой: скорее `test` не
находит ни одного теста и выходит, ничего не проверив (`PASS: 0 FAIL: 0`), а я
читаю пустую мишень как чистую.

Здесь та же матрица, но в пакет добавлен НАСТОЯЩИЙ `test "…" { … }`. Если с ним
`test` заговорит — значит прежнее молчание было про отсутствие тестов, и вывод
надо переписать. Если промолчит и с ним — вывод держится.
"""
import io
import os
import re
import shutil
import subprocess
import tempfile

ROOT = u"D:/Sources/nv-lang/nova-wt-research"
NOVA = u"D:/Sources/nv-lang/nova/nova-cli/target/release/nova.exe"
W = tempfile.mkdtemp(prefix="cvt2-")

MAIN_WITH_TEST = (u"module cvt2\n"
                  u"fn main() Io -> () => println(\"x\")\n\n"
                  u"test \"живой тест, чтобы прогону было что запускать\" {\n"
                  u"    assert(1 + 1 == 2)\n"
                  u"}\n")

CASES = [
    (u"дубль имени в peer-файле",
     u"module cvt2\ntype Dup { ro a int }\ntype Dup { ro b int }\nexport fn t() -> int => 1\n"),
    (u"необъявленное имя",
     u"module cvt2\nexport fn t() -> int => no_such_name()\n"),
    (u"синтаксическая ошибка",
     u"module cvt2\nexport fn t( -> int => 1\n"),
    (u"БЕЗ беды вовсе (контроль)",
     u"module cvt2\nexport fn t() -> int => 1\n"),
]


def run(mode, files):
    src = os.path.join(W, u"src")
    shutil.rmtree(src, ignore_errors=True)
    os.makedirs(src)
    io.open(os.path.join(W, u"nova.toml"), "w", encoding="utf-8").write(
        u"[package]\nname = \"cvt2\"\nversion = \"0.1.0\"\n")
    for n, t in files.items():
        io.open(os.path.join(src, n), "w", encoding="utf-8", newline="\n").write(t)
    r = subprocess.run([NOVA, mode, src], cwd=ROOT, capture_output=True, timeout=900)
    out = re.sub(u"\x1b\\[[0-9;]*m", u"",
                 ((r.stdout or b"") + (r.stderr or b"")).decode("utf-8", "replace"))
    pas = re.search(u"PASS:\\s*(\\d+)", out)
    fail = re.search(u"FAIL:\\s*(\\d+)", out)
    return (r.returncode, pas.group(1) if pas else u"-",
            fail.group(1) if fail else u"-",
            bool(re.search(u"error", out, re.IGNORECASE)))


print(u"%-34s %-22s %s" % (u"беда (в пакете ЕСТЬ тест)", u"nova check", u"nova test"))
for label, peer in CASES:
    files = {u"main.nv": MAIN_WITH_TEST, u"peer.nv": peer}
    rc_c, p_c, f_c, e_c = run(u"check", files)
    rc_t, p_t, f_t, e_t = run(u"test", files)
    print(u"%-34s rc=%d PASS=%-3s err=%-5s rc=%d PASS=%-3s FAIL=%-3s err=%s"
          % (label, rc_c, p_c, u"да" if e_c else u"нет", rc_t, p_t, f_t,
             u"да" if e_t else u"нет"))
shutil.rmtree(W, ignore_errors=True)
