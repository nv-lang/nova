#!/usr/bin/env bash
# scripts/tools/with-deadline.sh — запустить команду с ЖЁСТКИМ пределом времени.
#
# ЗАЧЕМ. Реестр 221.1 №475: страж `check-invariant-discipline.sh` завис 2026-08-08
# в 15:32, сжёг 2341 с процессорного времени и держал открытым `scripts/gate.sh`
# ЧЕТЫРЕ ЧАСА. Работа встала целиком, и — что хуже — молча: висящий страж внешне
# неотличим от медленного.
#
# Класс дефекта не «этот страж плох», а «у шага гейта нет предела времени».
# Предел, который надо помнить и дописывать в каждый новый шаг, — это инвариант
# на памяти, и он протечёт (ровно так протекло правило игнорирования .c, №509).
# Поэтому предел ставится ОБЁРТКОЙ, общей для всех шагов, а не в каждом шаге.
#
# ПОВЕДЕНИЕ. Превышение предела — это ОТКАЗ (код 124), а не предупреждение:
# страж, не уложившийся в свой бюджет, ничего не доказал, и считать его
# пройденным нельзя. Ровно эта подмена «не ответил» на «возражений нет» и
# стоила четырёх часов.
#
# ИСПОЛЬЗОВАНИЕ:
#   bash scripts/tools/with-deadline.sh <секунды> <команда> [аргументы...]
#
# САМОПРОВЕРКА (обязательна по конвенции стражей):
#   bash scripts/tools/with-deadline.sh --selftest
set -u
export LC_ALL=C

usage() {
    echo "использование: with-deadline.sh <секунды> <команда> [аргументы...]" >&2
    echo "               with-deadline.sh --selftest" >&2
}

