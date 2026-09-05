#!/usr/bin/env bash
# Проба к находке: check-flag-has-caller объявляет правилом
#   «Переменная-флаг, которую никто не взводит и нигде не описывает, — не фича»
#   «флаг без вызывающего — не фича»
# и честно называет ОДИН свой предел: «он проверяет УПОМИНАНИЕ, а не смысл».
# Про то, что смотрятся только ДВА каталога Rust-исходников и только запись
# env::var("ЛИТЕРАЛ"), не сказано ни в шапке, ни в летописи базы.
#
# Здесь тот же флаг выражен двумя другими синтаксисами:
#   (1) getenv("NOVA_PROBE_C_ONLY") в поставляемом C-рантайме nova_rt;
#   (2) env::var(<переменная>) за функцией-помощником, имя приходит с вызова.
# Оба нигде не описаны. Ожидание по правилу: красный. Замер — в verdict.txt.
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

# Один ЧЕСТНЫЙ флаг: без него ядро печатает flags=0 и страж выходит рано.
cat > "$T/compiler-codegen/src/lib.rs" <<'RS'
pub fn described() -> bool {
    std::env::var("NOVA_PROBE_DESCRIBED").is_ok()
}
RS
cat > "$T/docs/flags.md" <<'MD'
# flags
NOVA_PROBE_DESCRIBED - documented here, so it has a reader outside the code.
MD

# Форма 1: поставляемый C-рантайм читает флаг через getenv.
cat > "$T/compiler-codegen/nova_rt/rt.c" <<'C'
#include <stdlib.h>
int probe_trace_on(void) {
    return getenv("NOVA_PROBE_C_ONLY") != NULL;
}
C

# Форма 2: имя приходит аргументом в функцию-помощник.
cat > "$T/nova-cli/src/main.rs" <<'RS'
fn env_path_override(var: &str) -> Option<String> {
    std::env::var(var).ok()
}
pub fn probe() -> Option<String> {
    env_path_override("NOVA_PROBE_HELPER")
}
RS

echo "--- artefact check ---"
for f in compiler-codegen/src/lib.rs compiler-codegen/nova_rt/rt.c nova-cli/src/main.rs docs/flags.md; do
    if [ -s "$T/$f" ]; then echo "  present: $f"; else echo "TARGET MISSING: $f" >&2; exit 99; fi
done
echo "--- flags that no one sets and nothing describes: NOVA_PROBE_C_ONLY, NOVA_PROBE_HELPER ---"
grep -rn "NOVA_PROBE_C_ONLY\|NOVA_PROBE_HELPER" "$T" | sed "s|$T|<tree>|"

printf 'silent_flags=0\n' > "$TMP/fc.baseline"

echo "--- guard ---"
NOVA_FLAGCALLER_BASELINE="$TMP/fc.baseline" bash "$GUARD" "$T"
rc=$?
echo "RC=$rc"

echo
echo "--- part 2: measurement on the REAL tree (read-only) ---"
python "$HERE/measure_real_tree.py" "$NOVA_ROOT"
exit "$rc"
