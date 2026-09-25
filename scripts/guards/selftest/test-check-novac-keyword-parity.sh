#!/bin/sh
# Самотест check-novac-keyword-parity.py (план 274.7, волна В10).
#
# Шов — КОРЕНЬ: страж принимает его первым аргументом и читает ровно два файла,
# `spec/decisions/03-syntax.md` и `novac/src/lex/lex.nv`. Поэтому тест строит
# крошечный поддельный корень и не трогает дерево.
#
# ЧТО ОБЯЗАН ЛОВИТЬ СТРАЖ, и каждый случай ниже — отдельная ось:
#   1. объявленная разница — зелёная (иначе страж требует невозможного: `true`
#      ключевым словом лексера быть не должен);
#   2. НЕобъявленная сверх базы — красная (главный случай: слово языка, о котором
#      никто ничего не сказал);
#   3. `debt` без этапа — красная (обещание без срока есть норма, а не долг);
#   4. вид объявления не назван — красная (иначе «KEYWORD-PARITY: loop -- потом»
#      проходит и выглядит объявлением);
#   5. обратная разница (слово в лексере, которого нет в списке спеки) считается
#      тоже — иначе страж слеп к половине предмета;
#   6. объявление слова, которое разницей уже НЕ является, — красное (протухший
#      долг: лексер выучил слово, строка «debt» осталась; 2026-09-25).
export LC_ALL=C
GD="$(cd "$(dirname "$0")/.." && pwd)"
G="$GD/check-novac-keyword-parity.py"
T="${TMPDIR:-/tmp}/novac-kwparity-selftest.$$"
trap 'rm -rf "$T" "$T.decl"' 0
fails=0
ok()  { echo "  ok: $1"; }
bad() { echo "  FAIL: $1" >&2; fails=$((fails+1)); }

# Поддельный корень: список спеки из четырёх слов, лексер знает два.
# Разницы: `loop` и `true` со стороны спеки, `extern` со стороны лексера — три.
mkroot() {
    rm -rf "$T"
    mkdir -p "$T/spec/decisions" "$T/novac/src/lex" "$T/scripts/guards"
    cat > "$T/spec/decisions/03-syntax.md" <<'SPEC'
# fake

#### Full reserved word list (the heading below is what the guard looks for)

#### Полный список зарезервированных слов

**Control flow:** `if`, `loop`.

**Литералы:** `true`.

#### Что запрещено

nothing.
SPEC
    {
        echo "module novac.lex"
        echo ""
        cat "$T.decl"
        echo "/// Keyword kind for an identifier text, or Ident when it is not a keyword."
        echo "fn keyword_kind(t str) -> TokenKind => match t {"
        echo '    "if" => TokenKind.KwIf'
        echo '    "extern" => TokenKind.KwExtern'
        echo "    _ => TokenKind.Ident"
        echo "}"
    } > "$T/novac/src/lex/lex.nv"
    echo "undeclared=$1" > "$T/scripts/guards/keyword-parity.baseline"
}
run() { python "$G" "$T" > "$T/out" 2> "$T/err"; }

# --- 1. все три разницы объявлены — зелёный -------------------------------
cat > "$T.decl" <<'DECL'
// KEYWORD-PARITY: loop -- debt (274.7 B10)
// KEYWORD-PARITY: true -- by design (a boolean literal)
// KEYWORD-PARITY: extern -- spec-gap (absent from the list)
DECL
mkroot 0
# Зелёный САМ ПО СЕБЕ ничего не доказывает: страж со сломанным разбором списка спеки
# увидел бы НОЛЬ разниц и тоже напечатал «ok». Поэтому ассертятся ДВА числа — что разниц
# он увидел три и что необъявленных из них ноль.
if run; then
    if grep -q "необъявленных 0" "$T/out" && grep -q "разниц 3" "$T/out"; then
        ok "все разницы объявлены — зелёный, и посчитан он верно (3 разницы, 0 необъявленных)"
    else
        bad "зелёный, но числа не те: $(cat "$T/out")"
    fi
else
    bad "объявленные разницы покраснели: $(cat "$T/err")"
fi

# --- 2. ГЛАВНЫЙ случай: необъявленная разница сверх базы — красный --------
cat > "$T.decl" <<'DECL'
// KEYWORD-PARITY: true -- by design (a boolean literal)
// KEYWORD-PARITY: extern -- spec-gap (absent from the list)
DECL
mkroot 0
if run; then
    bad "необъявленный 'loop' прошёл — страж не ловит свой главный случай"
else
    grep -q "loop" "$T/err" && ok "необъявленная разница поймана и НАЗВАНА" \
        || bad "красный, но слово не названо: $(cat "$T/err")"
fi

# --- 3. debt без этапа — красный ------------------------------------------
cat > "$T.decl" <<'DECL'
// KEYWORD-PARITY: loop -- debt, we will get to it
// KEYWORD-PARITY: true -- by design (a boolean literal)
// KEYWORD-PARITY: extern -- spec-gap (absent from the list)
DECL
mkroot 0
if run; then
    bad "'debt' без этапа прошёл — обещание без срока становится нормой"
else
    ok "'debt' без этапа пойман"
fi

# --- 4. вид не назван — красный -------------------------------------------
cat > "$T.decl" <<'DECL'
// KEYWORD-PARITY: loop -- later maybe
// KEYWORD-PARITY: true -- by design (a boolean literal)
// KEYWORD-PARITY: extern -- spec-gap (absent from the list)
DECL
mkroot 0
if run; then
    bad "объявление без вида прошло — 'потом' выглядит объявлением, не будучи им"
else
    ok "объявление без вида поймано"
fi

# --- 5. обратная разница считается тоже -----------------------------------
# `extern` есть в лексере и НЕТ в списке спеки. Убираем ЕГО объявление, оставив
# остальные: если страж смотрит только в одну сторону, он этого не заметит.
cat > "$T.decl" <<'DECL'
// KEYWORD-PARITY: loop -- debt (274.7 B10)
// KEYWORD-PARITY: true -- by design (a boolean literal)
DECL
mkroot 0
if run; then
    bad "обратная разница не замечена — страж слеп к половине предмета"
else
    grep -q "extern" "$T/err" && ok "обратная разница (лексер \\ спека) поймана" \
        || bad "красный, но не про обратную разницу: $(cat "$T/err")"
fi

# --- 6. объявление без разницы — красный ----------------------------------
# `if` лексер знает и спека называет: разницы нет, а объявление её утверждает.
cat > "$T.decl" <<'DECL'
// KEYWORD-PARITY: loop -- debt (274.7 B10)
// KEYWORD-PARITY: true -- by design (a boolean literal)
// KEYWORD-PARITY: extern -- spec-gap (absent from the list)
// KEYWORD-PARITY: if -- debt (274.7 B10)
DECL
mkroot 0
if run; then
    bad "протухшее объявление 'if' прошло — долг назван долгом после закрытия"
else
    grep -q " if" "$T/err" && ok "объявление без разницы поймано и НАЗВАНО" \
        || bad "красный, но слово не названо: $(cat "$T/err")"
fi

if [ "$fails" -eq 0 ]; then
    echo "test-check-novac-keyword-parity: ok (6)"
    exit 0
fi
echo "test-check-novac-keyword-parity: FAIL ($fails)" >&2
exit 1