# --------------------------------------------------------------------------
# Самопроверка: обёртка обязана и пропускать быстрое, и рубить зависшее.
# --------------------------------------------------------------------------
if [ "${1:-}" = "--selftest" ]; then
    SELF="$0"
    rc_ok=0
    fails=0

    # 1. Быстрая команда проходит и сохраняет свой код возврата (успех).
    bash "$SELF" 5 true
    rc_ok=$?
    if [ "$rc_ok" -ne 0 ]; then
        echo "selftest FAIL: быстрая успешная команда дала $rc_ok, ожидалось 0" >&2
        fails=$((fails + 1))
    fi

    # 2. Быстрая команда сохраняет НЕнулевой код — обёртка не глотает отказ.
    bash "$SELF" 5 sh -c 'exit 3'
    rc_ok=$?
    if [ "$rc_ok" -ne 3 ]; then
        echo "selftest FAIL: код 3 не дошёл через обёртку, получено $rc_ok" >&2
        fails=$((fails + 1))
    fi

    # 3. Зависшая команда рубится и даёт ровно 124.
    bash "$SELF" 1 sleep 30 >/dev/null 2>&1
    rc_ok=$?
    if [ "$rc_ok" -ne 124 ]; then
        echo "selftest FAIL: зависшая команда дала $rc_ok, ожидалось 124" >&2
        fails=$((fails + 1))
    fi

    # 4. Рубка происходит ПО ПРЕДЕЛУ, а не по завершению команды: `sleep 30`
    #    с пределом 1 с обязан вернуться за считаные секунды, иначе обёртка
    #    ждёт саму команду и предел ничего не значит.
    t0=$(date +%s)
    bash "$SELF" 1 sleep 30 >/dev/null 2>&1
    t1=$(date +%s)
    if [ "$((t1 - t0))" -gt 5 ]; then
        echo "selftest FAIL: предел 1с отработал за $((t1 - t0))с — обёртка ждёт команду" >&2
        fails=$((fails + 1))
    fi

    # 5. Негодные аргументы отвергаются, а не исполняются как попало.
    bash "$SELF" >/dev/null 2>&1
    if [ "$?" -eq 0 ]; then
        echo "selftest FAIL: вызов без аргументов дал успех" >&2
        fails=$((fails + 1))
    fi
    bash "$SELF" не-число true >/dev/null 2>&1
    if [ "$?" -eq 0 ]; then
        echo "selftest FAIL: нечисловой предел дал успех" >&2
        fails=$((fails + 1))
    fi

    # 6-8. ПРЕДЕЛ ПАМЯТИ (задача #6). Модель причины: внук обёртки забирает
    #    300 МБ и спит — как `novac check`, раздувшийся на пачке фаззера; сама
    #    обёртка обязана выжить, снять дерево быстро и назвать память. Хряк
    #    ограничен сам (300 МБ), чтобы сломанный сторож не съел машину прогона.
    #    Только Linux: на прочих ОС сторож не ставится — пропуск говорится.
    checks=5
    if [ "$(uname -s 2>/dev/null)" = Linux ]; then
        checks=8
        hog='awk "BEGIN { for (i = 0; i < 300; i++) a[i] = sprintf(\"%1048576s\", i); system(\"sleep 20\") }"; :'
        t0=$(date +%s)
        err=$(NOVA_MEM_CAP_MB=64 bash "$SELF" 30 sh -c "$hog" 2>&1 >/dev/null)
        rc_ok=$?
        t1=$(date +%s)
        case "$err" in *"ПРЕДЕЛ ПАМЯТИ"*) mem_named=1 ;; *) mem_named=0 ;; esac
        if [ "$rc_ok" -ne 124 ] || [ "$mem_named" -ne 1 ] || [ "$((t1 - t0))" -gt 15 ]; then
            echo "selftest FAIL: дерево сверх предела памяти: rc=$rc_ok (ожидалось 124), строка ПРЕДЕЛ ПАМЯТИ=$mem_named, за $((t1 - t0))с" >&2
            fails=$((fails + 1))
        fi
        # 7. ОБРАТНАЯ: тот же хряк при выключенном стороже (0) снимается
        #    ПРЕДЕЛОМ ВРЕМЕНИ, и строки о памяти нет — слово не звучит зря.
        err=$(NOVA_MEM_CAP_MB=0 bash "$SELF" 3 sh -c "$hog" 2>&1 >/dev/null)
        rc_ok=$?
        case "$err" in *"ПРЕДЕЛ ПАМЯТИ"*) mem_named=1 ;; *) mem_named=0 ;; esac
        if [ "$rc_ok" -ne 124 ] || [ "$mem_named" -ne 0 ]; then
            echo "selftest FAIL: сторож выключен, а исход rc=$rc_ok, строка ПРЕДЕЛ ПАМЯТИ=$mem_named (ожидалось 124 и 0)" >&2
            fails=$((fails + 1))
        fi
        # 8. Законная работа под пределом не снимается: 16 МБ при пределе 256.
        NOVA_MEM_CAP_MB=256 bash "$SELF" 10 sh -c 'awk "BEGIN { for (i = 0; i < 16; i++) a[i] = sprintf(\"%1048576s\", i); system(\"sleep 2\") }"' >/dev/null 2>&1
        rc_ok=$?
        if [ "$rc_ok" -ne 0 ]; then
            echo "selftest FAIL: 16 МБ под пределом по умолчанию сняты, rc=$rc_ok" >&2
            fails=$((fails + 1))
        fi
    else
        echo "with-deadline selftest: пропуск клеток 6-8 (предел памяти) — сторож живёт только на Linux"
    fi

    if [ "$fails" -eq 0 ]; then
        echo "with-deadline selftest: OK ($checks проверок)"
        exit 0
    fi
    echo "with-deadline selftest: ПРОВАЛ, отказов $fails" >&2
    exit 1
fi

