#!/bin/sh
# Самотест check-after-compact-budget (П16: страж без доказательства красноты
# запрещён). Красноту доказываем МУТАЦИЕЙ ПОДСУДНОГО: у подложного дерева
# отнимаем по одной части механизма и ждём красного с названной причиной.
#
# Судимое здесь — не текст правил, а ТРУБА: список, читающий его хук и потолок.
# Поэтому подложное дерево содержит все три части, и каждая проба ломает ровно
# одну.
export LC_ALL=C
GD="$(cd "$(dirname "$0")/.." && pwd)"
G="$GD/check-after-compact-budget.py"
T="${TMPDIR:-/tmp}/nova-after-compact-selftest.$$"
trap 'rm -rf "$T"' 0
fails=0
ok()  { echo "  ok   $1"; }
bad() { echo "  FAIL $1" >&2; fails=$((fails+1)); }

# ── подложное дерево со ВСЕМИ частями механизма ──────────────────────────────
mk_tree() {
    rm -rf "$T/tree"
    mkdir -p "$T/tree/.claude/commands" "$T/tree/scripts/guards" "$T/tree/scripts/claude-hooks"
    printf 'x%.0s' $(seq 1 100) > "$T/tree/.claude/commands/a.md"
    printf '# list\n\n.claude/commands/a.md\n' > "$T/tree/.claude/after-compact.list"
    printf '# cap\n1000\n' > "$T/tree/scripts/guards/after-compact-budget.baseline"
    printf 'read the .claude/after-compact.list here\n' \
        > "$T/tree/scripts/claude-hooks/inject-after-compact.py"
}

run() { python "$G" "$T/tree" >"$T/out" 2>"$T/err"; echo $?; }
said() { grep -q "$1" "$T/out" "$T/err" 2>/dev/null; }

# ── 1. живое дерево — зелёное, и говорит числа ───────────────────────────────
mk_tree
[ "$(run)" = "0" ] && said "100" && said "1000" \
    && ok "здоровое дерево: зелёный, назвал и объём, и потолок" \
    || bad "здоровое дерево должно быть зелёным и печатать оба числа"

# ── 2. нет потолка — красный ────────────────────────────────────────────────
mk_tree; rm -f "$T/tree/scripts/guards/after-compact-budget.baseline"
[ "$(run)" = "1" ] && ok "потолок убран — красный" || bad "без потолка обязан краснеть"

# ── 3. потолок не разобран — красный ────────────────────────────────────────
mk_tree; printf '# cap\nмного\n' > "$T/tree/scripts/guards/after-compact-budget.baseline"
[ "$(run)" = "1" ] && ok "потолок не число — красный" || bad "неразобранный потолок обязан краснеть"

# ── 4. инжектора нет — красный ──────────────────────────────────────────────
mk_tree; rm -f "$T/tree/scripts/claude-hooks/inject-after-compact.py"
[ "$(run)" = "1" ] && ok "инжектор убран — красный" || bad "список без читателя обязан краснеть"

# ── 5. инжектор перестал читать список — красный (выхолащивание) ────────────
mk_tree; printf 'inject something, but not from any list\n' \
    > "$T/tree/scripts/claude-hooks/inject-after-compact.py"
[ "$(run)" = "1" ] && ok "инжектор не читает список — красный" \
    || bad "выхолощенный механизм обязан краснеть"

# ── 6. файл из списка пропал — красный (это и есть молчание хука) ───────────
mk_tree; rm -f "$T/tree/.claude/commands/a.md"
[ "$(run)" = "1" ] && said "a.md" \
    && ok "файл списка пропал — красный, и назван поимённо" \
    || bad "пропавший файл обязан краснеть с именем"

# ── 7. объём выше потолка — красный ─────────────────────────────────────────
mk_tree; printf 'y%.0s' $(seq 1 2000) > "$T/tree/.claude/commands/a.md"
[ "$(run)" = "1" ] && ok "объём выше потолка — красный" || bad "превышение обязано краснеть"

# ── 8. потолок ровно по объёму — зелёный (граница не ложнит) ────────────────
mk_tree; printf '# cap\n100\n' > "$T/tree/scripts/guards/after-compact-budget.baseline"
[ "$(run)" = "0" ] && ok "объём РОВНО в потолок — зелёный" || bad "равенство не должно краснить"

# ── 9. списка нет вовсе — зелёный, судить нечего ────────────────────────────
mk_tree; rm -f "$T/tree/.claude/after-compact.list"
[ "$(run)" = "0" ] && ok "списка нет — зелёный, судить нечего" \
    || bad "без списка судить нечего, краснеть не за что"

# ── 10. шапка YAML не считается — как её не считает инжектор ────────────────
mk_tree
{ printf -- '---\ndescription: "x"\n---\n'; printf 'z%.0s' $(seq 1 50); } \
    > "$T/tree/.claude/commands/a.md"
printf '# cap\n50\n' > "$T/tree/scripts/guards/after-compact-budget.baseline"
[ "$(run)" = "0" ] && ok "YAML-шапка в объём не входит" \
    || bad "шапка обязана сниматься, как её снимает инжектор"

echo "самотест check-after-compact-budget: PASS $((10 - fails)) FAIL $fails"
[ "$fails" -eq 0 ] || exit 1
