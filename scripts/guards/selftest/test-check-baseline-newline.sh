#!/bin/sh
# Самотест check-baseline-newline.sh (класс №891). Доказывает ОБЕ стороны:
# база без завершающего перевода строки краснеет и названа по имени, целая —
# зелёная. Шов $2 — сканируемая директория, поэтому живое дерево не трогается.
export LC_ALL=C
GD="$(cd "$(dirname "$0")/.." && pwd)"
ROOT="$(cd "$GD/../.." && pwd)"
G="$GD/check-baseline-newline.sh"
T="${TMPDIR:-/tmp}/baseline-newline-selftest.$$"
trap 'rm -rf "$T"' 0
fails=0
ok()  { echo "  ok: $1"; }
bad() { echo "  FAIL: $1" >&2; fails=$((fails+1)); }

# Подложка — настоящий git-репозиторий: страж берёт список баз через
# `git ls-files`, поэтому файл, не добавленный в индекс, он не увидит, и
# самотест на голом каталоге доказывал бы пустоту (класс №519).
mk() {
    d="$T/$1"; mkdir -p "$d/scripts/guards"
    git -C "$d" init -q 2>/dev/null
    git -C "$d" config user.email t@t; git -C "$d" config user.name t
    echo "$d"
}
add() { printf '%s' "$2" > "$1/scripts/guards/$3"; git -C "$1" add "scripts/guards/$3"; }

# --- целая база: зелено ------------------------------------------------------
D=$(mk good)
add "$D" "rows=3
" "a.baseline"
if sh "$G" "$D" "$D" > "$T/out" 2> "$T/err"; then
    grep -q "баз 1" "$T/out" && ok "целая база — зелено, и число названо" \
        || bad "зелено, но не сказано сколько баз"
else
    bad "целая база покраснела: $(cat "$T/err")"
fi

# --- ГЛАВНЫЙ случай: нет завершающего перевода строки ------------------------
D=$(mk bad)
add "$D" "rows=3" "a.baseline"
if sh "$G" "$D" "$D" > "$T/out" 2> "$T/err"; then
    bad "база без перевода строки прошла — главный случай не ловится"
else
    grep -q "a.baseline" "$T/err" && ok "база без перевода строки поймана и названа" \
        || bad "красный, но имя файла не названо"
fi

# --- ложных срабатываний нет: одна целая рядом с одной битой -----------------
D=$(mk mixed)
add "$D" "x=1
" "good.baseline"
add "$D" "y=2" "bad.baseline"
if sh "$G" "$D" "$D" > "$T/out" 2> "$T/err"; then
    bad "битая база среди целых прошла"
else
    if grep -q "bad.baseline" "$T/err" && ! grep -q "good.baseline" "$T/err"; then
        ok "названа ровно виновная база, целая не оболгана"
    else
        bad "названы не те файлы: $(cat "$T/err")"
    fi
fi

# --- пустая база — тоже отказ (сверять не с чем, класс №519) -----------------
D=$(mk empty)
add "$D" "" "a.baseline"
if sh "$G" "$D" "$D" > "$T/out" 2> "$T/err"; then
    bad "пустая база прошла молча"
else
    grep -q "ПУСТА" "$T/err" && ok "пустая база — красный" || bad "красный, но не про пустоту"
fi

# --- нет баз вообще: судить нечего, но честно сказано ------------------------
D=$(mk none)
if sh "$G" "$D" "$D" > "$T/out" 2> "$T/err"; then
    grep -q "судить нечего" "$T/out" && ok "нет баз — сказано «судить нечего»" \
        || bad "зелено без объяснения пустоты"
else
    bad "отсутствие баз покрасило стража"
fi

echo "итог: FAIL $fails"
if [ "$fails" -eq 0 ]; then
    echo "test-check-baseline-newline ok: обе стороны доказаны — битая база краснеет и названа, целая зелёная, пустая отказана"
    exit 0
fi
exit 1
