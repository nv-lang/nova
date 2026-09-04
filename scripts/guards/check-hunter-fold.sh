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
# ЧЕГО ЛЕДЖЕР НЕ СМЕЕТ (дыры, пробитые панелью критиков 2026-08-30 ЗАПУСКОМ —
# каждая опускала храповик покрытия или надувала знаменатель меры одной
# рукописной строкой, без охоты):
#   5. строка о свёртке отчёта, которого НИКОГДА НЕ БЫЛО в истории по пути
#      docs/dev/hunts/<трек>/<stem>.md — фантомная охота: «находок 0 | —»
#      закрывала клетку карты навсегда без отчёта, проб и реестра;
#   6. ссылка №NNN, повторённая в двух строках леджера, или взятая у ЧУЖОЙ
#      охоты: дата метки строки реестра обязана совпадать с датой в стеме —
#      иначе чужая честная строка «подтверждала» выдуманную свёртку;
#   7. ссылок больше, чем «находок N»: несколько находок на одну строку реестра
#      законны (так и было с lex×К2), обратное — нет.
#
# ОСТАТОЧНЫЙ РИСК (назван, не спрятан): само ЧИСЛО «находок N» при свёртке не
# сверяется с текстом удалённого отчёта — читать историю на каждый прогон
# дорого. Соврать им можно только вместе с поддельной строкой реестра, чья
# метка датирована днём охоты, — двухслойный умысел, не дрейф.
#
# Самотест: scripts/guards/selftest/test-check-hunter-fold.sh.
export LC_ALL=C
ROOT="${1:-$(cd "$(dirname "$0")/../.." && pwd)}"
REG="$ROOT/docs/plans/221.1-bug-sweep.md"
HUNTS_BASE="${NOVA_HUNTS_DIR:-$ROOT/docs/dev/hunts}"
BASE="${NOVA_HUNTER_FOLD_BASELINE:-$(cd "$(dirname "$0")" && pwd)/hunter-fold.baseline}"

# Ключ читается СТРОГО один раз: дописанная в конец вторая строка побеждала бы
# при `tail -1`, оставляя число владельца в файле на виду (дыра панели).
NKEY=$(grep -cE '^max_open=' "$BASE" 2>/dev/null || true)
if [ "${NKEY:-0}" -ne 1 ]; then
    echo "check-hunter-fold: FAIL — в базе $BASE ключ max_open= встречается ${NKEY:-0} раз(а), должен ровно один" >&2
    exit 1
fi
MAX_OPEN=$(grep -E '^max_open=' "$BASE" | cut -d= -f2)

rc=0
SUMMARY=""
TSEEN="${TMPDIR:-/tmp}/hunter-fold-refs.$$"
trap 'rm -f "$TSEEN"' 0 2 15
for TRACK in novac oracle guards; do
    DIR="$HUNTS_BASE/$TRACK"
    : > "$TSEEN"
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
            stem=$(printf '%s\n' "$line" | awk -F'|' '{v=$2; gsub(/[ \t]/, "", v); print v}')
            sdate=$(printf '%s' "$stem" | cut -c1-10)

            # (5) свёрнутый отчёт обязан был СУЩЕСТВОВАТЬ в истории
            if [ -z "$(git -C "$ROOT" log --diff-filter=A --format=%H -1 -- "docs/dev/hunts/$TRACK/$stem.md" 2>/dev/null)" ]; then
                echo "check-hunter-fold: FAIL — леджер $TRACK сворачивает отчёт «$stem», которого никогда не было в истории по пути docs/dev/hunts/$TRACK/$stem.md: фантомная охота закрывает клетку без охоты" >&2
                rc=1
            fi
            if [ "${nfound:-0}" -gt 0 ]; then
                REFLIST=$(printf '%s\n' "$refs" | grep -oE '№[0-9]+' || true)
                if [ -z "$REFLIST" ]; then
                    echo "check-hunter-fold: FAIL — свёрнуто $nfound находок трека $TRACK без единой ссылки №NNN на реестр: свёртка потеряла находки" >&2
                    rc=1
                else
                    NREF=$(printf '%s\n' "$REFLIST" | grep -c . || true)
                    # (7) ссылок не больше находок
                    if [ "${NREF:-0}" -gt "$nfound" ]; then
                        echo "check-hunter-fold: FAIL — в строке леджера $TRACK ссылок ($NREF) больше, чем находок ($nfound): несколько находок на одну строку реестра законны, обратное — нет" >&2
                        rc=1
                    fi
                    for ref in $REFLIST; do
                        # строка реестра НЕ содержит собственного «№NNN» — она
                        # начинается «| NNN |» (замер 2026-08-30); «№NNN» в
                        # тексте — перекрёстные ссылки ЧУЖИХ строк, по ним
                        # судить нельзя.
                        num=$(printf '%s' "$ref" | grep -oE '[0-9]+')
                        # (6) метка строки реестра датирована днём ЭТОЙ охоты
                        if ! grep -E "^\| *$num \|" "$REG" 2>/dev/null | grep -qF "НАЙДЕНО ОХОТНИКОМ $sdate ($TRACK)"; then
                            echo "check-hunter-fold: FAIL — леджер $TRACK ссылается на $ref, но строки «| $num |» с меткой «НАЙДЕНО ОХОТНИКОМ $sdate ($TRACK)» в реестре нет: ссылка на чужую охоту или на несуществующую строку" >&2
                            rc=1
                        fi
                        # (6) та же ссылка в двух строках леджера
                        if grep -q "^$num\$" "$TSEEN"; then
                            echo "check-hunter-fold: FAIL — ссылка $ref повторена в двух строках леджера $TRACK: свёртка отобразила разные охоты на одну строку реестра" >&2
                            rc=1
                        fi
                        echo "$num" >> "$TSEEN"
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
