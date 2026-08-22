#!/bin/sh
# Самотест check-novac-match-exhaustive.py (П16). Шов $2 — сканируемая директория.
export LC_ALL=C
GD="$(cd "$(dirname "$0")/.." && pwd)"
ROOT="$(cd "$GD/../.." && pwd)"
G="$GD/check-novac-match-exhaustive.py"
T="${TMPDIR:-/tmp}/novac-match-exh-selftest.$$"
mkdir -p "$T"
trap 'rm -rf "$T"' 0
fails=0
ok()  { echo "  ok: $1"; }
bad() { echo "  FAIL: $1" >&2; fails=$((fails+1)); }
run() { python "$G" "$ROOT" "$1" > "$T/out" 2> "$T/err"; }

# подложка: сумма из трёх вариантов и match по ней
mk() { d="$T/$1"; mkdir -p "$d/m"; shift; printf '%s\n' "$@" > "$d/m/m.nv"; echo "$d"; }

FULL='module novac.m

export type Color enum
    | Red
    | Green
    | Blue

fn name(c Color) -> str {
    match c {
        Red => "r"
        Green => "g"
        Blue => "b"
    }
}'
D=$(mk full "$FULL")
if run "$D"; then
    grep -q "судимых по сумме 1" "$T/out" && ok "полный match — зелёный, счёт верный" || bad "зелёный, но счёт не тот [$(cat "$T/out")]"
else
    bad "полный match покраснел: $(cat "$T/err")"
fi

# --- ГЛАВНЫЙ случай: вариант без ветки ------------------------------------
HOLE='module novac.m

export type Color enum
    | Red
    | Green
    | Blue

fn name(c Color) -> str {
    match c {
        Red => "r"
        Green => "g"
    }
}'
D=$(mk hole "$HOLE")
if run "$D"; then
    bad "дырявый match прошёл — главный случай не ловится"
else
    grep -q "Blue" "$T/err" && ok "непокрытый вариант пойман и НАЗВАН" || bad "красный, но не назвал Blue [$(cat "$T/err")]"
fi

# --- or-образцы считаются покрытием ---------------------------------------
ORPAT='module novac.m

export type Color enum
    | Red
    | Green
    | Blue

fn warm(c Color) -> bool {
    match c {
        Red | Green => true
        Blue => false
    }
}'
D=$(mk orpat "$ORPAT")
run "$D" && ok "or-образец покрывает оба варианта" || bad "or-образец не зачтён: $(cat "$T/err")"

# --- payload в образце не мешает ------------------------------------------
PAY='module novac.m

export type Shape enum
    | Dot
    | Line(int)
    | Box { w int, h int }

fn area(s Shape) -> int {
    match s {
        Dot => 0
        Line(n) => n
        Box { .. } => 1
    }
}'
D=$(mk pay "$PAY")
run "$D" && ok "payload в образце разобран, покрытие полное" || bad "payload сбил разбор: $(cat "$T/err")"

# --- подстановочник судит соседний страж, здесь пропуск --------------------
WILD='module novac.m

export type Color enum
    | Red
    | Green
    | Blue

fn name(c Color) -> str {
    match c {
        Red => "r"
        _ => "x"
    }
}'
D=$(mk wild "$WILD")
if run "$D"; then
    grep -q "вне суда 1" "$T/out" && ok "match с _ пропущен и посчитан отдельно" || bad "зелёный, но не посчитал пропуск [$(cat "$T/out")]"
else
    bad "match с _ покраснел здесь (его судит no-default-branch): $(cat "$T/err")"
fi

# --- match не по сумме novac (Option из std) — судить нечем ---------------
OPT='module novac.m

export type Color enum
    | Red
    | Green

fn go(x Option[int]) -> int {
    match x {
        Some(v) => v
        None => 0
    }
}'
D=$(mk opt "$OPT")
run "$D" && ok "match по чужой сумме пропущен, не выдуман" || bad "чужая сумма покраснела: $(cat "$T/err")"

# --- мишень потеряна -------------------------------------------------------
D="$T/nosum"; mkdir -p "$D/m"; printf 'module novac.m\n\nfn f() -> int => 1\n' > "$D/m/m.nv"
if run "$D"; then
    bad "дерево без сумм прошло молча"
else
    grep -q "мишень\|не найдено ни одной суммы" "$T/err" && ok "нет сумм — красный (класс №519)" || bad "красный, но не про мишень"
fi

run "$T/absent"; grep -q "судить нечего" "$T/out" && ok "нет директории — судить нечего" || bad "ждали «судить нечего»"

echo "итог: FAIL $fails"
if [ "$fails" -eq 0 ]; then
    echo "test-check-novac-match-exhaustive ok: все случаи, включая непокрытый вариант, or-образцы и payload"
    exit 0
fi
exit 1
