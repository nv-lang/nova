#!/bin/sh
# scripts/guards/check-hunter-mark.sh — план 278 Ф.2 + Ф.6 (регламент владельца
# 2026-08-30: треки и барьер бумажной охоты).
#
# ЗАЧЕМ. Мера плана 278 — «доля находок охотника, дошедших до реестра» — обязана
# СЧИТАТЬСЯ, а не вспоминаться, и считаться ПО ТРЕКАМ: у оракула свои охоты, у
# novac свои (решение владельца 2026-08-30), сваленные в кучу они не читаются.
# Числитель трека — метки «НАЙДЕНО ОХОТНИКОМ <дата> (<трек>)» в
# docs/plans/221.1-bug-sweep.md (реестр остаётся ОДНИМ — разделение про кучи
# охоты, не про реестр). Знаменатель трека — строки «НАХОДКА |» в открытых
# отчётах docs/dev/hunts/<трек>/*.md ПЛЮС числа «находок N» из свёрнутых строк
# LEDGER.md того же трека (свёртка — check-hunter-fold.sh, формат леджера живёт
# там; формат отчёта и метки живёт ЗДЕСЬ, бриф охотника ссылается сюда).
#
# БАРЬЕР БУМАЖНОЙ ОХОТЫ (дыра, найденная всеми тремя критиками панели
# 2026-08-30: отчёт, существование которого двигает часы долга, можно написать
# не охотясь). Отчёт обязан привозить ПРОБЫ В ДЕРЕВЕ:
#   - рядом с отчётом <stem>.md существует каталог probes/<stem>/;
#   - в нём не меньше ТРЁХ проб — и для отчёта «НИЧЕГО НЕ НАШЁЛ» тоже:
#     пустая охота легальна, но обязана показать, ЧТО пробовала;
#   - каждая находка цитирует пробу (4-е поле строки «НАХОДКА |»), и эта
#     проба существует в probes/<stem>/.
# Подделать это можно только написав настоящие файлы-пробы — то есть поохотясь.
#
# ЧТО КРАСНИТ:
#   1. метка без разбираемой даты (YYYY-MM-DD) ИЛИ без трека «(novac)»/«(oracle)»
#      сразу после даты — трек ПОСЛЕ даты, чтобы регэксп даты не сломался
#      (дыра критика №3);
#   2. отчёт без находок И без явного «НИЧЕГО НЕ НАШЁЛ» — молчание не успех (№770);
#   3. отчёт без probes/<stem>/, с менее чем тремя пробами, или с находкой,
#      цитирующей несуществующую пробу — бумажная охота;
#   4. меток трека больше, чем находок трека (открытых + свёрнутых) — метка
#      без отчёта-источника.
#
# ЧИСЛО НЕ СУДИТСЯ: ноль охот — законно (0/0 зелёный). Судится ЦЕЛОСТНОСТЬ.
#
# Самотест: scripts/guards/selftest/test-check-hunter-mark.sh.
export LC_ALL=C
ROOT="${1:-$(cd "$(dirname "$0")/../.." && pwd)}"
REG="$ROOT/docs/plans/221.1-bug-sweep.md"
HUNTS_BASE="${NOVA_HUNTS_DIR:-$ROOT/docs/dev/hunts}"

rc=0

# ── форма меток: дата + трек, глобально ──────────────────────────────────
MARKS=$(grep -oE "НАЙДЕНО ОХОТНИКОМ[^|.]*" "$REG" 2>/dev/null || true)
if [ -n "$MARKS" ]; then
    BAD=$(printf '%s\n' "$MARKS" | grep -vcE "НАЙДЕНО ОХОТНИКОМ 20[0-9]{2}-[0-9]{2}-[0-9]{2} \((novac|oracle)\)" || true)
    if [ "${BAD:-0}" -gt 0 ]; then
        echo "check-hunter-mark: FAIL — меток без даты (YYYY-MM-DD) или без трека «(novac)»/«(oracle)» после даты: $BAD" >&2
        printf '%s\n' "$MARKS" | grep -vE "НАЙДЕНО ОХОТНИКОМ 20[0-9]{2}-[0-9]{2}-[0-9]{2} \((novac|oracle)\)" | head -3 | sed 's/^/    /' >&2
        rc=1
    fi
