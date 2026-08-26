# -*- coding: utf-8 -*-
"""scripts/claude-hooks/inject-after-compact.py — то, что нельзя потерять при сжатии
контекста, приезжает в новое окно ПОСЛЕ СЖАТИЯ (заведён 2026-08-26 как
inject-flow-delegate.py, в тот же день обобщён до списка).

ЗАЧЕМ. Правила режима работы (`flow`, `delegate`) приезжают импортом из `/next` и
из корневого `CLAUDE.md` — оба пути подают текст при ЗАГРУЗКЕ окна. Сессии
владельца живут максимально долго, и окно в них рождается СЖАТИЕМ, а не
открытием сессии; сводка сжатия текст команд не сохраняет. Замер 2026-08-26:
после сжатия окно 274 сделало целую волну без единого агента.

ЧТО ДЕЛАЕТ. Хук `SessionStart` с матчером `compact` (см. .claude/settings.json):
без аргументов печатает в stdout тела файлов из списка `.claude/after-compact.list`
(YAML-шапка команд снимается, `$ARGUMENTS` заменяется словами) — Claude Code
добавляет stdout такого хука в контекст нового окна. Механизм, не приказ: не
зависит ни от вызова `/next`, ни от того, что уцелело в сводке.

РЕЖИМЫ (для команды `/after-compact`):
  (нет аргументов)   печатать то, что приедет в окно — ровно то, что делает хук;
  --list             таблица списка: путь, байты, есть ли файл; итог;
  --add <путь>       добавить файл в список (путь от корня репозитория; файла нет —
                     отказ с кодом 1; дубль — не добавляется, код 0).

СПИСОК. `.claude/after-compact.list`: одна строка — один путь от корня, пустые
строки и `#`-комментарии пропускаются. Файл из списка, которого нет на диске, —
строка в stderr и код 0: хук не должен ломать старт окна из-за переименования.

Цена: stdout хука ложится в контекст ЦЕЛИКОМ и в каждом окне после сжатия —
`--list` показывает байты именно поэтому.
"""
import io
import os
import sys

ROOT = os.environ.get("CLAUDE_PROJECT_DIR") or os.path.dirname(
    os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
LIST_REL = os.path.join(".claude", "after-compact.list")
HEADER = (u"# Режим работы после сжатия контекста (хук inject-after-compact; "
          u"список — .claude/after-compact.list, команда /after-compact)")


def out(text):
    data = text.encode("utf-8")
    if hasattr(sys.stdout, "buffer"):
        sys.stdout.buffer.write(data)
    else:
        sys.stdout.write(data)


def err(text):
    data = (text + u"\n").encode("utf-8")
    if hasattr(sys.stderr, "buffer"):
        sys.stderr.buffer.write(data)
    else:
        sys.stderr.write(data)


def list_path(root):
    return os.path.join(root, LIST_REL)


def entries(root):
    p = list_path(root)
    if not os.path.isfile(p):
        return []
    res = []
    for line in io.open(p, encoding="utf-8").read().split("\n"):
        s = line.strip()
        if not s or s.startswith("#"):
            continue
        res.append(s)
    return res


def body_without_frontmatter(text):
    lines = text.split("\n")
    if lines and lines[0].strip() == "---":
        for i in range(1, len(lines)):
            if lines[i].strip() == "---":
                return "\n".join(lines[i + 1:]).strip("\n")
    return text.strip("\n")


def read_entry(root, rel):
    path = os.path.join(root, rel.replace("/", os.sep))
    try:
        return io.open(path, encoding="utf-8").read()
    except (IOError, OSError):
        return None


def inject(root):
    parts = [HEADER]
    for rel in entries(root):
        text = read_entry(root, rel)
        if text is None:
            err(u"inject-after-compact: нет файла %s — пропущен" % rel)
            continue
        body = body_without_frontmatter(text).replace(u"$ARGUMENTS", u"(текущая очередь)")
        parts.append(u"\n## %s\n\n%s" % (rel, body))
    out(u"\n".join(parts) + u"\n")
    return 0


def do_list(root):
    rows = entries(root)
    if not rows:
        out(u"after-compact: список пуст или отсутствует (%s)\n" % LIST_REL)
        return 0
    total = 0
    lines = [u"%-48s %8s  %s" % (u"путь", u"байт", u"есть")]
    for rel in rows:
        text = read_entry(root, rel)
        if text is None:
            lines.append(u"%-48s %8s  %s" % (rel, u"-", u"НЕТ ФАЙЛА"))
            continue
        n = len(body_without_frontmatter(text).encode("utf-8"))
        total += n
        lines.append(u"%-48s %8d  %s" % (rel, n, u"да"))
    lines.append(u"итого в контекст после каждого сжатия: %d байт, файлов %d" % (total, len(rows)))
    out(u"\n".join(lines) + u"\n")
    return 0


def do_add(root, arg):
    abs_path = os.path.abspath(arg if os.path.isabs(arg) else os.path.join(root, arg))
    if not os.path.isfile(abs_path):
        err(u"after-compact: файла нет — %s (путь от корня репозитория или абсолютный)" % arg)
        return 1
    rel = os.path.relpath(abs_path, root).replace(os.sep, "/")
    if rel.startswith(".."):
        err(u"after-compact: файл вне репозитория — %s; хук читает только своё дерево" % rel)
        return 1
    rows = entries(root)
    if rel in rows:
        out(u"after-compact: уже в списке — %s\n" % rel)
        return 0
    p = list_path(root)
    existing = io.open(p, encoding="utf-8").read() if os.path.isfile(p) else u""
    if existing and not existing.endswith("\n"):
        existing += u"\n"
    io.open(p, "w", encoding="utf-8", newline="\n").write(existing + rel + u"\n")
    out(u"after-compact: добавлен — %s\n" % rel)
    return do_list(root)


def main(argv):
    if not argv:
        return inject(ROOT)
    if argv[0] == "--list":
        return do_list(ROOT)
    if argv[0] == "--add" and len(argv) == 2:
        return do_add(ROOT, argv[1])
    err(u"usage: inject-after-compact.py [--list | --add <path>]")
    return 2


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
