#!/usr/bin/env bash
# Проба к находке: метрика `infer` объявлена как
#   «число вызовов infer_expr_c_type (собственный инференс типов прямо из
#    эмиссии, вместо чтения готового резолва из чекера)»
# — то есть свойство СЛОЯ эмиссии. Код же считает вхождения ровно в одном
# файле, compiler-codegen/src/codegen/emit_c.rs. Слой при этом давно не
# один файл: рядом лежат 15 файлов codegen/*.rs и подкаталог codegen/emit_c/
# с четырьмя (включая variant_ctor_channel.rs).
#
# Тут пять НАСТОЯЩИХ вызовов кладутся в codegen/emit_c/new_channel.rs при
# базе infer=1 и точном равенстве lines у emit_c.rs.
# Ожидание по объявленной метрике: красный. Замер — в verdict.txt.
#
# Запуск из любого cwd:  bash cmd.sh [корень-репы-nova]
set -u
HERE="$(cd "$(dirname "$0")" && pwd)"

find_root() {
    d="$HERE"
    while [ -n "$d" ] && [ "$d" != "/" ]; do
        [ -f "$d/scripts/guards/arch-ratchet.sh" ] && { printf '%s' "$d"; return 0; }
        d="$(dirname "$d")"
    done
    return 1
}
NOVA_ROOT="${1:-$(find_root)}"
GUARD="$NOVA_ROOT/scripts/guards/arch-ratchet.sh"
[ -f "$GUARD" ] || { echo "proba: guard not found; pass nova root as argv1" >&2; exit 2; }

TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' 0
D="$TMP/fixture"
mkdir -p "$D/scripts/guards" "$D/compiler-codegen/src/codegen/emit_c"
cp "$GUARD" "$D/scripts/guards/arch-ratchet.sh"

cat > "$D/compiler-codegen/src/codegen/emit_c.rs" <<'RS'
fn a(x: i64) -> i64 {
    infer_expr_c_type(x)
}
RS

# Тот же слой, соседний файл: пять новых собственных инференсов из эмиссии.
cat > "$D/compiler-codegen/src/codegen/emit_c/new_channel.rs" <<'RS'
pub fn t1(x: i64) -> i64 { infer_expr_c_type(x) }
pub fn t2(x: i64) -> i64 { infer_expr_c_type(x) }
pub fn t3(x: i64) -> i64 { infer_expr_c_type(x) }
pub fn t4(x: i64) -> i64 { infer_expr_c_type(x) }
pub fn t5(x: i64) -> i64 { infer_expr_c_type(x) }
RS

L=$(wc -l < "$D/compiler-codegen/src/codegen/emit_c.rs" | tr -d ' ')
printf 'lines=%s\ninfer=1\n' "$L" > "$D/scripts/guards/arch-ratchet.baseline"

echo "--- artefact check ---"
for f in compiler-codegen/src/codegen/emit_c.rs compiler-codegen/src/codegen/emit_c/new_channel.rs scripts/guards/arch-ratchet.baseline; do
    if [ -s "$D/$f" ]; then echo "  present: $f"; else echo "TARGET MISSING: $f" >&2; exit 99; fi
done
echo "--- real infer_expr_c_type calls in the fixture layer ---"
grep -rn "infer_expr_c_type" "$D/compiler-codegen" | sed "s|$D|<fixture>|"
echo "--- baseline ---"
cat "$D/scripts/guards/arch-ratchet.baseline"

echo "--- guard ---"
( cd "$D" && bash scripts/guards/arch-ratchet.sh )
rc=$?
echo "RC=$rc"

echo
echo "--- part 2: the same question on the REAL tree (read-only) ---"
echo "calls counted by the guard (emit_c.rs only):"
( cd "$NOVA_ROOT" && grep -n "infer_expr_c_type" compiler-codegen/src/codegen/emit_c.rs \
    | sed -e 's/^[0-9]*://' -e 's/^[[:space:]]*//' | grep -vc -E '^(//|\*)' )
echo "same metric on the sibling files of the same layer:"
( cd "$NOVA_ROOT" && for f in compiler-codegen/src/codegen/*.rs compiler-codegen/src/codegen/emit_c/*.rs; do
    case "$f" in */emit_c.rs) continue;; esac
    n=$(grep -n "infer_expr_c_type" "$f" 2>/dev/null | sed -e 's/^[0-9]*://' -e 's/^[[:space:]]*//' | grep -vc -E '^(//|\*)')
    [ "${n:-0}" -gt 0 ] && echo "  $f  $n"
  done; true )
exit "$rc"
