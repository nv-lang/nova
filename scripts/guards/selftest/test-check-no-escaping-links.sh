#!/usr/bin/env bash
# selftest для check-no-escaping-links.py (правило владельца 2026-09-12).
#
# Доказывает три вещи, и третья — та, ради которой страж отдельный:
#   * зелёный на настоящем дереве;
#   * КРАСНЫЙ на ссылке, уходящей за корень;
#   * НЕ ложнит на `../`, которая за корень НЕ уходит — глубина файла считается,
#     а не число точек. Текстовый образец этой разницы не видит, и без этого
#     случая страж свёлся бы к «запретить `../`», что сломало бы половину доки.
set -u
export LC_ALL=C
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
G="$ROOT/scripts/guards/check-no-escaping-links.py"
TMP="$(mktemp -d)"; trap 'rm -rf "$TMP"' EXIT
PASS=0; FAIL=0
ok()  { PASS=$((PASS+1)); echo "  ok   $1"; }
bad() { FAIL=$((FAIL+1)); echo "  FAIL $1" >&2; }

mk() { rm -rf "$TMP/r"; mkdir -p "$TMP/r/docs/dev" "$TMP/r/docs/plans"
       git -C "$TMP/r" init -q 2>/dev/null; }
addf() { mkdir -p "$(dirname "$TMP/r/$1")"; printf '%s\n' "$2" > "$TMP/r/$1"
         git -C "$TMP/r" add "$1" 2>/dev/null; }

echo "== проходит =="
out=$(python "$G" "$ROOT" 2>&1); rc=$?
if [ "$rc" -eq 0 ] && printf '%s' "$out" | grep -q 'ok: .md'; then
    ok "настоящее дерево — зелёный"
else bad "ложный красный на дереве: $out"; fi

mk; addf docs/dev/a.md '[внутрь](../../AGENTS.md) и [ещё глубже](../plans/x.md)'
addf AGENTS.md 'root'
out=$(python "$G" "$TMP/r" 2>&1); rc=$?
if [ "$rc" -eq 0 ]; then ok "переход вверх, не покидающий корень — не считается"; else bad "ложный красный на внутренней ссылке: $out"; fi

mk; addf docs/dev/b.md '[наружу](../../../nova-private/docs/x.md)'
out=$(python "$G" "$TMP/r" 2>&1); rc=$?
if [ "$rc" -ne 0 ] && printf '%s' "$out" | grep -q 'за корень'; then
    ok "ссылка за корень — красный, файл и цель названы"
else bad "не поймал ссылку за корень: $out"; fi

mk; addf docs/dev/c.md '[домой](../../../Users/someone/.claude/memory/x.md)'
out=$(python "$G" "$TMP/r" 2>&1); rc=$?
if [ "$rc" -ne 0 ]; then ok "домашний каталог за корнем — красный"; else bad "не поймал домашний путь: $out"; fi

mk; addf docs/dev/d.md '[сеть](https://example.org/../../x) и [якорь](#section)'
out=$(python "$G" "$TMP/r" 2>&1); rc=$?
if [ "$rc" -eq 0 ]; then ok "http и якорь — не считаются"; else bad "ложный красный на http/якоре: $out"; fi

echo "итог: $PASS ok, $FAIL FAIL"
if [ "$FAIL" -eq 0 ]; then
    echo "selftest check-no-escaping-links: OK (зелёный на дереве и 3 законных формах / красный на 2 формах ухода)"
    exit 0
fi
echo "selftest check-no-escaping-links: ПРОВАЛ" >&2
exit 1
