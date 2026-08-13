#!/usr/bin/env bash
# Самотест check-effect-boundary-shape.sh.

set -u
export LC_ALL=C

DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
G="$DIR/check-effect-boundary-shape.sh"
TMP="${TMPDIR:-/tmp}/selftest_effshape_$$"
FAILED=0
ok()  { echo "  ok: $1"; }
bad() { echo "  ПРОВАЛ: $1" >&2; FAILED=1; }

# $1 — содержимое .nv, $2 — база
setup() {
    rm -rf "$TMP"; mkdir -p "$TMP/std/src" "$TMP/examples" "$TMP/scripts/guards"
    printf '%s\n' "$1" > "$TMP/std/src/probe.nv"
    printf 'violations=%s\n' "$2" > "$TMP/scripts/guards/effect-boundary-shape.baseline"
    cp "$DIR/effect-boundary-shape.awk" "$TMP/scripts/guards/"
}
run() { bash "$G" "$TMP" 2>&1; }
trap 'rm -rf "$TMP"' EXIT

CLEAN='type Good effect {
    read_all(path Path) -> Result[[]u8, IoError]
    write_all(path Path, data []u8) -> Result[(), IoError]
}'

# 1. Чистая граница — норма.
setup "$CLEAN" 0
out=$(run); rc=$?
if [ "$rc" -eq 0 ]; then ok "чистая граница проходит"; else bad "ложный отказ: $out"; fi

# 2. Сырая ручка как int (D456 §5) — ловится.
setup 'type Bad effect {
    close(fd int) -> int
}' 0
out=$(run); rc=$?
if [ "$rc" -eq 1 ] && echo "$out" | grep -q "R6"; then ok "ловит сырую ручку как int"; else bad "не поймал R6 (rc=$rc): $out"; fi

# 3. out-параметр (D456 §4) — ловится. Дословный пример решения.
setup 'type Bad effect {
    stream_peer_addr(stream TcpStream, mut out []u8) -> ()
}' 0
out=$(run); rc=$?
if [ "$rc" -eq 1 ] && echo "$out" | grep -q "R5"; then ok "ловит out-параметр"; else bad "не поймал R5 (rc=$rc): $out"; fi

# 4. Обход по индексу (D456 §3) — ловится ПАРОЙ, а не одной операцией.
setup 'type Bad effect {
    env_len() -> int
    env_key_at(i int) -> str
}' 0
out=$(run); rc=$?
if [ "$rc" -eq 1 ] && echo "$out" | grep -q "R2"; then ok "ловит обход по индексу"; else bad "не поймал R2 (rc=$rc): $out"; fi

# 5. Одна только `*_len` без `*_at` — НЕ нарушение: длина сама по себе законна.
setup 'type Good effect {
    env_len() -> int
}' 0
out=$(run); rc=$?
if [ "$rc" -eq 0 ]; then ok "одинокая *_len не считается нарушением"; else bad "ложняк на *_len: $out"; fi

# 6. Счётчик рядом с данными (D456 §6) — ловится.
setup 'type Bad effect {
    run(argv []u8, argc int) -> int
}' 0
out=$(run); rc=$?
if [ "$rc" -eq 1 ] && echo "$out" | grep -q "R3"; then ok "ловит счётчик рядом с данными"; else bad "не поймал R3 (rc=$rc): $out"; fi

# 7. Позиционная простыня (D456 §8) — ловится.
setup 'type Bad effect {
    proc(a int, b int, c int, d int, e int, f int, g int) -> int
}' 0
out=$(run); rc=$?
if [ "$rc" -eq 1 ] && echo "$out" | grep -q "R4"; then ok "ловит позиционную простыню"; else bad "не поймал R4 (rc=$rc): $out"; fi

# 8. Ровно шесть параметров — НЕ нарушение: граница правила проверена с обеих
#    сторон, иначе страж краснел бы на законном.
setup 'type Good effect {
    proc(a int, b int, c int, d int, e int, f int) -> int
}' 0
out=$(run); rc=$?
if [ "$rc" -eq 0 ]; then ok "шесть параметров законны"; else bad "ложняк на шести: $out"; fi

# 9. Сырой указатель (D456 §5) — ловится.
setup 'type Bad effect {
    poke(p *mut u8) -> ()
}' 0
out=$(run); rc=$?
if [ "$rc" -eq 1 ] && echo "$out" | grep -q "R1"; then ok "ловит сырой указатель"; else bad "не поймал R1 (rc=$rc): $out"; fi

# 10. ПОД границей C-формы законны: та же сырая ручка ВНЕ `type … effect`
#     нарушением не является. Это и есть та черта, ради которой страж писался.
setup 'fn real_fs_close(fd int) -> int {
    unsafe { c_close(fd) }
}' 0
out=$(run); rc=$?
if [ "$rc" -eq 0 ]; then ok "C-формы ПОД границей не трогаются"; else bad "страж полез под границу: $out"; fi

# 11. Храповик: долг в пределах базы — зелено; сверх базы — красно.
setup 'type Bad effect {
    close(fd int) -> int
}' 1
out=$(run); rc=$?
if [ "$rc" -eq 0 ]; then ok "долг в пределах базы проходит"; else bad "храповик не пропускает базу: $out"; fi

# 12. Снижение долга — подсказка, а не отказ.
setup "$CLEAN" 5
out=$(run); rc=$?
if [ "$rc" -eq 0 ] && echo "$out" | grep -q "СНИЗИЛСЯ"; then ok "снижение — подсказка"; else bad "снижение обработано неверно (rc=$rc): $out"; fi

# 13. На настоящем дереве зелёный.
REAL="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
out=$(bash "$G" "$REAL" 2>&1); rc=$?
if [ "$rc" -eq 0 ]; then ok "на настоящем дереве зелёный"; else bad "красный на настоящем дереве: $out"; fi

# 14. Страж назван на странице правил.
if grep -q "check-effect-boundary-shape.sh" "$REAL/docs/dev/rules-for-agents.md" 2>/dev/null; then
    ok "страж назван на странице правил"
else
    bad "страж не назван в docs/dev/rules-for-agents.md"
fi

if [ "$FAILED" -eq 0 ]; then echo "селфтест check-effect-boundary-shape: все проверки ok"; exit 0; fi
echo "селфтест check-effect-boundary-shape: ЕСТЬ ПРОВАЛЫ" >&2
exit 1
