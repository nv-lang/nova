#!/bin/sh
# scripts/tools/novac-fuzz-mutations.sh — the E1 zero-panic acceptance
# (plan 274 §9/Э1, CORE rank; bootstrap §8): mutate a corpus and demand that
# novac check never panics or crashes — honest diagnostics or clean exits
# only, on EVERY input.
#
# Deterministic by construction (no RNG): mutations are (a) truncation at
# every step-th byte, (b) a corruption byte written at every step-th offset,
# (c) a deleted run of step bytes, (d) two adjacent bytes swapped, (e) an
# unbalanced closer injected, (f) doubled file and doubled first half. A red
# run is reproducible by its printed case id alone.
#
# Cost (plan 274.2): all mutations are GENERATED first, then judged in
# BATCHES of CHUNK files (one process per chunk; a single check is ~120ms of
# which most is process start). Chunking is not a nicety: the whole list on
# one command line blows the Windows 32k argument limit the moment the run
# gets dense, and the failure mode is a truncated list judged as green —
# a fuzzer that silently stops fuzzing. Verdict per case = its own
# diagnostic lines; a panic kills its chunk, so a chunk that ends before its
# last case is itself the red signal, and the killer is found by bisection
# over that chunk (rare path, only on a red run).
#
# Usage: sh scripts/tools/novac-fuzz-mutations.sh [step] [corpus-dir] [chunk]
#   step        byte stride of the mutations, default 40 (smaller = denser)
#   corpus-dir  directory scanned for *.nv, default examples/basics
#   chunk       files per novac process, default 150
# Проверялся: Windows (Git Bash), 2026-08-17.
export LC_ALL=C
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
. "$ROOT/scripts/guards/lib/novac.sh"   # novac_is_panic_rc: контракт §7 — только 0/1
# OBA NAPISANIYA BINARYA, i env-override sverh nih (pravka 2026-09-09).
# Bylo `novac.exe` odnoy strokoy: na Linux instrument ne nashyol by nichego i
# skazal by ob etom "net $NOVAC" -- gromko, no o NE TOY prichine. Za dvoe sutok
# eto CHETVYORTYY nositel odnoy slepoty (dezhurstvo kontrolyora, novac-corpus-diff,
# machine-watch, teper zdes) -- reestr No847.
# NOVAC_BIN/ORACLE_BIN sverhu nuzhny ne tolko dlya perenosimosti: oni ZASHIVAYUT
# SHOV, cherez kotoryy samotest podsovyvaet zaglushki i proveryaet klassifikaciyu
# krasnyh v obe storony. Bez shva novaya faza emit byla by nedokazuema.
NOVAC="${NOVAC_BIN:-}"
if [ -z "$NOVAC" ]; then
    NOVAC="$ROOT/novac/target/novac.exe"
    [ -x "$NOVAC" ] || NOVAC="$ROOT/novac/target/novac"
fi
STEP="${1:-40}"
CORPUS="${2:-$ROOT/examples/basics}"
CHUNK="${3:-150}"
T="${TMPDIR:-/tmp}/novac-fuzz.$$"
mkdir -p "$T/cases"
trap 'rm -rf "$T"' 2 15
[ -f "$NOVAC" ] || { echo "novac-fuzz: нет $NOVAC" >&2; exit 2; }
[ -d "$CORPUS" ] || { echo "novac-fuzz: нет корпуса $CORPUS" >&2; exit 2; }

# ---- 1. generate all cases (pure file ops, no compiler) ------------------
n=0
srcn=0
# ГЕНЕРАЦИЯ — ОДИН ПРОЦЕСС, А НЕ ДВА НА КАЖДЫЙ ФАЙЛ (правка 2026-09-16).
#
# Здесь стояли шелл-циклы: каждая мутация писалась парой `head`/`tail`, а у
# swap — четырьмя. Замер: шаг 400, корпус examples/basics — 724 файла за
# 165 560 мс; тот же набор питоновским генератором — 861 мс. Сто девяносто
# два раза, и это ВСЯ цена стража: на шаге 160 он шёл 901 с и был снят
# пределом, а на CI съедал 37 из 45 минут job'а (реестр №1138).
#
# ПОКРЫТИЕ НЕ ТРОНУТО: виды мутаций, смещения и ИМЕНА файлов повторены один
# в один, содержимое совпадает побайтно — проба сравнивает наборы, а не
# намерения. Дешевеет ФОРМА, а не предмет.
# srcn нужен ВЕРДИКТУ («n мутаций, srcn файлов корпуса»), а цикл, который его
# считал, снят вместе с генерацией. Без этой строки вердикт печатал бы ноль —
# то есть соврал бы о том, что мерил.
srcn=$(find "$CORPUS" -type f -name '*.nv' | wc -l | tr -d " ")
[ "$srcn" -gt 0 ] || { echo "novac-fuzz: в $CORPUS нет ни одного .nv" >&2; exit 2; }
GEN="$ROOT/scripts/tools/novac-fuzz-gen.py"
[ -f "$GEN" ] || { echo "novac-fuzz: нет генератора $GEN" >&2; exit 2; }
n=$(python "$GEN" "$CORPUS" "$T/cases" "$STEP") || {
    echo "novac-fuzz: генератор мутаций отказал" >&2; exit 2; }
