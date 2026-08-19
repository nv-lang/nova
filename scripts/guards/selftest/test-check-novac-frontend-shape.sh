#!/bin/sh
# Самотест check-novac-frontend-shape.py — оба направления (норма 254):
# ловит нарушение И не краснеет на законном; мягкая span-часть — WARN, не exit 1.
export LC_ALL=C
G="$(cd "$(dirname "$0")/.." && pwd)/check-novac-frontend-shape.py"
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
python "$G" "$ROOT" "$T/good" >/dev/null 2>&1 && ok "законная пара проходит" || bad "законная пара покраснела"
out=$(python "$G" "$ROOT" "$T/good" 2>/dev/null)
printf '%s\n' "$out" | grep -q 'ok:' && ok "печатает строку ok:" || bad "нет строки ok: (№645)"
printf '%s\n' "$out" | grep -q 'WARN' && bad "WARN на файле с полем span" || ok "поле span гасит предупреждение"

# 4. Нарушение: export fn ... -> Result[ — красный.
mkdir -p "$T/bad/parse"
cat > "$T/bad/parse/parser.nv" <<'EOF'
export fn parse(src str) -> Result[Tree, ParseError] {
}
EOF
python "$G" "$ROOT" "$T/bad" >/dev/null 2>&1 && bad "export -> Result[ прошёл" || ok "export -> Result[ пойман"

# 5. Законный: Result у НЕэкспортированного хелпера — правило судит экспорты.
mkdir -p "$T/priv/lex"
cat > "$T/priv/lex/helper.nv" <<'EOF'
fn helper(s str) -> Result[int, E] {
}
export fn lex(src str) -> (Tokens, []Diagnostic, Span) {
}
EOF
python "$G" "$ROOT" "$T/priv" >/dev/null 2>&1 && ok "Result у приватного хелпера законен" || bad "приватный Result покраснел"

# Случай про WARN снят 2026-08-17 вместе с самой эвристикой. Она судила
# ИМЯ типа возврата (`-> []Token` не содержит слова Span) и потому
# промахивалась по построению, печатая предупреждение, на котором никто не
# краснел. Правило «позиции обязательны» теперь судит
# check-novac-diag-schema.sh по существу — на живом выводе novac: primary с
# непустым файлом и границами 0 <= start <= end <= размер файла. Проверка
# переехала, и доказательство её красноты переехало туда же (четыре
# мутации подсудимого: пустой primary, пустой файл, перевёрнутые границы,
# отрицательное начало).

# 8. Судить нечего: пустая директория — зелёный с честной строкой.
mkdir -p "$T/empty"
python "$G" "$ROOT" "$T/empty" 2>/dev/null | grep -q 'ok: судить нечего' && ok "пусто — честное «судить нечего»" || bad "пусто — нет «ok: судить нечего»"

rm -rf "$T"
if [ "$fails" -eq 0 ]; then
    echo "test-check-novac-frontend-shape ok: 8/8"
    exit 0
fi
echo "test-check-novac-frontend-shape: FAIL ($fails)" >&2
exit 1
