#!/usr/bin/env bash
# scripts/guards/selftest/test-check-cli-output-language.sh
#
# Самотест стража языка поставляемого вывода CLI (реестр 221.1 №823).
#
# ЗАЧЕМ ОТДЕЛЬНЫЙ ФАЙЛ, если у самого стража есть `--selftest`. Внутренний
# режим проверяет СЧЁТЧИК и арифметику храповика на синтетическом тексте —
# то есть свои кирпичи. Он НЕ может проверить главное: что страж, натравленный
# на целое дерево, действительно красит нарушение и действительно молчит на
# здоровом. Здесь строятся два поддельных дерева и страж запускается на них
# целиком, как в гейте.
#
# ПОДДЕЛЬНЫЙ БИНАРЬ, А НЕ КОПИЯ НАСТОЯЩЕГО: страж судит ВЫВОД, значит для
# проверки достаточно чего угодно, что этот вывод печатает. Копировать
# `nova.exe` (десятки мегабайт) ради самотеста — плата без выгоды, а заодно
# зависимость самотеста от того, собран ли компилятор.
#
# ЧТО ДОКАЗЫВАЕТСЯ, обе стороны:
#   1. кириллица в `--help`            → красно;
#   2. кириллица в JSON-схеме          → красно;
#   3. кириллица в тексте ошибки (.rs) → красно;
#   4. кириллица ТОЛЬКО в комментарии  → ЗЕЛЕНО (иначе страж наказывал бы за
#      русский комментарий, которого правило не касается — тот случай, ради
#      которого счётчик исходника вообще переписан на разбор кавычек);
#   5. чистое дерево                   → зелено;
#   6. отсутствие бинаря               → красно, а не тихо зелено (№813).

set -u
export LC_ALL=C

ROOT_REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
GUARD="$ROOT_REPO/scripts/guards/check-cli-output-language.sh"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

pass=0
fail=0
ok()   { echo "  ok: $1"; pass=$((pass + 1)); }
bad()  { echo "  ПРОВАЛ: $1"; fail=$((fail + 1)); }

# Строит дерево: $1 — каталог, $2 — «yes»/«no» кириллица в help,
# $3 — в схеме, $4 — в тексте ошибки, $5 — в комментарии.
make_tree() {
    local d="$1" h="$2" s="$3" e="$4" c="$5"
    mkdir -p "$d/nova-cli/target/release" "$d/nova-cli/src" "$d/scripts/guards"
    local help_extra="" schema_extra=""
    [ "$h" = "yes" ] && help_extra=$'\n  --flag   \xd0\xbf\xd1\x83\xd1\x82\xd1\x8c'
    [ "$s" = "yes" ] && schema_extra=$'\n  "description": "\xd0\xbe\xd0\xbf\xd0\xb8\xd1\x81"'
    cat > "$d/nova-cli/target/release/nova" <<STUB
#!/usr/bin/env bash
if [ "\$1" = "doc" ] && [ "\$2" = "--json-schema" ]; then
  printf '{"\\\$id":"x"}%s\n' '$schema_extra'
  exit 0
fi
if [ "\$1" = "--help" ]; then
  printf 'Usage: nova <cmd>\nCommands:\n  build   b\nOptions:\n  -h%s\n' '$help_extra'
  exit 0
fi
printf 'sub help\n'
STUB
    chmod +x "$d/nova-cli/target/release/nova"
    {
        echo 'fn main() {'
        if [ "$e" = "yes" ]; then
            printf '    bail!("\xd0\xbe\xd1\x88\xd0\xb8\xd0\xb1\xd0\xba\xd0\xb0");\n'
        else
            echo '    bail!("error");'
        fi
        if [ "$c" = "yes" ]; then
            printf '    let x = 1; // \xd0\xbf\xd0\xbe\xd1\x8f\xd1\x81\xd0\xbd\xd0\xb5\xd0\xbd\xd0\xb8\xd0\xb5\n'
        fi
        echo '}'
    } > "$d/nova-cli/src/main.rs"
    printf 'cli_output=0\ncli_error_texts=0\n' > "$d/scripts/guards/cli-output-language.baseline"
}

run_guard() { bash "$GUARD" "$1" >/dev/null 2>&1; echo $?; }

# --- 1. внутренний режим ------------------------------------------------------
if [ "$(bash "$GUARD" --selftest >/dev/null 2>&1; echo $?)" = "0" ]; then
    ok "внутренний --selftest зелёный"
else
    bad "внутренний --selftest красный"
fi

# --- 2. здоровое дерево -------------------------------------------------------
make_tree "$TMP/clean" no no no no
[ "$(run_guard "$TMP/clean")" = "0" ] \
    && ok "чистое дерево зелёное" \
    || bad "чистое дерево покрашено (ложное срабатывание)"

# --- 3. кириллица в help ------------------------------------------------------
make_tree "$TMP/help" yes no no no
[ "$(run_guard "$TMP/help")" = "1" ] \
    && ok "ловит кириллицу в --help" \
    || bad "НЕ ловит кириллицу в --help"

# --- 4. кириллица в схеме -----------------------------------------------------
make_tree "$TMP/schema" no yes no no
[ "$(run_guard "$TMP/schema")" = "1" ] \
    && ok "ловит кириллицу в JSON-схеме" \
    || bad "НЕ ловит кириллицу в JSON-схеме"

# --- 5. кириллица в тексте ошибки ---------------------------------------------
make_tree "$TMP/err" no no yes no
[ "$(run_guard "$TMP/err")" = "1" ] \
    && ok "ловит кириллицу в тексте ошибки" \
    || bad "НЕ ловит кириллицу в тексте ошибки"

# --- 6. кириллица ТОЛЬКО в комментарии ----------------------------------------
# Это не мелочь: прежняя редакция счётчика отбрасывала комментарии шаблоном
# `^\s*//` и хвостовой комментарий считала кодом. Страж, краснеющий на законном,
# отключают первым же окном — и дальше он не ловит уже ничего.
make_tree "$TMP/comment" no no no yes
[ "$(run_guard "$TMP/comment")" = "0" ] \
    && ok "русский комментарий НЕ красит (счётчик отличает его от кода)" \
    || bad "русский комментарий покрасил — счётчик снова не видит хвостового комментария"

# --- 7. бинаря нет — красно, а не тихо зелено ---------------------------------
make_tree "$TMP/nobin" no no no no
rm -f "$TMP/nobin/nova-cli/target/release/nova"
[ "$(run_guard "$TMP/nobin")" = "1" ] \
    && ok "нет бинаря — красно, а не тихо зелено" \
    || bad "нет бинаря — страж промолчал (класс №813)"

echo "селфтест check-cli-output-language: $pass/$((pass + fail)) ok"
[ "$fail" = "0" ] || exit 1
