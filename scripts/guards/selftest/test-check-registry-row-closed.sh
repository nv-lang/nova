#!/bin/sh
# Самотест check-registry-row-closed (П16: страж без доказательства красноты
# запрещён). Краснота доказывается МУТАЦИЕЙ ПОДСУДНОГО: у подложного реестра
# отнимаем замыкающую трубу и ждём красного с названным номером строки.
export LC_ALL=C
SD="$(cd "$(dirname "$0")" && pwd)"
ROOT="$(cd "$SD/../../.." && pwd)"
G="$ROOT/scripts/guards/check-registry-row-closed.sh"
T="${TMPDIR:-/tmp}/nova-registry-row-closed-selftest.$$"
mkdir -p "$T"
trap 'rm -rf "$T"' 0
fails=0
ok()  { echo "  ok   $1"; }
bad() { echo "  FAIL $1" >&2; fails=$((fails+1)); }

[ -f "$G" ] || { echo "  FAIL нет судимого файла: $G" >&2; exit 1; }

run() { NOVA_REGISTRY_FILE="$1" bash "$G" "$ROOT" 2>&1; }

# ── проходит: все строки закрыты ──────────────────────────────────────────
cat > "$T/good.md" <<'MD'
# registry
| 100 | K1 | text one |
| 101 | K2 | text two |
MD
out=$(run "$T/good.md"); rc=$?
[ "$rc" -eq 0 ] && ok "все строки закрыты — зелёный" || bad "закрытые строки покраснели (rc=$rc)"
case "$out" in *"ok:"*) ok "зелёный назвал себя" ;; *) bad "зелёный без строки ok" ;; esac
case "$out" in *" 2 "*) ok "назвал число строк" ;; *) bad "не назвал число строк" ;; esac

# ── краснеет: одна строка без трубы ───────────────────────────────────────
cat > "$T/bad.md" <<'MD'
# registry
| 100 | K1 | text one |
| 101 | K2 | text two without a closing pipe
MD
out=$(run "$T/bad.md"); rc=$?
[ "$rc" -ne 0 ] && ok "незакрытая строка — красный" || bad "незакрытая строка проглочена (rc=0)"
case "$out" in *101*) ok "назвал НОМЕР незакрытой строки" ;; *) bad "не назвал номер" ;; esac
case "$out" in *467*) ok "назвал цену правила (пример 467)" ;; *) bad "не назвал цену правила" ;; esac

# ── краснеет: труба съедена вместе с хвостом (та самая порча) ─────────────
cat > "$T/eaten.md" <<'MD'
# registry
| 100 | K1 | the full stop at the end was eaten
MD
out=$(run "$T/eaten.md"); rc=$?
[ "$rc" -ne 0 ] && ok "воспроизведённая порча — красный" || bad "порча проглочена"

# ── проходит: пробелы после трубы не считаются нарушением ─────────────────
printf '# registry\n| 100 | K1 | trailing spaces after the pipe |   \n' > "$T/spaces.md"
out=$(run "$T/spaces.md"); rc=$?
[ "$rc" -eq 0 ] && ok "пробелы после трубы — не нарушение" || bad "пробелы после трубы покраснели"

# ── проходит: строки НЕ-реестра не судятся ────────────────────────────────
cat > "$T/other.md" <<'MD'
# registry
| N | class | description
| 100 | K1 | a real row |
MD
out=$(run "$T/other.md"); rc=$?
[ "$rc" -eq 0 ] && ok "шапка таблицы без номера не судится" || bad "шапка таблицы покраснела"

# ── краснеет: файла реестра нет вовсе (страж потерял мишень) ──────────────
out=$(NOVA_REGISTRY_FILE="$T/nope.md" bash "$G" "$ROOT" 2>&1); rc=$?
[ "$rc" -ne 0 ] && ok "нет файла реестра — красный (мишень потеряна)" || bad "отсутствие реестра проглочено"

# ── настоящий реестр дерева обязан быть зелёным ───────────────────────────
out=$(bash "$G" "$ROOT" 2>&1); rc=$?
[ "$rc" -eq 0 ] && ok "реестр дерева зелёный" || bad "реестр дерева красный: $out"

echo "самотест check-registry-row-closed: PASS $((11-fails)) FAIL $fails"
[ "$fails" -eq 0 ]
