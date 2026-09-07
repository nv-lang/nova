# -*- coding: utf-8 -*-
"""scripts/guards/check-plan-status-closure-leads.py — строка статуса, которую читает
машина, не должна ПРОТИВОРЕЧИТЬ телу своего же абзаца (заведён 2026-09-07).

Дом правила: `docs/dev/dev-workflow.md` (статус живёт в самом плане, сводка
генерируется). Маркер: `[M-plan-status-closure-must-lead-the-line]` в
`docs/plans/backlog-followups.md`.

ЗАЧЕМ. Абзац статуса читается человеком ЦЕЛИКОМ, а машиной — ОДНОЙ СТРОКОЙ:
`scripts/tools/gen-plan-status.sh` берёт `grep -m1` по маркеру и кладёт в сводку
`docs/plans/STATUS.md` ровно её. Значит закрытие, дописанное в конец абзаца,
закрывает работу только для людей, а всякий, кто читает сводку (её и читают
затем, чтобы не открывать двадцать планов), видит незакрытое.

ЗАМЕР ДНЯ ЗАВЕДЕНИЯ. `docs/plans/283.1-float-parse-own-module.md`: первая
строка абзаца — «ЗАПЛАНИРОВАН 2026-09-07», а «ВОЛНА ЗАКРЫТА 2026-09-07» стояла
ШЕСТОЙ строкой того же абзаца. Волна была сделана, принята и влита — сводка
сообщала бы «ЗАПЛАНИРОВАН».

ПОЧЕМУ ПРАВИЛО ИМЕННО ПРО ПРОТИВОРЕЧИЕ, А НЕ ПРО «ЗАКРЫТИЕ ПЕРВЫМ СЛОВОМ».
Наивная форма («если слово закрытия есть в абзаце — оно обязано быть в первой
строке») была написана первой и ОТВЕРГНУТА ЗАМЕРОМ в тот же час: на дереве она
дала шесть срабатываний, из которых ПЯТЬ законные и одно настоящее.
Законные — это ровно та проза, ради которой абзац и пишут:
  * `173.4` — «✅ ЗАКРЫТ 2026-07-10» относится к РОДИТЕЛЬСКОМУ плану 173;
  * `277` и `283` — закрыты и приняты ФАЗЫ, сам план в работе;
  * `233` — план закрыт, и первая строка это говорит, но словом «ИСПОЛНЕН»,
    которого в словаре закрытий не было.
Страж, краснеющий на пяти здоровых из шести, снимается первым же окном, и
правило умирает вместе с ним. Поэтому судится УЗКОЕ и однозначное: первая
строка объявляет работу НЕ НАЧАТОЙ, а ниже в том же абзаце сказано, что она
закрыта. Это не вопрос вкуса — это самопротиворечие, и оно всегда дефект.

ЧЕГО СТРАЖ НЕ ЛОВИТ, И ЭТО НАЗВАНО НАРОЧНО: план «в работе», чьё закрытие
дописано в конец абзаца. Там противоречия нет («в работе» → «закрыта» это
законная хроника), и отличить закрытие ПЛАНА от закрытия ФАЗЫ машинно нечем.
Эта часть маркера остаётся открытой; закроет её либо соглашение о форме
(закрытие плана пишется первым словом), либо структурное поле вместо прозы.

ИСПОЛЬЗОВАНИЕ:
    python scripts/guards/check-plan-status-closure-leads.py [КОРЕНЬ]
"""
import io
import os
import re
import sys

NAME = "check-plan-status-closure-leads"
MARK = u"**Статус:**"

# Первая строка объявляет работу НЕ НАЧАТОЙ.
NOT_STARTED = re.compile(
    u"ЗАПЛАНИРОВАН[А]?\\b"
    u"|НЕ НАЧАТ[АО]?\\b"
    u"|ПРЕДЛОЖЕН[А]?\\b"
    u"|ЧЕРНОВИК\\b"
    u"|ОЖИДАЕТ (?:РЕШЕНИЯ|СЛОВА)\\b"
)
# Тело абзаца утверждает, что работа закрыта.
CLOSED = re.compile(
    u"ЗАКРЫТ[АО]?\\b"
    u"|СДАН[А]?\\b"
    u"|ПРИНЯТ[АО]?\\b"
    u"|ЗАВЕРШЁ?Н[АО]?\\b"
    u"|ИСПОЛНЕН[А]?\\b"
)


def status_paragraph(lines):
    """Абзац статуса: от первой строки с маркером до пустой строки.

    Первой считается ровно та строка, которую берёт генератор (`grep -m1`), —
    иначе страж судил бы не то, что попадает в сводку.
    """
    idx = next((i for i, l in enumerate(lines) if MARK in l), None)
    if idx is None:
        return None
    para = []
    for line in lines[idx:]:
        if line.strip() == "":
            break
        para.append(line)
    return para


def scan(root):
    plans_dir = os.path.join(root, "docs", "plans")
    seen = 0
    bad = []
    for base, _dirs, files in os.walk(plans_dir):
        for fname in sorted(files):
            if not fname.endswith(".md"):
                continue
            path = os.path.join(base, fname)
            try:
                text = io.open(path, encoding="utf-8", errors="replace").read()
            except IOError:
                continue
            para = status_paragraph(text.split("\n"))
            if not para:
                continue
            seen += 1
            if len(para) < 2:
                continue
            head, tail = para[0], "\n".join(para[1:])
            if NOT_STARTED.search(head) and CLOSED.search(tail):
                rel = os.path.relpath(path, root).replace("\\", "/")
                bad.append((rel, NOT_STARTED.search(head).group(0),
                            CLOSED.search(tail).group(0)))
    return seen, bad


def main(argv):
    root = argv[1] if len(argv) > 1 else os.path.join(
        os.path.dirname(os.path.abspath(__file__)), "..", "..")
    root = os.path.abspath(root)
    seen, bad = scan(root)
    if bad:
        sys.stderr.write(
            u"%s: FAIL — строка статуса противоречит своему же абзацу: %d\n" % (NAME, len(bad)))
        for rel, started, closed in bad:
            sys.stderr.write(
                u"    %s: машине видно «%s», а ниже в абзаце — «%s»\n" % (rel, started, closed))
        sys.stderr.write(
            u"    Сводка `docs/plans/STATUS.md` собирается по ОДНОЙ строке\n"
            u"    (`gen-plan-status.sh`, `grep -m1`), поэтому закрытая работа\n"
            u"    будет числиться незапланированной. Перенеси слово закрытия в\n"
            u"    НАЧАЛО абзаца, обоснование оставь следом.\n")
        return 1
    print(u"%s ok: планов со строкой статуса %d, самопротиворечивых 0" % (NAME, seen))
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