fi

SUMMARY=""
for TRACK in novac oracle; do
    DIR="$HUNTS_BASE/$TRACK"
    if [ ! -d "$DIR" ]; then
        echo "check-hunter-mark: FAIL — нет каталога трека $DIR (треки — решение владельца 2026-08-30)" >&2
        rc=1
        continue
    fi

    N_MARKS=$(grep -oE "НАЙДЕНО ОХОТНИКОМ 20[0-9]{2}-[0-9]{2}-[0-9]{2} \($TRACK\)" "$REG" 2>/dev/null | wc -l | tr -d ' ')

    N_FINDINGS=0
    N_REPORTS=0
    for f in "$DIR"/*.md; do
        [ -f "$f" ] || continue
        base=${f##*/}
        [ "$base" = "LEDGER.md" ] && continue
        N_REPORTS=$((N_REPORTS + 1))
        stem=${base%.md}
        n=$(grep -cE "^НАХОДКА \|" "$f" || true)
        N_FINDINGS=$((N_FINDINGS + n))
        if [ "$n" -eq 0 ] && ! grep -qE "НИЧЕГО НЕ НАШЁЛ" "$f"; then
            echo "check-hunter-mark: FAIL — отчёт без находок И без явного «НИЧЕГО НЕ НАШЁЛ»: $TRACK/$base (молчание — не успех, №770)" >&2
            rc=1
        fi
        # барьер бумажной охоты: пробы в дереве
        pd="$DIR/probes/$stem"
        if [ ! -d "$pd" ]; then
            echo "check-hunter-mark: FAIL — отчёт $TRACK/$base без каталога проб probes/$stem/ — отчёт без проб не свидетельство охоты" >&2
            rc=1
        else
            np=$(ls "$pd" 2>/dev/null | wc -l | tr -d ' ')
            if [ "$np" -lt 3 ]; then
                echo "check-hunter-mark: FAIL — в probes/$stem/ только $np проб(ы), нужно не меньше трёх — и пустой охоте тоже: покажи, что пробовал" >&2
                rc=1
            fi
            for cited in $(grep -E "^НАХОДКА \|" "$f" | awk -F'|' '{v=$4; gsub(/[ \t]/, "", v); print v}'); do
                if [ -n "$cited" ] && [ ! -e "$pd/$cited" ]; then
                    echo "check-hunter-mark: FAIL — находка в $TRACK/$base цитирует пробу «$cited», которой нет в probes/$stem/" >&2
                    rc=1
                fi
            done
        fi
    done

    # свёрнутые находки трека (формат леджера — дом check-hunter-fold.sh)
    N_FOLDED=0
    if [ -f "$DIR/LEDGER.md" ]; then
        N_FOLDED=$(awk -F'|' '/^СВЁРНУТО \|/ {v=$5; gsub(/[^0-9]/, "", v); s+=v} END {print s+0}' "$DIR/LEDGER.md")
    fi

    DEN=$((N_FINDINGS + N_FOLDED))
    if [ "$N_MARKS" -gt "$DEN" ]; then
        echo "check-hunter-mark: FAIL — меток трека $TRACK в реестре ($N_MARKS) больше, чем находок в его отчётах и леджере ($DEN): метка без отчёта-источника" >&2
        rc=1
    fi
    SUMMARY="$SUMMARY $TRACK: отчётов $N_REPORTS, находок $N_FINDINGS+$N_FOLDED свёрнутых, меток $N_MARKS;"
done

if [ "$rc" -eq 0 ]; then
    echo "check-hunter-mark ok:$SUMMARY обе половины меры считаются грепом, пробы в дереве"
fi
exit "$rc"
