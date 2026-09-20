# -*- coding: utf-8 -*-
"""1187, шаг 2: первая партия проб — по одной на код, у каждой КОНТРОЛЬ.

Форма пробы берётся из ТЕКСТА САМОЙ ДИАГНОСТИКИ в месте печати, а не из догадки:
сообщение обычно называет и запрещённую форму, и разрешённую замену, — вторая и
становится контролем.

Три исхода на код, и они названы до прогона:
  * ЖИВОЙ      — проба поднимает ИМЕННО этот код;
  * ЗАТЕНЁННЫЙ — проба поднимает ДРУГОЙ код (перехватчик назван в выдаче);
  * НЕ ДОШЛА   — ни одного кода; это «моя форма не дошла», а НЕ «код мёртв».

usage: python batch1-probes.py <дерево оракула> <каталог проб> <вывод>
"""
import io
import json
import os
import re
import subprocess
import sys

sys.stdout.reconfigure(encoding="utf-8", errors="replace", newline="\n")
ROOT, PROBES, OUT = os.path.abspath(sys.argv[1]), sys.argv[2], sys.argv[3]
NOVA = os.path.join(ROOT, "nova-cli", "target", "release", "nova.exe")

CASES = [
    # ── уже подтверждённые живыми (остаются в партии как контроль конвейера)
    ("E_BINDING_REQUIRES_INIT",
     u"fn main() Io -> () {\n    ro x int\n    print(\"x\")\n}\n",
     u"fn main() Io -> () {\n    ro x = 1\n    print(\"x\")\n}\n"),
    ("E_AMP_LITERAL",
     u"fn main() Io -> () {\n    unsafe { ro p = raw &1 }\n    print(\"x\")\n}\n",
     u"fn main() Io -> () {\n    ro v = 1\n    unsafe { ro p = raw &v }\n    print(\"x\")\n}\n"),
    ("E_ADDR_OF_NON_LVALUE",
     u"fn f() -> int => 1\n\nfn main() Io -> () {\n    unsafe { ro p = raw &f() }\n    print(\"x\")\n}\n",
     u"fn main() Io -> () {\n    ro v = 1\n    unsafe { ro p = raw &v }\n    print(\"x\")\n}\n"),
    ("E_ARRAY_INDEX_PTR_BANNED",
     u"fn main() Io -> () {\n    ro a = []int.of(1, 2)\n    unsafe { ro p = raw &a[0] }\n    print(\"x\")\n}\n",
     u"fn main() Io -> () {\n    ro a = []int.of(1, 2)\n    ro v = a[0]\n    unsafe { ro p = raw &v }\n    print(\"x\")\n}\n"),

    # ── переписанные после чтения условия
    ("E_BAD_FORMAT_SPEC",
     u"fn main() Io -> () {\n    ro n = 1\n    print(\"{n:-5}\")\n}\n",
     u"fn main() Io -> () {\n    ro n = 1\n    print(\"{n:+5}\")\n}\n"),
    ("E_BOUND_UNKNOWN",
     u"fn f[T: NoSuchProtocol](x T) -> int => 1\n\nfn main() Io -> () => print(\"{f(1)}\")\n",
     u"fn f[T](x T) -> int => 1\n\nfn main() Io -> () => print(\"{f(1)}\")\n"),

    # ── из ДВЕНАДЦАТИ без места печати: код собирается не в литерале
    ("E_CONST_RO_REDUNDANT",
     u"const ro X = 1\n\nfn main() Io -> () => print(\"x\")\n",
     u"const X = 1\n\nfn main() Io -> () => print(\"x\")\n"),
    ("E_CONST_MUT_CONFLICT",
     u"const mut X = 1\n\nfn main() Io -> () => print(\"x\")\n",
     u"const X = 1\n\nfn main() Io -> () => print(\"x\")\n"),
    ("E_CONST_CONSUME_CONFLICT",
     u"const consume X = 1\n\nfn main() Io -> () => print(\"x\")\n",
     u"const X = 1\n\nfn main() Io -> () => print(\"x\")\n"),
    ("E_LINT_EXPECT_NO_REASON",
     u"// nova:expect E_BINDING_REQUIRES_INIT\nfn main() Io -> () => print(\"x\")\n",
     u"// nova:expect E_BINDING_REQUIRES_INIT -- prichina nazvana\nfn main() Io -> () => print(\"x\")\n"),
    ("E_TYPE_ARITY_MISMATCH",
     u"fn f(x Option[int, int]) -> int => 1\n\nfn main() Io -> () => print(\"x\")\n",
     u"fn f(x Option[int]) -> int => 1\n\nfn main() Io -> () => print(\"x\")\n"),

    # ── прочие кандидаты партии
    ("E_BOUND_NOT_PROTOCOL",
     u"fn f[T: int](x T) -> int => 1\n\nfn main() Io -> () => print(\"{f(1)}\")\n",
     u"fn f[T](x T) -> int => 1\n\nfn main() Io -> () => print(\"{f(1)}\")\n"),
    ("E_CALL_NOT_CALLABLE",
     u"fn main() Io -> () {\n    ro x = 1\n    ro y = x()\n    print(\"x\")\n}\n",
     u"fn f() -> int => 1\n\nfn main() Io -> () {\n    ro y = f()\n    print(\"x\")\n}\n"),
    ("E_AMP_RECORD_LITERAL",
     u"type Rec {\n    a int\n}\n\nfn main() Io -> () {\n    unsafe { ro p = raw &Rec { a: 1 } }\n    print(\"x\")\n}\n",
     u"type Rec {\n    a int\n}\n\nfn main() Io -> () {\n    ro acc = Rec { a: 1 }\n    unsafe { ro p = raw &acc }\n    print(\"x\")\n}\n"),
    ("E_CONST_FIELD_IN_LITERAL",
     u"type Rec {\n    a int\n}\n\nconst A = 1\n\nfn main() Io -> () {\n    ro r = Rec { A }\n    print(\"x\")\n}\n",
     u"type Rec {\n    a int\n}\n\nconst A = 1\n\nfn main() Io -> () {\n    ro r = Rec { a: A }\n    print(\"x\")\n}\n"),
]


