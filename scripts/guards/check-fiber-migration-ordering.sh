#!/bin/sh
# scripts/guards/check-fiber-migration-ordering.sh — обычные (неатомарные)
# поля контекста файбера безопасны при миграции между воркерами ТОЛЬКО потому,
# что переход держит пару release/acquire. Страж держит эту пару.
#
# Реестр: docs/plans/221.1-bug-sweep.md №443 (решение 2026-08-14: точечная
# волна «атомарные спутники» до тега). Разбор 2026-08-16: в NovaSpawnCtxBase
# 14 полей, 3 атомарных, 11 обычных — и у всех одиннадцати ОДИН режим доступа:
# пишет и читает сам файбер, между парком и резюмом, возможно на разных
# воркерах. Гонки нет не «по выравниванию int64» (как стоял комментарий), а
# потому что КАЖДАЯ миграция идёт через две двери:
#   уход:   nova_fiber_state_store(IDLE)   → nova_aint_store  → __ATOMIC_RELEASE
#   приход: nova_fiber_state_cas(IDLE→RUNNING) → nova_aint_cas → __ATOMIC_ACQ_REL
# Всё, что файбер писал до ухода, happens-before всего, что он читает после
# прихода. Ослабь любую из двух — и все одиннадцать полей станут гонкой разом,
# а не одно. Поэтому страж смотрит не на поле, а на ДВЕРИ.
#
# ЧТО ПРОВЕРЯЕТСЯ (грепом — это статический инвариант трёх строк):
#   1. nova_aint_store — RELEASE (или SEQ_CST);
#   2. nova_aint_cas — ACQ_REL (или SEQ_CST);
#   3. _nova_fiber_state меняется ТОЛЬКО через nova_fiber_state_store/_cas —
#      никакого прямого __atomic_*/присваивания в обход дверей.
#
# $1 — корень репозитория. Самотест — selftest/test-check-fiber-migration-ordering.sh
export LC_ALL=C
# Корень приводится к АБСОЛЮТНОМУ пути: относительный `.` уводил поиск
# бинаря мимо цели, и страж писал «сломан раннер» о здоровом дереве
# (2026-08-18). Ложная краснота стоит дороже отсутствующей проверки:
# по ней идут искать поломку, которой нет, и в стража перестают верить.
# Если cd не удался — значение СОХРАНЯЕТСЯ как было: пустой ROOT судил бы
# корень файловой системы, а это хуже исходной болезни.
ROOT="${1:-$(dirname "$0")/../..}"
ROOT="$(cd "$ROOT" 2>/dev/null && pwd || printf '%s' "$ROOT")"
NAME=check-fiber-migration-ordering
SYNC="$ROOT/compiler-codegen/nova_rt/sync.h"
FIB="$ROOT/compiler-codegen/nova_rt/fibers.h"
[ -f "$SYNC" ] && [ -f "$FIB" ] || { echo "$NAME: FAIL — нет sync.h/fibers.h под $ROOT" >&2; exit 1; }
FAILED=0

# 1. store — RELEASE. Берём тело функции (до первой '}') и ищем порядок.
body=$(awk '/nova_aint_store\(volatile nova_atomic_int\* p, int32_t v\)/{f=1} f{print} f&&/^}/{exit}' "$SYNC")
if [ -z "$body" ]; then
    echo "$NAME: FAIL — nova_aint_store не найден в sync.h" >&2; FAILED=1
elif ! printf '%s\n' "$body" | grep -qE "__ATOMIC_(RELEASE|SEQ_CST)"; then
    echo "$NAME: FAIL — nova_aint_store без RELEASE: уход файбера с воркера перестал публиковать его поля (№443)" >&2; FAILED=1
fi

# 2. cas — ACQ_REL.
body=$(awk '/static inline bool nova_aint_cas\(/{f=1} f{print} f&&/^}/{exit}' "$SYNC")
if [ -z "$body" ]; then
    echo "$NAME: FAIL — nova_aint_cas не найден в sync.h" >&2; FAILED=1
elif ! printf '%s\n' "$body" | grep -qE "__ATOMIC_(ACQ_REL|SEQ_CST)"; then
    echo "$NAME: FAIL — nova_aint_cas без ACQ_REL: приход файбера на воркер перестал видеть его поля (№443)" >&2; FAILED=1
fi

# 3. Никаких обходов дверей: _nova_fiber_state трогают только три accessor'а
#    (store/cas/load) и объявление. Всё остальное — обход.
stray=$(grep -nE "_nova_fiber_state\b" "$FIB" "$ROOT/compiler-codegen/nova_rt/runtime.c" 2>/dev/null \
    | grep -vE "nova_atomic_int\s+_nova_fiber_state;" \
    | grep -vE "static inline .*nova_fiber_state_(cas|store|load)\(" \
    | grep -vE "nova_aint_(cas|store|load)\(&base->_nova_fiber_state" \
    | grep -vE "^\S+:[0-9]+:\s*(\*|/\*|//)" \
    | grep -vE "^\S+:[0-9]+:\s*$" || true)
if [ -n "$stray" ]; then
    echo "$NAME: FAIL — _nova_fiber_state меняется мимо дверей store/cas (№443):" >&2
    printf '%s\n' "$stray" | sed 's/^/    /' >&2
    FAILED=1
fi

[ "$FAILED" -eq 0 ] || exit 1
echo "$NAME ok: уход RELEASE, приход ACQ_REL, _nova_fiber_state только через две двери — обычные поля контекста файбера безопасны при миграции (№443)"
exit 0
