# -*- coding: utf-8 -*-
"""scripts/tools/insert-note.py — вставка текста в документ БЕЗ участия оболочки.

ЗАЧЕМ (требование владельца 2026-08-08: «правила не работают, сделай
автоматическое правило»): интегратор ТРИЖДЫ за сессию покорёжил текст в
`docs/plans/221.1-bug-sweep.md` и `249-*.md`, передавая русский markdown с
обратными апострофами через `python -c "..."` в Bash. Оболочка исполняет
содержимое апострофов как команду и МОЛЧА подставляет её вывод — в логе видно
`nova: command not found`, а скрипт при этом печатает «ok» и коммит проходит.
Записанная в память заметка не помогла: наступил снова через несколько часов.

МЕХАНИЗМ: текст берётся ИЗ ФАЙЛА и никогда не проходит через командную строку.
Апострофы, `$`, кавычки, эмодзи — безопасны по построению.

ИСПОЛЬЗОВАНИЕ:
    python scripts/tools/insert-note.py <документ> <файл-с-текстом> \
        --anchor "<якорь>" [--before|--after] [--marker "<уникальный маркер>"]

  --anchor  — подстрока в документе, относительно которой вставляем.
  --before  — вставить ПЕРЕД якорем (по умолчанию), --after — после.
  --marker  — если задан и уже есть в документе, вставка пропускается
              (идемпотентность: повторный запуск не плодит дубли).

Скрипт САМ печатает самопроверку: вставлено ли, цел ли текст, нет ли дублей.
Верить факту нулевого кода возврата нельзя — читать вывод.
"""
import argparse
import io
import sys


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument('document')
    ap.add_argument('note_file')
    ap.add_argument('--anchor', required=True)
    ap.add_argument('--after', action='store_true')
    ap.add_argument('--marker')
    a = ap.parse_args()

    note = io.open(a.note_file, encoding='utf-8').read()
    doc = io.open(a.document, encoding='utf-8').read()

    if a.marker and a.marker in doc:
        print('SKIP: маркер уже в документе, вставка не нужна')
        return 0

    if a.anchor not in doc:
        print('ОШИБКА: якорь не найден в документе', file=sys.stderr)
        return 1

    i = doc.index(a.anchor)
    pos = i + len(a.anchor) if a.after else i
    doc = doc[:pos] + note + doc[pos:]
    io.open(a.document, 'w', encoding='utf-8').write(doc)

    # ── самопроверка: текст обязан лежать в документе ДОСЛОВНО ──
    back = io.open(a.document, encoding='utf-8').read()
    lines = [l for l in note.split('\n') if l.strip()]
    missing = [l for l in lines if l not in back]
    dup = back.count(a.marker) if a.marker else 0

    print('вставлено строк: %d' % len(lines))
    print('потеряно строк: %d %s' % (len(missing), missing[:2] if missing else ''))
    if a.marker:
        print('вхождений маркера: %d (обязано быть 1)' % dup)
    print('ИТОГ: %s' % ('ok' if not missing and dup <= 1 else 'ПРОВЕРЬ ВРУЧНУЮ'))
    return 0 if not missing else 2


if __name__ == '__main__':
    sys.exit(main())
