#!/usr/bin/env bash
# КОНТРОЛЬ к p_check-flag-has-caller_getenv-and-helper: то же дерево, но тот же
# самый безмолвный флаг записан КАНОНИЧЕСКОЙ формой — env::var("ЛИТЕРАЛ") в
# compiler-codegen/src. Страж обязан покраснеть и краснеет, называя имя.
# Значит зелёный в парной пробе — не «страж мёртв», а перечисление синтаксисов.
#
# Запуск из любого cwd:  bash cmd.sh [корень-репы-nova]
set -u
HERE="$(cd "$(dirname "$0")" && pwd)"

find_root() {
    d="$HERE"
    while [ -n "$d" ] && [ "$d" != "/" ]; do
        [ -f "$d/scripts/guards/check-flag-has-caller.sh" ] && { printf '%s' "$d"; return 0; }
        d="$(dirname "$d")"
    done
    return 1
}
NOVA_ROOT="${1:-$(find_root)}"
GUARD="$NOVA_ROOT/scripts/guards/check-flag-has-caller.sh"
[ -f "$GUARD" ] || { echo "proba: guard not found; pass nova root as argv1" >&2; exit 2; }

TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' 0
T="$TMP/tree"
mkdir -p "$T/compiler-codegen/src" "$T/compiler-codegen/nova_rt" "$T/nova-cli/src" "$T/docs"

cat > "$T/compiler-codegen/src/lib.rs" <<'RS'
pub fn described() -> bool {
    std::env::var("NOVA_PROBE_DESCRIBED").is_ok()
}
RS
cat > "$T/docs/flags.md" <<'MD'
# flags
NOVA_PROBE_DESCRIBED - documented here, so it has a reader outside the code.
MD

# То же самое свойство, но канонической формой, которую страж грепает.
cat > "$T/compiler-codegen/src/other.rs" <<'RS'
pub fn c_only() -> bool {
    std::env::var("NOVA_PROBE_C_ONLY").is_ok()
}
RS

echo "--- artefact check ---"
for f in compiler-codegen/src/lib.rs compiler-codegen/src/other.rs docs/flags.md; do
    if [ -s "$T/$f" ]; then echo "  present: $f"; else echo "TARGET MISSING: $f" >&2; exit 99; fi
done

printf 'silent_flags=0\n' > "$TMP/fc.baseline"

echo "--- guard ---"
NOVA_FLAGCALLER_BASELINE="$TMP/fc.baseline" bash "$GUARD" "$T"
rc=$?
echo "RC=$rc"
exit "$rc"
