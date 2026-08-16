#!/usr/bin/env bash
# selftest для check-process-exit-under-pool.sh (№694).
#
# Что доказывает: страж (1) зелёный, когда каждая из N проб завершается;
# (2) КРАСНЫЙ, когда хотя бы одна проба зависает — именно «хотя бы одна из
# многих», потому что дефект №694 вероятностный (~1% при 16 воркерах) и
# единичный запуск его не ловит по построению; (3) красный, когда проба не
# собралась — «судить нечего» не выдаётся за «зелено» (класс F1/№645).
#
# Настоящий компилятор здесь НЕ нужен: подменяется через шов
# NOVA_EXIT_GUARD_NOVA поддельным `nova`, который по `-o <exe>` кладёт
# скрипт-пробу заданного поведения. Так самотест идёт секунды, а не полминуты,
# и не зависит от того, собран ли nova-cli.
set -u
export LC_ALL=C
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
G="$ROOT/scripts/guards/check-process-exit-under-pool.sh"
TMP="$(mktemp -d)"; trap 'rm -rf "$TMP"' EXIT
PASS=0; FAIL=0
ok()  { PASS=$((PASS+1)); echo "  ok   $1"; }
bad() { FAIL=$((FAIL+1)); echo "  FAIL $1" >&2; }

# make_nova <тело пробы> — поддельный компилятор: -o <exe> получает скрипт с
# этим телом. Тело читает счётчик запусков из файла, чтобы «зависнуть на k-м».
make_nova() {
    printf '%s\n' "$1" > "$TMP/probe_body.sh"
    cat > "$TMP/nova" <<'EOS'
#!/usr/bin/env bash
out=""; prev=""
for a in "$@"; do [ "$prev" = "-o" ] && out="$a"; prev="$a"; done
[ -n "$out" ] || exit 1
{ echo '#!/usr/bin/env bash'; cat "@@TMP@@/probe_body.sh"; } > "$out"
chmod +x "$out"
exit 0
EOS
    sed -i "s|@@TMP@@|$TMP|g" "$TMP/nova"
    chmod +x "$TMP/nova"
}

# Пробы под Git Bash: .exe-суффикс страж даёт сам, bash исполняет скрипт по
# shebang независимо от расширения.

echo "== проходит =="
make_nova 'echo ok; exit 0'
: > "$TMP/counter"
out=$(NOVA_EXIT_GUARD_NOVA="$TMP/nova" bash "$G" "$ROOT" 12 2>&1); rc=$?
if [ "$rc" -eq 0 ] && printf '%s' "$out" | grep -q 'ok: 12 запусков'; then
    ok "12 из 12 завершились — зелёный, и строка называет число"
else
    bad "ложный красный на всегда-выходящей пробе (rc=$rc): $out"
fi

echo "== ловит =="
# Виснет ровно один запуск из 12 (7-й): «редко» — тот самый профиль №694.
make_nova 'f="@@TMP@@/counter"; n=$(cat "$f" 2>/dev/null || echo 0); n=$((n+1)); echo $n > "$f"; if [ "$n" -eq 7 ]; then sleep 30; fi; exit 0'
sed -i "s|@@TMP@@|$TMP|g" "$TMP/probe_body.sh"
: > "$TMP/counter"
out=$(NOVA_EXIT_GUARD_NOVA="$TMP/nova" bash "$G" "$ROOT" 12 2>&1); rc=$?
if [ "$rc" -ne 0 ] && printf '%s' "$out" | grep -q '1 из 12 запусков не завершились'; then
    ok "один зависший из 12 — красный, и сказано, который"
else
    bad "пропустил зависание 1/12 (rc=$rc): $out"
fi

# Не собралось — красный, не «судить нечего».
cat > "$TMP/nova" <<'EOS'
#!/usr/bin/env bash
echo "error: pretend build failure" >&2
exit 1
EOS
chmod +x "$TMP/nova"
out=$(NOVA_EXIT_GUARD_NOVA="$TMP/nova" bash "$G" "$ROOT" 3 2>&1); rc=$?
if [ "$rc" -ne 0 ] && printf '%s' "$out" | grep -q 'проба не собралась'; then
    ok "проба не собралась — красный, не тихий ноль"
else
    bad "сборка провалилась, а страж не покраснел (rc=$rc): $out"
fi

# Строка ok — контракт №645: ноль без неё не считается.
if grep -q '\$NAME ok:' "$G"; then ok "страж печатает свою строку ok:"; else bad "у стража нет строки ok:"; fi

echo "итог: $PASS ok, $FAIL FAIL"
[ "$FAIL" -eq 0 ] || { echo "selftest check-process-exit-under-pool: ПРОВАЛ" >&2; exit 1; }
echo "selftest check-process-exit-under-pool: OK (зелёный на выходящей пробе / красный на 1 из 12 зависших / красный на несобравшейся)"
exit 0
