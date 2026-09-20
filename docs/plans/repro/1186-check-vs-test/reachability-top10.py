# -*- coding: utf-8 -*-
u"""Достижимость верхней десятки кодов БЕЗ фикстуры (строка реестра 1187).

Способ — тот, что подтвердился вчера: форма берётся ИЗ МЕСТА ПЕЧАТИ кода, а не
из его имени. Угадывание по имени дало 0 из 4.

Выдача — по форме интегратора: код · форма-репро · получен да/нет · что пришло
вместо · ФАЙЛ-ЭМИТТЕР (последнее обязательно: у одного файла бывает несколько
кодов на разные случаи, и без адреса следующий повторит мою первую ошибку).
"""
import io
import os
import re
import shutil
import subprocess
import tempfile

ROOT = u"D:/Sources/nv-lang/nova-wt-research"
NOVA = u"D:/Sources/nv-lang/nova/nova-cli/target/release/nova.exe"
W = tempfile.mkdtemp(prefix="reach10-")

# (код, файл-эмиттер, что сказано в условии, тело main.nv)
PROBES = [
    (u"E_CONST_FN_EVAL_PANIC", u"const_fn_eval.rs", u"arity mismatch calling const fn",
     u"module rp\nconst fn cf(a int) -> int => a\nconst K = cf(1, 2)\n"
     u"fn main() Io -> () => println(\"${K}\")\n"),
    (u"E_CONST_NOT_CONSTEXPR", u"codegen/emit_c.rs", u"function call not allowed in const",
     u"module rp\nfn f() -> int => 1\nconst K = f()\n"
     u"fn main() Io -> () => println(\"${K}\")\n"),
    (u"E_UNSAFE_REQUIRED", u"types/mod.rs", u"`raw &x` requires unsafe context",
     u"module rp\nfn main() Io -> () {\n    ro x = 1\n    ro p = raw &x\n"
     u"    println(\"${x}\")\n}\n"),
    (u"E_UNUSED_PREFIX_TYPEVAR", u"types/mod.rs", u"generic declared and unused",
     u"module rp\nfn f[T]() -> int => 1\nfn main() Io -> () => println(\"${f[int]()}\")\n"),
    (u"E_SERDE_BAD_ATTRIBUTE", u"parser/mod.rs", u"`#serde` requires arguments",
     u"module rp\n#serde\ntype X { ro a int }\nfn main() Io -> () => println(\"x\")\n"),
    (u"E_UNSAFE_T_READ_REQUIRES_WRAP", u"types/mod.rs", u"member read requiring wrap",
     u"module rp\nextern \"C\" fn cp() -> *u8\n"
     u"fn main() Io -> () {\n    ro p = cp()\n    ro v = p.len\n    println(\"${v}\")\n}\n"),
    (u"E_POINTER_PREFIX_MODIFIER", u"parser/mod.rs", u"`<modifier> *` not allowed",
     u"module rp\nfn f(p mut *u8) -> int => 1\nfn main() Io -> () => println(\"x\")\n"),
    (u"E_BOUND_UNKNOWN", u"types/mod.rs", u"unknown type used as generic bound",
     u"module rp\nfn f[T NoSuchProtocolHere](x T) -> int => 1\n"
     u"fn main() Io -> () => println(\"x\")\n"),
    (u"E_PRIMITIVE_MUT_METHOD", u"field_cache.rs", u"mut-метод на примитиве",
     u"module rp\nfn main() Io -> () {\n    mut n = 1\n    n.push(2)\n    println(\"${n}\")\n}\n"),
    (u"E_BAD_FORMAT_SPEC", u"ast/format_spec.rs", u"явный флаг `-` в спецификации",
     u"module rp\nfn main() Io -> () {\n    ro x = 1\n    println(\"${x:-5}\")\n}\n"),
]


def run(text):
    src = os.path.join(W, u"src")
    shutil.rmtree(src, ignore_errors=True)
    os.makedirs(src)
    io.open(os.path.join(W, u"nova.toml"), "w", encoding="utf-8").write(
        u"[package]\nname = \"rp\"\nversion = \"0.1.0\"\n")
    io.open(os.path.join(src, u"main.nv"), "w", encoding="utf-8", newline="\n").write(text)
    r = subprocess.run([NOVA, "check", src], cwd=ROOT, capture_output=True, timeout=600)
    out = re.sub(u"\x1b\\[[0-9;]*m", u"",
                 ((r.stdout or b"") + (r.stderr or b"")).decode("utf-8", "replace"))
    codes = sorted(set(re.findall(u"\\bE_[A-Z][A-Z0-9_]{2,}\\b", out)))
    first = u""
    for line in out.splitlines():
        if u"error" in line.lower():
            first = line.strip()
            break
    return r.returncode, codes, first


print(u"%-32s %-6s %-5s %s" % (u"код", u"rc", u"есть", u"что пришло / файл-эмиттер"))
got = 0
for code, emitter, cond, text in PROBES:
    rc, codes, first = run(text)
    hit = code in codes
    got += 1 if hit else 0
    other = u", ".join(c for c in codes if c != code) or (u"(кодов нет)" if rc else u"ЧИСТО rc=0")
    print(u"%-32s %-6d %-5s %s | %s"
          % (code, rc, u"ДА" if hit else u"нет",
             (u"—" if hit else other[:60]), emitter))
print(u"\nполучено %d из %d" % (got, len(PROBES)))
shutil.rmtree(W, ignore_errors=True)