# --------------------------------------------------------------------------
# ПРЕДЕЛ ПАМЯТИ — второй ресурс, который шаг не смеет забрать у машины целиком.
#
# ЗАМЕР (2026-10-05, задача #6, реестр 221.1 №TBD). CI run 37319155528 на
# fac09cb9e, обе попытки: шаг `gate-novac.sh` умер кодом 143 через 208 и 267 с
# после начала `novac-heavy` — без строки отказа и без «ОБРЫВ». Тот же блок на
# диагностической ветке (run 37340303025) с замером каждые 5 с: ОДИН процесс
# `novac check` (пачка фаззера из 40 мутаций) рос ~70 МБ/с до 15 ГБ RSS при
# 16 ГБ у раннера; MemAvailable упал до 58 МБ, своп 2.4 ГБ, и раннер снял
# шаг («The operation was canceled» / код 143). Внутренний `timeout 300`
# фаззера до своего предела не дожил — машина кончилась раньше. Ни предел
# времени шага, ни разбор кодов в par_run этого не ловят: снимают не стража,
# а весь job, и снимает его не наш процесс.
#
# Почему RSS дерева, а не `ulimit -v`/`-d`: рантайм Nova резервирует арену
# файберов приватным RW-отображением на 16384 слота на поток
# (compiler-codegen/nova_rt/fiber_arena.c, MAP_NORESERVE) — RLIMIT_AS и
# RLIMIT_DATA считают это адресное пространство, и Карина упала бы на старте.
# Считается то, что машина реально отдала: сумма RSS всех потомков обёртки,
# раз в секунду. Превышение — дерево снимается (TERM, через 3 с KILL), выход
# 124 со строкой «ПРЕДЕЛ ПАМЯТИ»: то же «снят, вердикта нет», что и у
# предела времени, — par_run гейта называет это ОБРЫВОМ этого стража.
#
# Предел: NOVA_MEM_CAP_MB (МБ на дерево обёрнутой команды); 0 — выключен;
# не задан — 60% MemTotal: шаг, которому законно нужно больше, на общей
# машине CI сам по себе опасен. Только Linux: замеренный отказ живёт там
# (раннер GitHub), а `ps -o` и /proc/meminfo — его инструменты; на Windows
# сторож не ставится и обёртка ведёт себя как прежде.
# --------------------------------------------------------------------------
mem_cap_mb() {
    [ "$(uname -s 2>/dev/null)" = Linux ] || return 1
    ps -eo pid=,ppid=,rss= >/dev/null 2>&1 || return 1
    case "${NOVA_MEM_CAP_MB:-}" in
        0) return 1 ;;
        '') ;;
        *[!0-9]*) echo "with-deadline: NOVA_MEM_CAP_MB='$NOVA_MEM_CAP_MB' не число — беру предел по умолчанию" >&2 ;;
        *) echo "$NOVA_MEM_CAP_MB"; return 0 ;;
    esac
    _mt=$(awk '/^MemTotal:/ { print int($2 / 1024) }' /proc/meminfo 2>/dev/null)
    [ -n "$_mt" ] && [ "$_mt" -gt 0 ] || return 1
    echo $(( _mt * 60 / 100 ))
}

# Потомки $1 без поддерева $2 (самого сторожа): «МБ pid pid ...».
mem_tree() {
    ps -eo pid=,ppid=,rss= 2>/dev/null | awk -v r="$1" -v me="$2" '
        { rss[$1] = $3; kids[$2] = kids[$2] " " $1 }
        END {
            t = 0; h = 1
            n = split(kids[r], a, " ")
            for (i = 1; i <= n; i++) if (a[i] != me) q[++t] = a[i]
            while (h <= t) {
                p = q[h++]; s += rss[p]; l = l " " p
                m = split(kids[p], b, " ")
                for (j = 1; j <= m; j++) q[++t] = b[j]
            }
            printf "%d%s\n", s / 1024, l
        }'
}

mem_watch() {   # $1 = корень дерева, $2 = предел МБ, $3 = файл-флаг
    _me=$BASHPID
    _root=$1; _cap=$2; _flag=$3
    while :; do
        sleep 1
        # shellcheck disable=SC2046
        set -- $(mem_tree "$_root" "$_me")
        [ "$#" -ge 2 ] || continue
        [ "$1" -gt "$_cap" ] || continue
        echo "$1" > "$_flag"
        shift
        # СНАЧАЛА ЗАМОРОЗИТЬ, ПОТОМ СНИМАТЬ. Замер самотеста (WSL, клетка 6):
        # процесс, порождённый ПОСЛЕ снимка (`system("sleep 20")` внутри
        # хряка), не попадал в список, после смерти родителя уходил к init и
        # держал канал вывода 20 с. Остановленный процесс детей не плодит —
        # снимок повторяется, пока не перестанет расти.
        _pids=" $* "
        kill -STOP "$@" 2>/dev/null
        for _round in 1 2 3 4 5; do
            _new=""
            for _p in $(mem_tree "$_root" "$_me" | cut -s -d' ' -f2-); do
                case "$_pids" in *" $_p "*) ;; *) _new="$_new $_p" ;; esac
            done
            [ -n "$_new" ] || break
            # shellcheck disable=SC2086
            kill -STOP $_new 2>/dev/null
            _pids="$_pids$_new "
        done
        # TERM даёт ловушкам прибрать за собой; KILL — тем, кто его поймал и
        # жив (фаззер ловит 15 и продолжает цикл), плюс их новым детям.
        # shellcheck disable=SC2086
        kill -TERM $_pids 2>/dev/null
        # shellcheck disable=SC2086
        kill -CONT $_pids 2>/dev/null
        sleep 3
        # shellcheck disable=SC2046,SC2086
        kill -KILL $_pids $(mem_tree "$_root" "$_me" | cut -s -d' ' -f2-) 2>/dev/null
        return 0
    done
}

