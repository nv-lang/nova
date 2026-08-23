#!/usr/bin/env bash
# scripts/guards/check-sigsegv-altstack.sh — обработчик переполнения стека
# обязан иметь стек, на котором он может выполниться.
#
# ЗАЧЕМ (реестр 221.1 №745, слияние 2ad3a3cd6). `_arena_sigsegv_handler` в
# `compiler-codegen/nova_rt/fiber_arena.c` стоял с `SA_SIGINFO | SA_NODEFER` и
# БЕЗ `SA_ONSTACK`, а `sigaltstack` не встречался в рантайме нигде. Чтобы
# доставить SIGSEGV, ядро кладёт кадр сигнала НА ТЕКУЩИЙ СТЕК; при переполнении
# стека места там ровно ноль. То есть обработчик, написанный специально под
# переполнение, был единственным, до кого переполнение не могло достучаться.
#
# ЦЕНА, ЗАМЕРЕННАЯ, А НЕ ПРЕДПОЛОЖЕННАЯ: диагностики не было вовсе, процесс рос
# до убийства ядром и оставлял core dump на 140 ГБ, забивавший диск под ноль.
# Windows не затронут — там VEH ловит STATUS_STACK_OVERFLOW, и место на сбойном
# стеке ему не нужно; оттого фикстура `standalone/fiber_stack_overflow` была
# зелёной здесь и красной на Linux годами.
#
# ПОЧЕМУ СТРАЖ, А НЕ ФИКСТУРА. Фикстура `fiber_stack_overflow` существует и
# именно она нашла дефект — но она в `conformance-known-red.list` (вторая
# половина №745: после печати процесс уходит в `do_exit`, ядро пишет core, и
# 64с таймаута не хватает). Значит СЕЙЧАС снятие `SA_ONSTACK` не покраснит
# ничего: фикстура и так числится красной. Механизм, который нельзя сломать
# заметно, — не механизм (Г8). Этот страж и есть заметность, и он снимается,
# когда фикстура вернётся в зелёные.
#
# ПРАВКА ТОГО ЖЕ ДНЯ: №745 ЗАКРЫТ, фикстура ВЕРНУЛАСЬ в зелёные — то
# есть условие снятия, написанное выше, НАСТУПИЛО. Страж остаётся, и честное
# здесь — назвать новую причину, а не оставить старую стоять враньём:
# фикстура сообщает о поломке ТАЙМАУТОМ в 64 секунды и словом «TIMEOUT»,
# из которого причина не читается вовсе — именно так дефект и простоял
# незамеченным. Этот страж говорит то же за миллисекунды и НАЗЫВАЕТ причину.
# Два судьи на одно свойство — не дубль, когда они отвечают с разной
# точностью и за разную цену.
#
# ЧТО ПРОВЕРЯЕТ (обе половины обязательны — одна без другой бесполезна):
#   (1) `sigaction` для SIGSEGV поставлен с флагом `SA_ONSTACK`;
#   (2) альтернативный стек РЕАЛЬНО заводится — в файле есть вызов
#       `sigaltstack(`.
#
# ЧЕГО НЕ ЛОВИТ (сказано честно): что стек заводится на КАЖДОМ потоке, который
# может исполнять файберы. Это свойство порядка вызовов, а не текста; проверить
# его текстом значило бы гадать. Судит его проба с `NOVA_KILL_ALTSTACK=1`.
#
# ИСПОЛЬЗОВАНИЕ:
#   bash scripts/guards/check-sigsegv-altstack.sh [КОРЕНЬ]
#   bash scripts/guards/check-sigsegv-altstack.sh --selftest

set -u
export LC_ALL=C

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

