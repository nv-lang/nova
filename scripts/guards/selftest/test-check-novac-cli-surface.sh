#!/bin/sh
# Самотест check-novac-cli-surface.sh (П16: обязан доказать, что ловит).
# Швы: $2 — путь к main.nv, $3 — файл со списком команд nova-cli.
export LC_ALL=C
GD="$(cd "$(dirname "$0")/.." && pwd)"
ROOT="$(cd "$GD/../.." && pwd)"
G="$GD/check-novac-cli-surface.sh"
T="${TMPDIR:-/tmp}/novac-cli-surface-selftest.$$"
mkdir -p "$T"
trap 'rm -rf "$T"' 0
fails=0
ok()  { echo "  ok: $1"; }
bad() { echo "  FAIL: $1" >&2; fails=$((fails+1)); }
run() { sh "$G" "$1" "$2" "$3" > "$T/out" 2> "$T/err"; }

# фикстурный корень: свой main.nv и свой allow
mkroot() {
    r="$T/$1"; mkdir -p "$r/novac/src"
    shift
    { echo "module novac"; echo "fn main() {"
      for c in "$@"; do echo "    if a[1] == \"$c\" { work() }"; done
      echo "}" ; } > "$r/novac/src/main.nv"
    printf '%s\n' "check" "build" "test" "lint" > "$r/cli.txt"
    echo "$r"
}

# --- 1. команды novac — подмножество: зелёный ----------------------------
R=$(mkroot g1 check build)
if run "$R" "$R/novac/src/main.nv" "$R/cli.txt"; then
    grep -q "команд novac: 2" "$T/out" && ok "подмножество — зелёный, счёт верный" || bad "зелёный, но счёт не тот [$(cat "$T/out")]"
else
    bad "подмножество покраснело: $(cat "$T/err")"
fi

# --- 2. ГЛАВНЫЙ случай: выдуманная команда — красный --------------------
R=$(mkroot g2 check emit)
if run "$R" "$R/novac/src/main.nv" "$R/cli.txt"; then
    bad "выдуманная команда прошла — страж не ловит свой главный случай"
else
    grep -q "emit" "$T/err" && ok "выдуманная команда поймана и названа" || bad "красный, но emit не назван [$(cat "$T/err")]"
fi

# --- 3. та же команда, записанная в allow с причиной — зелёный ----------
R=$(mkroot g3 check emit)
printf '%s\n' "# осознанное расхождение" "emit # причина и условие схождения" > "$R/novac/cli-divergences.allow"
if run "$R" "$R/novac/src/main.nv" "$R/cli.txt"; then
    grep -q "осознанных расхождений: 1" "$T/out" && ok "запись в allow снимает красноту и считается" || bad "прошло, но не сосчитано [$(cat "$T/out")]"
else
    bad "запись в allow не сработала: $(cat "$T/err")"
fi

# --- 4. allow с ДРУГОЙ командой не покрывает нашу ------------------------
R=$(mkroot g4 check emit)
printf '%s\n' "run # другое расхождение" > "$R/novac/cli-divergences.allow"
run "$R" "$R/novac/src/main.nv" "$R/cli.txt" && bad "чужая запись в allow покрыла emit" || ok "allow покрывает только названную команду"

# --- 4а. флаг novac, которого нет у nova-cli, — красный (П26 п.5) ---------
R=$(mkroot g4a check)
printf '    if a[2] == "--std" { work() }
' >> "$R/novac/src/main.nv"
printf '%s
' "check" "build" "--verbose" "--quiet" > "$R/cli.txt"
if run "$R" "$R/novac/src/main.nv" "$R/cli.txt"; then bad "флаг --std, которого нет у nova-cli, прошёл"; else grep -q -- "--std" "$T/err" && ok "novac-only флаг пойман" || bad "красный, но не про флаг"; fi
# флаг, который у nova-cli есть, — зелёный
R=$(mkroot g4b check)
printf '    if a[2] == "--verbose" { work() }
' >> "$R/novac/src/main.nv"
printf '%s
' "check" "build" "--verbose" "--quiet" > "$R/cli.txt"
run "$R" "$R/novac/src/main.nv" "$R/cli.txt" && ok "флаг nova-cli проходит" || bad "законный флаг покраснел: $(cat "$T/err")"

# --- 5. пустой список команд nova-cli — КРАСНЫЙ, не зелёный -------------
R=$(mkroot g5 check)
: > "$R/cli-empty.txt"
if run "$R" "$R/novac/src/main.nv" "$R/cli-empty.txt"; then
    bad "пустой список команд дал зелёный — вечнозелёный страж (класс №519)"
else
    grep -q "разбор --help сломался" "$T/err" && ok "пустой список — красный, названо почему" || bad "красный, но без объяснения"
fi

# --- 6. нет main.nv — судить нечего --------------------------------------
R=$(mkroot g6 check)
run "$R" "$R/novac/src/absent.nv" "$R/cli.txt"
grep -q "судить нечего" "$T/out" && ok "нет main.nv — судить нечего" || bad "ждали «судить нечего»"

# --- 7. настоящее дерево -------------------------------------------------
sh "$G" "$ROOT" >/dev/null 2>&1 && ok "настоящее дерево — зелёное" || bad "настоящее дерево покраснело: $(sh "$G" "$ROOT" 2>&1 | head -3)"

echo "итог: FAIL $fails"
if [ "$fails" -eq 0 ]; then
    echo "test-check-novac-cli-surface ok: все случаи, включая выдуманную команду и покрытие через allow"
    exit 0
fi
exit 1