case "$n" in ""|*[!0-9]*) echo "novac-fuzz: генератор не назвал число файлов" >&2; exit 2 ;; esac
[ "$n" -gt 0 ] || { echo "novac-fuzz: генератор не создал ни одного случая" >&2; exit 2; }
ls "$T/cases" | sort > "$T/list"

# ---- 2. batched check; a panic/hang/crash of a process is red ------------
judge() {   # $1 = list file; 0 = green, 1 = something died
    # shellcheck disable=SC2046
    ( cd "$T/cases" && NOVA_STD_PATH="$ROOT/std/src" timeout 300 "$NOVAC" check $(cat "$1") ) \
        > "$T/out" 2> "$T/err"
    rc=$?
    if novac_is_panic_rc "$rc" || grep -qi "panic" "$T/err"; then
        return 1
    fi
    return 0
}
# ---- 2b. THE SAME CASES THROUGH EMIT ------------------------------------
# ZACHEM. Do 2026-09-09 priyomka E1 ranga CORE ("novac ne padaet ni na odnom
# vhode") konchalas' na `check`: mutaciya, proshedshaya proverku tipov,
# ob'yavlyalas' zelyonoy i dal'she ne shla. Emissiya gonyalas' TOL'KO po korpusu,
# to est' po zavedomo pravil'nym faylam. Peresechenie "neobychnyy vhod x stadiya
# emissii" ne osmatrival nikto -- a tam 43 `ice()` v `emit_c/` i 16 v `lower/`,
# i kazhdyy utverzhdaet "syuda popast' nel'zya".
#
# TRI KORZINY, I RAZVEDENY ONI ZDES', A NE PRI RAZBORE OTCHYOTA. Dve pervye
# nazval ya, TRET'YU -- okno 274, i bez neyo ih zhe sluchai (`println('z')`,
# u64-slot interpolyacii) uehali by v "shum":
#
#   LEGAL/NOVAC   check prinyal, emit upal, ORAKUL SOBRAL. Forma zakonna,
#                 chekker prav, nezakryta chast' podmnozhestva v EMISSII. Hudshiy
#                 klass i samyy chastyy: granicu podmnozhestva obyazan derzhat'
#                 CHEKKER, a ne otkaz emittera.
#   CHECKER       check prinyal, emit upal, ORAKUL OTKAZAL. Forma nezakonna --
#                 znachit chekker novac prinyal to, chto ne dolzhen byl. Nahodka
#                 o CHEKKERE, chinitsya v drugom meste.
#   NOT-OUR-CASE  check otkazal, a emit vsyo ravno upal. V zhivom konveyere takoy
#                 fayl do emissii ne doydyot; nahodka o tom, chto emit zapustili.
#
# ORAKUL ZDES' SUD'YA, i eto metod okna 274, a ne moya vydumka: on otvechaet na
# vopros, na kotoryy sam novac otvetit' ne mozhet -- zakonna li forma. Esli
# binarya orakula net, sluchay pomechaetsya `ORACLE-ABSENT` i schitaetsya
# NERAZOBRANNYM: molchalivoe otnesenie v luchshuyu korzinu -- eto nezakrytyy
# defekt, odetyy v uspeh.
#
# POCHEMU POFAYLOVO, a ne partiey kak `check`: `novac emit` beryot ODIN fayl i
# pishet v stdout (novac-emit-corpus.sh). Cena izmerena oknom 274: korpus iz 150
# faylov -- sekundy. Orakul zovyotsya TOL'KO dlya krasnyh, poetomu na zelyonom
# progone ne stoit nichego.

# Oba napisaniya binarya: znat' tol'ko `nova.exe` -- tretiy nositel' etoy slepoty
# za dvoe sutok (reestr No847).
ORACLE="${ORACLE_BIN:-}"
if [ -z "$ORACLE" ]; then
    ORACLE="$ROOT/nova-cli/target/release/nova.exe"
    [ -x "$ORACLE" ] || ORACLE="$ROOT/nova-cli/target/release/nova"
