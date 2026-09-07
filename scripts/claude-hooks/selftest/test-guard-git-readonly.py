# -*- coding: utf-8 -*-
# Самотест правила «state-changing без -C»: обе стороны — ловит меняющие и НЕ
# трогает читающие.
#
# Из чего вырос (2026-09-07, реестр 221.1 №TBD). Абзац над правилом обещает:
# «Читающие команды НЕ трогаем... Ловим только те, что МЕНЯЮТ состояние». Замер
# показал, что обещание держалось не везде: `\b` между `merge` и дефисом — тоже
# граница слова, поэтому `merge\b` ловил `merge-base`, `commit\b` —
# `commit-graph`, `checkout\b` — `checkout-index`; `worktree` и `tag` ловились
# вместе со своими читающими под-командами. Шесть форм получали отказ, который
# называл их «state-changing», — страж утверждал то, чего не устанавливал.
#
# Почему самотест ЗАПУСКАЕТ хук, а не вырезает из него регулярку. Первая проба
# доставала паттерн из исходника регуляркой по двум строкам — и замолчала, как
# только правка сделала их три: инструмент измерения тихо стал мерить меньше.
# Подпроцесс судит то, что реально исполнится.
#
# Путь к хуку выводится от расположения теста — литеральный путь к машине автора
# в отслеживаемом скрипте это класс №698 (см. check-no-machine-paths).
import json
import os
import subprocess
import sys

H = os.path.join(os.path.dirname(os.path.abspath(__file__)), os.pardir, "guard-git.py")
R = "/d/Sources/nv-lang/nova"  # дерево без слияния в процессе

cases = [
    # ── ЛОВИМ: меняет состояние, дерево не названо ────────────────────────
    ("git add file.txt", 2, "add bez -C"),
    ("git commit -s --only -- a.md", 2, "commit bez -C"),
    ("git push origin main", 2, "push bez -C"),
    ("git merge origin/main", 2, "merge bez -C"),
    ("git checkout main", 2, "checkout bez -C"),
    ("git switch main", 2, "switch bez -C"),
    ("git reset HEAD~1", 2, "reset bez -C"),
    ("git rm file.txt", 2, "rm bez -C"),
    ("git mv a.md b.md", 2, "mv bez -C"),
    ("git worktree add ../x br", 2, "worktree add bez -C"),
    ("git tag v0.1.0", 2, "tag <name> bez -C"),
    ("git branch -d old", 2, "branch -d bez -C"),

    # ── НЕ ТРОГАЕМ: читающие формы, которые правило ловило до 2026-09-07 ──
    # Каждая — отдельный носитель одного класса: имя меняющей команды есть
    # ПРЕФИКСОМ имени читающей, либо читающая стоит под-командой.
    ("git merge-base --is-ancestor A B", 0, "merge-base (chitaet)"),
    ("git commit-graph verify", 0, "commit-graph (chitaet)"),
    ("git checkout-index --help", 0, "checkout-index (chitaet)"),
    ("git worktree list", 0, "worktree list (chitaet)"),
    ("git worktree list --porcelain", 0, "worktree list --porcelain"),
    ("git tag -l", 0, "tag -l (chitaet)"),
    ("git tag --list", 0, "tag --list (chitaet)"),

    # ── НЕ ТРОГАЕМ: читающие, названные в самом комментарии правила ───────
    ("git status --short", 0, "status"),
    ("git log -1 --format=%h", 0, "log"),
    ("git diff --stat", 0, "diff"),
    ("git show HEAD", 0, "show"),
    ("git branch --list", 0, "branch --list"),
    ("git ls-files", 0, "ls-files"),
    ("git rev-list --count main..HEAD", 0, "rev-list"),

    # ── НЕ ТРОГАЕМ: дерево названо явно ──────────────────────────────────
    ("git -C %s merge origin/main" % R, 0, "merge s -C"),
    ("git -C %s push origin br" % R, 0, "push s -C"),
]

bad = 0
for cmd, want, label in cases:
    p = subprocess.run([sys.executable, H],
                       input=json.dumps({"tool_input": {"command": cmd}}),
                       capture_output=True, text=True)
    got = p.returncode
    ok = got == want
    if not ok:
        bad += 1
    print("  %-7s %-30s want=%d got=%d" % ("ok" if ok else "PROVAL", label, want, got))

print("SLUCHAEV: %d" % len(cases))
print("PROVALOV: %d" % bad)
sys.exit(1 if bad else 0)
