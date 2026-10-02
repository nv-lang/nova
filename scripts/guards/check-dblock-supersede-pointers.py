# -*- coding: utf-8 -*-
"""scripts/guards/check-dblock-supersede-pointers.py -- D-блок, перекрытый новым,
обязан нести под своим заголовком указатель на новый.

ЗАЧЕМ (слово владельца 2026-10-01). Агент читает первый попавшийся D-блок
спеки; если он устарел и не ссылается на новый, получается неоднозначность:
два блока говорят разное, и ничто не сообщает, какой старше. Образец --
D488: в разделе `### Что заменено` перечислены D326, D228, D473, и под
заголовком каждого стоит строка `> **Уточнено [D488](...)**: ...`.

ПОВОД: D488 родился из плана docs/plans/172.15-value-placement-abi-audit.md и
перекрыл девять старых блоков; указатели в них расставлены вручную тем же
слиянием, а это страж, чтобы следующее перекрытие их не забыло.

ЧТО СУДИТСЯ.
  1. Во всех spec/decisions/*.md ищутся блоки `## Dnnn` с разделом
     `### Что заменено` (или `### Supersedes`); из раздела (до следующего
     `###`/`##`) берутся номера `D\\d+`, кроме номера самого блока.
  2. Для каждого такого Dold ищется заголовок (`## Dold` или `### Dold `) в
     любом файле каталога. Заголовка нет -- отказ (опечатка в разделе).
  3. В первых 8 непустых строках после заголовка Dold должна быть строка,
     начинающаяся с `>` и содержащая `Dnew`. Нет -- отказ.
     Если заголовков Dold несколько, достаточно одного с указателем.

Знаменатель печатается всегда: сколько блоков с разделом и сколько
указателей проверено; ноль осмотренного -- не «чисто».

usage: python scripts/guards/check-dblock-supersede-pointers.py [КОРЕНЬ]
"""
import os
import pathlib
import sys

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
from spec_dblocks import LOOKAHEAD, has_pointer, scan  # noqa: E402  (общий разбор)

NAME = "check-dblock-supersede-pointers"

sys.stdout.reconfigure(encoding="utf-8", errors="replace", newline="\n")
sys.stderr.reconfigure(encoding="utf-8", errors="replace", newline="\n")


def main():
    root = sys.argv[1] if len(sys.argv) > 1 else "."
    files, headings, supers = scan(root)
    blocks = 0
    checked = 0
    bad = []
    for new, rel, line, olds in supers:
        blocks += 1
        for old in sorted({o for o in olds if o != new}, key=int):
            checked += 1
            hs = headings.get(old)
            if not hs:
                bad.append("D%s: заголовок не найден (названо в «Что заменено» D%s, %s:%d) -- опечатка?"
                           % (old, new, rel, line))
                continue
            if not any(has_pointer(lines, ln - 1, new) for (_f, ln, lines) in hs):
                f, ln, _ = hs[0]
                bad.append("D%s (%s:%d) не указывает на D%s" % (old, f, ln, new))

    print("%s: файлов %d, блоков с «Что заменено» %d, указателей проверено %d"
          % (NAME, len(files), blocks, checked))
    if not files:
        print("%s: spec/decisions не найден -- судить нечего (это не «чисто»)" % NAME,
              file=sys.stderr)
        return 1
    if bad:
        for b in bad:
            print("  " + b, file=sys.stderr)
        print("  Поставь под заголовком Dold строку `> **Уточнено [Dnew](...)**: ...` "
              "(в первых %d непустых строках)." % LOOKAHEAD, file=sys.stderr)
        print("%s: FAIL" % NAME, file=sys.stderr)
        return 1
    print("%s ok: блоков с «Что заменено» %d, указателей проверено %d"
          % (NAME, blocks, checked))
    return 0


if __name__ == "__main__":
    sys.exit(main())