fi

judge_emit() {   # $1 = list file; 0 = zelyono, 1 = chto-to umerlo pri emissii
    # ПАРАЛЛЕЛЬНО, А НЕ В ОЧЕРЕДЬ (правка 2026-09-16, реестр №1138).
    #
    # Здесь стоял `while read` с ОДНИМ вызовом `novac emit` на файл: на шаге
    # 160 корпус даёт 1372 случая, то есть 1372 запуска процесса ПОДРЯД, при
    # том что соседний `judge` зовёт `novac check` пачками по 150. Замер:
    # после починки генерации страж всё равно упирался в 900с, и остаток —
    # целиком здесь.
    #
    # ПОКРЫТИЕ НЕ ТРОНУТО: те же случаи, тот же предел 60с на каждый, то же
    # различение снятия (124) и паники. Меняется только то, что независимые
    # запуски идут восемью потоками вместо одного. Случаи независимы по
    # построению — каждый это отдельный файл и отдельный процесс.
    emit_red=""
    _ej="${NOVAC_FUZZ_JOBS:-8}"
    _ed="$T/emit"
    rm -rf "$_ed"; mkdir -p "$_ed"
    _ei=0
    _erun=0
    while IFS= read -r case_nv; do
        [ -n "$case_nv" ] || continue
        _ei=$((_ei + 1))
        printf "%s" "$case_nv" > "$_ed/$_ei.name"
        (
            cd "$T/cases" && NOVA_STD_PATH="$ROOT/std/src" timeout 60 \
                "$NOVAC" emit "$case_nv" > /dev/null 2> "$_ed/$_ei.err"
            rc=$?
            # ТРЕТЬЕ СЛОВО СТОИТ РЯДОМ С ВЫЗОВОМ, а не через тридцать строк:
            # так его видит и человек, и страж «падение по таймауту неотличимо
            # от настоящего отказа». Разбор ниже остаётся — он собирает исходы
            # со всех потоков; здесь же исход НАЗЫВАЕТСЯ в тот момент, когда
            # случился.
            [ "$rc" -eq 124 ] && echo "СНЯТ ПРЕДЕЛОМ 60с: вердикта нет" >> "$_ed/$_ei.err"
            echo "$rc" > "$_ed/$_ei.rc"
        ) &
        _erun=$((_erun + 1))
        if [ "$_erun" -ge "$_ej" ]; then wait; _erun=0; fi
    done < "$1"
    wait
    _ek=1
    while [ "$_ek" -le "$_ei" ]; do
        read -r case_nv < "$_ed/$_ek.name"
        erc=""
        [ -f "$_ed/$_ek.rc" ] && read -r erc < "$_ed/$_ek.rc"
        # ТРЕТЬЕ СЛОВО У ПРЕДЕЛА (Г16): 124 — СНЯТИЕ ПО ВРЕМЕНИ, а не отказ и
        # не паника. Для фаззера зависание — тоже находка (приёмка Э1 говорит
        # «не падает и не виснет»), но НАЗЫВАТЬСЯ обязано своим именем: иначе
        # чинящий пойдёт искать панику там, где процесс просто не успел.
        # Пустой код — тоже не вердикт: процесс не оставил ответа.
        if [ -z "$erc" ]; then
            echo "novac-fuzz: EMIT не оставил кода возврата на $case_nv: вердикта нет" >&2
            emit_red="$emit_red $case_nv"
        elif [ "$erc" -eq 124 ]; then
            echo "novac-fuzz: EMIT СНЯТ ПРЕДЕЛОМ 60с на $case_nv: вердикта нет, это зависание" >&2
            emit_red="$emit_red $case_nv"
        elif novac_is_panic_rc "$erc" || grep -qi "panic" "$_ed/$_ek.err"; then
            emit_red="$emit_red $case_nv"
        fi
        _ek=$((_ek + 1))
    done
    rm -rf "$_ed"
    [ -z "$emit_red" ]
}

