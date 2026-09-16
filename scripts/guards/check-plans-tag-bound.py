# -*- coding: utf-8 -*-
"""scripts/guards/check-plans-tag-bound.py — план, чей срок держался на теге
оракула, обязан получить ВЕРДИКТ по лестнице версий Карины.

Адрес: план 274 (раздел «Лестница версий»), подплан 274.11 пункт 9.

ЗАЧЕМ. Решение владельца 2026-09-15: оракул в релиз не выходит, тега v0.1 не
будет. Вместе с тегом исчезло СОБЫТИЕ, к которому были привязаны сроки трёх
десятков планов, — но сами фразы «до тега» / «после тега» остались в их телах и
читаются сегодня как живая зависимость от того, чего не случится. Решение
владельца 2026-09-16 дало им новый адрес: лестница 0.2 -> 0.3 -> 1.0.

Пока у плана нет вердикта, он висит в положении, которое НЕЛЬЗЯ ОТЛИЧИТЬ от
трёх разных: ждёт 1.0 · является исключением и делается сейчас · уже исполнен, а
фраза осталась летописью. Этот страж считает такие планы и не даёт их числу
расти.

ВЕРДИКТ ИЩЕТСЯ В СТАТУС-БЛОКЕ ПЛАНА, А НЕ В ТАБЛИЦЕ ГДЕ-ТО ЕЩЁ. Второй дом
статуса разошёлся бы с первым на первой правке — так протухли обе копии счёта
стражей в `AGENTS.md`. Поэтому одно из четырёх слов обязано стоять в самом
файле плана:

    ЖДЁТ 1.0        — работа назначена на 1.0, до 0.3 не начинается;
    ИСКЛЮЧЕНИЕ 0.2  — без неё не берётся ступень 0.2 (или не делать дороже);
    ИСКЛЮЧЕНИЕ 0.3  — то же про 0.3;
    ЛЕТОПИСЬ ТЕГА   — зависимости нет, фраза осталась текстом о прошлом.

ЧЕГО НЕ ПРОВЕРЯЕТ (сказано честно): что вердикт ВЕРЕН. Машина видит слово,
человек — обоснованность; «ИСКЛЮЧЕНИЕ 0.2» без причины пройдёт этого стража и
не пройдёт владельца.

ПОЧЕМУ БАЗА, А НЕ НОЛЬ. Натравленный на историю страж с нулём покраснел бы на
42 файлах разом и был бы отключён первым же окном, которому помешал. База ходит
ТОЛЬКО ВНИЗ: рост — FAIL, падение — обязанность опустить базу тем же слиянием.

ЗАМЕР В ЧАС ЗАВЕДЕНИЯ (2026-09-16): кандидатов 42, без вердикта 42.

$1 — корень репозитория.
"""
import glob
import io
import os
import re
import sys

NAME = "check-plans-tag-bound"

# Фраза, по которой план считается тег-зависимым.
TAG_RE = re.compile(u"(?:до|после|к)\\s+"
                    u"тег[ауе]", re.IGNORECASE)

# Не ожидающие планы, а сам предмет: закрытый план выпуска оракула с его
# реестром, файлы Карины (они пишут о теге по своей работе) и закрытый 278,
# где упоминания — летопись охоты. Список ЯВНЫЙ: молчаливое исключение по
# маске однажды спрячет настоящего кандидата.
SUBJECT = {
    "221-release-v0-1.md",
    "221.1-bug-sweep.md",
    "274-novac-self-hosted-compiler.md",
    "274.5-read-own-source.md",
    "274.11-carina-release.md",
    "274.12-novac-divergences.md",
    "278-defect-hunter-agent.md",
}

VERDICTS = (
    u"ЖДЁТ 1.0",
    u"ИСКЛЮЧЕНИЕ 0.2",
    u"ИСКЛЮЧЕНИЕ 0.3",
    u"ЛЕТОПИСЬ ТЕГА",
)


def read(path):
    return io.open(path, encoding="utf-8", errors="replace").read()


def baseline_value(path, key):
    """Значение = РОВНО одна строка `key=`; первая выигрывает (конвенция баз)."""
    if not os.path.exists(path):
        return None
    for line in read(path).splitlines():
        line = line.strip()
        if line.startswith("#") or "=" not in line:
            continue
        k, _, v = line.partition("=")
        if k.strip() == key:
            try:
                return int(v.strip())
            except ValueError:
                return None
    return None


def main():
    root = sys.argv[1] if len(sys.argv) > 1 else "."
    plans = os.path.join(root, "docs", "plans")
    if not os.path.isdir(plans):
        print("%s: FAIL - net kataloga %s" % (NAME, plans), file=sys.stderr)
        return 1

    candidates, without = [], []
    for path in sorted(glob.glob(os.path.join(plans, "*.md"))):
        name = os.path.basename(path)
        if not re.match(r"^\d", name) or name in SUBJECT:
            continue
        text = read(path)
        if not TAG_RE.search(text):
            continue
        candidates.append(name)
        if not any(v in text for v in VERDICTS):
            without.append(name)

    base_path = os.path.join(root, "scripts", "guards", "plans-tag-bound.baseline")
    base = baseline_value(base_path, "no_verdict")
    if base is None:
        print("%s: FAIL - v baze net stroki no_verdict= (%s)" % (NAME, base_path),
              file=sys.stderr)
        return 1

    print("%s: kandidatov %d, bez verdikta %d (baza %d)"
          % (NAME, len(candidates), len(without), base))

    if len(without) > base:
        print("%s: NARUSHENIE - planov bez verdikta stalo bol'she:" % NAME,
              file=sys.stderr)
        for n in without:
            print("    %s" % n, file=sys.stderr)
        print("", file=sys.stderr)
        print("    Plan, ssylayushchiysya na 'do tega', bez verdikta nerazlichim:",
              file=sys.stderr)
        print("    zhdet 1.0 / isklyuchenie / letopis'. Postav' odno iz chetyreh slov",
              file=sys.stderr)
        print("    v status-blok plana: ZhDET 1.0 | ISKLYuChENIE 0.2 | ISKLYuChENIE 0.3",
              file=sys.stderr)
        print("    | LETOPIS' TEGA (kirillicej). Lestnica - plan 274.", file=sys.stderr)
        print("%s: FAIL" % NAME, file=sys.stderr)
        return 1

    if len(without) < base:
        print("%s: FAIL - bez verdikta %d < bazy %d: opusti bazu tem zhe sliyaniem"
              % (NAME, len(without), base), file=sys.stderr)
        print("    Baza, ostavlennaya vysokoj, molcha razreshaet otkatit'sya rovno",
              file=sys.stderr)
        print("    na stol'ko, na skol'ko ty prodvinulsya: %s" % base_path,
              file=sys.stderr)
        return 1

    print("%s ok: rost planov bez verdikta ne dopushchen" % NAME)
    return 0


if __name__ == "__main__":
    sys.exit(main())
