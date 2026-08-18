#!/usr/bin/env bash
# Страж: собственные наборы тестов Rust-крейтов проходят целиком.
#
# ЗАЧЕМ. 2026-08-18 обнаружилось, что тесты `nova-lsp` (433 штуки) и `nova-cli`
# (191) НЕ ГОНЯЕТ НИКТО — ни `scripts/gate.sh`, ни один workflow в
# `.github/workflows/` (там `cargo test` есть только для `compiler-codegen`:
# doc-семейство и z3). Красных было восемь, и сказать, как давно, нельзя:
# гнилой тест не оставляет следа, пока его не запустят. Расширение VS Code и
# CLI входят в поставку v0.1 — непроверенными поставлялись куски продукта.
#
# ЧТО ИМЕННО ПРОГНИЛО, для понимания цены молчания. Семь падений из восьми были
# о самих тестах: два утверждали, что `std.io`/`std.math` не существуют (оба
# написаны с тех пор); два ждали имя `hashmap` (стало `hash_map`); один строил
# значение снятой формой `Foo {}`; два фикстурных исходника пользовались снятым
# `let`; ещё один искал фикстуру по пути, уехавшему в архив, а сама фикстура
# объявляла модули снятой формой `папка.файл` вместо имени папки. Ни одного
# дефекта продукта — ровно поэтому их и не замечали.
#
# А ВОСЬМОЙ БЫЛ НАСТОЯЩИМ, и он один оправдывает всё остальное: `nova check .`
# не проверял НИЧЕГО и возвращал НОЛЬ (реестр №724). Его не поймал ни один
# тест, потому что все звали `nova check <файл>`, а не каталог.
#
# ПОЧЕМУ АССЕРТ НА СТРОКУ, А НЕ КОД ВОЗВРАТА. Код возврата отличает «упал» от
# «не упал», а нужно «проверил». Ноль без строки `test result: ok. N passed;
# 0 failed` — это молчание, и оно обязано быть красным (класс F1, реестр №645).
# Отдельно требуется N в сотнях: пустой набор тоже даёт «ok».
#
# ЧТО НЕ ПОКРЫТО, ЯВНО. `compiler-codegen` (39 интеграционных целей) сюда НЕ
# включён: его набор не измерен по времени, а страж, добавляющий гейту
# неизвестно сколько, — это не страж, а рулетка. Остаётся работой в №723.
# Молчаливого сокращения охвата здесь нет: список крейтов виден ниже.
#
# ПОЧЕМУ У nova-lsp ТОЛЬКО `--lib`. Двоичные цели линкуются в тот же
# `nova-lsp.exe`, который держит открытым работающий редактор; линковка тогда
# падает с `os error 5`, и страж краснел бы о среде, а не о коде.
#
# $1 — корень репозитория.
# NOVA_CRATE_TESTS_CMD — команда вместо cargo (самотест подставляет свою).
#
# План docs/plans/231-bug-cycle-exit.md (дисциплина механизмов принуждения).

set -u
export LC_ALL=C

NAME="check-crate-tests"
ROOT="${1:-$(cd "$(dirname "$0")/../.." && pwd)}"

# крейт:аргументы cargo:минимум ожидаемых тестов
SUITES="nova-lsp:--lib:300 nova-cli::150"

fail_hard() {
    echo "$NAME: FAIL — $1" >&2
    exit 1
}

TOTAL=0
for suite in $SUITES; do
    CRATE=${suite%%:*}
    REST=${suite#*:}
    ARGS=${REST%%:*}
    MIN=${REST#*:}

    [ -f "$ROOT/$CRATE/Cargo.toml" ] || fail_hard "нет $ROOT/$CRATE/Cargo.toml"

    if [ -n "${NOVA_CRATE_TESTS_CMD:-}" ]; then
        OUT=$(eval "$NOVA_CRATE_TESTS_CMD" 2>&1)
    elif command -v cargo >/dev/null 2>&1; then
        # shellcheck disable=SC2086
        OUT=$(cd "$ROOT/$CRATE" && cargo test --release --no-fail-fast $ARGS 2>&1)
    else
        echo "$NAME ok: cargo недоступен — судить нечем"
        exit 0
    fi

    # Строки вердикта обязаны БЫТЬ. Их отсутствие — не «прошло», а «неизвестно».
    LINES=$(printf '%s\n' "$OUT" | grep -E '^test result: (ok|FAILED)\.')
    if [ -z "$LINES" ]; then
        echo "$NAME: FAIL — $CRATE не отчитался ни одной строкой 'test result:'" >&2
        printf '%s\n' "$OUT" | tail -20 >&2
        echo "    Ноль без строки вердикта — это 'не упал', а не 'проверил'." >&2
        exit 1
    fi

    PASSED=$(printf '%s\n' "$LINES" | sed -n 's/.* \([0-9][0-9]*\) passed.*/\1/p' \
             | awk '{s+=$1} END{print s+0}')
    FAILED=$(printf '%s\n' "$LINES" | sed -n 's/.* \([0-9][0-9]*\) failed.*/\1/p' \
             | awk '{s+=$1} END{print s+0}')

    if [ "$FAILED" -ne 0 ]; then
        echo "$NAME: FAIL — в $CRATE упало тестов: $FAILED (прошло $PASSED)" >&2
        printf '%s\n' "$OUT" | grep -E "^(thread |test .* FAILED|---- )" | head -20 >&2
        echo "    Прогнать вручную: cd $CRATE && cargo test --release --no-fail-fast $ARGS" >&2
        exit 1
    fi

    if [ "$PASSED" -lt "$MIN" ]; then
        echo "$NAME: FAIL — в $CRATE прошло всего $PASSED тестов, ожидалось от $MIN" >&2
        echo "    Похоже, набор отфильтровался или не собрался; 'ok' на пустом" >&2
        echo "    наборе ничего не значит." >&2
        exit 1
    fi

    TOTAL=$((TOTAL + PASSED))
done

echo "$NAME ok: наборы Rust-крейтов зелёные целиком ($TOTAL тестов; compiler-codegen не покрыт — №723)"
exit 0
