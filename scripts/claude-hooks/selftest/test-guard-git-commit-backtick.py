# -*- coding: utf-8 -*-
# Проверка нового правила перехватчика: сообщение коммита с обратным апострофом
# через -m должно отвергаться, а -F и обычный текст — проходить.
import json, os, subprocess, sys

# Путь ОТ СЕБЯ, не от машины автора (№765, 2026-08-26). Здесь стоял
# абсолютный windows-путь. На раннере его нет, python выходит с кодом 2
# для КАЖДОГО случая — и два случая, которые ЖДУТ 2, «проходят» по совпадению,
# а три, которые ждут 0, падают. Три ночи красного яруса `full`.
H = os.path.join(os.path.dirname(os.path.dirname(os.path.abspath(__file__))),
                 "guard-git.py")
if not os.path.exists(H):
    sys.stderr.write("самотест: не найден перехватчик %s\n" % H)
    sys.exit(1)
BT = chr(96)  # обратный апостроф — сам через переменную, чтобы не подставить его

cases = [
    ('git -C /r commit -m "text with ' + BT + 'mut sender' + BT + ' inside"', 2, "backtick via -m"),
    # Область (`--only -- <путь>`) добавлена 2026-08-23: с этого дня коммит без
    # названной области отвергает ДРУГОЕ правило того же хука, и без пути эти
    # два случая проверяли бы уже не апострофы, а его.
    ('git -C /r commit -F /tmp/msg.txt --only -- a.md', 0, "-F is fine"),
    ('git -C /r commit -m "plain text, no backticks" --only -- a.md', 0, "plain -m is fine"),
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
