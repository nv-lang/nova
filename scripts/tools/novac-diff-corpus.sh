#!/bin/sh
# scripts/tools/novac-diff-corpus.sh — полнокорпусный дифф-прогон novac
# против оракула (план 274 §9/Э1 ОБВЯЗКА; с Э2 — механизм храповика §10.4).
#
# Прогоняет ОБЕ реализации по корпусу (по умолчанию examples/**/*.nv) и
# классифицирует исходы по коду возврата:
#   совпали-приняли · совпали-отвергли ·
#   subset  (novac отверг, оракул принял) — ожидаемое отставание novac;
#           раскладывается по корзинам §10.4:
#             «вне точки»   — файл ДОБАВЛЕН в git после spec-point И отвергнут
#                             (двухчастный прокси bootstrap §3; спорные — руками);
#             «заблокировано оракулом» — носители [LEGACY-#NNN]/EXPECT_CC_ERROR;
#             остальное     — честное отставание подмножества;
#   DANGER  (novac ПРИНЯЛ, оракул отверг) — класс К7; красный вне allow;
#   PANIC   (rc>=124 или 'panic' в stderr novac) — всегда красный.
#
# Второе монотонное число (274 §9/Э2): файлы «совпали-приняли», собранные
# ОБОИМИ компиляторами с поведенческим совпадением (exit+stdout байт-в-байт)
# — через novac-e1-smoke.sh; без него регресс кодогена невидим до Э5.
#
# Расстояние до самосборки (§10.4): novac/src/**/*.nv через novac check;
# отвергнутые — отдельное число, в знаменатель храповика НЕ входят.
#
# Шапка прогона несёт ревизию оракула и режим сборки novac (§10.3 — иначе
# классификация невоспроизводима через неделю). Последняя строка — машинная,
# её парсит check-novac-differential.sh для сверки с novac-corpus.baseline.
#
# Usage: sh scripts/tools/novac-diff-corpus.sh [corpus-dir]
# Бюджет: examples (60 файлов) ~2–4 мин под нагрузкой; полный корпус
# (std+nova_tests+spec_tests, ~2.8k) — только кнопкой/ночью (bootstrap §3).
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
SPEC_POINT=$(tr -d '\r' < "$ROOT/novac/nova.toml" | sed -n 's/^#[[:space:]]*spec-point:[[:space:]]*\([0-9-]*\)$/\1/p')
ORACLE_REV=$(git -C "$(dirname "$ORACLE")" rev-parse --short HEAD 2>/dev/null)
echo "novac-diff-corpus: oracle-pin=$PIN oracle-HEAD=$ORACLE_REV spec-point=$SPEC_POINT сборка novac=single-file корпус=$CORPUS"

find "$CORPUS" -type f -name '*.nv' | sort > "$T/list"
N=$(wc -l < "$T/list" | tr -d ' ')
[ "$N" -gt 0 ] || { echo "novac-diff-corpus: корпус пуст" >&2; exit 2; }

acc=0; rej=0; subset=0; outpoint=0; blocked=0; danger=0; panic=0; allowed=0
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
        # Корзины §10.4 — принадлежность решает МАШИНА (bootstrap §3).
        if grep -q 'LEGACY-#\|EXPECT_CC_ERROR' "$f"; then
            blocked=$((blocked+1))
        else
            added=$(git -C "$ROOT" log --diff-filter=A --follow --format=%as -1 -- "$rel" 2>/dev/null)
            if [ -n "$added" ] && [ -n "$SPEC_POINT" ] && [ "$added" \> "$SPEC_POINT" ]; then
                outpoint=$((outpoint+1))
                echo "$rel ($added)" >> "$T/outpoint"
            else
                subset=$((subset+1))
                echo "$rel" >> "$T/subset"
            fi
        fi
    elif [ "$rn" -eq 0 ]; then
        acc=$((acc+1)); echo "$rel" >> "$T/acc"
    else
        rej=$((rej+1))
    fi
done < "$T/list"

# Поведенческое число: каждый совпали-принятый файл через смоук (эмиссия
# novac + релинк драйвером + побайтовый дифф stdout/exit против оракула).
beh=0; behfail=0
if [ -f "$T/acc" ]; then
    while IFS= read -r rel; do
        if sh "$ROOT/scripts/tools/novac-e1-smoke.sh" "$rel" >/dev/null 2>&1; then
            beh=$((beh+1))
        else
            echo "  ПОВЕДЕНИЕ РАЗОШЛОСЬ: $rel (оба check-принимают, но бинарь novac != оракула)" >> "$T/red"
            behfail=$((behfail+1))
        fi
    done < "$T/acc"
fi

# Расстояние до самосборки: собственный исходник через novac check.
self_total=0; self_rej=0
for f in "$ROOT"/novac/src/*/*.nv "$ROOT"/novac/src/*.nv; do
    [ -f "$f" ] || continue
    self_total=$((self_total+1))
    timeout 10 "$NOVAC" check "$f" >/dev/null 2>&1 </dev/null || self_rej=$((self_rej+1))
done
wall=$(( ( $(date +%s%N) - wall0 ) / 1000000 ))

echo "novac-diff-corpus: файлов $N — совпали-приняли $acc · совпали-отвергли $rej · отставание $subset · вне-точки $outpoint · заблокировано-оракулом $blocked · DANGER $danger · PANIC $panic · allow $allowed"
echo "novac-diff-corpus: поведенчески совпали $beh из $acc · самосборка: отвергнуто $self_rej из $self_total"
echo "novac-diff-corpus: цена прогона — novac ${t_novac}ms, оракул ${t_oracle}ms, стена ${wall}ms"
if [ -f "$T/acc" ]; then
    echo "  в подмножестве (оба приняли):"
    sed 's/^/    /' "$T/acc"
fi
if [ -f "$T/outpoint" ]; then
    echo "  вне точки (добавлены после $SPEC_POINT, спорные разбираются поимённо):"
    sed 's/^/    /' "$T/outpoint"
fi
if [ "$danger" -gt 0 ] || [ "$panic" -gt 0 ] || [ "$behfail" -gt 0 ]; then
    echo "novac-diff-corpus: FAIL" >&2
    cat "$T/red" >&2
    exit 1
fi
# Машинная строка — её парсит check-novac-differential.sh (храповик §10.4).
echo "novac-diff-corpus baseline-numbers: contract-match=$((acc+rej)) behavior-match=$beh out-of-point=$outpoint oracle-blocked=$blocked self-distance=$self_rej/$self_total"
echo "novac-diff-corpus ok"
exit 0
