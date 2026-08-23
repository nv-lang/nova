#!/bin/sh
# Самотест check-novac-plan-donor (П16: страж без доказательства красноты
# запрещён). Красноту доказываем ПОДЛОЖНЫМ РЕПОЗИТОРИЕМ: план с припиской
# оракулу обязан краснеть, тот же план с законным донором — нет.
#
# Главные случаи — третий и четвёртый: разрешение работает по СТРОКЕ, а не по
# файлу (иначе один законный абзац открыл бы весь файл), и протухшее разрешение
# красное (иначе список молча пропустит следующее нарушение — ровно то, из-за
# чего строка «донор: оракул» прожила в плане три дня).
export LC_ALL=C
GD="$(cd "$(dirname "$0")/.." && pwd)"
ROOT="$(cd "$GD/../.." && pwd)"
G="$GD/check-novac-plan-donor.py"
T="${TMPDIR:-/tmp}/novac-plan-donor-selftest.$$"
mkdir -p "$T"
trap 'rm -rf "$T"' 0
fails=0
ok()  { echo "  ok   $1"; }
bad() { echo "  FAIL $1" >&2; fails=$((fails+1)); }

REPO="$T/tree"
mk() {
    rm -rf "$REPO"
    mkdir -p "$REPO/docs/plans" "$REPO/docs/dev" "$REPO/scripts/guards"
    printf 'Plan text.\nDonor: rustc keeps the ABI in the header.\n' \
        > "$REPO/docs/plans/274-x.md"
    printf 'Conventions.\nForm donors: rustc, Go, Zig.\n' \
        > "$REPO/docs/dev/novac-conv.md"
    : > "$REPO/allow"
    (cd "$REPO" && git init -q . && git add -A && \
        git -c user.name=t -c user.email=t@t commit -q -m init) >/dev/null 2>&1
}
run() { python "$G" "$REPO" "$REPO/allow" > "$T/o" 2> "$T/e"; }
red()   { if run; then bad "$1: зелёный, а должен краснеть"; \
          elif grep -q "^check-novac-plan-donor FAIL:" "$T/o"; then ok "$1"; \
          else bad "$1: красный без строки FAIL:"; fi }
green() { if run; then grep -q "^check-novac-plan-donor ok:" "$T/o" \
              && ok "$1" || bad "$1: зелёный без строки ok:"; \
          else bad "$1: ложняк — $(head -2 "$T/o")"; fi }

# ── 1. чистое дерево — зелёный ───────────────────────────────────────────
mk
green "план с донором rustc проходит"

# ── 2. приписка оракулу в плане — КРАСНЫЙ (прецедент 2026-08-20..23) ─────
mk
printf 'listy zhivut vnutri FnDecl (donor: oracle keeps the ABI tag).\n' \
    >> "$REPO/docs/plans/274-x.md"
(cd "$REPO" && git add -A && git -c user.name=t -c user.email=t@t commit -q -m x) >/dev/null 2>&1
red "приписка оракулу в плане"

# ── 3. разрешение по СТРОКЕ снимает именно её ───────────────────────────
printf 'docs/plans/274-x.md|donor: oracle keeps the ABI tag\n' > "$REPO/allow"
green "разрешение по строке снимает вхождение"

# ── 4. ПРОТУХШЕЕ разрешение — КРАСНЫЙ ───────────────────────────────────
printf 'docs/plans/274-x.md|takoy stroki v faile net\n' > "$REPO/allow"
red "протухшее разрешение"

# ── 5. приписка в конвенции novac — тоже КРАСНЫЙ (обе области) ──────────
mk
printf 'we take the shape from the oracle, our donor for now.\n' \
    >> "$REPO/docs/dev/novac-conv.md"
(cd "$REPO" && git add -A && git -c user.name=t -c user.email=t@t commit -q -m x) >/dev/null 2>&1
red "приписка оракулу в конвенции novac"

# ── 6. неотслеживаемый файл не судится (git ls-files — область) ─────────
mk
printf 'donor: the oracle, again.\n' > "$REPO/docs/plans/274-untracked.md"
green "неотслеживаемый файл вне области"

# ── 7. живое дерево репозитория — зелёный ───────────────────────────────
if python "$G" "$ROOT" > "$T/o" 2> "$T/e"; then
    ok "живое дерево зелёное"
else
    bad "живое дерево КРАСНОЕ: $(head -3 "$T/o")"
fi

echo "самотест check-novac-plan-donor: $( [ "$fails" -eq 0 ] && echo PASS || echo "FAIL $fails" )"
[ "$fails" -eq 0 ]