CODE = re.compile(r"\[(E_[A-Z][A-Z0-9_]{2,}|W_[A-Z][A-Z0-9_]{2,})\]")


def run(path):
    p = subprocess.run([NOVA, "check", path], cwd=ROOT, capture_output=True)
    text = (p.stdout + p.stderr).decode("utf-8", "replace").replace("\r", "")
    codes, msgs = [], []
    for line in text.split("\n"):
        s = line.strip()
        if not s:
            continue
        msgs.append(s[:150])
        for c in CODE.findall(s):
            if c not in codes:
                codes.append(c)
    return p.returncode, codes, msgs[:3]


rows = []
for code, suspect, control in CASES:
    d = os.path.join(PROBES, code.lower().replace("_", "-"))
    os.makedirs(d, exist_ok=True)
    io.open(os.path.join(d, "p.nv"), "w", encoding="utf-8", newline="\n").write(suspect)
    io.open(os.path.join(d, "control.nv"), "w", encoding="utf-8", newline="\n").write(control)
    io.open(os.path.join(d, "cmd.sh"), "w", encoding="utf-8", newline="\n").write(
        u"#!/bin/sh\n"
        u"R=\"${1:-/d/Sources/nv-lang/nova-wt-research}\"\n"
        u"D=\"$(cd \"$(dirname \"$0\")\" && pwd)\"\n"
        u"cd \"$R\" || exit 2\n"
        u"echo '== forma =='\n"
        u"./nova-cli/target/release/nova.exe check \"$D/p.nv\"; echo \"rc=$?\"\n"
        u"echo '== KONTROL =='\n"
        u"./nova-cli/target/release/nova.exe check \"$D/control.nv\"; echo \"rc=$?\"\n")
    rc_s, codes_s, msgs_s = run(os.path.join(d, "p.nv"))
    rc_c, codes_c, msgs_c = run(os.path.join(d, "control.nv"))
    if code in codes_s:
        verdict = u"ЖИВОЙ"
    elif codes_s:
        verdict = u"ЗАТЕНЁН: " + ", ".join(codes_s[:3])
    else:
        verdict = u"НЕ ДОШЛА"
    ctrl = u"чист" if not codes_c else u"КОНТРОЛЬ ГРЯЗЕН: " + ", ".join(codes_c[:3])
    rows.append((code, verdict, ctrl, msgs_s[0] if msgs_s else u""))

with io.open(OUT, "w", encoding="utf-8", newline="\n") as fh:
    fh.write(u"# 1187, шаг 2: партия 1 — пробы по одной на код, у каждой контроль\n")
    fh.write(u"# Снято помощником 2026-09-20. ЗАМЕР, НЕ ВЕРДИКТ.\n")
    fh.write(u"# Кодов в партии %d из 178 базы долга.\n#\n" % len(CASES))
    fh.write(u"# «НЕ ДОШЛА» значит: МОЯ форма не подняла ни одного кода. Это не\n"
             u"# «код мёртв» — утверждать смерть можно только чтением условия.\n\n")
    for code, verdict, ctrl, first in rows:
        fh.write(u"%-34s %-28s контроль: %s\n" % (code, verdict, ctrl))
        if first:
            fh.write(u"    %s\n" % first)

live = sum(1 for r in rows if r[1] == u"ЖИВОЙ")
shad = sum(1 for r in rows if r[1].startswith(u"ЗАТЕНЁН"))
none = sum(1 for r in rows if r[1] == u"НЕ ДОШЛА")
dirty = sum(1 for r in rows if r[2] != u"чист")
print(u"партия %d: живых %d, затенённых %d, не дошло %d; грязных контролей %d"
      % (len(rows), live, shad, none, dirty))
print(u"вывод: %s" % OUT)
