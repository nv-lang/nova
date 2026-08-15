#!/usr/bin/env bash
# Самотест check-novac-type-field-docs.sh — каждый тип, каждая функция и
# каждое поле novac несут ///-док (конвенция П13, слова владельца
# 2026-08-14/15). Норма самотестов — план 231 §4в: ловит нарушение И не даёт
# ложняка.
#
# ПОДЛОЖКА, ДВЕ ЧАСТИ. У стража два входа: $2 — сканируемая директория (её
# подменяем деревом .nv), и $1 — корень, ИЗ КОТОРОГО страж выводит строгость
# полей: он читает пин оракула из novac/nova.toml и спрашивает git, содержит
# ли пин ревизию 9a69411b3 (D104 rev-2: поля берут ///). Значит корней тоже
# два, и оба поддельные:
#   root_t — nova.toml БЕЗ строки oracle-pin: пин пуст, git не зовётся,
#            режим переходный («//» на поле — законная форма);
#   root_s — nova.toml с пином 9a69411b3 и файлом .git, указывающим на
#            общий git-каталог настоящей репы (коммит — предок самого себя,
#            поэтому режим строгий детерминированно, без зависимости от того,
#            куда пин переехал сегодня в рабочем дереве).
# Одно и то же дерево «поля с //» проверяется ОБОИМИ корнями: зелено в
# переходном, красно в строгом — так самотест судит и само самоистечение
# переходной формы, а не только два греп-правила.
set -u
export LC_ALL=C
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
G="$ROOT/scripts/guards/check-novac-type-field-docs.sh"
TMP="$(mktemp -d)"; trap 'rm -rf "$TMP"' EXIT
PASS=0; FAIL=0; SKIP=0
ok()  { PASS=$((PASS+1)); echo "  ok   $1"; }
bad() { FAIL=$((FAIL+1)); echo "  FAIL $1" >&2; }
skip(){ SKIP=$((SKIP+1)); echo "  skip $1"; }
check(){ if [ "$2" = "$3" ]; then ok "$1"; else bad "$1 (ждал '$3', получил '$2')"; fi; }
has()  { if grep -q "$2" "$1"; then ok "$3"; else bad "$3 (нет '$2' в $1: $(tr '\n' '|' < "$1"))"; fi; }

# --- два поддельных корня ---------------------------------------------------
RT="$TMP/root_t"; mkdir -p "$RT/novac"
printf '# nova.toml без пина оракула\n#   spec-point: 2026-08-14\n' > "$RT/novac/nova.toml"

RS="$TMP/root_s"; mkdir -p "$RS/novac"
printf '# nova.toml с пином, содержащим 9a69411b3\n#   oracle-pin: 9a69411b3\n' > "$RS/novac/nova.toml"
COMMON="$(git -C "$ROOT" rev-parse --path-format=absolute --git-common-dir 2>/dev/null || true)"
STRICT_OK=0
if [ -n "$COMMON" ] && git -C "$ROOT" cat-file -e '9a69411b3^{commit}' 2>/dev/null; then
    printf 'gitdir: %s\n' "$COMMON" > "$RS/.git"
    STRICT_OK=1
fi

# --- деревья .nv ------------------------------------------------------------
mkd() { mkdir -p "$TMP/$1"; echo "$TMP/$1"; }

OK_T="$(mkd ok_t)"
cat > "$OK_T/token.nv" <<'EOF'
/// A token of the source text.
type Token {
    kind int // which kind of token this is
    // byte offset of the token in the source
    pos int
}

/// Make the end-of-file token.
#impl(inline)
fn eof() -> Token {
    return Token.new(0, 0)
}
EOF

OK_S="$(mkd ok_s)"
cat > "$OK_S/source.nv" <<'EOF'
/// One source file loaded into the compiler.
type SourceFile {
    path str /// where the text came from
    /// byte length of the text
    size int
}

/// Load a source file from disk.
fn load(p: str) -> SourceFile {
    return SourceFile.new(p, 0)
}
EOF

NO_TYPE="$(mkd no_type_doc)"
cat > "$NO_TYPE/a.nv" <<'EOF'
type Undocumented {
    a int /// a field with a doc
}
EOF

NO_FN="$(mkd no_fn_doc)"
cat > "$NO_FN/b.nv" <<'EOF'
/// A documented type.
type Documented {
    a int /// a field with a doc
}

fn undocumented() -> int {
    return 1
}
EOF

NO_FIELD="$(mkd no_field_doc)"
cat > "$NO_FIELD/c.nv" <<'EOF'
/// A documented type with a bare field.
type Bare {
    a int
}

/// A documented function.
fn f() -> int {
    return 1
}
EOF

runt() { sh "$G" "$RT" "$1" > "$TMP/out" 2> "$TMP/err"; echo $?; }
runs() { sh "$G" "$RS" "$1" > "$TMP/out" 2> "$TMP/err"; echo $?; }

echo "== переходный режим (пина нет) =="
sh "$G" "$RT" "$TMP/absent" > "$TMP/out" 2>&1
check "сканируемой директории нет — зелёный" "$?" "0"
has "$TMP/out" 'ok: судить нечего' "«судить нечего» напечатано (№645)"

check "поля с // и атрибут между доком и fn — зелёный" "$(runt "$OK_T")" "0"
has "$TMP/out" 'переходный' "режим полей назван в зелёной строке"
check "поля с /// (строгая форма) — тоже зелёный" "$(runt "$OK_S")" "0"

check "тип без ///-дока — красный" "$(runt "$NO_TYPE")" "1"
has "$TMP/err" 'тип без ' "тип назван"
check "функция без ///-дока — красный" "$(runt "$NO_FN")" "1"
has "$TMP/err" 'функция без ' "функция названа"
check "поле совсем без комментария — красный" "$(runt "$NO_FIELD")" "1"
has "$TMP/err" 'поле без комментария' "поле названо переходной формулировкой"

echo "== строгий режим (пин содержит 9a69411b3, D104 rev-2) =="
if [ "$STRICT_OK" = 1 ]; then
    check "поля с /// — зелёный" "$(runs "$OK_S")" "0"
    has "$TMP/out" 'строго' "режим полей назван в зелёной строке"
    check "те же поля с // — КРАСНЫЙ (переходная форма истекла)" "$(runs "$OK_T")" "1"
    has "$TMP/err" 'поле без ///-дока' "поле названо строгой формулировкой"
    check "тип без ///-дока — красный и в строгом" "$(runs "$NO_TYPE")" "1"
    check "поле совсем без комментария — красный и в строгом" "$(runs "$NO_FIELD")" "1"
else
    skip "строгий режим недостижим: коммита 9a69411b3 нет в этой репе (проверь git cat-file)"
fi

echo "== настоящее дерево =="
sh "$G" "$ROOT" >/dev/null 2>&1
check "novac/src проекта документирован" "$?" "0"

echo "итог: $PASS ok, $FAIL FAIL, $SKIP skip"
if [ "$FAIL" -eq 0 ]; then
    echo "test-check-novac-type-field-docs ok: $PASS/$PASS (skip: $SKIP)"
    exit 0
fi
exit 1
