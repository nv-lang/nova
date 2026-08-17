#!/usr/bin/env python3
"""Самотест guard-shell-nonascii.py — обе стороны.

Перехватчик, который не умеет пропускать, остановит всю работу; перехватчик,
который не умеет блокировать, бесполезен. Поэтому проверяются оба направления
и отдельно — ремонтное исключение.
"""
import json
import os
import subprocess
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
HOOK = os.path.join(os.path.dirname(HERE), "guard-shell-nonascii.py")

PASS = 0
FAIL = 0


def run(cmd, env_extra=None):
    env = dict(os.environ)
    env.pop("NOVA_ALLOW_NONASCII_CMD", None)
    if env_extra:
        env.update(env_extra)
    payload = json.dumps({"tool_input": {"command": cmd}})
    p = subprocess.run(
        [sys.executable, HOOK],
        input=payload.encode("utf-8"),
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        env=env,
    )
    return p.returncode


def check(label, got, want):
    global PASS, FAIL
    if got == want:
        PASS += 1
        print("  ok   %s" % label)
    else:
        FAIL += 1
        print("  FAIL %s (want %s, got %s)" % (label, want, got), file=sys.stderr)


print("== propuskaet ==")
check("chistyi ASCII", run("git status --porcelain"), 0)
check("puti bez kirillicy", run("python C:/Temp/x.py && ls -la /d/Sources"), 0)
check("pustaya komanda", run(""), 0)

print("== lovit ==")
# Кириллица собирается из кодов: сам файл самотеста обязан остаться ASCII,
# иначе он повторит ту самую ошибку, которую сторожит.
CYR = "".join(chr(c) for c in (0x41A, 0x43E, 0x43D, 0x441, 0x43E, 0x43B, 0x44C))
check("heredoc s russkim tekstom",
      run("cat > f.py <<'PY'\n# %s\nPY" % CYR), 2)
check("sed -i s russkim", run("sed -i 's/a/%s/' file" % CYR), 2)
check("echo s russkim", run("echo '%s' >> log" % CYR), 2)
check("em-dash (ne kirillica, no ne-ASCII)", run("echo 'a \u2014 b'"), 2)

print("== remontnoe isklyuchenie ==")
check("NOVA_ALLOW_NONASCII_CMD=1 propuskaet",
      run("echo '%s'" % CYR, {"NOVA_ALLOW_NONASCII_CMD": "1"}), 0)

print()
print("selftest guard-shell-nonascii: PASS=%d FAIL=%d" % (PASS, FAIL))
sys.exit(1 if FAIL else 0)