MEM_FLAG=""
MEM_CAP=""
WATCH_PID=""
mem_watch_start() {
    MEM_CAP=$(mem_cap_mb) || return 0
    MEM_FLAG=$(mktemp "${TMPDIR:-/tmp}/with-deadline-mem.XXXXXX") || { MEM_FLAG=""; return 0; }
    # Сторож отвязан от stdio: его `sleep`, переживший сторожа на секунду,
    # иначе держал бы канал, и каждый `$(with-deadline ...)` ждал бы его.
    mem_watch "$$" "$MEM_CAP" "$MEM_FLAG" </dev/null >/dev/null 2>&1 &
    WATCH_PID=$!
}
mem_watch_stop() {   # сторож снимал дерево -> строка отказа и выход 124
    [ -n "$WATCH_PID" ] || return 0
    # Сработавшего сторожа ДОЖИДАЕМСЯ: убить его посреди снятия значило бы
    # оставить без KILL тех, кто пережил TERM.
    if [ ! -s "$MEM_FLAG" ]; then
        kill "$WATCH_PID" 2>/dev/null
    fi
    wait "$WATCH_PID" 2>/dev/null
    _used=$(cat "$MEM_FLAG" 2>/dev/null)
    rm -f "$MEM_FLAG"
    if [ -n "$_used" ]; then
        echo "with-deadline: ПРЕДЕЛ ПАМЯТИ ${MEM_CAP}МБ ПРЕВЫШЕН — дерево $* заняло ${_used}МБ и снято." >&2
        echo "  Это ОБРЫВ, а не вердикт: шаг ничего не доказал, но и машину не отнял (реестр 221.1 №TBD)." >&2
        exit 124
    fi
}

# --------------------------------------------------------------------------
# Обычный режим.
# --------------------------------------------------------------------------
[ "$#" -ge 2 ] || { usage; exit 2; }

LIMIT="$1"
shift
case "$LIMIT" in
    ''|*[!0-9]*) echo "with-deadline: предел должен быть целым числом секунд, дано: $LIMIT" >&2; exit 2 ;;
esac
[ "$LIMIT" -gt 0 ] || { echo "with-deadline: предел должен быть больше нуля" >&2; exit 2; }

# GNU coreutils `timeout` есть в MSYS2/git-bash — используем его, он умеет
# послать SIGKILL, если процесс не умер по SIGTERM. Без --kill-after висящий
# страж, игнорирующий TERM, пережил бы собственный предел.
mem_watch_start
if command -v timeout >/dev/null 2>&1; then
    timeout --kill-after=10s "${LIMIT}s" "$@"
    rc=$?
    mem_watch_stop "$@"
    if [ "$rc" -eq 124 ] || [ "$rc" -eq 137 ]; then
        echo "with-deadline: ПРЕДЕЛ ${LIMIT}с ПРЕВЫШЕН — $* убит." >&2
        echo "  Это ОТКАЗ, а не предупреждение: шаг ничего не доказал (реестр 221.1 №475)." >&2
        exit 124
    fi
    exit "$rc"
fi

# Запасной путь без coreutils: сторож в фоне.
"$@" &
cmd_pid=$!
(
    sleep "$LIMIT"
    kill -TERM "$cmd_pid" 2>/dev/null
    sleep 10
    kill -KILL "$cmd_pid" 2>/dev/null
) &
watch_pid=$!

wait "$cmd_pid" 2>/dev/null
rc=$?
kill -TERM "$watch_pid" 2>/dev/null
wait "$watch_pid" 2>/dev/null
mem_watch_stop "$@"

# Убит сигналом => код 128+сигнал; TERM(15) и KILL(9) означают наш предел.
if [ "$rc" -eq 143 ] || [ "$rc" -eq 137 ]; then
    echo "with-deadline: ПРЕДЕЛ ${LIMIT}с ПРЕВЫШЕН — $* убит." >&2
    echo "  Это ОТКАЗ, а не предупреждение: шаг ничего не доказал (реестр 221.1 №475)." >&2
    exit 124
fi
exit "$rc"
