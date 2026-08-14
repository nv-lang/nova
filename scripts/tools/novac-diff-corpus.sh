#!/bin/sh
# scripts/tools/novac-diff-corpus.sh — полнокорпусный дифф-прогон novac
# против оракула (план 274 §9/Э1, ранг ОБВЯЗКА: «полный дифф-раннер по
# корпусу с замером цены прогона»).
#
# Прогоняет ОБЕ реализации по корпусу (по умолчанию examples/**/*.nv) и
# классифицирует исходы по коду возврата:
#   совпали-приняли · совпали-отвергли ·
#   subset  (novac отверг честным E_NOVAC_SUBSET, оракул принял) — ШТАТНО
#           для Э1 по построению: подмножество крошечное; считается, не краснит;
#   DANGER  (novac ПРИНЯЛ, оракул отверг) — класс К7: novac считает языком
#           то, что языком не является; красный вне novac/divergences.allow;
#   PANIC   (rc>=124 или 'panic' в stderr novac) — всегда красный, приёмка
#           «ноль паник» действует на диком корпусе так же, как на мутациях.
#
# Шапка прогона несёт ревизию оракула и режим сборки novac (§10.3, строка
# «каждый прогон дифф-гейта несёт ревизию оракула и режим сборки — иначе
# классификация невоспроизводима через неделю»). Цена прогона печатается
# итоговой строкой (сумма по бинарям + стена).
#
# Usage: sh scripts/tools/novac-diff-corpus.sh [corpus-dir]
# Проверялся: Windows (Git Bash), 2026-08-14.
export LC_ALL=C
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
CORPUS="${1:-$ROOT/examples}"
NOVAC="$ROOT/novac/target/novac.exe"
ALLOW="$ROOT/novac/divergences.allow"
T="${TMPDIR:-/tmp}/novac-diff-corpus.$$"
mkdir -p "$T"
trap 'rm -rf "$T"' 0

[ -f "$NOVAC" ] || { echo "novac-diff-corpus: нет $NOVAC" >&2; exit 2; }
ORACLE="$ROOT/nova-cli/target/release/nova.exe"
if [ ! -f "$ORACLE" ]; then
    MAINROOT=$(git -C "$ROOT" rev-parse --path-format=absolute --git-common-dir 2>/dev/null)
    [ -n "$MAINROOT" ] && ORACLE="$MAINROOT/../nova-cli/target/release/nova.exe"
fi
[ -f "$ORACLE" ] || { echo "novac-diff-corpus: оракул не собран" >&2; exit 2; }

# Строго hex-ревизия: в том же файле есть ПРОЗА со словом «oracle-pin:»
# (комментарий-контракт) — свободный якорь цеплял и её.
PIN=$(tr -d '\r' < "$ROOT/novac/nova.toml" | sed -n 's/^#[[:space:]]*oracle-pin:[[:space:]]*\([0-9a-f][0-9a-f]*\)$/\1/p')
ORACLE_REV=$(git -C "$(dirname "$ORACLE")" rev-parse --short HEAD 2>/dev/null)
echo "novac-diff-corpus: oracle-pin=$PIN oracle-HEAD=$ORACLE_REV сборка novac=single-file корпус=$CORPUS"

find "$CORPUS" -type f -name '*.nv' | sort > "$T/list"
N=$(wc -l < "$T/list" | tr -d ' ')
[ "$N" -gt 0 ] || { echo "novac-diff-corpus: корпус пуст" >&2; exit 2; }

acc=0; rej=0; subset=0; danger=0; panic=0; allowed=0
t_novac=0; t_oracle=0
wall0=$(date +%s%N)
while IFS= read -r f; do
    rel=${f#"$ROOT"/}
    s=$(date +%s%N)
    timeout 10 "$NOVAC" check "$f" >/dev/null 2>"$T/err" </dev/null
    rn=$?
    t_novac=$(( t_novac + ( $(date +%s%N) - s ) / 1000000 ))
    s=$(date +%s%N)
    "$ORACLE" check "$f" >/dev/null 2>&1 </dev/null
    ro=$?
    t_oracle=$(( t_oracle + ( $(date +%s%N) - s ) / 1000000 ))
    if [ "$rn" -ge 124 ] || grep -qi "panic" "$T/err"; then
        panic=$((panic+1))
        echo "  PANIC/HANG: $rel (novac rc=$rn)" >> "$T/red"
        head -1 "$T/err" | sed 's/^/    /' >> "$T/red"
    elif [ "$rn" -eq 0 ] && [ "$ro" -ne 0 ]; then
        if [ -f "$ALLOW" ] && grep -Fxq "$rel" "$ALLOW"; then
            allowed=$((allowed+1))
        else
            danger=$((danger+1))
            echo "  DANGER (К7): $rel — novac принял, оракул отверг (rc=$ro)" >> "$T/red"
        fi
    elif [ "$rn" -ne 0 ] && [ "$ro" -eq 0 ]; then
        subset=$((subset+1))
        echo "$rel" >> "$T/subset"
    elif [ "$rn" -eq 0 ]; then
        acc=$((acc+1)); echo "$rel" >> "$T/acc"
    else
        rej=$((rej+1))
    fi
done < "$T/list"
wall=$(( ( $(date +%s%N) - wall0 ) / 1000000 ))

echo "novac-diff-corpus: файлов $N — совпали-приняли $acc · совпали-отвергли $rej · subset(ожидаемо) $subset · DANGER $danger · PANIC $panic · allow $allowed"
echo "novac-diff-corpus: цена прогона — novac ${t_novac}ms, оракул ${t_oracle}ms, стена ${wall}ms"
if [ -f "$T/acc" ]; then
    echo "  в подмножестве Э1 (оба приняли):"
    sed 's/^/    /' "$T/acc"
fi
if [ "$danger" -gt 0 ] || [ "$panic" -gt 0 ]; then
    echo "novac-diff-corpus: FAIL" >&2
    cat "$T/red" >&2
    exit 1
fi
echo "novac-diff-corpus ok"
exit 0
