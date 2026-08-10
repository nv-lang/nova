#!/usr/bin/env bash
# scripts/guards/check-checker-entrypoints.sh — новый прямой вызов check_module*
# в обход prepare_module_for_check краснеет гейт.
#
# ЗАЧЕМ (реестр 221.1 №531, план 262 Ф.А.1-bis): у чекера несколько точек
# входа (`nova check`, `nova build`, `nova test`, доктест-раннер, LSP), и
# каждая раньше собирала свой список проходов «резолв импортов + embed_resolve
# + alpha_rename + number_exprs» по памяти того, кто её писал. `nova-lsp/src/
# compiler.rs` звал `resolve_imports_inline` (без `*_test.nv` соседей) и НЕ
# звал `resolve_embeds` вовсе — шесть файлов с верным кодом краснели в
# редакторе, хотя `nova check` давал `rc=0`. Тем же утром окно `p-novadoc`
# нашло тот же класс в доктест-раннере (не хватало `alpha_rename`/
# `number_exprs`). Общая форма: список проходов живёт в голове автора точки
# входа, а не в одном месте, которое можно проверить машиной.
#
# Фикс класса — `compiler-codegen/src/check_pipeline.rs::prepare_module_for_
# check[_with]`: ОДНА функция, собирающая весь список. Все настоящие точки
# входа (`nova check`, `nova build`, `nova test`, доктест-раннер, LSP-
# диагностика И LSP-provenance для hover/goto) теперь зовут её.
#
# ЧТО ПРОВЕРЯЕТ: находит каждый файл, где есть ПРЯМОЙ (не в комментарии)
# вызов `check_module(`, `check_module_with_sig_table(`,
# `check_module_with_expr_types(` или `check_module_with_expr_types_ide(`.
# Если В ТОМ ЖЕ ФАЙЛЕ нет вызова `prepare_module_for_check` (любого из двух:
# `prepare_module_for_check(` / `prepare_module_for_check_with(`) — файл
# считается «пропустил проходы мимо общей функции». Если такой файл НЕ
# перечислен в `check-checker-entrypoints.baseline` (сегодняшние принятые
# исключения — see тот файл за причиной каждого) — гейт красный: это НОВЫЙ
# прямой вызов, тот самый класс дефекта, который привёл к №531.
#
# Почему по ФАЙЛУ, а не по функции: греп синтаксически не умеет надёжно
# сопоставить «этот конкретный check_module() относится к пайплайну, где
# prepare был вызван раньше по потоку управления» — это требует построения
# CFG. Проверка по файлу — тот же компромисс, каким уже пользуются
# check-no-path-deps.sh и check-invariant-discipline.sh: не точный анализ
# потока, а страж класса, ловящий ИМЕННО тот случай, который уже дважды
# случился (два разных файла, два разных списка проходов по памяти).
#
# Определения (`compiler-codegen/src/types/mod.rs`) исключены из проверки
# структурно — файл, где `check_module`/`check_module_with_sig_table`
# ОБЪЯВЛЕНЫ, неизбежно содержит их имя без вызова prepare (сам prepare
# определён в другом файле и зовёт ИХ, а не наоборот).
#
# ИСПОЛЬЗОВАНИЕ:
#   bash scripts/guards/check-checker-entrypoints.sh [КОРЕНЬ]
#   bash scripts/guards/check-checker-entrypoints.sh --selftest

set -u
export LC_ALL=C

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
BASELINE_FILE="$SCRIPT_DIR/check-checker-entrypoints.baseline"

CALL_PATTERN='check_module\(|check_module_with_sig_table\(|check_module_with_expr_types\(|check_module_with_expr_types_ide\('

# Определения самого чекера — структурно исключены (см. шапку файла).
EXCLUDE_DEFS_RE='compiler-codegen/src/types/mod\.rs$|compiler-codegen/src/check_pipeline\.rs$'

