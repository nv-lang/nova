# -*- coding: utf-8 -*-
"""scripts/guards/check-lint-rule-covered.py — правило линта обязано хоть раз
доказать, что умеет сработать.

Адрес: реестр 221.1 №1114 (родня №1112 — `W_FFI_BARE_HANDLE`), дом починки —
план 288 Ф.1.

ЗАЧЕМ. Правило, зарегистрированное в `lints.rs`, зовущееся при `nova lint` и
печатающееся в `--list-rules`, но не названное НИ ОДНОЙ фикстурой, ничем не
отличимо от правила исправного: ноль находок у мёртвого и ноль у живого — это
один и тот же ноль. Замер 2026-09-15 нашёл такое состояние у тринадцати правил
сразу; `W_FFI_BARE_HANDLE` учил канону `§4а` и при этом не срабатывал даже на
файле, где его собственное условие выполнено дословно, — и никто не заметил,
потому что замечать было нечем. Это ровно Г19 конвенций: измеритель обязан
различать мир с предметом и без него.

ЧТО ПРОВЕРЯЕТСЯ: у каждого `W_*`, объявленного строковым литералом в
`compiler-codegen/src/lints.rs`, имя встречается хотя бы в одном файле
`spec_tests/`.

ЧЕГО ЭТОТ СТРАЖ НЕ ПРОВЕРЯЕТ, И ЭТО СКАЗАНО, А НЕ УМОЛЧАНО. Свойство целиком
звучит «фикстура НАЗЫВАЕТ правило И правило на ней СРАБАТЫВАЕТ». Здесь судится
только первая половина: вторая требует запуска линта, а текстовый ярус его не
поднимает. Вторая половина НЕ брошена — её держит сверка СОСТАВА в
`scripts/gate.sh` (`CONV_POS_RULES`): `conv_pos.nv` обязан давать ровно
названный набор правил, так что правило, переставшее срабатывать, красит гейт
составом. Страж без этой оговорки обещал бы больше, чем проверяет.

НО ССЫЛКА НА ГЕЙТ ПОКРЫВАЕТ НЕ ВСЕХ, И ЧИСЛО ПЕЧАТАЕТСЯ, А НЕ ПОДРАЗУМЕВАЕТСЯ.
`CONV_POS_RULES` держит срабатывание только для правил семейства conv, попавших
в `conv_pos.nv`. Сколько это от общего числа — страж СЧИТАЕТ САМ, читая тот же
`gate.sh`, и печатает в вердикте («срабатывание доказано сверкой состава для N
из M»). Рукописного числа здесь нет намеренно: оно разошлось бы с гейтом на
первой же правке списка, а ссылка «вторую половину держит гейт» стала бы
покрывать собой больше, чем покрывает, — ровно тот дефект, который страж ловит.
Для M−N правил вторая половина НЕ держится НИЧЕМ.

КОНТРОЛЬ НЕ ДОЛЖЕН НАЗЫВАТЬ ПРАВИЛО ПО ИМЕНИ — правило авторства фикстур,
найденное пробой в обе стороны 2026-09-15 и стоившее этому стражу ложного
зелёного. Первая редакция упоминала `W_FFI_BARE_HANDLE` и в позитиве
(`conv_pos.nv`), и в КОНТРОЛЕ (`conv_clean.nv`, где правило обязано МОЛЧАТЬ).
Проба сняла имя из позитива — страж остался ЗЕЛЁН, потому что засчитал
упоминание в контроле. То есть страж мерил не то: фикстура, доказывающая
молчание, выдавала себя за фикстуру, доказывающую срабатывание, — ровно та
болезнь, которую он судит. Лечится в корпусе, а не здесь: контроль описывает
канон словами и НЕ называет имя правила. После правки проба идёт как надо:
14 -> сломано 15 с именем в отказе -> восстановлено 14.

ВЕРХНЯЯ ГРАНИЦА, А НЕ ПЕРЕПИСЬ (оговорка замера, перенесена дословно): ноль
здесь значит, что ИМЯ правила не встречается в `spec_tests`. Фикстура, которая
правило ТРОГАЕТ, не называя, в счёт не попадёт — значит число непокрытых есть
ВЕРХНЯЯ граница. Встретив такую фикстуру, её надо пометить именем правила, а не
занижать базу: база, опущенная без фикстуры, снимает долг, не закрыв его.

ВТОРАЯ СЛЕПАЯ ЗОНА, СИММЕТРИЧНАЯ ПЕРВОЙ И ХУДШАЯ ИЗ ДВУХ: страж не отличает
«правило не покрыто» от «фикстура есть, но правило на ней МОЛЧИТ». Второе хуже,
потому что выглядит покрытием — в реестре фикстура числится, имя названо, долг
списан, — и сегодня не ловится НИЧЕМ, кроме сверки состава для тех N правил,
что попали в `conv_pos.nv`. Названа здесь, хотя и не закрыта: неназванная
слепая зона со временем становится обещанием.

ДВА РАЗНЫХ СОСТОЯНИЯ СЧИТАЮТСЯ ОТДЕЛЬНО, потому что смешивать их нельзя.
Правило, непокрытое фикстурой, но встречающееся в `std/` строкой `nova:allow`,
КЕМ-ТО ПОЛУЧЕНО и подавлено — значит срабатывать оно умеет, и это долг покрытия.
Правило, не показавшее ничего нигде, не доказало даже этого. Замер 2026-09-15:
пятнадцать непокрытых, из них два первого рода (`W_TRY_WITHOUT_SIBLING`,
`W_PARAM_NO_CONTRACT`) и тринадцать второго.

База scripts/guards/lint-rule-covered.baseline, ключи `uncovered=N` и
`judged=N`; `uncovered` только вниз. `judged` — размер мишени: если правил под
судом стало вдвое меньше базы, это отказ, а не успех (№911).

Самотест: scripts/guards/selftest/test-check-lint-rule-covered.sh (шесть случаев).

$1 — корень репозитория.
"""
import io
import os
import re
import sys
import time

