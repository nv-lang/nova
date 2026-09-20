# -*- coding: utf-8 -*-
u"""Строка 1186: чего `nova test` НЕ проверяет против `nova check` — ПЕРЕЧЕНЬ.

Метод поправлен после собственного промаха: в КАЖДОМ пакете есть НАСТОЯЩИЙ
`test "…" { assert(…) }`. Без него `test` не находит, что запускать, печатает
`PASS: 0 FAIL: 0` и выходит нулём — и шесть семейств подряд выглядели
«невидимыми», хотя мерилась пустая мишень.

КОЛОНКА, КОТОРОЙ НЕ БЫЛО: не только «расходятся ли вердикты», но и «называет ли
`test` ТОТ ЖЕ АДРЕС». Отказ по чужому адресу хуже слепоты: он выглядит работающей
проверкой и отправляет человека чинить не то (случай `Duration.ZERO`).

ОТКУДА СПИСОК СЕМЕЙСТВ: из головы — назван честно, как требует приёмка. Он не
претендует на полноту: это классы, которые я умею посадить в две строки кода,
а не перечень всех проверок компилятора.
"""
import io
import os
import re
import shutil
import subprocess
import tempfile

ROOT = u"D:/Sources/nv-lang/nova-wt-research"
NOVA = u"D:/Sources/nv-lang/nova/nova-cli/target/release/nova.exe"
W = tempfile.mkdtemp(prefix="fam-")

MAIN = (u"module fam\n%s"
        u"fn main() Io -> () => println(\"x\")\n\n"
        u"test \"живой тест, иначе прогон ничего не запустит\" {\n"
        u"    assert(1 + 1 == 2)\n}\n")

FAMILIES = [
    (u"дубль имени (два peer-файла)", u"",
     u"module fam\ntype Dup { ro a int }\nexport fn t() -> int => 1\n",
     u"module fam\ntype Dup { ro b int }\nexport fn t2() -> int => 2\n"),
    (u"необъявленное имя", u"",
     u"module fam\nexport fn t() -> int => no_such_name()\n", None),
    (u"несовпадение типов", u"",
     u"module fam\nexport fn t() -> int => \"строка\"\n", None),
    (u"неверная арность вызова", u"",
     u"module fam\nfn two(a int, b int) -> int => a + b\n"
     u"export fn t() -> int => two(1)\n", None),
    (u"несуществующий импорт", u"",
     u"module fam\nimport std.no.such.module.{Thing}\nexport fn t() -> int => 1\n", None),
    (u"синтаксическая ошибка", u"",
     u"module fam\nexport fn t( -> int => 1\n", None),
    (u"неиспользованный импорт (предупреждение)", u"",
     u"module fam\nimport std.time.duration.{Duration}\nexport fn t() -> int => 1\n", None),
    (u"дубль имени + импорт модуля с этим именем (1183)",
     u"import std.time.duration.{Duration}\n",
     u"module fam\ntype Duration { ro a int }\nexport fn t() -> int => 1\n",
     u"module fam\ntype Duration { ro b int }\nexport fn t2() -> int => 2\n"),
]

ADDR = re.compile(u"([A-Za-z0-9_./\\\\-]+\\.nv):(\\d+)")


def run(mode, files):
    src = os.path.join(W, u"src")
    shutil.rmtree(src, ignore_errors=True)
    os.makedirs(src)
    io.open(os.path.join(W, u"nova.toml"), "w", encoding="utf-8").write(
        u"[package]\nname = \"fam\"\nversion = \"0.1.0\"\n")
    for n, t in files.items():
        io.open(os.path.join(src, n), "w", encoding="utf-8", newline="\n").write(t)
    r = subprocess.run([NOVA, mode, src], cwd=ROOT, capture_output=True, timeout=900)
    out = re.sub(u"\x1b\\[[0-9;]*m", u"",
                 ((r.stdout or b"") + (r.stderr or b"")).decode("utf-8", "replace"))
    # Адрес ищется по ПЕРВОЙ строке, где он вообще есть, а не по первой строке
    # со словом error: stdout и stderr склеены, и сводка `PASS/FAIL` (stdout)
    # шла раньше самих отказов (stderr) — из-за чего адрес check выходил пустым
    # во всех строках сразу. Поймано тем, что «—» стояло РОВНО ВЕЗДЕ: величина,
    # одинаковая во всех клетках, обычно говорит о мерке, а не о предмете.
    msg = u""
    for line in out.splitlines():
        s = line.strip()
        if ADDR.search(s) and (u"error" in s.lower() or u"FAIL" in s):
            msg = s
            break
    if not msg:
        for line in out.splitlines():
            s = line.strip()
            if u"error" in s.lower() or u"FAIL" in s:
                msg = s
                break
    m = ADDR.search(msg)
    addr = u"%s:%s" % (os.path.basename(m.group(1)), m.group(2)) if m else u"—"
    return r.returncode, msg, addr


print(u"%-44s %-24s %-24s %-10s %s"
      % (u"семейство", u"check", u"test", u"расход?", u"адрес одинаков?"))
for label, imp, peer, peer2 in FAMILIES:
    files = {u"main.nv": MAIN % imp, u"peer.nv": peer}
    if peer2:
        files[u"peer2.nv"] = peer2
    rc_c, msg_c, addr_c = run(u"check", files)
    rc_t, msg_t, addr_t = run(u"test", files)
    v = lambda rc: u"ОТКАЗ" if rc else u"молчит"
    same = (u"—" if rc_c == 0 and rc_t == 0
            else (u"да" if addr_c == addr_t and addr_c != u"—" else u"НЕТ (%s / %s)" % (addr_c, addr_t)))
    print(u"%-44s %-24s %-24s %-10s %s"
          % (label, u"%s rc=%d" % (v(rc_c), rc_c), u"%s rc=%d" % (v(rc_t), rc_t),
             u"ДА" if (rc_c != 0) != (rc_t != 0) else u"нет", same))
shutil.rmtree(W, ignore_errors=True)
