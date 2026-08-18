#!/bin/sh
# scripts/guards/check-novac-match-exhaustive.sh — `match` по сумме novac
# покрывает ВСЕ её варианты (конвенция П21, вторая половина).
#
# ЗАЧЕМ ЭТОТ ФАЙЛ ПОЯВИЛСЯ. Соседний страж `check-novac-no-default-branch`
# прямо писал в шапке: «НЕ ПРОВЕРЯЕТ полноту самого match — это работа
# компилятора». Посылка оказалась ЛОЖНОЙ. Проба 2026-08-16 (окно 274):
#
#     type Color enum | Red | Green | Blue
#     fn name(c Color) -> str { match c { Red => "r"  Green => "g" } }
#     fn main() { println(name(Color.Blue)) }
#
# `nova check` — ok, сборка — ok, запуск печатает ПУСТУЮ строку и выходит с
# кодом 0. Ни ошибки компиляции, ни ошибки рантайма: непокрытый вариант
# молча даёт пустое значение. Это №652 на уровне языка, и он снимает опору
# из-под всей конвенции П21 («новый вариант — ошибка компиляции ровно там,
# где обязаны решить»). Дефект эскалирован; до его закрытия эту работу
# делает страж — иначе «исчерпывающий match» в novac держится на честном
# слове автора.
#
# ПРОВЕРЯЕТ по novac/src/**/*.nv (тесты включены: тест с дырявым match врёт
#   так же, как код):
#   * собирает суммы: `type NAME enum` + строки-варианты `| Var`, `| Var(T)`,
#     `| Var { ... }`;
#   * собирает `match ... {` вместе с именами армов (учитывая or-образцы
#     `A | B =>` и payload `Var(x)` / `Var { .. }`);
#   * если множество армов ЦЕЛИКОМ ложится в одну известную сумму и при этом
#     покрывает её не полностью — красный с перечислением недостающих.
# НЕ ПРОВЕРЯЕТ: match по не-суммам (литералы, строки, Option из std —
#   кандидата нет, судить нечем: такие пропускаются и считаются отдельным
#   числом, чтобы «ничего не нашлось» не выглядело зелёным); достижимость
#   армов; вложенные образцы глубже первого уровня.
#
# $1 — корень репозитория; $2 — override сканируемой директории (шов самотеста).
# Проверялся: Windows (Git Bash), 2026-08-16.
export LC_ALL=C
# Корень приводится к АБСОЛЮТНОМУ пути: относительный `.` уводил поиск
# бинаря мимо цели, и страж писал «сломан раннер» о здоровом дереве
# (2026-08-18). Ложная краснота стоит дороже отсутствующей проверки:
# по ней идут искать поломку, которой нет, и в стража перестают верить.
# Если cd не удался — значение СОХРАНЯЕТСЯ как было: пустой ROOT судил бы
# корень файловой системы, а это хуже исходной болезни.
ROOT="${1:-$(dirname "$0")/../..}"
ROOT="$(cd "$ROOT" 2>/dev/null && pwd || printf '%s' "$ROOT")"
SRC="${2:-$ROOT/novac/src}"
NAME=check-novac-match-exhaustive

if [ ! -d "$SRC" ]; then
    echo "$NAME ok: судить нечего (нет $SRC)"
    exit 0
fi

T="${TMPDIR:-/tmp}/novac-match-exh.$$"
mkdir -p "$T" || exit 1
trap 'rm -rf "$T"' 0

find "$SRC" -type f -name '*.nv' | sort > "$T/files"
if [ ! -s "$T/files" ]; then
    echo "$NAME: FAIL — в $SRC нет ни одного .nv: страж потерял мишень (класс №519)" >&2
    exit 1
fi

# --- суммы: "СУММА вариант" ------------------------------------------------
# ОДИН cat и ОДИН tr на всё дерево вместо `tr` на каждый файл (2026-08-19).
xargs cat < "$T/files" | tr -d '\r' | awk '
    /^(export )?type [A-Z][A-Za-z0-9_]* enum/ {
        for (i = 1; i <= NF; i++) if ($i == "enum") sum = $(i-1)
        next
    }
    sum != "" && /^[[:space:]]*\|[[:space:]]*[A-Z]/ {
        line = $0
        sub(/^[[:space:]]*\|[[:space:]]*/, "", line)
        # имя варианта — до "(", "{" или пробела
        if (match(line, /^[A-Za-z0-9_]+/)) print sum " " substr(line, RSTART, RLENGTH)
        next
    }
    # Комментарий ВНУТРИ перечисления его не заканчивает. Без этой строки
    # `///`-док у варианта обрывал сбор: у TokenKind собиралось 26 имён из
    # 64, после чего ни один match по нему не опознавался как match по этой
    # сумме, и все они уходили «вне суда» — молча, числом в ЗЕЛЁНОЙ строке
    # (2026-08-18). Оракул их тоже не судит: арм с OR-группой отключает у
    # него E_MATCH_NON_EXHAUSTIVE. Значит не проверял никто.
    sum != "" && /^[[:space:]]*\/\// { next }
    sum != "" && /^[[:space:]]*$/ { next }
    sum != "" { sum = "" }
' | sort -u > "$T/variants"

