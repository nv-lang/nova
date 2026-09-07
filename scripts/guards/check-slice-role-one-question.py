# -*- coding: utf-8 -*-
"""scripts/guards/check-slice-role-one-question.py — на вопрос «удовлетворяет ли
тип роли среза `RangeIndex`» отвечает ОГРАНИЧЕННЫЙ, названный здесь набор мест,
и каждое из них называет ОБА селектора роли.
План: docs/plans/284-str-slice-one-door.md; норма — D470, доктрина канала 196.

ПОЧЕМУ ЭТОТ СТРАЖ ПОЯВИЛСЯ (находка перепроверки, 2026-09-07). Волна 284 Ф.1
сделала признак «умеет срез» ролью, и после неё на один вопрос отвечают ДВА
места: предикат эмиттера `satisfies_range_index_role` (читает `all_methods`) и
отказ чекера `E_SLICE_ROLE_UNSATISFIED` (читает `find_method_decl`). Разные
реестры, один вопрос. Сегодня они согласны, потому что оба выводят одно базовое
имя типа, но ДЕРЖИТСЯ это только вниманием окна — а инвариант на честном слове
запрещён нормой (`docs/dev/conventions-governance.md`). Расхождение было бы
тихим и злым: тип прошёл бы чекер и упал на выпуске, либо наоборот.

ЧТО ЛОВИТ. Строковый литерал `"end_index"` в коде (комментарии сняты) внутри
`compiler-codegen/src/**/*.rs`:
  * в файле, которого нет в списке ниже, — ЧЕТВЁРТАЯ дверь к вопросу, отказ;
  * в файле из списка, где рядом нет литерала `"index"`, — половина роли, отказ:
    роль по D470 всё-или-ничего, и место, спрашивающее один селектор из двух,
    отвечает не на тот вопрос;
  * файл из списка, где литерал исчез вовсе, — отказ: либо дверь переехала
    молча, либо канальный фикс уже сделан и список надо сократить ОСОЗНАННО,
    правкой этого стража с летописью.

ЧЕГО НЕ ЛОВИТ: `end_index` в прозе комментариев и докстрок (это объяснение, не
вопрос), в `.nv`-исходниках std (там роль ОБЪЯВЛЕНА методом — это её дом, а не
второй ответ) и в `lints.rs` требование пары не проверяет смысл: там литерал —
засев достижимости (реестр 221.1 №1011), и он обязан называть оба имени ровно
потому, что оба синтезируются.

УСЛОВИЕ СНЯТИЯ, названное сразу (иначе страж переживёт свою причину): когда
чекер начнёт ПИСАТЬ решение о роли в канал, а эмиттер — читать его вместо
собственного предиката (доктрина 196), мест станет два, и `emit_c.rs` уйдёт из
списка. Тогда этот страж сокращается до одного места и держит уже другое
утверждение — «ответ один».

ПОЧЕМУ PYTHON: старт процесса дороже самой проверки (П14).

$1 — корень репозитория; $2 — override каталога compiler-codegen/src (шов
самотеста).
"""
import pathlib
import re
import sys

sys.stdout.reconfigure(encoding="utf-8", errors="replace", newline="\n")

NAME = "check-slice-role-one-question"

# Место -> зачем оно там. Путь относительно каталога src.
EXPECTED = {
    "codegen/emit_c.rs": "predikat emittera satisfies_range_index_role",
    "types/mod.rs": "otkaz chekera E_SLICE_ROLE_UNSATISFIED",
    "lints.rs": "zasev dostizhimosti DCE (reestr 1011)",
}

RE_LINE_COMMENT = re.compile(r"//.*$")
LIT_END = '"end_index"'
LIT_IDX = '"index"'


def code_lines(path):
    """Строки файла с снятыми строчными комментариями. Блочные комментарии в
    этом дереве в Rust почти не встречаются, а `///` — тот же `//`."""
    out = []
    try:
        raw = path.read_text(encoding="utf-8", errors="replace")
    except OSError as exc:
        print("%s: FAIL — не читается %s: %s" % (NAME, path, exc), file=sys.stderr)
        sys.exit(1)
    for i, line in enumerate(raw.split("\n"), 1):
        out.append((i, RE_LINE_COMMENT.sub("", line)))
    return out


def main(argv):
    root = pathlib.Path(argv[1] if len(argv) > 1 else ".").resolve()
    src = pathlib.Path(argv[2]).resolve() if len(argv) > 2 else root / "compiler-codegen" / "src"
    if not src.is_dir():
        print("%s: FAIL — нет каталога %s (переехал?)" % (NAME, src), file=sys.stderr)
        return 1

    found = {}
    for path in sorted(src.rglob("*.rs")):
        rel = path.relative_to(src).as_posix()
        lines = code_lines(path)
        ends = [n for n, text in lines if LIT_END in text]
        if not ends:
            continue
        has_idx = any(LIT_IDX in text for _, text in lines)
        found[rel] = (ends, has_idx)

    problems = []

    for rel, (ends, has_idx) in sorted(found.items()):
        if rel not in EXPECTED:
            problems.append(
                "ЧЕТВЁРТАЯ ДВЕРЬ: %s:%s спрашивает роль, но этого места нет в списке "
                "стража. На вопрос «удовлетворяет ли тип роли среза» должен отвечать "
                "ОДИН владелец; новое место — либо канальный фикс (тогда сократи "
                "список), либо второй ответ, который тихо разойдётся с первым."
                % (rel, ends[0])
            )
        elif not has_idx:
            problems.append(
                "ПОЛОВИНА РОЛИ: %s:%s называет `end_index`, но литерала `index` в файле "
                "нет. Роль по D470 всё-или-ничего: место, спрашивающее один селектор из "
                "двух, отвечает не на тот вопрос." % (rel, ends[0])
            )

    for rel, why in sorted(EXPECTED.items()):
        if rel not in found:
            problems.append(
                "ДВЕРЬ ИСЧЕЗЛА: %s больше не называет `end_index` (ожидалось: %s). "
                "Либо она переехала молча, либо канальный фикс сделан — тогда правь "
                "список стража осознанно, с летописью." % (rel, why)
            )

    if problems:
        print("%s: FAIL — %d нарушени(й)" % (NAME, len(problems)), file=sys.stderr)
        for p in problems:
            print("    " + p, file=sys.stderr)
        print(
            "    Норма: инвариант держит конструкция, а не внимание "
            "(docs/dev/conventions-governance.md); дом решения — D470.",
            file=sys.stderr,
        )
        return 1

    print(
        "%s ok: вопрос роли задаётся в %d названных местах, каждое называет оба "
        "селектора" % (NAME, len(EXPECTED))
    )
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
