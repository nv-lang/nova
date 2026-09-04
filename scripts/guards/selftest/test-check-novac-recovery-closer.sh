#!/usr/bin/env bash
# scripts/guards/selftest/test-check-novac-recovery-closer.sh — самотест стража
# «восстановление не съедает закрывающий токен» (274.3/F15).
#
# ПОЧЕМУ САМОТЕСТ. Страж заведён 2026-09-04 по вопросу владельца «кто это
# контролирует»: правило держалось одним вызовом предиката и комментарием, а
# ДЕРЖАТЕЛЬ, написанный первым, проходил с ПРАВИЛОМ СНЯТЫМ — он целил в формы,
# которые до предиката не доходят. Страж, который сам не проверен обеими
# сторонами, повторил бы ту же ошибку этажом выше.
#
# ШЕСТЬ СЛУЧАЕВ, и каждый отвечает на свой вопрос:
#   1. чистая подложка (предикат) — зелёный;
#   2. чистая подложка (метка) — зелёный;
#   3. голое место без защиты — КРАСНЫЙ, и место названо;
#   4. метка короче пяти слов — КРАСНЫЙ (отписка не считается решением);
#   5. защита ДАЛЬШЕ окна — КРАСНЫЙ (иначе метка «где-то в файле» проходила бы);
#   6. мишень потеряна (ноль мест) — КРАСНЫЙ, а не зелёный ноль.
set -u
export LC_ALL=C

ROOT="$(CDPATH= cd -- "$(dirname -- "$0")/../../.." && pwd)"
G="$ROOT/scripts/guards/check-novac-recovery-closer.py"
T="${TMPDIR:-/tmp}/novac-recovery-closer-selftest.$$"
FAILED=0

ok()  { echo "  ok: $1"; }
bad() { echo "  FAIL: $1"; FAILED=$((FAILED+1)); }
run() { python "$G" "$ROOT" "$1" >"$T/out" 2>"$T/err"; }

mkdir -p "$T"
trap 'rm -rf "$T"' EXIT

EAT='    kids.push(@node(NodeKind.Err, []Node.of(@take())))'

# --- 1. защита предикатом ------------------------------------------------------
mkdir -p "$T/pred"
{ echo 'module p'
  echo 'fn f() -> Node {'
  echo '    if is_terminator(@peek()) { return @error_node(TokenKind.ErrorTok) }'
  echo "$EAT"
  echo '}'; } > "$T/pred/a.nv"
if run "$T/pred"; then ok "предикат рядом — зелёный"; else bad "предикат рядом, а страж красный: $(cat "$T/err")"; fi

# --- 2. защита меткой ----------------------------------------------------------
mkdir -p "$T/mark"
{ echo 'module p'
  echo 'fn f() -> Node {'
  echo '    // RECOVERY-BOUNDED: the loop above stops at the closing brace here'
  echo "$EAT"
  echo '}'; } > "$T/mark/a.nv"
if run "$T/mark"; then ok "метка с причиной — зелёный"; else bad "метка есть, а страж красный: $(cat "$T/err")"; fi

# --- 3. ГЛАВНЫЙ случай: голое место -------------------------------------------
mkdir -p "$T/naked"
{ echo 'module p'; echo 'fn f() -> Node {'; echo "$EAT"; echo '}'; } > "$T/naked/a.nv"
if run "$T/naked"; then
    bad "голое место восстановления прошло зелёным"
else
    grep -q "a.nv:3" "$T/err" && ok "голое место поймано и названо строкой" || bad "красный, но без адреса"
fi

# --- 4. метка-отписка ----------------------------------------------------------
mkdir -p "$T/short"
{ echo 'module p'
  echo 'fn f() -> Node {'
  echo '    // RECOVERY-BOUNDED: ok'
  echo "$EAT"
  echo '}'; } > "$T/short/a.nv"
if run "$T/short"; then bad "метка из одного слова прошла как причина"; else ok "метка короче пяти слов — красный"; fi

# --- 5. защита за пределами окна ----------------------------------------------
mkdir -p "$T/far"
{ echo 'module p'
  echo 'fn f() -> Node {'
  echo '    // RECOVERY-BOUNDED: this reason stands far above the site itself'
  for _ in $(seq 1 14); do echo '    mut x = 0'; done
  echo "$EAT"
  echo '}'; } > "$T/far/a.nv"
if run "$T/far"; then bad "защита за пределами окна засчиталась"; else ok "защита дальше окна не считается"; fi

# --- 6. потерянная мишень ------------------------------------------------------
mkdir -p "$T/none"
{ echo 'module p'; echo 'fn f() -> int => 1'; } > "$T/none/a.nv"
if run "$T/none"; then
    bad "ноль мест восстановления — а страж сказал зелёный"
else
    grep -q "мишень" "$T/err" && ok "ноль мест — красный, и назван потерей мишени" || bad "красный, но не про мишень"
fi

echo "итог: FAIL $FAILED"
if [ "$FAILED" -eq 0 ]; then
    echo "test-check-novac-recovery-closer ok: голое место, отписка и защита вне окна краснеют; предикат и метка законны"
    exit 0
fi
exit 1