NSUM=$(cut -d' ' -f1 "$T/variants" | sort -u | wc -l | tr -d '[:space:]')
if [ "$NSUM" -eq 0 ]; then
    echo "$NAME: FAIL — не найдено ни одной суммы: разбор сломался, а молчать нельзя (класс №519)" >&2
    exit 1
fi

# --- матчи: "файл:строка армы..." -----------------------------------------
: > "$T/matches"
while IFS= read -r f; do
    rel=${f#"$SRC"/}
    tr -d '\r' < "$f" | awk -v REL="$rel" '
        function indent_of(s,   n) { n = 0; while (substr(s, n + 1, 1) == " ") n++; return n }
        {
            line = $0
            ind = indent_of(line)
            body = line; sub(/^[[:space:]]+/, "", body)
            # закрытие текущего match
            while (depth > 0 && body ~ /^\}/ && ind <= starts[depth]) {
                printf "%s:%d %s\n", REL, lines[depth], arms[depth]
                depth--
            }
            if (body ~ /^match .*\{[[:space:]]*$/ || body ~ /^match .*\{$/) {
                depth++; starts[depth] = ind; lines[depth] = NR; arms[depth] = ""; pend[depth] = ""
                next
            }
            # ПЕРЕНЕСЁННЫЙ арм: `A | B |` и продолжение ниже. Собирался
            # только до первой строки, и match с длинной OR-группой уходил
            # «вне суда» молча (2026-08-18). Оракул его тоже не судит --
            # арм с OR-группой отключает у него E_MATCH_NON_EXHAUSTIVE, --
            # так что пропуск здесь означал, что не проверяет НИКТО.
            if (depth > 0 && ind == starts[depth] + 4 && body !~ /=>/ &&
                body ~ /\|[[:space:]]*$/) {
                pend[depth] = pend[depth] " " body
                next
            }
            if (depth > 0 && ind == starts[depth] + 4 && body ~ /=>/) {
                head = pend[depth] " " body; pend[depth] = ""
                sub(/=>.*$/, "", head)
                # or-образцы: A | B | C
                n = split(head, parts, /\|/)
                for (i = 1; i <= n; i++) {
                    pat = parts[i]
                    gsub(/^[[:space:]]+|[[:space:]]+$/, "", pat)
                    if (match(pat, /^[A-Za-z_][A-Za-z0-9_]*/)) {
                        nm = substr(pat, RSTART, RLENGTH)
                        arms[depth] = arms[depth] " " nm
                    }
                }
            }
        }
        END { while (depth > 0) { printf "%s:%d %s\n", REL, lines[depth], arms[depth]; depth-- } }
    ' >> "$T/matches"
done < "$T/files"

# --- суд: ОДИН проход awk ---------------------------------------------------
# Первая версия звала awk+comm на каждую пару (match, сумма): 48 x 10 = почти
# пятьсот процессов и четыре минуты стены. Цена цикла — правило П14 и
# отдельный страж, поэтому суд собран в один проход: сначала читаются
# варианты, потом матчи, решение принимается в памяти.
awk '
    FILENAME == VARS {
        vars[$1] = vars[$1] " " $2
        cnt[$1]++
        next
    }
    {
        loc = $1
        n = split($0, f, " ")
        delete arms
        na = 0
        wild = 0
        for (i = 2; i <= n; i++) {
            a = f[i]
            if (a == "") continue
            if (a == "_") { wild = 1; continue }
            if (!(a in arms)) { arms[a] = 1; na++ }
        }
        if (wild || na == 0) { skip++; next }
        cand = ""; ncand = 0
        for (s in vars) {
            ok = 1
            for (a in arms) {
                if (index(" " vars[s] " ", " " a " ") == 0) { ok = 0; break }
            }
            if (ok) { cand = s; ncand++ }
        }
        if (ncand != 1) { skip++; next }
        judged++
        miss = ""
        m = split(vars[cand], vs, " ")
        for (i = 1; i <= m; i++) {
            if (vs[i] != "" && !(vs[i] in arms)) miss = miss " " vs[i]
        }
        if (miss != "") printf "  %s — match по сумме %s не покрывает:%s\n", loc, cand, miss > "/dev/stderr"
    }
    END { printf "%d %d\n", judged, skip }
' VARS="$T/variants" "$T/variants" "$T/matches" > "$T/stat" 2> "$T/bad"

NJUDGED=$(cut -d' ' -f1 "$T/stat")
NSKIP=$(cut -d' ' -f2 "$T/stat")
[ -n "$NJUDGED" ] || NJUDGED=0
[ -n "$NSKIP" ] || NSKIP=0

if [ -s "$T/bad" ]; then
    echo "$NAME: FAIL — match по сумме novac оставляет варианты без ответа (П21):" >&2
    cat "$T/bad" >&2
    echo "  Оракул это НЕ ловит (проба 2026-08-16: непокрытый вариант молча даёт" >&2
    echo "  пустое значение и код 0), поэтому решает страж: назови ветку для" >&2
    echo "  каждого варианта — хоть ice(), но осознанно." >&2
    exit 1
fi

NM=$(wc -l < "$T/matches" | tr -d '[:space:]')
echo "$NAME ok: сумм $NSUM, match'ей $NM — судимых по сумме $NJUDGED (все полные), вне суда $NSKIP"
exit 0
