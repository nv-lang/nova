#!/usr/bin/env bash
# Самотест check-novac-mangle-fixed-point.sh — мэнглинг novac держится
# диффом с оракулом (план 274, правило владельца 2026-08-15); норма
# самотестов — план 231 §4в: ловит нарушение И не даёт ложняка.
#
# ПОЧЕМУ ПОДЛОЖКА ТАКАЯ. Страж читает по ФИКСИРОВАННЫМ путям внутри
# $1-корня: бинарь novac/target/novac.exe, оболочку novac/src/emit_c/
# shell.tpl.c, заголовки рантайма compiler-codegen/**.h и файлы подмножества
# examples/basics/*.nv. Значит подложка = временный корень со всеми четырьмя,
# где «эмиссия» — это cat заранее написанного куска C: судится ЛОГИКА
# сравнения множеств имён, а не настоящая кодогенерация.
#
# В подложке ровно ОДИН файл examples/basics/*.nv: страж вычитает из имён
# эмиссии типы ИМЕННО того файла, который эмитил, а обманка отдаёт одну и ту
# же эмиссию на любой вход — два файла давали бы ложный красный на первом же
# пользовательском типе.
#
# Копия lib/novac.sh кладётся в подложку намеренно (см. тот же приём в
# test-check-novac-iteration-cost.sh).
set -u
export LC_ALL=C
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
G="$ROOT/scripts/guards/check-novac-mangle-fixed-point.sh"
TMP="$(mktemp -d)"; trap 'rm -rf "$TMP"' EXIT
PASS=0; FAIL=0
ok()  { PASS=$((PASS+1)); echo "  ok   $1"; }
bad() { FAIL=$((FAIL+1)); echo "  FAIL $1" >&2; }
check(){ if [ "$2" = "$3" ]; then ok "$1"; else bad "$1 (ждал '$3', получил '$2')"; fi; }
has()  { if grep -q "$2" "$1"; then ok "$3"; else bad "$3 (нет '$2' в $1)"; fi; }

FIX="$TMP/root"
mkdir -p "$FIX/novac/target" "$FIX/novac/src/emit_c" "$FIX/scripts/guards/lib" \
         "$FIX/examples/basics" "$FIX/compiler-codegen/nova_rt"
cp "$ROOT/scripts/guards/lib/novac.sh" "$FIX/scripts/guards/lib/novac.sh"
NOVAC="$FIX/novac/target/novac.exe"
SHELL_TPL="$FIX/novac/src/emit_c/shell.tpl.c"
EX="$FIX/examples/basics/hello.nv"

# Оболочка = «эмиссия оракула по probe»: имена, которые оракул определяет.
mkshell() { cat > "$SHELL_TPL"; }
# Заголовок рантайма: часть имён живёт там, а не в оболочке (nova_print_str).
mkhdr() { cat > "$FIX/compiler-codegen/nova_rt/nova_rt.h"; }
# Обманка-novac: печатает заготовленную «эмиссию» и выходит заготовленным кодом.
mkemit() { cat > "$FIX/emit.out"; }
mkrc()   { printf '%s\n' "$1" > "$FIX/emit.rc"; }
mkbin() {
    cat > "$NOVAC" <<'FAKE'
#!/bin/sh
R="$(cd "$(dirname "$0")/../.." && pwd)"
cat "$R/emit.out"
exit "$(cat "$R/emit.rc")"
FAKE
    chmod +x "$NOVAC"
}
run() { sh "$G" "$FIX" > "$TMP/out" 2> "$TMP/err"; echo $?; }

mkshell <<'EOF'
/* поддельная оболочка: имена, определённые оракулом */
void Nova_str_method_len(void);
void Nova_str_method_byte_len(void);
EOF
mkhdr <<'EOF'
/* поддельный заголовок рантайма */
void nova_print_str(const char *s);
EOF
printf 'fn main() { println("hi") }\n' > "$EX"
mkrc 0

echo "== «судить нечего» и дверь F1 =="
check "бинаря нет, novac/src/main.nv нет — зелёный" "$(run)" "0"
has "$TMP/out" 'ok: судить нечего' "«судить нечего» напечатано (№645)"

printf 'fn main() {}\n' > "$FIX/novac/src/main.nv"
check "novac/src/main.nv есть, бинаря нет — красный (274.3/F1)" "$(run)" "1"
rm -f "$FIX/novac/src/main.nv"

mkbin
mv "$SHELL_TPL" "$TMP/shell.saved"
check "оболочки нет — красный" "$(run)" "1"
mv "$TMP/shell.saved" "$SHELL_TPL"

echo "== эмиссия внутри оракульского словаря — проходит =="
mkemit <<'EOF'
static void f(void) {
    Nova_str_method_len(x);
    Nova_str_method_byte_len(x);
    nova_print_str("hi");
}
EOF
check "все имена есть в оболочке и заголовке — зелёный" "$(run)" "0"
# Пришпилено к КОНТРАКТУ, а не к прозе (реестр №817, 2026-08-30). Здесь стояло
# `ok: все method-имена` — фраза, которую страж печатал когда-то; потом его
# область расширилась с method-имён на все символы оракула, строка была
# переписана, а этот самотест — нет, и он краснел на обеих ветках, пока полный
# ярус до него не доходил. Прозу двух домов иметь нельзя: держим то, чего
# реально требует гейт (`guard()` в scripts/gate.sh) — имя стража плюс `ok:`.
has "$TMP/out" 'check-novac-mangle-fixed-point ok:' "зелёная строка с итогом (контракт гейта, не проза)"

printf 'export type Point {\n    x int // coordinate\n}\nfn main() {}\n' > "$EX"
mkemit <<'EOF'
static void f(void) {
    Nova_Point p;
    Nova_Point_Tag t;
    Nova_str_method_len(x);
}
EOF
check "тип ПОЛЬЗОВАТЕЛЯ (Nova_Point) объявлен в .nv — зелёный" "$(run)" "0"

mkrc 2
check "эмиссия отказала (вне подмножества) — зелёный, не судится" "$(run)" "0"
mkrc 0

echo "== ловит =="
printf 'fn main() {}\n' > "$EX"
mkemit <<'EOF'
static void f(void) {
    Nova_str_method_len(x);
    Nova_str_method_missing(x);
}
EOF
check "C-имя, которого нет в оболочке — красный" "$(run)" "1"
has "$TMP/err" 'Nova_str_method_missing' "разошедшееся имя названо поимённо"
has "$TMP/err" 'нет в оболочке' "причина названа"

mkemit <<'EOF'
static void f(void) {
    Nova_Point p;
}
EOF
check "Nova_Point без объявления type Point в .nv — красный" "$(run)" "1"

mkemit <<'EOF'
static void f(void) {
    nova_print_str("hi");
}
EOF
rm -f "$FIX/compiler-codegen/nova_rt/nova_rt.h"
check "имя из заголовка рантайма, заголовка нет — красный" "$(run)" "1"
has "$TMP/err" 'nova_print_str' "имя из заголовка названо поимённо"

echo "итог: $PASS ok, $FAIL FAIL"
if [ "$FAIL" -eq 0 ]; then
    echo "test-check-novac-mangle-fixed-point ok: $PASS/$PASS"
    exit 0
fi
exit 1
