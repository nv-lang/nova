# -*- coding: utf-8 -*-
"""Страж: один и тот же кусок текста не повторяется в двух разделах плана.

ДОМ И ОСНОВАНИЕ: реестр 221.1 №TBD-plan-dup; перенесено 2026-08-12 из
другого проекта владельца по его указанию.

ЗАЧЕМ. Разделы плана отвечают на РАЗНЫЕ вопросы: «Зачем» — почему это дорого,
«Проба» — что показал прогон, «Итог фазы» — что она дала. Когда один и тот же
абзац стоит в двух местах, план начинает врать по частям: правят одно
вхождение, второе остаётся и продолжает утверждать старое. Это тот же класс,
что №612 (запрет прозой против промпта, зовущего обратно) и №594 (реестр
говорит о том, чего в дереве нет) — расхождение двух записей одного факта.

ЧТО ПРОВЕРЯЕТСЯ: внутри ОДНОГО файла плана — предложение длиной от 60 значимых
символов, встретившееся в двух РАЗНЫХ разделах (`## `/`### `). Повтор внутри
одного раздела законен: там это перечисление или таблица.

ЧЕГО НЕ ЛОВИТ (сказано честно): пересказ теми же словами в другом порядке —
а именно так дублирование чаще и возникает. Страж ловит копипасту, то есть
самый дешёвый и самый частый её вид; остальное остаётся на просмотре плана
после фазы.

ПЕРИМЕТР И ИСКЛЮЧЕНИЯ:
  * `docs/plans/*.md` — планы;
  * `README.md` не план;
  * `STATUS.md` автогенерируемый — повторы там свойство генератора;
  * `wip/` — рабочие записки живых волн, они по природе черновые;
  * `221.1-bug-sweep.md` — реестр: его строки ОБЯЗАНЫ нести одинаковые
    формулы («фикс носителя приёмкой не считается»), этого требует другой
    страж, `check-registry-entry-shape.sh`. Ловить его здесь значило бы
    столкнуть два стража лбами.

ИСПОЛЬЗОВАНИЕ:
  python scripts/guards/check-plan-duplication.py [КОРЕНЬ]
Самотест — scripts/guards/selftest/test-check-plan-duplication.sh
"""

import os
import re
import sys

# Поток вердикта — с LF: python на Windows иначе печатает CRLF там, где shell
# печатал LF, и вывод молча расходится с shell-редакцией (правило
# check-guard-honesty, заведено 2026-08-19).
sys.stdout.reconfigure(encoding="utf-8", errors="replace", newline="\n")
sys.stderr.reconfigure(encoding="utf-8", errors="replace", newline="\n")

if hasattr(sys.stdout, "reconfigure"):
    sys.stdout.reconfigure(encoding="utf-8", errors="replace")
if hasattr(sys.stderr, "reconfigure"):
    sys.stderr.reconfigure(encoding="utf-8", errors="replace")

ROOT = sys.argv[1] if len(sys.argv) > 1 else os.path.normpath(
    os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "..")
)
PLANS = os.path.join(ROOT, "docs", "plans")
NOT_A_PLAN = ("README.md", "STATUS.md", "221.1-bug-sweep.md")

MIN_LENGTH = 60
MARKUP = re.compile(u"[`*_>|#\\[\\]()✅⬜\U0001F534\U0001F7E1❌]")

problems = []


def normalize(text):
    text = MARKUP.sub(u" ", text)
    return re.sub(r"\s+", u" ", text).strip().lower()


def sentences(block):
    for raw in re.split(r"(?<=[.!?])\s+|\n", block):
        norm = normalize(raw)
        if len(norm) >= MIN_LENGTH:
            yield norm, raw.strip()


def check(path, name):
    with open(path, encoding="utf-8") as f:
        text = f.read()

    parts = re.split(r"^(#{2,3} .+)$", text, flags=re.M)
    seen = {}
    section = u"(начало файла)"
    for chunk in parts:
        if re.match(r"^#{2,3} ", chunk):
            section = chunk.lstrip("#").strip()
            continue
        for norm, raw in sentences(chunk):
            if norm in seen and seen[norm] != section:
                problems.append(
                    u"docs/plans/%s: один и тот же текст в разделах «%s» и «%s»: «%s…»"
                    % (name, seen[norm], section, raw[:70])
                )
            else:
                seen.setdefault(norm, section)


def main():
    if not os.path.isdir(PLANS):
        print(u"check-plan-duplication ok: каталога docs/plans нет — проверять нечего")
        return 0

    for name in sorted(os.listdir(PLANS)):
        if not name.endswith(".md") or name in NOT_A_PLAN:
            continue
        check(os.path.join(PLANS, name), name)

    # ── Храповик ПОФАЙЛОВЫЙ, а не общим счётчиком ──────────────────────────
    # Первый прогон нашёл 89 повторов в закрытых планах прошлых волн. Чинить их
    # задним числом смысла нет — журналы закрытых волн мы не переписываем; цена
    # стража в том, чтобы НОВЫЙ повтор не появился. Общий счётчик такое прячет:
    # ушёл один, пришёл другой — итог тот же. Поэтому база пофайловая, и растёт
    # ни один файл не может.
    base_path = os.environ.get(
        "NOVA_PLAN_DUP_BASELINE",
        os.path.join(os.path.dirname(os.path.abspath(__file__)), "plan-duplication.baseline"),
    )
    base = {}
    if os.path.isfile(base_path):
        with open(base_path, encoding="utf-8") as f:
            for line in f:
                line = line.strip()
                if not line or line.startswith("#"):
                    continue
                bits = line.split()
                if len(bits) >= 2 and bits[1].isdigit():
                    base[bits[0]] = int(bits[1])

    now = {}
    for p in problems:
        name = p.split(":", 1)[0]
        now[name] = now.get(name, 0) + 1

    grown = []
    for name, n in sorted(now.items()):
        allowed = base.get(name, 0)
        if n > allowed:
            grown.append((name, n, allowed))

    if grown:
        sys.stderr.write(u"check-plan-duplication: дословный повтор между разделами ВЫРОС:\n")
        for name, n, allowed in grown:
            sys.stderr.write(u"    %s: %d > базы %d\n" % (name, n, allowed))
        sys.stderr.write(u"\n")
        for p in problems:
            if any(p.startswith(name) for name, _, _ in grown):
                sys.stderr.write(u"    " + p + u"\n")
        sys.stderr.write(u"\n")
        sys.stderr.write(u"    Разделы отвечают на разные вопросы. Повтор означает,\n")
        sys.stderr.write(u"    что план начнёт врать по частям: правят одно вхождение,\n")
        sys.stderr.write(u"    второе остаётся и продолжает утверждать старое.\n")
        sys.stderr.write(u"check-plan-duplication: FAIL\n")
        return 1

    dropped = sum(base.values()) - sum(now.values())
    if dropped > 0:
        print(u"check-plan-duplication: повторов стало меньше на %d — опусти базу" % dropped)
    print(u"check-plan-duplication ok: роста дословных повторов нет (%d файлов в базе)" % len(base))
    return 0


if __name__ == "__main__":
    sys.exit(main())
