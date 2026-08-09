#!/usr/bin/env bash
# Селфтест scripts/guards/check-checker-entrypoints.sh (план 262 Ф.А.1-bis,
# реестр 221.1 №531).
#
# Страж без селфтеста не работает — правило владельца 2026-07-27, и
# check-guard-wiring его энфорсит. Проверяем ЧЕТЫРЕ случая: wired-файл
# проходит, новый прямой вызов в обход prepare_module_for_check красит гейт,
# та же запись в baseline не ложнит, упоминание в комментарии не ловится
# вовсе.
set -u
export LC_ALL=C

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
G="$ROOT/scripts/guards/check-checker-entrypoints.sh"
FAILED=0
ok()  { echo "  ok: $1"; }
bad() { echo "  ПРОВАЛ: $1" >&2; FAILED=1; }

echo "== селфтест check-checker-entrypoints =="

# 0. На реальной репе — зелено (все прямые вызовы либо wired, либо в baseline).
if bash "$G" "$ROOT" >/dev/null 2>&1; then
    ok "реальная репа: все check_module* либо wired, либо в baseline"
else
    bad "реальная репа краснит — новый прямой вызов check_module* без prepare_module_for_check и без строки в baseline?"
fi

T=$(mktemp -d)
trap 'rm -rf "$T"' EXIT

# 1. POS: файл, вызывающий check_module И prepare_module_for_check — не красит.
mkdir -p "$T/pos/src"
cat > "$T/pos/src/wired.rs" <<'EOF'
fn f() {
    let _ = check_pipeline::prepare_module_for_check(a, b, c, d, true);
    let _ = types::check_module(&module);
}
EOF
if bash "$G" "$T/pos" /dev/null >/dev/null 2>&1; then
    ok "wired-файл (check_module + prepare_module_for_check) проходит"
else
    bad "wired-файл покраснел — ложняк"
fi

# 2. NEG: новый прямой вызов, БЕЗ prepare, НЕ в baseline — красит.
mkdir -p "$T/neg/src"
cat > "$T/neg/src/bypass.rs" <<'EOF'
fn f() {
    let _ = types::check_module(&module);
}
EOF
if bash "$G" "$T/neg" /dev/null >/dev/null 2>&1; then
    bad "НЕ поймал новый прямой вызов check_module в обход prepare_module_for_check"
else
    ok "ловит новый прямой вызов в обход prepare_module_for_check"
fi

# 3. EDGE: тот же bypass-файл, но перечислен в baseline — не ложнит.
printf 'src/bypass.rs\n' > "$T/neg/baseline.txt"
if bash "$G" "$T/neg" "$T/neg/baseline.txt" >/dev/null 2>&1; then
    ok "запись в baseline не ложнит на принятом исключении"
else
    bad "файл из baseline не должен был покраснить"
fi

# 4. EDGE: упоминание check_module( только в doc-комментарии не ловится.
mkdir -p "$T/edge2/src"
cat > "$T/edge2/src/mention.rs" <<'EOF'
/// See `types::check_module(&module)` for details.
fn f() {}
EOF
if bash "$G" "$T/edge2" /dev/null >/dev/null 2>&1; then
    ok "упоминание в doc-комментарии не ловится"
else
    bad "комментарий не должен был покраснить"
fi

if [ "$FAILED" -eq 0 ]; then
    echo "check-checker-entrypoints selftest: OK"
    exit 0
else
    echo "check-checker-entrypoints selftest: ПРОВАЛ" >&2
    exit 1
fi