# Klassifikaciya vinovnika: dva lishnih zapuska TOL'KO dlya krasnogo sluchaya.
classify_emit_red() {   # $1 = imya sluchaya; pechataet imya korziny
    ( cd "$T/cases" && NOVA_STD_PATH="$ROOT/std/src" timeout 60 \
        "$NOVAC" check "$1" ) > /dev/null 2> "$T/cerr"
    crc=$?
    # То же третье слово: снятый по времени `check` не говорит о случае НИЧЕГО,
    # и отнести его в корзину значило бы выдать незнание за ответ.
    if [ "$crc" -eq 124 ]; then
        echo "CHECK-TIMED-OUT"; return
    fi
    if novac_is_panic_rc "$crc" || grep -qi "panic" "$T/cerr"; then
        echo "CHECK-TOO"; return          # padaet i check -- sud'ya vyshe ego lovit
    fi
    if [ "$crc" -ne 0 ]; then
        echo "NOT-OUR-CASE"; return
    fi
    if [ ! -x "$ORACLE" ]; then
        echo "ORACLE-ABSENT"; return     # NE "zelyono": sluchay ne razobran
    fi
    ( cd "$T/cases" && timeout 300 "$ORACLE" build "$1" -o "$T/oracle.out" ) \
        > /dev/null 2>&1
    orc=$?
    # ТРЕТЬЕ СЛОВО (Г16): снятый по времени оракул НЕ СКАЗАЛ, законна ли форма.
    # Отнести такой случай в `CHECKER` значило бы обвинить чекер на основании
    # молчания судьи — то есть выдать незнание за ответ. Отдельная корзина.
    if [ "$orc" -eq 124 ]; then echo "ORACLE-TIMED-OUT"; return; fi
    if [ "$orc" -eq 0 ]; then echo "LEGAL/NOVAC"; else echo "CHECKER"; fi
}

split -l "$CHUNK" "$T/list" "$T/chunk." 2>/dev/null || {
    awk -v C="$CHUNK" -v P="$T/chunk." 'NR%C==1{f=sprintf("%s%03d",P,int(NR/C))} {print > f}' "$T/list"
}
red=""
for c in "$T"/chunk.*; do
    judge "$c" || red="$red $c"
done
if [ -z "$red" ]; then
    # CHECK zelyon. Teper TA ZHE partiya cherez EMIT -- stadiya, kotoruyu eta
    # priyomka ne osmatrivala nikogda (sm. dlinnyy kommentariy vyshe).
    emit_bad=0
    emit_report=""
    for c in "$T"/chunk.*; do
        case "$c" in *.a|*.b) continue ;; esac
        judge_emit "$c" || {
            for case_nv in $emit_red; do
                bucket=$(classify_emit_red "$case_nv")
                emit_bad=$((emit_bad+1))
                emit_report="$emit_report$bucket $case_nv
"
                cp "$T/cases/$case_nv" "$T/keep.emit.$case_nv" 2>/dev/null || true
            done
        }
    done
    if [ "$emit_bad" -gt 0 ]; then
        echo "novac-fuzz: EMIT upal na $emit_bad sluchayah iz $n" >&2
        printf "%s" "$emit_report" | sed "s/^/    /" >&2
        echo "    LEGAL/NOVAC   = forma zakonna, chekker prav: nezakryta chast podmnozhestva v EMISSII" >&2
        echo "    CHECKER       = orakul otkazal: chekker novac prinyal to, chto ne dolzhen byl" >&2
        echo "    NOT-OUR-CASE  = check otkazal, emit ne dolzhen byl zapuskatsya" >&2
        echo "    ORACLE-ABSENT = sluchay NE RAZOBRAN, a ne zelyon" >&2
        echo "    Sluchai sohraneny: $T/keep.emit.*" >&2
        exit 1
    fi
    rm -rf "$T"
    echo "novac-fuzz ok: $n мутаций ($srcn файлов корпуса, шаг $STEP), паник 0"
    exit 0
fi

# ---- 3. red: bisect the guilty chunks to name the killers ----------------
echo "novac-fuzz: чанк(и) красные (rc/паника) — ищу виновников бисекцией" >&2
bad=0
bisect() {   # $1 = list file
    cnt=$(wc -l < "$1")
    if [ "$cnt" -le 1 ]; then
        cid=$(cat "$1")
        bad=$((bad+1))
        echo "novac-fuzz: PANIC/CRASH/HANG на случае ${cid%.nv}" >&2
        head -2 "$T/err" | sed 's/^/    /' >&2
        cp "$T/cases/$cid" "$T/keep.$cid"
        return
    fi
    half=$((cnt/2))
    head -n "$half" "$1" > "$1.a"; tail -n +$((half+1)) "$1" > "$1.b"
    judge "$1.a" || bisect "$1.a"
    judge "$1.b" || bisect "$1.b"
}
for c in $red; do bisect "$c"; done
echo "novac-fuzz: FAIL — паник $bad из $n (репро в $T/keep.*)" >&2
exit 1
