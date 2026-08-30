#!/bin/sh
# scripts/guards/check-hunter-fold.sh — план 278 Ф.6 (регламент владельца
# 2026-08-30: «результат не должен сваливаться в кучу и бесконечно копиться»).
#
# ЗАЧЕМ. Отчёты охоты рождаются долгом (check-hunter-debt.sh) БЕСКОНЕЧНО, пока
# растёт поверхность, — значит куча docs/dev/hunts/<трек>/ обязана иметь предел
# и путь вниз. Путь вниз — СВЁРТКА: когда находки отчёта заведены в реестр,
# окно сворачивает отчёт в одну строку LEDGER.md его трека и удаляет файл отчёта
# вместе с его пробами (полный текст остаётся в истории git). Леджер — это
# карта покрытия, ему расти положено — по строке на охоту.
#
# ФОРМАТ СТРОКИ ЛЕДЖЕРА ЖИВЁТ ЗДЕСЬ (бриф и mark-страж ссылаются, второго дома
# нет):
#   СВЁРНУТО | <stem-отчёта> | <модуль> | К<n> | находок N | №NNN[,№NNN…]|—
# где stem = имя файла отчёта без .md (начинается с даты YYYY-MM-DD), поля
# 3–4 — клетка охоты (для многоклеточной охоты — строка на клетку, «находок N»
# у продолжений = 0), refs — строки реестра, рождённые охотой («—», если 0).
#
# ЧТО КРАСНИТ:
#   1. открытых отчётов трека больше max_open из базы hunter-fold.baseline —
#      сверни разобранные (предел — число ВЛАДЕЛЬЦА, не самоназначается);
#   2. строка СВЁРНУТО, не разбираемая по формату, — отказ на непонятой
#      форме (№801), молча пропущенная строка соврала бы мере mark-стража;
#   3. свёрнутая находка, чей №NNN не находится в реестре НА СТРОКЕ с меткой
#      охотника этого трека, — свёртка не смеет терять находки;
#   4. каталог probes/<stem>/ без открытого отчёта <stem>.md — свёрнутый отчёт
#      увозит пробы с собой (история хранит их в git).
#
# ОСТАТОЧНЫЙ РИСК (назван, не спрятан): «находок N» в строке леджера при
# свёртке не сверяется с уже удалённым отчётом — текстовый страж не умеет
# читать историю дёшево. Ложь здесь требует одновременно поддельной строки
# леджера И поддельной метки в реестре — двухслойный умысел, не дрейф.
#
# Самотест: scripts/guards/selftest/test-check-hunter-fold.sh.
export LC_ALL=C
ROOT="${1:-$(cd "$(dirname "$0")/../.." && pwd)}"
REG="$ROOT/docs/plans/221.1-bug-sweep.md"
HUNTS_BASE="${NOVA_HUNTS_DIR:-$ROOT/docs/dev/hunts}"
BASE="${NOVA_HUNTER_FOLD_BASELINE:-$(cd "$(dirname "$0")" && pwd)/hunter-fold.baseline}"

MAX_OPEN=$(grep -E '^max_open=' "$BASE" 2>/dev/null | tail -1 | cut -d= -f2)
if [ -z "$MAX_OPEN" ]; then
    echo "check-hunter-fold: FAIL — нет базы $BASE или в ней нет max_open=N" >&2
    exit 1
fi

rc=0
SUMMARY=""
for TRACK in novac oracle; do
    DIR="$HUNTS_BASE/$TRACK"
    [ -d "$DIR" ] || continue  # отсутствие каталога краснит mark-страж, не этот

    N_OPEN=0
    for f in "$DIR"/*.md; do
        [ -f "$f" ] || continue
        [ "${f##*/}" = "LEDGER.md" ] && continue
        N_OPEN=$((N_OPEN + 1))
    done
    if [ "$N_OPEN" -gt "$MAX_OPEN" ]; then
        echo "check-hunter-fold: FAIL — открытых отчётов трека $TRACK: $N_OPEN, предел $MAX_OPEN — сверни разобранные в $TRACK/LEDGER.md (формат в шапке этого стража)" >&2
        rc=1
    fi

    N_FOLDED=0
    if [ -f "$DIR/LEDGER.md" ]; then
        while IFS= read -r line; do
            case "$line" in "СВЁРНУТО |"*) ;; *) continue ;; esac
            N_FOLDED=$((N_FOLDED + 1))
            if ! printf '%s\n' "$line" | grep -qE '^СВЁРНУТО \| *20[0-9]{2}-[0-9]{2}-[0-9]{2}-[a-z0-9-]+ *\| *[a-z_]+ *\| *К[1-7] *\| *находок [0-9]+ *\|'; then
                echo "check-hunter-fold: FAIL — строка леджера $TRACK не разбирается по формату (№801): $(printf '%s' "$line" | head -c 100)" >&2
                rc=1
                continue
            fi
            nfound=$(printf '%s\n' "$line" | grep -oE 'находок [0-9]+' | grep -oE '[0-9]+')
            refs=$(printf '%s\n' "$line" | awk -F'|' '{print $6}')
            if [ "${nfound:-0}" -gt 0 ]; then
                REFLIST=$(printf '%s\n' "$refs" | grep -oE '№[0-9]+' || true)
                if [ -z "$REFLIST" ]; then
                    echo "check-hunter-fold: FAIL — свёрнуто $nfound находок трека $TRACK без единой ссылки №NNN на реестр: свёртка потеряла находки" >&2
                    rc=1
                else
                    for ref in $REFLIST; do
                        # строка реестра НЕ содержит собственного «№NNN» — она
                        # начинается «| NNN |» (замер 2026-08-30); «№NNN» в
                        # тексте — перекрёстные ссылки ЧУЖИХ строк, по ним
                        # судить нельзя.
                        num=$(printf '%s' "$ref" | grep -oE '[0-9]+')
                        if ! grep -E "^\| *$num \|" "$REG" 2>/dev/null | grep -qE "НАЙДЕНО ОХОТНИКОМ 20[0-9]{2}-[0-9]{2}-[0-9]{2} \($TRACK\)"; then
                            echo "check-hunter-fold: FAIL — леджер $TRACK ссылается на $ref, но строки «| $num |» с меткой охотника ($TRACK) в реестре нет" >&2
                            rc=1
                        fi
                    done
                fi
            else
                if ! printf '%s\n' "$refs" | grep -q '—'; then
                    echo "check-hunter-fold: FAIL — строка леджера $TRACK с «находок 0» обязана нести «—» в поле ссылок" >&2
                    rc=1
                fi
            fi
        done < "$DIR/LEDGER.md"
    fi

    if [ -d "$DIR/probes" ]; then
        for d in "$DIR/probes"/*/; do
            [ -d "$d" ] || continue
            stem=$(basename "$d")
            if [ ! -f "$DIR/$stem.md" ]; then
                echo "check-hunter-fold: FAIL — пробы-сироты probes/$stem/ трека $TRACK без открытого отчёта: свёртка увозит пробы с собой (git хранит)" >&2
                rc=1
            fi
        done
    fi
    SUMMARY="$SUMMARY $TRACK: открыто $N_OPEN/$MAX_OPEN, свёрнуто $N_FOLDED;"
done

if [ "$rc" -eq 0 ]; then
    echo "check-hunter-fold ok:$SUMMARY куча ограничена, свёртка не теряет находок"
fi
exit "$rc"
