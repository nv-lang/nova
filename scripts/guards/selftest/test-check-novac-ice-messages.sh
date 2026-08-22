#!/bin/sh
# Самотест check-novac-ice-messages.py (П16: обязан доказать, что ловит).
# Подложка через шов $2 — крошечные .nv во временной папке.
export LC_ALL=C
GD="$(cd "$(dirname "$0")/.." && pwd)"
ROOT="$(cd "$GD/../.." && pwd)"
G="$GD/check-novac-ice-messages.py"
T="${TMPDIR:-/tmp}/novac-ice-msgs-selftest.$$"
mkdir -p "$T"
trap 'rm -rf "$T"' 0
fails=0
ok()  { echo "  ok: $1"; }
bad() { echo "  FAIL: $1" >&2; fails=$((fails+1)); }
run() { python "$G" "$ROOT" "$1" > "$T/out" 2> "$T/err"; }

# --- 1. уникальные и с префиксом — зелёный --------------------------------
mkdir -p "$T/g1/sem" "$T/g1/diag"
printf 'module a\nfn f() -> int { ice("sem: leaf_text on a branch node") }\n' > "$T/g1/sem/a.nv"
printf 'module b\nfn g() -> int { ice("diag: to_json could not encode the DTO") }\n' > "$T/g1/diag/b.nv"
if run "$T/g1"; then
    grep -q "вызовов ice(): 2" "$T/out" && ok "уникальные с префиксом — зелёный, счёт верный" || bad "зелёный, но счёт не тот [$(cat "$T/out")]"
else
    bad "чистое дерево покраснело: $(cat "$T/err")"
fi

# --- 2. ГЛАВНЫЙ случай: один текст в двух местах — красный ----------------
mkdir -p "$T/g2/sem"
printf 'module a\nfn f() -> int { ice("sem: expected a leaf") }\nfn h() -> int { ice("sem: expected a leaf") }\n' > "$T/g2/sem/a.nv"
if run "$T/g2"; then
    bad "два одинаковых текста прошли — при схлопнутом site их не различить"
else
    if grep -q "expected a leaf" "$T/err" && grep -c "sem/a.nv" "$T/err" >/dev/null; then
        ok "дубль пойман, оба места показаны"
    else
        bad "красный, но без текста/мест [$(cat "$T/err")]"
    fi
fi

# --- 3. дубль в РАЗНЫХ файлах — тоже красный ------------------------------
mkdir -p "$T/g3/sem" "$T/g3/emit_c"
printf 'module a\nfn f() -> int { ice("sem: same words") }\n' > "$T/g3/sem/a.nv"
printf 'module b\nfn g() -> int { ice("sem: same words") }\n' > "$T/g3/emit_c/b.nv"
run "$T/g3" && bad "дубль между файлами прошёл" || ok "дубль между файлами пойман"

# --- 4. без префикса модуля — красный ------------------------------------
mkdir -p "$T/g4/diag"
printf 'module b\nfn g() -> int { ice("to_json failed somehow") }\n' > "$T/g4/diag/b.nv"
if run "$T/g4"; then
    bad "сообщение без префикса модуля прошло"
else
    grep -q "без префикса модуля" "$T/err" && ok "отсутствие префикса поймано" || bad "красный, но не про префикс"
fi

# --- 5. форма «модуль_с_подчёркиванием: …» законна ------------------------
mkdir -p "$T/g5/emit_c"
printf 'module c\nfn g() -> int { ice("emit_c: statement kind outside the subset") }\n' > "$T/g5/emit_c/c.nv"
run "$T/g5" && ok "префикс с подчёркиванием законен" || bad "emit_c: покраснел зря: $(cat "$T/err")"

# --- 6. тесты исключены ---------------------------------------------------
mkdir -p "$T/g6/sem"
printf 'module a\nfn f() -> int { ice("dup in test") }\nfn h() -> int { ice("dup in test") }\n' > "$T/g6/sem/a_test.nv"
run "$T/g6" && ok "файл *_test.nv исключён" || bad "тест покраснел: $(cat "$T/err")"

# --- 7. нет вызовов ice — судить нечего -----------------------------------
mkdir -p "$T/g7/sem"
printf 'module a\nfn f() -> int => 1\n' > "$T/g7/sem/a.nv"
run "$T/g7"
grep -q "судить нечего" "$T/out" && ok "нет вызовов ice — судить нечего" || bad "ждали «судить нечего» [$(cat "$T/out")]"

# --- 8. настоящее дерево --------------------------------------------------
python "$G" "$ROOT" >/dev/null 2>&1 && ok "настоящий novac/src — зелёный" || bad "настоящее дерево покраснело: $(python "$G" "$ROOT" 2>&1 | head -3)"

# --- условный ice обязан быть assert (2026-08-16) -------------------------
mkdir -p "$T/cond/sem"
printf 'module a
fn f(t int) -> int {
    if t < 0 { ice("sem: t is negative") }
    t
}
' > "$T/cond/sem/a.nv"
if run "$T/cond"; then
    bad "условный ice прошёл — вторая половина стража не ловит"
else
    grep -q "условный ice" "$T/err" && ok "условный ice пойман (у него есть условие — значит есть assert)" || bad "красный, но не про условный ice"
fi

mkdir -p "$T/asrt/sem"
printf 'module a
fn f(t int) -> int {
    assert(t >= 0, "sem: t is negative")
    t
}
' > "$T/asrt/sem/a.nv"
run "$T/asrt" && ok "assert вместо условного ice — зелёный" || bad "assert покраснел: $(cat "$T/err")"

mkdir -p "$T/val/sem"
printf 'module a
fn f(t int) -> str {
    match t {
        0 => "zero"
        _ => ice("sem: only zero is in the subset")
    }
}
' > "$T/val/sem/a.nv"
run "$T/val" && ok "ice в позиции значения остаётся законным" || bad "значение-ice покраснел: $(cat "$T/err")"

echo "итог: FAIL $fails"
if [ "$fails" -eq 0 ]; then
    echo "test-check-novac-ice-messages ok: все случаи, включая дубль в одном файле и между файлами"
    exit 0
fi
exit 1