sys.stdout.reconfigure(encoding="utf-8", errors="replace", newline="\n")
sys.stderr.reconfigure(encoding="utf-8", errors="replace", newline="\n")

NAME = "check-lint-rule-covered"
LINTS_REL = "compiler-codegen/src/lints.rs"
CORPUS_REL = "spec_tests"
STD_REL = "std"
BASE_REL = "scripts/guards/lint-rule-covered.baseline"

# ПРАВИЛО СЧЁТА — МЕСТА ВЫДАЧИ (`rule: "W_..."`), а не любой литерал `"W_..."`.
# Разница измерена и содержательна: любой литерал даёт 43, места выдачи — 42.
# Лишним оказался `W_PARAM_TYPE_POS_MUT`, который встречается ТОЛЬКО в
# утверждении юнит-теста `!ws.iter().any(|w| w.rule == ...)` — правило СНЯТО
# (решение владельца 2026-07-17), и тест законно стережёт его отсутствие.
# Судить снятое правило за отсутствие фикстуры значило бы требовать фикстуру на
# то, чего компилятор не печатает. Поле `id:` (реестр ConvRule, 35 штук) в
# знаменатель тоже не годится: оно уже множества выдаваемых правил, и семь
# правил других семейств выпали бы из-под суда молча.
RE_RULE_DECL = re.compile(r'rule:\s*"(W_[A-Z0-9_]+)"')
GATE_REL = "scripts/gate.sh"
RE_CONV_POS = re.compile(r'^CONV_POS_RULES="([^"]*)"', re.M)
RE_UNCOVERED = re.compile(r"^uncovered=(\d+)\s*$", re.M)
RE_JUDGED = re.compile(r"^judged=(\d+)\s*$", re.M)


def read(path):
    try:
        return io.open(path, encoding="utf-8", errors="replace").read()
    except IOError:
        return ""


def walk_text(root, exts):
    for dirpath, dirnames, filenames in os.walk(root):
        dirnames[:] = [d for d in dirnames if d not in ("target", ".git")]
        for fn in filenames:
            if fn.endswith(exts):
                yield os.path.join(dirpath, fn)


def fail(msg):
    sys.stderr.write("%s: FAIL - %s\n" % (NAME, msg))
    return 1