run_check() {
    local root="$1"
    local f="$root/compiler-codegen/nova_rt/fiber_arena.c"
    local fail=0

    if [ ! -f "$f" ]; then
        echo "check-sigsegv-altstack: FAIL — нет $f: страж потерял мишень (класс №519)" >&2
        return 1
    fi

    if ! grep -q "SA_ONSTACK" "$f"; then
        echo "check-sigsegv-altstack: НАРУШЕНИЕ — в $f нет SA_ONSTACK" >&2
        echo "    Обработчик SIGSEGV без SA_ONSTACK НЕ ЗАПУСКАЕТСЯ на переполнении" >&2
        echo "    стека — в том единственном случае, ради которого написан (№745)." >&2
        fail=1
    fi

    if ! grep -q "sigaltstack(" "$f"; then
        echo "check-sigsegv-altstack: НАРУШЕНИЕ — в $f не заводится альтернативный стек" >&2
        echo "    SA_ONSTACK без sigaltstack() не делает ничего: флаг говорит «на" >&2
        echo "    альтернативном стеке», а стека нет (№745)." >&2
        fail=1
    fi

    if [ "$fail" -eq 0 ]; then
        echo "check-sigsegv-altstack ok: обработчик переполнения стека имеет стек, на котором может выполниться"
    fi
    return "$fail"
}

# ── самопроверка: ловит снятие каждой из двух половин, не ложнит на годном ──
run_selftest() {
    local tmp rc
    tmp=$(mktemp -d) || { echo "selftest: mktemp failed" >&2; return 1; }
    trap 'rm -rf "$tmp"' RETURN
    local d="$tmp/pos/compiler-codegen/nova_rt"
    mkdir -p "$d"

    # POS: обе половины на месте — зелено.
    cat > "$d/fiber_arena.c" <<'EOF'
static void arm(void) { sigaltstack(&ss, NULL); }
sa.sa_flags = SA_SIGINFO | SA_NODEFER | SA_ONSTACK;
EOF
    if run_check "$tmp/pos" >/dev/null 2>&1; then
        echo "  ok: годный файл проходит"
    else
        echo "  FAIL: годный файл покраснел (ложняк)"; return 1
    fi

    # NEG1: снят SA_ONSTACK — красно.
    mkdir -p "$tmp/neg1/compiler-codegen/nova_rt"
    cat > "$tmp/neg1/compiler-codegen/nova_rt/fiber_arena.c" <<'EOF'
static void arm(void) { sigaltstack(&ss, NULL); }
sa.sa_flags = SA_SIGINFO | SA_NODEFER;
EOF
    if run_check "$tmp/neg1" >/dev/null 2>&1; then
        echo "  FAIL: снятый SA_ONSTACK прошёл"; return 1
    else
        echo "  ok: ловит снятый SA_ONSTACK"
    fi

    # NEG2: флаг есть, стека нет — красно (флаг без стека не делает ничего).
    mkdir -p "$tmp/neg2/compiler-codegen/nova_rt"
    cat > "$tmp/neg2/compiler-codegen/nova_rt/fiber_arena.c" <<'EOF'
sa.sa_flags = SA_SIGINFO | SA_NODEFER | SA_ONSTACK;
EOF
    if run_check "$tmp/neg2" >/dev/null 2>&1; then
        echo "  FAIL: SA_ONSTACK без sigaltstack прошёл"; return 1
    else
        echo "  ok: ловит SA_ONSTACK без sigaltstack"
    fi

    # NEG3: файла нет — красно, а не «судить нечего».
    mkdir -p "$tmp/neg3"
    if run_check "$tmp/neg3" >/dev/null 2>&1; then
        echo "  FAIL: пропавшая мишень прошла"; return 1
    else
        echo "  ok: пропавшая мишень краснит"
    fi

    echo "check-sigsegv-altstack selftest: ALL OK"
    return 0
}

if [ "${1:-}" = "--selftest" ]; then
    run_selftest
    exit $?
fi

ROOT="${1:-$(cd "$SCRIPT_DIR/../.." && pwd)}"
run_check "$ROOT"
exit $?
