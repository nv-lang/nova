#!/usr/bin/env bash
# test-lint-no-silent-int-fallback.sh — САМОТЕСТ стража
# `lint-no-silent-int-fallback.sh` (Plan 70 Ф.2).
#
# Механизм принуждения без собственного теста — доверие на слово (план 231
# трек Ж). Доказываются ОБА свойства: (1) ЛОВИТ нарушение, (2) НЕ даёт
# ложняка. Плюс третье, ради которого страж и правился в окне №740:
# площадки, завёрнутые в `erase_unk(...)`, НАМЕРЕННЫЕ и не должны краснеть —
# именно из-за них счёт разошёлся с базой (21 против 7) и стража перестали
# читать.
#
# Случаи кодируют ЗАМЕР, а не допущение: числа 2 и 14 — это база стража на
# день заведения самотеста, и она задана в нём же.
#
# Запуск: scripts/guards/selftest/test-lint-no-silent-int-fallback.sh
# Выход: 0 — страж исправен, 1 — сломан.

set -uo pipefail
export LC_ALL=C

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]:-$0}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../../.." && pwd)"
GUARD="$REPO_ROOT/scripts/guards/lint-no-silent-int-fallback.sh"

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

fails=0
check() { # имя, ожидаемый_код, фактический_код
    if [ "$2" -eq "$3" ]; then
        echo "  ok: $1"
    else
        echo "  ПРОВАЛ: $1 — ожидался код $2, получен $3" >&2
        fails=$((fails + 1))
    fi
}

mktree() { # каталог — готовит пустое дерево вида КОРЕНЬ/compiler-codegen/src
    rm -rf "$1"
    mkdir -p "$1/compiler-codegen/src"
}

echo "самотест lint-no-silent-int-fallback:"

# (1) НЕ ловит: дерево без единой площадки (счёт 0 при базе 2).
mktree "$tmp/clean"
cat > "$tmp/clean/compiler-codegen/src/emit.rs" <<'RS'
fn f(&self) -> Result<String, String> {
    self.type_ref_to_c(t).map_err(|e| self.err_no_int_fallback("param", &e))
}
RS
"$GUARD" "$tmp/clean" >/dev/null 2>&1
check "НЕ ловит дерево на каноне (счёт ниже базы)" 0 $?

# (2) ЛОВИТ: три голые площадки Cat A1 при базе 2.
mktree "$tmp/dirty"
cat > "$tmp/dirty/compiler-codegen/src/emit.rs" <<'RS'
let a = self.type_ref_to_c(t).unwrap_or_else(|_| "nova_int".into());
let b = self.type_ref_to_c(t).unwrap_or_else(|_| "nova_int".to_string());
let c = self.type_ref_to_c(&p.ty).unwrap_or_else(|_| "nova_int".into());
RS
"$GUARD" "$tmp/dirty" >/dev/null 2>&1
check "ловит рост Cat A1 над базой" 1 $?

# (3) НЕ ловит: намеренные обёртки erase_unk — сколько бы их ни было.
mktree "$tmp/erased"
cat > "$tmp/erased/compiler-codegen/src/emit.rs" <<'RS'
let a = erase_unk(self.type_ref_to_c(t).unwrap_or_else(|_| "nova_int".into()));
let b = erase_unk(self.type_ref_to_c(t).unwrap_or_else(|_| "nova_int".into()));
let c = erase_unk(self.type_ref_to_c(t).unwrap_or_else(|_| "nova_int".into()));
let d = erase_unk(self.type_ref_to_c(t).unwrap_or_else(|_| "nova_int".into()));
let e = erase_unk(self.type_ref_to_c(t).unwrap_or_else(|_| "nova_int".into()));
RS
"$GUARD" "$tmp/erased" >/dev/null 2>&1
check "НЕ ловит намеренные обёртки erase_unk (иначе счёт снова разойдётся)" 0 $?

# (4) ЛОВИТ голую площадку РЯДОМ с намеренными — исключение по форме не
#     должно глушить соседей.
mktree "$tmp/mixed"
cat > "$tmp/mixed/compiler-codegen/src/emit.rs" <<'RS'
let a = erase_unk(self.type_ref_to_c(t).unwrap_or_else(|_| "nova_int".into()));
let b = self.type_ref_to_c(t).unwrap_or_else(|_| "nova_int".into());
let c = self.type_ref_to_c(t).unwrap_or_else(|_| "nova_int".into());
let d = self.type_ref_to_c(t).unwrap_or_else(|_| "nova_int".into());
RS
"$GUARD" "$tmp/mixed" >/dev/null 2>&1
check "ловит голую площадку рядом с намеренными" 1 $?

# (5) ЛОВИТ рост Cat A2 (wildcard) над базой 14.
mktree "$tmp/wild"
: > "$tmp/wild/compiler-codegen/src/emit.rs"
i=0
while [ "$i" -lt 15 ]; do
    echo '        _ => "nova_int",' >> "$tmp/wild/compiler-codegen/src/emit.rs"
    i=$((i + 1))
done
"$GUARD" "$tmp/wild" >/dev/null 2>&1
check "ловит рост Cat A2 над базой" 1 $?

# (6) Печатает строку ok: — иначе обёртка `guard` в gate.sh засчитает шаг
#     как «ничего не доказал» (реестр 221.1 №645).
out="$("$GUARD" "$tmp/clean" 2>&1)"
printf '%s\n' "$out" | grep -q 'ok:'
check "печатает строку ok: на зелёном" 0 $?

# (7) НЕ ловит настоящее дерево nova (страж не ломает сам себя).
"$GUARD" "$REPO_ROOT" >/dev/null 2>&1
check "НЕ ловит настоящую репу nova" 0 $?

if [ "$fails" -ne 0 ]; then
    echo "самотест ПРОВАЛЕН: $fails свойств(а) стража не выполняются" >&2
    exit 1
fi
echo "самотест ok: страж ловит рост A1/A2 и не краснеет на намеренных erase_unk"
