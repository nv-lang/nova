#!/bin/sh
# selftest/test-check-hunter-debt.sh — страж долга охоты умеет краснеть.
# Живая половина + макет-репозиторий git: долг над бюджетом краснит, свежий
# отчёт (коммит с ДОБАВЛЕНИЕМ файла) гасит долг, свёртка (удаление) — НЕ гасит.
export LC_ALL=C
ROOT="$(cd "$(dirname "$0")/../../.." && pwd)"
GUARD="$ROOT/scripts/guards/check-hunter-debt.sh"
T="${TMPDIR:-/tmp}/selftest-hunter-debt.$$"
trap 'rm -rf "$T"' 0 2 15
rc=0

# Живая половина: настоящее дерево обязано быть зелёным.
if ! sh "$GUARD" "$ROOT" >"$T.live" 2>&1; then
    echo "FAIL: живая половина красная на настоящем дереве:" >&2
    tail -2 "$T.live" | sed 's/^/    /' >&2
    rc=1
fi
rm -f "$T.live"

# Макет-репозиторий: поверхность novac + один отчёт охоты.
mkdir -p "$T/repo/novac/src" "$T/repo/docs/dev/hunts/novac" "$T/repo/docs/dev/hunts/oracle"
git -C "$T/repo" init -q
git -C "$T/repo" config user.email selftest@example.com
git -C "$T/repo" config user.name selftest
git -C "$T/repo" config commit.gpgsign false
git -C "$T/repo" config core.autocrlf false
seq 1 10 | sed 's/^/line /' > "$T/repo/novac/src/a.nv"
printf 'KLETKA stub\n' > "$T/repo/docs/dev/hunts/novac/2026-01-01-parse-k1.md"
git -C "$T/repo" add novac/src/a.nv docs/dev/hunts/novac/2026-01-01-parse-k1.md
git -C "$T/repo" commit -qm "seed: surface and first hunt report"
ANCHOR=$(git -C "$T/repo" rev-parse HEAD)
printf 'budget_novac=5\nbudget_oracle=5\nanchor=%s\n' "$ANCHOR" > "$T/base"

# Здоровье: долг 0 при свежей охоте.
if ! NOVA_HUNTER_DEBT_BASELINE="$T/base" sh "$GUARD" "$T/repo" >"$T.o0" 2>&1; then
    echo "FAIL: макет с нулевым долгом красный:" >&2; tail -2 "$T.o0" | sed 's/^/    /' >&2
    rc=1
fi

# ── подделка 1: рост поверхности над бюджетом без новой охоты ───────────
seq 1 20 | sed 's/^/grown /' >> "$T/repo/novac/src/a.nv"
if NOVA_HUNTER_DEBT_BASELINE="$T/base" sh "$GUARD" "$T/repo" >"$T.o1" 2>&1; then
    echo "FAIL: долг 20 строк при бюджете 5 прошёл — триггер не принуждает" >&2
    rc=1
fi

# ── гашение: коммит роста + коммит НОВОГО отчёта → долг снова 0 ─────────
git -C "$T/repo" add novac/src/a.nv
git -C "$T/repo" commit -qm "grow surface"
printf 'KLETKA stub 2\n' > "$T/repo/docs/dev/hunts/novac/2026-02-02-lex-k2.md"
git -C "$T/repo" add docs/dev/hunts/novac/2026-02-02-lex-k2.md
git -C "$T/repo" commit -qm "second hunt report"
if ! NOVA_HUNTER_DEBT_BASELINE="$T/base" sh "$GUARD" "$T/repo" >"$T.o2" 2>&1; then
    echo "FAIL: свежий отчёт не погасил долг:" >&2; tail -2 "$T.o2" | sed 's/^/    /' >&2
    rc=1
fi

# ── подделка 2: свёртка (удаление отчёта) не смеет двигать часы ─────────
seq 1 20 | sed 's/^/more /' >> "$T/repo/novac/src/a.nv"
git -C "$T/repo" add novac/src/a.nv
git -C "$T/repo" commit -qm "grow again"
git -C "$T/repo" rm -q docs/dev/hunts/novac/2026-02-02-lex-k2.md
git -C "$T/repo" commit -qm "fold: remove report"
if NOVA_HUNTER_DEBT_BASELINE="$T/base" sh "$GUARD" "$T/repo" >"$T.o3" 2>&1; then
    echo "FAIL: удаление отчёта погасило долг — свёртка стала способом обойти охоту (дыра критиков)" >&2
    rc=1
fi

# ── подделка 3: якорь-мусор в базе ──────────────────────────────────────
printf 'budget_novac=5\nbudget_oracle=5\nanchor=deadbeef\n' > "$T/base2"
if NOVA_HUNTER_DEBT_BASELINE="$T/base2" sh "$GUARD" "$T/repo" >"$T.o4" 2>&1; then
    echo "FAIL: якорь-не-коммит прошёл — базу можно сломать молча" >&2
    rc=1
fi

[ "$rc" -eq 0 ] && echo "test-check-hunter-debt ok: три подделки покраснели, гашение отчётом работает, живая половина зелёная"
exit "$rc"
