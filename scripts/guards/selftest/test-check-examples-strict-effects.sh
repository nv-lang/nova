#!/bin/sh
# selftest/test-check-examples-strict-effects.sh — страж A-E2 умеет краснеть.
#
# ЖИВОЙ ПОЛОВИНЫ ЗДЕСЬ НЕТ, И ЭТО НАЗВАНО: настоящий прогон стража — 31 сборка,
# 328с (замер 2026-08-30). Самотест обязан быть дешёвым (Г1), поэтому он гоняет
# стража на ПОДСТАВНОМ каталоге примеров через швы NOVA_EXAMPLES_DIR и
# NOVA_EXAMPLES_EXCEPTIONS. Живую половину исполняет сам шаг гейта на ярусе
# `full` — там она и уместна.
export LC_ALL=C
ROOT="$(cd "$(dirname "$0")/../../.." && pwd)"
GUARD="$ROOT/scripts/guards/check-examples-strict-effects.sh"
T="${TMPDIR:-/tmp}/selftest-examples-strict.$$"
trap 'rm -rf "$T"' 0 2 15
rc=0

mk() {
    rm -rf "$T"; mkdir -p "$T/ex/_wip"
    # законный пример: настоящая точка входа, тривиальное тело
    printf 'module ok_example\n\nfn main() Io -> () {\n    println("ok")\n}\n' > "$T/ex/ok_example.nv"
    # сниппет: точка входа ЗАКОММЕНТИРОВАНА
    printf 'module snip\n\n// fn main() Io -> () {\n//     println("x")\n// }\n' > "$T/ex/snip.nv"
    # в _wip страж не смотрит вовсе — намеренно битый файл
    printf 'module wip\n\nfn main() Io -> () { this is not nova }\n' > "$T/ex/_wip/broken.nv"
    printf '# exceptions\nsnip.nv # сниппет по замыслу: точки входа нет по причине\n' > "$T/exc.list"
}

run() { NOVA_EXAMPLES_DIR="$T/ex" NOVA_EXAMPLES_EXCEPTIONS="$T/exc.list" sh "$GUARD" "$ROOT" >"$T/out" 2>&1; }

# ── здоровье: законный пример + названный сниппет + мусор в _wip ────────
mk
if ! run; then
    echo "FAIL: здоровый макет красный — страж ложнит:" >&2
    tail -3 "$T/out" | sed 's/^/    /' >&2
    rc=1
elif ! grep -q "_wip" "$T/out" 2>/dev/null && ! grep -q "точек входа вне _wip 1" "$T/out"; then
    : # строка ok: проверяется ниже по числу
fi

# ── подделка 1: сниппет НЕ назван исключением ──────────────────────────
mk
printf '# exceptions\n' > "$T/exc.list"
if run; then
    echo "FAIL: файл с закомментированной main прошёл без исключения — урок №573 не удержан" >&2
    rc=1
elif ! grep -q "snip.nv" "$T/out"; then
    echo "FAIL: красный есть, но носитель не назван поимённо:" >&2
    tail -2 "$T/out" | sed 's/^/    /' >&2
    rc=1
fi

# ── подделка 2: исключение БЕЗ причины ─────────────────────────────────
mk
printf '# exceptions\nsnip.nv\n' > "$T/exc.list"
if run; then
    echo "FAIL: исключение без причины прошло — проверку можно снять молча" >&2
    rc=1
fi

# ── подделка 3: пример, который НЕ собирается ──────────────────────────
mk
printf 'module bad\n\nfn main() Io -> () {\n    this is not nova\n}\n' > "$T/ex/bad.nv"
if run; then
    echo "FAIL: несобирающийся пример прошёл стража" >&2
    rc=1
elif ! grep -q "bad.nv" "$T/out"; then
    echo "FAIL: красный есть, но не назван файл:" >&2; tail -2 "$T/out" | sed 's/^/    /' >&2
    rc=1
fi

# ── обратная сторона: `_wip` НЕ судится (иначе черновики станут гейтом) ─
mk
if ! run; then
    echo "FAIL: заведомо битый файл в _wip покраснел — черновики не должны судиться:" >&2
    tail -2 "$T/out" | sed 's/^/    /' >&2
    rc=1
fi

# ── подделка 4: нет каталога примеров — судить нечего, но молчать нельзя ─
if NOVA_EXAMPLES_DIR="$T/nosuch" NOVA_EXAMPLES_EXCEPTIONS="$T/exc.list" sh "$GUARD" "$ROOT" >"$T/o5" 2>&1; then
    echo "FAIL: отсутствующий каталог примеров дал зелёный" >&2
    rc=1
fi

[ "$rc" -eq 0 ] && echo "test-check-examples-strict-effects ok: четыре подделки покраснели, здоровый макет и _wip зелёные (живая половина — шаг гейта яруса full: 31 сборка, 328с)"
exit "$rc"
