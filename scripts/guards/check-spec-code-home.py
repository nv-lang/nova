# -*- coding: utf-8 -*-
"""scripts/guards/check-spec-code-home.py -- у кода диагностики один ДОМ в спеке.

ЗАЧЕМ (слово владельца 2026-10-02: «спека должна быть однозначно понятна из любого
места; если D изменяется позже, должна быть ссылка на новый D»; парный страж --
check-spec-amend-places.py, он ловит амендмент в момент коммита, этот -- смотрит
дерево). Код диагностики `E_*`/`W_*` -- естественная метка правила. Названный в
нескольких D-блоках, он говорит, что у правила несколько мест; агент, открывший
старое, получает старое правило, если оно не отсылает к новому.

ПРАВИЛО ДОМА. Для кода, названного в >= 2 D-блоках spec/decisions/*.md, ДОМ --
блок с наибольшим номером среди называющих (новое решение перекрывает старое).
Каждый другой называющий блок обязан содержать `D<дом>` где-либо в тексте либо
сам быть домом. Нарушение -- пара «код блок» (блок без ссылки на дом).
Проверено на настоящем дереве 2026-10-02: E_LIT_OUT_OF_RANGE -> D489 (правило
литералов), E_CONSUME_IN_CONDITION -> D486 (уточнил D34), E_PRIVATE_TYPE_IN_PUBLIC
-> единственный блок D47 (одно место, нарушений нет).

БАЗА scripts/guards/spec-code-home.baseline -- строки «код D<блок>»: нарушения на день
заведения. Храповик вниз: новое нарушение -- отказ; строка, за которой нарушения
больше нет, -- отказ (вычеркни тем же диффом). Починка: добавь в блок указатель
`> **Уточнено [D<дом>](...)**` под заголовком (его же требует страж указателей).

ЧЕСТНАЯ ГРАНИЦА. «Новейший по номеру» -- эвристика: код, упомянутый новым блоком мимоходом
(не как правило), станет домом без права. Тогда строки базы не пишутся вслепую -- блок
получает указатель, он нужен читателю в любом случае. Код в блоках D-номеров,
переиспользованных в разных файлах, считается суммой текстов.
Ноль разобранных блоков или ноль кодов -- отказ (мишень потеряна).

usage: python scripts/guards/check-spec-code-home.py [КОРЕНЬ]
       NOVA_SPEC_CODE_HOME_BASELINE=<файл> -- подменить базу (шов самотеста)
"""
import io
import os
import pathlib
import re
import sys

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
import spec_dblocks as sdb  # noqa: E402  (общий разбор блоков)

sys.stdout.reconfigure(encoding="utf-8", errors="replace", newline="\n")
sys.stderr.reconfigure(encoding="utf-8", errors="replace", newline="\n")

NAME = "check-spec-code-home"
RE_CODE = re.compile(r"(?<![A-Za-z0-9_])[EW]_[A-Z0-9_]*[A-Z0-9](?![A-Za-z0-9_*])")


def violations(root):
    """-> (число блоков, {код: {блоки}}, множество «код D<блок>», {код: дом})."""
    blocks = sdb.block_texts(root)
    where = {}
    for n, text in blocks.items():
        for c in set(RE_CODE.findall(text)):
            where.setdefault(c, set()).add(n)
    viol = set()
    homes = {}
    for c, ns in where.items():
        if len(ns) < 2:
            continue
        home = max(ns)
        homes[c] = home
        pat = re.compile(r"\bD%d\b" % home)
        for n in ns:
            if n != home and not pat.search(blocks[n]):
                viol.add("%s D%d" % (c, n))
    return len(blocks), where, viol, homes


def main():
    root = sys.argv[1] if len(sys.argv) > 1 else str(pathlib.Path(__file__).resolve().parents[2])
    base_path = os.environ.get("NOVA_SPEC_CODE_HOME_BASELINE") or os.path.join(
        os.path.dirname(os.path.abspath(__file__)), "spec-code-home.baseline")
    nblocks, where, viol, homes = violations(root)
    if nblocks == 0 or not where:
        print("%s: FAIL -- разобрано блоков %d, кодов %d: мишень потеряна (spec/decisions "
              "переименован?)" % (NAME, nblocks, len(where)), file=sys.stderr)
        return 1
    base = set()
    if os.path.exists(base_path):
        with io.open(base_path, encoding="utf-8") as f:
            for ln in f:
                ln = ln.strip()
                if ln and not ln.startswith("#"):
                    base.add(ln)
    rc = 0
    for v in sorted(viol - base):
        code, blk = v.split()
        print("%s: FAIL -- %s назван в %s, но дом правила -- D%d (новейший из называющих) и %s на "
              "него не ссылается. Поставь под заголовком %s строку `> **Уточнено [D%d](...)**: ...` "
              "-- или, если дом выбран неверно, скажи интегратору, не вписывай в базу вслепую"
              % (NAME, code, blk, homes[code], blk, blk, homes[code]), file=sys.stderr)
        rc = 1
    for v in sorted(base - viol):
        print("%s: FAIL -- строка базы «%s» больше не нарушение: вычеркни её тем же диффом "
              "(база только убывает)" % (NAME, v), file=sys.stderr)
        rc = 1
    if rc == 0:
        multi = len(homes)
        print("%s ok: блоков %d, кодов %d, в >=2 блоках %d, нарушений %d -- все в базе (%d строк)"
              % (NAME, nblocks, len(where), multi, len(viol), len(base)))
    return rc


if __name__ == "__main__":
    sys.exit(main())
