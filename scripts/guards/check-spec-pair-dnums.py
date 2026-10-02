# -*- coding: utf-8 -*-
"""scripts/guards/check-spec-pair-dnums.py -- пара обзорных страниц спеки
`spec/X.md` / `spec/X.ru.md` ссылается на ОДИН И ТОТ ЖЕ набор D-блоков.

ЗАЧЕМ (план docs/plans/278-defect-hunter-agent.md, трек охоты spec, класс С6 —
docs/dev/hunts/spec/LEDGER.md; слово владельца 2026-10-01 через интегратора:
«привести спеку в однозначное, непротиворечивое состояние и держать её такой»).
Нормативна русская страница (`X.ru.md`), английская — её перевод. Перевод,
отставший от оригинала, читается так же уверенно, как свежий: агент, открывший
английскую страницу, не узнает, что правило уже другое. Смысл абзаца страж не
судит — это работа охоты; он судит то, что механизируется без суждения: какие
D-блоки страница называет. Блок, упомянутый в одной половине пары и не
упомянутый в другой, почти всегда значит, что абзац с ним в другую половину не
перенесён. Замер 2026-10-01 при заведении: 5 таких мест в двух парах, все —
английская сторона отстаёт (conversions: D475; syntax: D238, D470, D480, D483).

ЧТО СУДИТСЯ.
  1. Пары: каждый `spec/X.ru.md`, у которого есть `spec/X.md`. Страницы без пары
     (только русские) не судятся и печатаются в знаменателе.
  2. В каждой половине — множество упоминаний `D<число>` (граница слова).
  3. Каждое расхождение `X.md only-ru Dn` / `X.md only-en Dn` обязано стоять
     строкой в базе `scripts/guards/spec-pair-dnums.baseline` — иначе отказ
     (новый рассинхрон). Строка базы, расхождения за которой больше нет, —
     тоже отказ: база только убывает, починенное вычёркивается тем же диффом.
  4. Ноль пар — отказ (мишень потеряна: переименовали страницы — страж обязан
     покраснеть, а не напечатать «чисто»).

Знаменатель печатается всегда: сколько пар, сколько упоминаний, сколько строк
базы.

ЧЕГО НЕ СУДИТ: смысл абзацев (С6 по смыслу — охота); пары в spec/decisions/
(там нет переводов); якоря и ссылки (check-dead-anchors.py).

usage: python scripts/guards/check-spec-pair-dnums.py [КОРЕНЬ]
       NOVA_SPEC_PAIR_BASELINE=<файл> — подменить базу (шов самотеста)
"""
import io
import os
import re
import sys

NAME = "check-spec-pair-dnums"
DREF = re.compile(r"\bD(\d{1,4})\b")

sys.stdout.reconfigure(encoding="utf-8", errors="replace", newline="\n")
sys.stderr.reconfigure(encoding="utf-8", errors="replace", newline="\n")


def refs(path):
    with io.open(path, encoding="utf-8", errors="replace") as f:
        return {int(n) for n in DREF.findall(f.read())}


def main():
    root = sys.argv[1] if len(sys.argv) > 1 else os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "..")
    spec = os.path.join(root, "spec")
    base_path = os.environ.get("NOVA_SPEC_PAIR_BASELINE") or os.path.join(
        os.path.dirname(os.path.abspath(__file__)), "spec-pair-dnums.baseline")
    if not os.path.isdir(spec):
        print(f"{NAME}: FAIL — нет каталога {spec}", file=sys.stderr)
        return 1
    base = set()
    if os.path.exists(base_path):
        with io.open(base_path, encoding="utf-8") as f:
            for ln in f:
                ln = ln.strip()
                if ln and not ln.startswith("#"):
                    base.add(ln)
    pairs, lonely, nrefs, diffs = 0, [], 0, set()
    for fn in sorted(os.listdir(spec)):
        if not fn.endswith(".ru.md"):
            continue
        en = fn[:-len(".ru.md")] + ".md"
        if not os.path.exists(os.path.join(spec, en)):
            lonely.append(fn)
            continue
        pairs += 1
        a, b = refs(os.path.join(spec, en)), refs(os.path.join(spec, fn))
        nrefs += len(a) + len(b)
        diffs |= {f"{en} only-en D{n}" for n in a - b}
        diffs |= {f"{en} only-ru D{n}" for n in b - a}
    rc = 0
    if pairs == 0:
        print(f"{NAME}: FAIL — ни одной пары spec/X.md + spec/X.ru.md: мишень потеряна", file=sys.stderr)
        return 1
    new = sorted(diffs - base)
    stale = sorted(base - diffs)
    for d in new:
        page, side, dn = d.split()
        other = page if side == "only-ru" else page[:-3] + ".ru.md"
        have = page[:-3] + ".ru.md" if side == "only-ru" else page
        print(f"{NAME}: FAIL — spec/{have} называет {dn}, а spec/{other} — нет: абзац с ним в пару не "
              f"перенесён (нормативна .ru.md). Перенеси — или, если расхождение осознанное, впиши строку "
              f"«{d}» в {os.path.basename(base_path)} с причиной", file=sys.stderr)
        rc = 1
    for d in stale:
        print(f"{NAME}: FAIL — строка базы «{d}» больше не расхождение: вычеркни её тем же диффом "
              f"(база только убывает)", file=sys.stderr)
        rc = 1
    if rc == 0:
        print(f"{NAME} ok: пар {pairs} (без пары, не судятся: {len(lonely)}), упоминаний D {nrefs}, "
              f"расхождений {len(diffs)} — все в базе ({len(base)} строк)")
    return rc


if __name__ == "__main__":
    sys.exit(main())