def main():
    t0 = time.time()
    if len(sys.argv) < 2:
        return fail("нужен корень репозитория первым аргументом")
    root = os.path.abspath(sys.argv[1])

    lints = os.path.join(root, LINTS_REL)
    if not os.path.isfile(lints):
        return fail("нет %s - судить нечего, мишень потеряна" % LINTS_REL)
    rules = sorted(set(RE_RULE_DECL.findall(read(lints))))
    if not rules:
        return fail("в %s не объявлено ни одного правила `W_*` - мишень "
                    "потеряна, зелёный ноль здесь был бы ложью" % LINTS_REL)

    corpus = os.path.join(root, CORPUS_REL)
    named = set()
    for fp in walk_text(corpus, (".nv", ".md", ".txt")):
        text = read(fp)
        for r in rules:
            if r not in named and r in text:
                named.add(r)

    # Второе состояние: правило подавлено в std строкой `nova:allow` - значит
    # кто-то его ПОЛУЧИЛ, и срабатывать оно умеет.
    suppressed = set()
    stdroot = os.path.join(root, STD_REL)
    for fp in walk_text(stdroot, (".nv",)):
        text = read(fp)
        if "nova:allow" not in text:
            continue
        for line in text.splitlines():
            if "nova:allow" not in line:
                continue
            for r in rules:
                if r in line:
                    suppressed.add(r)

    # Сколько правил имеют доказанное СРАБАТЫВАНИЕ: список сверки состава
    # читается из самого gate.sh, а не переписывается сюда — рукописная копия
    # разошлась бы с гейтом на первой правке и стала бы обещать покрытие,
    # которого нет.
    m_cp = RE_CONV_POS.search(read(os.path.join(root, GATE_REL)))
    fires_proven = sorted(set(m_cp.group(1).split()) & set(rules)) if m_cp else []

    uncovered = [r for r in rules if r not in named]
    alive = sorted(r for r in uncovered if r in suppressed)
    silent = sorted(r for r in uncovered if r not in suppressed)

    base_t = read(os.path.join(root, BASE_REL))
    if not base_t:
        return fail("нет базы %s (ключи uncovered=N и judged=N) - храповик "
                    "судить нечем" % BASE_REL)
    m_u = RE_UNCOVERED.search(base_t)
    m_j = RE_JUDGED.search(base_t)
    if not m_u or not m_j:
        return fail("в базе %s нет строки uncovered=N и/или judged=N - "
                    "храповик судить нечем" % BASE_REL)
    base_uncovered = int(m_u.group(1))
    base_judged = int(m_j.group(1))

    if len(rules) * 2 < base_judged:
        return fail("под судом %d правил, а база помнит %d - мишень потеряна "
                    "больше чем наполовину. «Непокрытых меньше» здесь значило "
                    "бы «сканер ослеп» (№911)" % (len(rules), base_judged))

    if len(uncovered) > base_uncovered:
        sys.stderr.write(
            "%s: FAIL - правил линта без единой фикстуры, называющей их: %d, "
            "база %d (реестр 221.1 №1114). Правило, ни разу не доказавшее, что "
            "умеет сработать, учит канону и ничего не проверяет:\n"
            % (NAME, len(uncovered), base_uncovered))
        for r in silent:
            sys.stderr.write("    %s - не показало НИЧЕГО: ни фикстуры, ни подавления\n" % r)
        for r in alive:
            sys.stderr.write("    %s - подавлено в std (`nova:allow`), значит срабатывать "
                             "умеет; не хватает фикстуры\n" % r)
        sys.stderr.write(
            "  Лечится фикстурой, НАЗЫВАЮЩЕЙ правило, на которой оно срабатывает, "
            "плюс контролем рядом, где оно обязано молчать. Образец - пара\n"
            "  spec_tests/conformance/lint/conv_pos.nv + conv_clean.nv, состав "
            "сверяется `CONV_POS_RULES` в scripts/gate.sh.\n")
        return 1

    tail = ""
    if len(uncovered) < base_uncovered:
        tail = " - база устарела, опусти uncovered до %d тем же коммитом" % len(uncovered)
    sys.stdout.write(
        "%s ok: правил под судом %d, без фикстуры %d (база %d)%s; из них "
        "подавленных в std %d, не показавших ничего %d. СРАБАТЫВАНИЕ доказано "
        "сверкой состава для %d из %d — для остальных %d вторая половина "
        "свойства не держится ничем; за %.1fс\n"
        % (NAME, len(rules), len(uncovered), base_uncovered, tail,
           len(alive), len(silent), len(fires_proven), len(rules),
           len(rules) - len(fires_proven), time.time() - t0))
    return 0


if __name__ == "__main__":
    sys.exit(main())
