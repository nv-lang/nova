#!/usr/bin/env bash
# test-check-single-mco-resume.sh — САМОТЕСТ стража
# `check-single-mco-resume.sh` (221.1 №446/№447, окно presume-cas-gate).
#
# Доказывает ОБА обязательных свойства (план 231 трек Ж §4в):
#   (1) ЛОВИТ нарушение — посторонний mco_resume() вне nova_resume_fiber;
#   (2) НЕ ловит законные случаи — вызов внутри тела nova_resume_fiber,
#       комментарии-упоминания, minicoro.h (третья сторона), test_*.c
#       (standalone unit-тесты миникоро), и настоящую репу nova.
#
# Запуск: scripts/guards/selftest/test-check-single-mco-resume.sh
# Выход: 0 — страж исправен, 1 — страж сломан.

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]:-$0}")" && pwd)"
# Скрипт живёт в scripts/guards/selftest/ — корень репы на три уровня выше.
REPO_ROOT="$(cd "$SCRIPT_DIR/../../.." && pwd)"
GUARD="$REPO_ROOT/scripts/guards/check-single-mco-resume.sh"

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

fails=0
# №765 (2026-08-26): при провале печатаем СЛОВА стража, а не только коды. Все
# три фикстурных случая красны на раннере и зелены на Windows, и по ночному
# логу нельзя было сказать почему — «ожидался 0, получен 1» и всё. Самотест,
# который не говорит, ЧТО увидел, стоит трёх ночей.
LAST_OUT=""
run_guard() { # каталог -> код в $?, вывод в $LAST_OUT
    LAST_OUT="$("$GUARD" "$1" 2>&1)"
}
check() { # имя, ожидаемый_код, фактический_код
    if [ "$2" -eq "$3" ]; then
        echo "  ok: $1"
    else
        echo "  ПРОВАЛ: $1 — ожидался код $2, получен $3" >&2
        if [ -n "${LAST_OUT:-}" ]; then
            printf '%s\n' "$LAST_OUT" | sed 's/^/      | /' >&2
        else
            echo "      | (страж не сказал ничего)" >&2
        fi
        fails=$((fails + 1))
    fi
}

echo "самотест check-single-mco-resume:"

# ── Фикстура «чистая»: единственный вызов внутри nova_resume_fiber ────
mkdir -p "$tmp/clean/compiler-codegen/nova_rt"
cat > "$tmp/clean/compiler-codegen/nova_rt/fibers.h" <<'EOF'
/* fibers.h — fixture stub */
static inline NovaResumeOutcome nova_resume_fiber(mco_coro* co, void* tls_ctx,
                                                    NovaFiberTlsHook restore_inner,
                                                    NovaFiberTlsHook save_inner) {
    NovaResumeOutcome out;
    out.owned = false;
    if (mco_status(co) != MCO_SUSPENDED) {
        return out;
    }
    out.owned = (bool)nova_fiber_state_cas(co, NOVA_FIBER_STATE_IDLE, NOVA_FIBER_STATE_RUNNING);
    if (!out.owned) return out;
    if (restore_inner) restore_inner(tls_ctx);
    mco_result r = mco_resume(co);   /* the ONE legitimate call */
    if (save_inner) save_inner(tls_ctx);
    return out;
}

/* Doc-comment mentioning mco_resume(F) must NOT be counted as a call:
 *   4. Worker pops F twice -> mco_resume(F) on two iterations -> corruption
 */
// another_comment: mco_resume(x) inside a line comment, also not a call

static inline void nova_fiber_run(void (*entry)(mco_coro*), void* user) {
    mco_coro* co = NULL;
    mco_result r = mco_resume(co);
    (void)r;
}
EOF
cat > "$tmp/clean/compiler-codegen/nova_rt/runtime.c" <<'EOF'
/* runtime.c — fixture stub, no direct mco_resume calls */
static void _worker_main(void) {
    NovaResumeOutcome ro = nova_resume_fiber(co, base, restore_fn, save_fn);
    (void)ro;
}
EOF
cat > "$tmp/clean/compiler-codegen/nova_rt/minicoro.h" <<'EOF'
/* vendored third-party lib — defines mco_resume, out of scope */
mco_result mco_resume(mco_coro* co) { return MCO_SUCCESS; }
EOF
cat > "$tmp/clean/compiler-codegen/nova_rt/test_fibers_deep.c" <<'EOF'
/* standalone unit test, out of scope */
static void test_x(void) {
    mco_resume(co);
    mco_resume(co);
}
EOF
run_guard "$tmp/clean"
check "НЕ ловит чистую фикстуру (единственный вызов + allowlist'ы)" 0 $?

# ── Фикстура «грязная»: посторонний mco_resume() в runtime.c ──────────
mkdir -p "$tmp/dirty/compiler-codegen/nova_rt"
cp "$tmp/clean/compiler-codegen/nova_rt/fibers.h" "$tmp/dirty/compiler-codegen/nova_rt/fibers.h"
cp "$tmp/clean/compiler-codegen/nova_rt/minicoro.h" "$tmp/dirty/compiler-codegen/nova_rt/minicoro.h"
cat > "$tmp/dirty/compiler-codegen/nova_rt/runtime.c" <<'EOF'
/* runtime.c — someone opened a NEW resume site, bypassing nova_resume_fiber */
static void _worker_run_one_fiber_NEW(mco_coro* co) {
    mco_result r = mco_resume(co);   /* VIOLATION: bypasses the single gate */
    (void)r;
}
EOF
run_guard "$tmp/dirty"
check "ловит посторонний mco_resume() в runtime.c" 1 $?

# ── Фикстура «сабботаж внутри fibers.h»: mco_resume ВНЕ тела
#    nova_resume_fiber, но в том же файле ────────────────────────────
mkdir -p "$tmp/dirty2/compiler-codegen/nova_rt"
cp "$tmp/clean/compiler-codegen/nova_rt/minicoro.h" "$tmp/dirty2/compiler-codegen/nova_rt/minicoro.h"
cat > "$tmp/dirty2/compiler-codegen/nova_rt/fibers.h" <<'EOF'
static inline NovaResumeOutcome nova_resume_fiber(mco_coro* co, void* tls_ctx,
                                                    NovaFiberTlsHook restore_inner,
                                                    NovaFiberTlsHook save_inner) {
    NovaResumeOutcome out;
    out.owned = true;
    mco_result r = mco_resume(co);
    (void)r;
    return out;
}

/* A SECOND resume function someone added outside the gate. */
static inline void nova_sneaky_resume(mco_coro* co) {
    mco_resume(co);   /* VIOLATION: second call site outside nova_resume_fiber */
}
EOF
run_guard "$tmp/dirty2"
check "ловит второй resume-сайт внутри fibers.h вне тела nova_resume_fiber" 1 $?

# ── Реальная репа nova не считается нарушением (страж не ломает себя) ──
run_guard "$REPO_ROOT"
check "НЕ ловит настоящую репу nova (после фикса №446/№447)" 0 $?

if [ "$fails" -ne 0 ]; then
    echo "самотест ПРОВАЛЕН: $fails свойств(а) стража не выполняются" >&2
    exit 1
fi
echo "самотест ok: страж ловит посторонний mco_resume и не даёт ложных срабатываний"
