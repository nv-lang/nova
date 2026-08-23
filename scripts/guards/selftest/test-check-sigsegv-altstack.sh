#!/usr/bin/env bash
# Селфтест scripts/guards/check-sigsegv-altstack.sh (реестр 221.1 №745).
#
# Страж без селфтеста не работает — правило владельца 2026-07-27, и
# check-guard-wiring его энфорсит. Проверяем ОБА направления: на настоящем
# дереве зелено, и каждая из двух половин механизма, снятая по отдельности,
# краснит. Сами случаи живут в самом страже (`--selftest`) — здесь один
# источник, а не две расходящиеся копии, и вдобавок отдельная проверка того,
# что живое дерево проходит.
set -u
export LC_ALL=C

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
G="$ROOT/scripts/guards/check-sigsegv-altstack.sh"
FAILED=0
ok()  { echo "  ok: $1"; }
bad() { echo "  ПРОВАЛ: $1" >&2; FAILED=1; }

echo "== селфтест check-sigsegv-altstack =="

# 1. Настоящее дерево — зелено (фикс №745 стоит: SA_ONSTACK + sigaltstack).
if bash "$G" "$ROOT" >/dev/null 2>&1; then
    ok "живое дерево проходит"
else
    bad "живое дерево покраснело — либо фикс №745 откачен, либо страж ложнит"
fi

# 2. Случаи самого стража: снятый SA_ONSTACK, SA_ONSTACK без стека, пропавшая
#    мишень. Каждый обязан краснить, годный образец — нет.
if out=$(bash "$G" --selftest 2>&1); then
    echo "$out" | sed 's/^/  /'
    ok "все случаи стража отработали"
else
    echo "$out" | sed 's/^/  /' >&2
    bad "случаи стража не сошлись"
fi

if [ "$FAILED" -eq 0 ]; then
    echo "селфтест check-sigsegv-altstack: ok"
    exit 0
fi
echo "селфтест check-sigsegv-altstack: ЕСТЬ ПРОВАЛЫ" >&2
exit 1
