# -*- coding: utf-8 -*-
# Проверка нового правила перехватчика: сообщение коммита с обратным апострофом
# через -m должно отвергаться, а -F и обычный текст — проходить.
import json, subprocess, sys

H = r"d:\Sources\nv-lang\nova\scripts\claude-hooks\guard-git.py"
BT = chr(96)  # обратный апостроф — сам через переменную, чтобы не подставить его

cases = [
    ('git -C /r commit -m "text with ' + BT + 'mut sender' + BT + ' inside"', 2, "backtick via -m"),
    ('git -C /r commit -F /tmp/msg.txt', 0, "-F is fine"),
    ('git -C /r commit -m "plain text, no backticks"', 0, "plain -m is fine"),
    ('echo "' + BT + 'date' + BT + '"', 0, "not a commit at all"),
    ('git -C /r commit -m "one" && echo ' + BT + 'x' + BT, 2, "backtick after -m in chain"),
]

bad = 0
for cmd, want, label in cases:
    p = subprocess.run([sys.executable, H],
                       input=json.dumps({"tool_input": {"command": cmd}}),
                       capture_output=True, text=True)
    got = p.returncode
    ok = (got == want)
    if not ok:
        bad += 1
    print("  %-7s %-26s want=%d got=%d" % ("ok" if ok else "PROVAL", label, want, got))

print("PROVALOV: %d" % bad)
sys.exit(1 if bad else 0)