# ── основная проверка ───────────────────────────────────────────────────────
run_check() {
    local root="$1"
    local baseline="${2:-$BASELINE_FILE}"
    local fail=0
    local violations=""

    # Files with a REAL (non-comment) call to one of the four functions.
    # `grep -v` filters out lines whose first non-space chars are `//`, `///`
    # or `*` (block-comment continuation) — a mention in a doc comment does
    # not count as a call.
    local hits
    # `.claude/worktrees/**` — рабочие копии окон-агентов, физически лежащие
    # ВНУТРИ дерева. Это не наш исходник, а чужие снимки на других коммитах:
    # 2026-08-10 страж покраснел на шести файлах из двух таких копий, где
    # `prepare_module_for_check` (план 262 А) ещё не существовал. Судить наш
    # код по чужому снимку нельзя — исключаем так же, как `target/`.
    hits=$(cd "$root" && grep -rEn "$CALL_PATTERN" --include='*.rs' . 2>/dev/null \
        | grep -v '/target/' \
        | grep -v '^\./\.claude/' \
        | grep -vE ':[0-9]+:[[:space:]]*(//|///|\*)')

    if [ -z "$hits" ]; then
        echo "check-checker-entrypoints ok: ни одного вызова check_module* не найдено (?)" >&2
        return 0
    fi

    local files
    files=$(printf '%s\n' "$hits" | cut -d: -f1 | sort -u)

    local baseline_norm=""
    if [ -f "$baseline" ]; then
        baseline_norm=$(grep -vE '^[[:space:]]*(#|$)' "$baseline" | sed 's/[[:space:]]*$//')
    fi

    local f rel
    while IFS= read -r f; do
        [ -n "$f" ] || continue
        rel="${f#./}"
        if printf '%s\n' "$rel" | grep -qE "$EXCLUDE_DEFS_RE"; then
            continue
        fi
        # Wired: same file also calls prepare_module_for_check[_with].
        if grep -q 'prepare_module_for_check' "$root/$rel" 2>/dev/null; then
            continue
        fi
        # Not wired — accepted only if on the baseline.
        if printf '%s\n' "$baseline_norm" | grep -qxF "$rel"; then
            continue
        fi
        violations="$violations$rel
"
        fail=1
    done <<< "$files"

    if [ "$fail" -ne 0 ]; then
        echo "check-checker-entrypoints: НАРУШЕНИЕ — прямой вызов check_module* в обход prepare_module_for_check, и файл не в baseline:" >&2
        printf '%s' "$violations" | sed 's/^/    /' >&2
        echo "    (либо позвать prepare_module_for_check[_with] в этом файле, либо добавить строку в $baseline с причиной)" >&2
        return 1
    fi

    echo "check-checker-entrypoints ok: все прямые вызовы check_module* либо через prepare_module_for_check, либо в baseline"
    return 0
}

# ── самопроверка (--selftest): обе стороны ──────────────────────────────────
run_selftest() {
    local tmp
    tmp=$(mktemp -d) || { echo "selftest: mktemp failed" >&2; return 1; }
    trap 'rm -rf "$tmp"' RETURN
    local overall=0

    # POS: a file calling check_module AND prepare_module_for_check in the
    # same file → must be GREEN (properly wired).
    mkdir -p "$tmp/pos/src"
    cat > "$tmp/pos/src/wired.rs" <<'EOF'
fn f() {
    let _ = check_pipeline::prepare_module_for_check(a, b, c, d, true);
    let _ = types::check_module(&module);
}
EOF
    if run_check "$tmp/pos" /dev/null >/tmp/entrypoints_selftest_pos.log 2>&1; then
        echo "selftest POS: ok (wired-файл проходит)"
    else
        echo "selftest POS: FAIL — wired-файл покраснел (ложняк):"; cat /tmp/entrypoints_selftest_pos.log
        overall=1
    fi

    # NEG: a NEW file calling check_module directly, no prepare call, not in
    # baseline → must be RED (this is the class #531 is about).
    mkdir -p "$tmp/neg/src"
    cat > "$tmp/neg/src/bypass.rs" <<'EOF'
fn f() {
    let _ = types::check_module(&module);
}
EOF
    if run_check "$tmp/neg" /dev/null >/tmp/entrypoints_selftest_neg.log 2>&1; then
        echo "selftest NEG: FAIL — новый прямой вызов должен был покраснить, но прошло:"; cat /tmp/entrypoints_selftest_neg.log
        overall=1
    else
        echo "selftest NEG: ok (ловит новый прямой вызов в обход prepare)"
    fi

    # EDGE: same bypass file, but listed on the baseline → must be GREEN
    # (accepted exception does not false-positive).
    printf 'src/bypass.rs\n' > "$tmp/neg/baseline.txt"
    if run_check "$tmp/neg" "$tmp/neg/baseline.txt" >/tmp/entrypoints_selftest_edge.log 2>&1; then
        echo "selftest EDGE: ok (baseline-запись не ложнит)"
    else
        echo "selftest EDGE: FAIL — файл из baseline не должен краснить:"; cat /tmp/entrypoints_selftest_edge.log
        overall=1
    fi

    # EDGE2: a comment-only mention of check_module( must not count as a
    # violation at all (no file even flagged).
    mkdir -p "$tmp/edge2/src"
    cat > "$tmp/edge2/src/mention.rs" <<'EOF'
/// See `types::check_module(&module)` for details.
fn f() {}
EOF
    if run_check "$tmp/edge2" /dev/null >/tmp/entrypoints_selftest_edge2.log 2>&1; then
        echo "selftest EDGE2: ok (упоминание в комментарии не ловится)"
    else
        echo "selftest EDGE2: FAIL — комментарий не должен был покраснить:"; cat /tmp/entrypoints_selftest_edge2.log
        overall=1
    fi

    if [ "$overall" -eq 0 ]; then
        echo "check-checker-entrypoints selftest: ALL OK"
    fi
    return "$overall"
}

ROOT_ARG="${1:-}"
if [ "$ROOT_ARG" = "--selftest" ]; then
    run_selftest
    exit $?
fi

# Второй позиционный аргумент — необязательный override пути к baseline
# (нужен внешнему селфтесту scripts/guards/selftest/test-check-checker-
# entrypoints.sh, который натравливает страж на игрушечные фикстуры со СВОИМ
# baseline, не трогая реальный). Без аргумента — baseline самого стража.
ROOT="${ROOT_ARG:-$(cd "$SCRIPT_DIR/../.." && pwd)}"
BASELINE_ARG="${2:-$BASELINE_FILE}"
run_check "$ROOT" "$BASELINE_ARG"
exit $?
