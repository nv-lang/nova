#!/bin/sh
# Самотест четырёх стражей-«ожидающих бинарь» novac:
#   check-novac-differential.sh, check-novac-diag-schema.sh,
#   check-novac-no-cascade.sh, check-novac-no-panic.sh.
# Оба направления (норма 254): ловит нарушение И не краснит законное;
# отсутствие бинаря — зелёное «судить нечего» со строкой ok: (№645).
# Бинарь подменяется fake-скриптом через $2, корень — фейковым деревом через $1.
export LC_ALL=C
GD="$(cd "$(dirname "$0")/.." && pwd)"
T="${TMPDIR:-/tmp}/novac-binary-guards-selftest.$$"
R="$T/root"
mkdir -p "$R/novac/fixtures" "$R/nova-cli/target/release" "$T/bins"
fails=0

ok() { echo "  ok: $1"; }
bad() { echo "  FAIL: $1" >&2; fails=$((fails+1)); }
run() { sh "$GD/check-novac-$1.sh" "$R" "$2" > "$T/out" 2> "$T/err"; }

# --- фейковый корень: фикстуры и оракул ---
echo 'fn main() {}' > "$R/novac/fixtures/pos_a.nv"
echo 'fn main() { let x = 1 }' > "$R/novac/fixtures/pos_b.nv"
echo 'fn main() { broken' > "$R/novac/fixtures/neg_a.nv"
printf '#!/bin/sh\nexit 0\n' > "$R/nova-cli/target/release/nova.exe"
chmod +x "$R/nova-cli/target/release/nova.exe"

# --- фейковые бинари novac ---
mkbin() { cat > "$T/bins/$1"; chmod +x "$T/bins/$1"; }
mkbin agree.exe <<'EOF'
#!/bin/sh
exit 0
EOF
mkbin diverge.exe <<'EOF'
#!/bin/sh
case "$2" in *pos_b.nv) exit 1 ;; esac
exit 0
EOF
mkbin onediag.exe <<'EOF'
#!/bin/sh
printf '%s\n' '[{"id":"D1","code":"E_TEST","severity":"error","primary":{"file":"f.nv","line":1,"col":1},"message":"one error, one cause"}]'
exit 1
EOF
mkbin cascade.exe <<'EOF'
#!/bin/sh
printf '%s\n' '[{"id":"D1","code":"E_A","severity":"error","primary":{"file":"f.nv","line":1,"col":1},"message":"first"},{"id":"D2","code":"E_B","severity":"error","primary":{"file":"f.nv","line":2,"col":1},"message":"cascade echo"}]'
exit 1
EOF
mkbin badschema.exe <<'EOF'
#!/bin/sh
printf '%s\n' '[{"code":"E_A","severity":"error"}]'
exit 1
EOF
mkbin panic.exe <<'EOF'
#!/bin/sh
echo "thread 'main' panicked at novac/src/parse.rs:10:5" >&2
exit 101
EOF
mkbin crash.exe <<'EOF'
#!/bin/sh
exit 139
EOF

# 1-4. Отсутствие бинаря — у всех четырёх зелёное «судить нечего» со строкой ok:.
for g in differential diag-schema no-cascade no-panic; do
    if run "$g" "$R/novac/target/novac.exe" && grep -q 'ok: судить нечего' "$T/out"; then
        ok "$g: нет бинаря — зелёный «судить нечего»"
    else
        bad "$g: нет бинаря — ожидался зелёный «судить нечего»"
    fi
done

# 5. differential, законное: исходы совпали с оракулом — зелёный со строкой ok:.
if run differential "$T/bins/agree.exe" && grep -q 'ok:' "$T/out"; then
    ok "differential: совпадение с оракулом проходит, строка ok: есть"
else
    bad "differential: законное совпадение покраснело или без строки ok:"
fi

# 6. differential, нарушение: расхождение вне allow — красный, фикстура названа.
if run differential "$T/bins/diverge.exe"; then
    bad "differential: расхождение вне allow прошло"
elif grep -q 'pos_b.nv' "$T/err"; then
    ok "differential: расхождение вне allow поймано и названо"
else
    bad "differential: красный, но виновная фикстура не названа"
fi

# 7. differential, законное: то же расхождение записано в allow — зелёный.
echo 'novac/fixtures/pos_b.nv' > "$R/novac/divergences.allow"
if run differential "$T/bins/diverge.exe" && grep -q 'ok:' "$T/out"; then
    ok "differential: расхождение из allow не краснит"
else
    bad "differential: разрешённое расхождение покраснело"
fi
rm -f "$R/novac/divergences.allow"

# 8. no-cascade, законное: ровно один severity=error — зелёный со строкой ok:.
if run no-cascade "$T/bins/onediag.exe" && grep -q 'ok:' "$T/out"; then
    ok "no-cascade: один диагностик проходит, строка ok: есть"
else
    bad "no-cascade: законный одиночный диагностик покраснел"
fi

# 9. no-cascade, нарушение: два severity=error — красный.
if run no-cascade "$T/bins/cascade.exe"; then
    bad "no-cascade: каскад из двух ошибок прошёл"
else
    ok "no-cascade: каскад из двух ошибок пойман"
fi

# 10. diag-schema, законное: все пять полей на месте — зелёный.
if run diag-schema "$T/bins/onediag.exe" && grep -q 'ok:' "$T/out"; then
    ok "diag-schema: валидная диагностика проходит"
else
    bad "diag-schema: валидная диагностика покраснела"
fi

# 11. diag-schema, нарушение: нет id/primary/message — красный.
if run diag-schema "$T/bins/badschema.exe"; then
    bad "diag-schema: диагностика без обязательных полей прошла"
else
    ok "diag-schema: пропавшие поля пойманы"
fi

# 12. no-panic, законное: обычное отвержение (код 1, stderr чист) — зелёный.
if run no-panic "$T/bins/onediag.exe" && grep -q 'ok:' "$T/out"; then
    ok "no-panic: обычное отвержение проходит"
else
    bad "no-panic: обычное отвержение покраснело"
fi

# 13. no-panic, нарушение: слово panic в stderr — красный.
if run no-panic "$T/bins/panic.exe"; then
    bad "no-panic: panic в stderr прошёл"
else
    ok "no-panic: panic в stderr пойман"
fi

# 14. no-panic, нарушение: код возврата 139 (>=128) — красный.
if run no-panic "$T/bins/crash.exe"; then
    bad "no-panic: код возврата 139 прошёл"
else
    ok "no-panic: крэш-код возврата пойман"
fi

rm -rf "$T"
if [ "$fails" -eq 0 ]; then
    echo "test-check-novac-binary-guards ok: 14/14"
    exit 0
fi
echo "test-check-novac-binary-guards: FAIL ($fails)" >&2
exit 1
