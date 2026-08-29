#!/usr/bin/env bash
# Селфтест scripts/guards/check-agent-definitions-wired.sh — обе половины связи
# и обе стороны каждой.
set -u
export LC_ALL=C

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
G="$ROOT/scripts/guards/check-agent-definitions-wired.sh"
FAILED=0
ok()  { echo "  ok: $1"; }
bad() { echo "  ПРОВАЛ: $1" >&2; FAILED=1; }

TMP=$(mktemp -d); trap 'rm -rf "$TMP"' EXIT
AG="$TMP/.claude/agents"; CMD="$TMP/.claude/commands"
mk() { rm -rf "$TMP/.claude"; mkdir -p "$AG" "$CMD"; }

# 1. определение названо командой, ссылка ведёт в существующий файл — зелено.
mk
printf 'agent def\n' > "$AG/spec-reader.md"
printf 'use subagent_type: "spec-reader" for reading\n' > "$CMD/read-spec.md"
out=$(bash "$G" "$TMP" 2>&1); rc=$?
if [ "$rc" -eq 0 ]; then ok "связь в обе стороны — зелено"; else bad "ложный отказ: $out"; fi

# 2. определение есть, но о нём не говорит ни одна команда — красно.
mk
printf 'agent def\n' > "$AG/lonely.md"
printf 'a command about nothing in particular\n' > "$CMD/other.md"
out=$(bash "$G" "$TMP" 2>&1); rc=$?
if [ "$rc" -eq 1 ] && echo "$out" | grep -q "lonely"; then
    ok "безымянное определение — красно, и названо"
else
    bad "определение без упоминания обязано краснеть (код $rc): $out"
fi

# 3. команда зовёт агента, которого нет — красно.
mk
printf 'call subagent_type: "ghost" here\n' > "$CMD/x.md"
out=$(bash "$G" "$TMP" 2>&1); rc=$?
if [ "$rc" -eq 1 ] && echo "$out" | grep -q "ghost"; then
    ok "висячая ссылка — красно, и названа"
else
    bad "ссылка на несуществующего агента обязана краснеть (код $rc): $out"
fi

# 4. упоминание БЕЗ кавычек тоже считается ссылкой.
mk
printf 'agent def\n' > "$AG/plain.md"
printf 'subagent_type: plain\n' > "$CMD/x.md"
out=$(bash "$G" "$TMP" 2>&1); rc=$?
if [ "$rc" -eq 0 ]; then ok "ссылка без кавычек разбирается"; else bad "форма без кавычек обязана считаться: $out"; fi

# 5. упоминание имени в ЛЮБОМ месте команды засчитывается как осведомление —
#    команда может говорить об агенте прозой, не вызывая его.
mk
printf 'agent def\n' > "$AG/spec-reader.md"
printf 'take the spec-reader agent, do not roll your own\n' > "$CMD/delegate.md"
out=$(bash "$G" "$TMP" 2>&1); rc=$?
if [ "$rc" -eq 0 ]; then ok "упоминание прозой засчитывается"; else bad "проза тоже осведомляет: $out"; fi

# 5b. упомянут ТОЛЬКО в СКИЛЛЕ, ни одна команда о нём не говорит — зелено.
#     Это главный путь осведомления: скилл всплывает сам, команду надо звать
#     слэшем, а слэш-меню принадлежит владельцу, не агенту.
mk
mkdir -p "$TMP/.claude/skills/read-spec"
printf 'agent def\n' > "$AG/spec-reader.md"
printf 'take the spec-reader agent\n' > "$TMP/.claude/skills/read-spec/SKILL.md"
out=$(bash "$G" "$TMP" 2>&1); rc=$?
if [ "$rc" -eq 0 ]; then ok "упоминание только в скилле засчитывается"; else bad "скилл обязан осведомлять наравне с командой: $out"; fi

# 5c. ссылка subagent_type из СКИЛЛА на несуществующего агента — красно.
mk
mkdir -p "$TMP/.claude/skills/x"
printf 'subagent_type: "phantom"\n' > "$TMP/.claude/skills/x/SKILL.md"
out=$(bash "$G" "$TMP" 2>&1); rc=$?
if [ "$rc" -eq 1 ] && echo "$out" | grep -q "phantom"; then ok "висячая ссылка из скилла — красно"; else bad "скилл, зовущий несуществующего агента, обязан краснеть (код $rc): $out"; fi

# 6. ни каталогов, ни файлов — зелено, судить нечего.
rm -rf "$TMP/.claude"
out=$(bash "$G" "$TMP" 2>&1); rc=$?
if [ "$rc" -eq 0 ]; then ok "каталогов нет — зелено"; else bad "пустое дерево краснеть не должно: $out"; fi

# 7. есть команды, но нет ни одного определения — зелено (агентов просто нет).
mk
printf 'no agents here\n' > "$CMD/x.md"
out=$(bash "$G" "$TMP" 2>&1); rc=$?
if [ "$rc" -eq 0 ]; then ok "команды без агентов — зелено"; else bad "отсутствие агентов не нарушение: $out"; fi

if [ "$FAILED" -eq 0 ]; then echo "селфтест check-agent-definitions-wired: 9/9 ok"; exit 0; fi
echo "селфтест check-agent-definitions-wired: ЕСТЬ ПРОВАЛЫ" >&2
exit 1
