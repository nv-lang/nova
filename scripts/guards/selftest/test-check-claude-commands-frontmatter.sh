#!/bin/sh
# Самотест check-claude-commands-frontmatter (П16: страж без доказательства
# красноты запрещён). Красноту доказываем ПОДЛОЖНОЙ ПАПКОЙ команд: каждый случай
# ломает шапку ровно одним способом и ждёт красный.
#
# Главный случай — второй: НЕЗАКАВЫЧЕННОЕ ДВОЕТОЧИЕ в `description`. Именно эта
# форма прошла мимо меня 2026-08-23 и сломала две команды из четырёх; если
# случай зазеленеет, страж перестал ловить ровно то, ради чего заведён.
export LC_ALL=C
GD="$(cd "$(dirname "$0")/.." && pwd)"
ROOT="$(cd "$GD/../.." && pwd)"
G="$GD/check-claude-commands-frontmatter.py"
T="${TMPDIR:-/tmp}/claude-commands-frontmatter-selftest.$$"
mkdir -p "$T"
trap 'rm -rf "$T"' 0
fails=0
ok()  { echo "  ok   $1"; }
bad() { echo "  FAIL $1" >&2; fails=$((fails+1)); }

D="$T/commands"
mk_dir() { rm -rf "$D"; mkdir -p "$D"; }
good() {
    printf -- '---\ndescription: "a plain command"\nargument-hint: "[what]"\n---\n\nBody.\n' > "$D/good.md"
}

# Красный ждём с названной причиной; зелёный — со строкой ok:.
expect_red() {
    if python "$G" "$ROOT" "$D" > "$T/o" 2> "$T/e"; then
        bad "$1: страж зелёный, а должен краснеть"
    elif grep -q "^check-claude-commands-frontmatter FAIL:" "$T/o"; then
        ok "$1"
    else
        bad "$1: красный, но без строки FAIL: ($(head -1 "$T/o" 2>/dev/null))"
    fi
}
expect_green() {
    if python "$G" "$ROOT" "$D" > "$T/o" 2> "$T/e"; then
        if grep -q "^check-claude-commands-frontmatter ok:" "$T/o"; then
            ok "$1"
        else
            bad "$1: зелёный, но без строки ok:"
        fi
    else
        bad "$1: ложняк — красный на законном входе ($(head -2 "$T/o" 2>/dev/null))"
    fi
}

# ── 1. законная шапка — зелёный ──────────────────────────────────────────
mk_dir; good
expect_green "законная шапка"

# ── 2. незакавыченное двоеточие — КРАСНЫЙ (прецедент 2026-08-23) ─────────
mk_dir; good
printf -- '---\ndescription: recheck: proof, not memory\n---\n\nBody.\n' > "$D/colon.md"
expect_red "двоеточие в значении: YAML читает вложенное отображение"

# ── 3. значение, начинающееся с `[` — КРАСНЫЙ (список, не строка) ────────
mk_dir; good
printf -- '---\ndescription: "ok"\nargument-hint: [a; b]\nx: {\n---\n\nBody.\n' > "$D/seq.md"
expect_red "неполное отображение в шапке"

# ── 4. шапка не закрыта — КРАСНЫЙ ────────────────────────────────────────
mk_dir; good
printf -- '---\ndescription: "unterminated"\n\nBody without a closing fence.\n' > "$D/unterminated.md"
expect_red "шапка не закрыта вторым разделителем"

# ── 5. шапки нет вовсе — КРАСНЫЙ ─────────────────────────────────────────
mk_dir; good
printf -- '# Just a document\n\nNo frontmatter at all.\n' > "$D/nofm.md"
expect_red "нет шапки"

# ── 6. пустой description — КРАСНЫЙ (команда безымянна в меню) ───────────
mk_dir; good
printf -- '---\ndescription: "   "\n---\n\nBody.\n' > "$D/empty.md"
expect_red "пустой description"

# ── 7. description отсутствует — КРАСНЫЙ ─────────────────────────────────
mk_dir; good
printf -- '---\nargument-hint: "[x]"\n---\n\nBody.\n' > "$D/nodesc.md"
expect_red "нет description"

# ── 8. ноль файлов — зелёное молчание, а не красный ──────────────────────
mk_dir
expect_green "ноль файлов: судить нечего"

# ── 9. папки нет — зелёное молчание ──────────────────────────────────────
rm -rf "$D"
expect_green "папки команд нет: судить нечего"

# ── 10. живое дерево репозитория — зелёный ───────────────────────────────
if python "$G" "$ROOT" > "$T/o" 2> "$T/e"; then
    ok "живое дерево репозитория зелёное"
else
    bad "живое дерево репозитория КРАСНОЕ: $(head -3 "$T/o" 2>/dev/null)"
fi

echo "самотест check-claude-commands-frontmatter: $( [ "$fails" -eq 0 ] && echo PASS || echo "FAIL $fails" )"
[ "$fails" -eq 0 ]
