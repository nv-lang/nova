# -*- coding: utf-8 -*-
u"""Где компилятор ГОВОРИТ про столкновение имён, а где молчит — матрица.

Задание интегратора: найти форму, на которой компилятор называет столкновение,
чтобы у №699 появился ПОЛОЖИТЕЛЬНЫЙ КОНТРОЛЬ. Искать по одной форме бессмысленно:
нужно знать ГРАНИЦУ — что говорит, что молчит.

Оси, по которым разложены случаи:
  * ЧТО объявлено дважды: метод · свободная функция · ТИП · константа;
  * ГДЕ второе объявление: тот же файл · соседний peer-файл того же модуля.

Форма из корпуса, про которую известно, что она краснеет
(`spec_tests/conformance/neg/neg_same_module_dup.nv`, D84), взята первой
клеткой: если покраснеет она и только она, значит остальные молчат не из-за
поломки прогона.
"""
import io
import json
import os
import shutil
import subprocess
import tempfile

ROOT = u"D:/Sources/nv-lang/nova-wt-research"
NOVA = u"D:/Sources/nv-lang/nova/nova-cli/target/release/nova.exe"

HEAD = u"module p699m\n"
CASES = [
    (u"метод дважды, один файл", {
        u"main.nv": HEAD + u"type Foo { ro x int }\n"
                    u"fn Foo @bar() -> int => 1\n"
                    u"fn Foo @bar() -> int => 2\n"
                    u"fn main() Io -> () => println(\"x\")\n"}),
    (u"свободная fn дважды, один файл", {
        u"main.nv": HEAD + u"fn dup() -> int => 1\n"
                    u"fn dup() -> int => 2\n"
                    u"fn main() Io -> () => println(\"x\")\n"}),
    (u"ТИП дважды, один файл", {
        u"main.nv": HEAD + u"type Node { ro a int }\n"
                    u"type Node { ro b int }\n"
                    u"fn main() Io -> () => println(\"x\")\n"}),
    (u"const дважды, один файл", {
        u"main.nv": HEAD + u"const K = 1\nconst K = 2\n"
                    u"fn main() Io -> () => println(\"x\")\n"}),
    (u"ТИП дважды, ДВА peer-файла", {
        u"main.nv": HEAD + u"type Node { ro a int }\n"
                    u"fn main() Io -> () => println(\"x\")\n",
        u"peer.nv": HEAD + u"type Node { ro b int }\n"
                    u"fn touch() -> int => 7\n"}),
    (u"свободная fn дважды, ДВА peer-файла", {
        u"main.nv": HEAD + u"fn dup() -> int => 1\n"
                    u"fn main() Io -> () => println(\"x\")\n",
        u"peer.nv": HEAD + u"fn dup() -> int => 2\n"}),
    (u"const дважды, ДВА peer-файла", {
        u"main.nv": HEAD + u"const K = 1\n"
                    u"fn main() Io -> () => println(\"x\")\n",
        u"peer.nv": HEAD + u"const K = 2\n"}),
]

W = tempfile.mkdtemp(prefix="coll699-")
unparsed = 0
print(u"%-34s %-9s %s" % (u"случай", u"вердикт", u"первое сообщение"))
for name, files in CASES:
    src = os.path.join(W, u"src")
    shutil.rmtree(src, ignore_errors=True)
    os.makedirs(src)
    io.open(os.path.join(W, u"nova.toml"), "w", encoding="utf-8").write(
        u"[package]\nname = \"p699m\"\nversion = \"0.1.0\"\n")
    for fn, text in files.items():
        io.open(os.path.join(src, fn), "w", encoding="utf-8", newline="\n").write(text)
    r = subprocess.run([NOVA, "check", src], cwd=ROOT,
                       capture_output=True, timeout=600)
    out = ((r.stdout or b"") + (r.stderr or b"")).decode("utf-8", "replace")
    msg = u""
    for line in out.splitlines():
        s = line.strip()
        if not s:
            continue
        if u"error" in s.lower() or u"duplicate" in s.lower():
            msg = s
            break
    verdict = u"ГОВОРИТ" if (r.returncode != 0 or u"duplicate" in out.lower()) else u"молчит"
    import re as _re
    clean = _re.sub(u"\x1b\\[[0-9;]*m", u"", msg)[:96]
    print(u"%-34s %-9s %s" % (name, verdict, clean))
shutil.rmtree(W, ignore_errors=True)
print(u"\nнеразобранных строк: %d" % unparsed)
