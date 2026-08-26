# -*- coding: utf-8 -*-
"""scripts/claude-hooks/inject-flow-delegate.py — режим работы приезжает в окно
контекста ПОСЛЕ СЖАТИЯ, а не только при открытии сессии (заведён 2026-08-26).

ЗАЧЕМ. Правила режима работы — `flow` («сделал → доложил → сделал») и `delegate`
(механику — дешёвым агентам) — лежат в `.claude/commands/` и приезжают двумя
путями: импортом из `/next` и импортом из корневого `CLAUDE.md`. Оба пути
подают текст при ЗАГРУЗКЕ окна контекста. Сессии владельца живут максимально
долго, и окно в такой сессии рождается не открытием сессии, а СЖАТИЕМ; замер
2026-08-26: после сжатия окно 274 сделало целую волну без единого агента, потому
что текст `/next` вместе с импортами в сводку не попал.

ЧТО ДЕЛАЕТ. Хук `SessionStart` с матчером `compact`: печатает в stdout тела
обоих файлов без YAML-шапки — Claude Code добавляет stdout такого хука в
контекст нового окна. Это механизм, а не приказ: он не зависит ни от того,
вызвана ли `/next`, ни от того, что уцелело в сводке.

ЧЕГО НЕ ДЕЛАЕТ. Не судит и не запрещает — только подаёт текст. Не запускается
на обычных ходах (только на событии сжатия), поэтому цена — один раз на окно.

Проверка руками: `python scripts/claude-hooks/inject-flow-delegate.py` печатает
оба тела и завершается нулём; отсутствующий файл — сообщение в stderr и код 0,
чтобы хук не ломал старт окна из-за переименованной команды.
"""
import io
import os
import sys

ROOT = os.environ.get("CLAUDE_PROJECT_DIR") or os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
FILES = ("flow.md", "delegate.md")


def body_without_frontmatter(text):
    lines = text.split("\n")
    if lines and lines[0].strip() == "---":
        for i in range(1, len(lines)):
            if lines[i].strip() == "---":
                return "\n".join(lines[i + 1:]).strip("\n")
    return text.strip("\n")


def main():
    out = [u"# Режим работы после сжатия контекста (хук inject-flow-delegate; те же правила, "
           u"что приезжают через /next и CLAUDE.md)"]
    for name in FILES:
        path = os.path.join(ROOT, ".claude", "commands", name)
        try:
            text = io.open(path, encoding="utf-8").read()
        except (IOError, OSError) as e:
            sys.stderr.write(u"inject-flow-delegate: нет %s (%s)\n" % (path, e))
            continue
        out.append(u"\n## %s\n\n%s" % (name, body_without_frontmatter(text).replace(u"$ARGUMENTS", u"(текущая очередь)")))
    text = u"\n".join(out) + u"\n"
    if hasattr(sys.stdout, "buffer"):
        sys.stdout.buffer.write(text.encode("utf-8"))
    else:
        sys.stdout.write(text.encode("utf-8"))
    return 0


if __name__ == "__main__":
    sys.exit(main())
