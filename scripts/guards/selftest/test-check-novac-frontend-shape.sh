#!/bin/sh
# Самотест check-novac-frontend-shape.sh — оба направления (норма 254):
# ловит нарушение И не краснеет на законном; мягкая span-часть — WARN, не exit 1.
export LC_ALL=C
G="$(cd "$(dirname "$0")/.." && pwd)/check-novac-frontend-shape.sh"
ROOT="$(cd "$(dirname "$0")/../../.." && pwd)"
T="${TMPDIR:-/tmp}/novac-frontend-shape-selftest.$$"
mkdir -p "$T"
fails=0

ok() { echo "  ok: $1"; }
bad() { echo "  FAIL: $1" >&2; fails=$((fails+1)); }

# 1–3. Законный: пары «(узел, диагностики)», тип с полем span — зелено,
# строка ok:, БЕЗ предупреждений.
mkdir -p "$T/good/lex" "$T/good/parse"
cat > "$T/good/lex/lexer.nv" <<'EOF'
type Token {
    kind int
    span Span
}
export fn lex(src str) -> (TokenStream, []Diagnostic) {
}
EOF
cat > "$T/good/parse/parser.nv" <<'EOF'
export fn parse(tokens TokenStream) -> (Tree, []Diagnostic) {
}
EOF
sh "$G" "$ROOT" "$T/good" >/dev/null 2>&1 && ok "законная пара проходит" || bad "законная пара покраснела"
out=$(sh "$G" "$ROOT" "$T/good" 2>/dev/null)
printf '%s\n' "$out" | grep -q 'ok:' && ok "печатает строку ok:" || bad "нет строки ok: (№645)"
printf '%s\n' "$out" | grep -q 'WARN' && bad "WARN на файле с полем span" || ok "поле span гасит предупреждение"

# 4. Нарушение: export fn ... -> Result[ — красный.
mkdir -p "$T/bad/parse"
cat > "$T/bad/parse/parser.nv" <<'EOF'
export fn parse(src str) -> Result[Tree, ParseError] {
}
EOF
sh "$G" "$ROOT" "$T/bad" >/dev/null 2>&1 && bad "export -> Result[ прошёл" || ok "export -> Result[ пойман"

# 5. Законный: Result у НЕэкспортированного хелпера — правило судит экспорты.
mkdir -p "$T/priv/lex"
cat > "$T/priv/lex/helper.nv" <<'EOF'
fn helper(s str) -> Result[int, E] {
}
export fn lex(src str) -> (Tokens, []Diagnostic, Span) {
}
EOF
sh "$G" "$ROOT" "$T/priv" >/dev/null 2>&1 && ok "Result у приватного хелпера законен" || bad "приватный Result покраснел"

# 6–7. Мягкая часть: параметр-текст без Span в возврате и без типа с полем
# span — exit 0 (не роняет), но WARN в stdout.
mkdir -p "$T/warn/syntax"
cat > "$T/warn/syntax/forms.nv" <<'EOF'
export fn classify(src str) -> (Tree, []Diagnostic) {
}
EOF
sh "$G" "$ROOT" "$T/warn" >/dev/null 2>&1 && ok "span-часть не роняет (exit 0)" || bad "span-часть уронила гейт (обещана мягкой)"
sh "$G" "$ROOT" "$T/warn" 2>/dev/null | grep -q 'WARN' && ok "span-часть печатает WARN в stdout" || bad "нет WARN на параметре-тексте без Span"

# 8. Судить нечего: пустая директория — зелёный с честной строкой.
mkdir -p "$T/empty"
sh "$G" "$ROOT" "$T/empty" 2>/dev/null | grep -q 'ok: судить нечего' && ok "пусто — честное «судить нечего»" || bad "пусто — нет «ok: судить нечего»"

rm -rf "$T"
if [ "$fails" -eq 0 ]; then
    echo "test-check-novac-frontend-shape ok: 8/8"
    exit 0
fi
echo "test-check-novac-frontend-shape: FAIL ($fails)" >&2
exit 1
