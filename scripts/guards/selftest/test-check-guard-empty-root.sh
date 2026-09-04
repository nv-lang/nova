#!/bin/sh
# scripts/guards/selftest/test-check-guard-empty-root.sh — самотест МЕТА-СТРАЖА
# check-guard-empty-root.py (реестр 221.1 №911: страж с уехавшей мишенью печатает
# зелёный ноль с правдоподобным числом).
#
# ШЕСТЬ СЛУЧАЕВ, каждый отвечает на свой вопрос:
#   1. честный + отказывающий при базе lying=0 — ЗЕЛЁНЫЙ (ложняка нет);
#   2. лгущий сверх базы — КРАСНЫЙ, и в отказе назван АДРЕС (имя стража);
#   3. лгущий ровно на базе (lying=1) — ЗЕЛЁНЫЙ (храповик держит, а не запрещает);
#   4. НОЛЬ подсудных стражей — КРАСНЫЙ как потеря мишени, а не «лгущих 0»:
#      мета-страж не имеет права заболеть тем, что судит;
#   5. семья ужалась больше чем вдвое против judged — КРАСНЫЙ: «лгущих меньше,
#      потому что судить некого» выглядит улучшением и им не является;
#   6. база без ключей — КРАСНЫЙ: судить нечем != зелено.
#
# Фикстурные «стражи» — свои, в своём временном каталоге; настоящее дерево
# самотест не читает (кроме самого файла стража, который он и проверяет).
set -u
export LC_ALL=C

ROOT="$(CDPATH= cd -- "$(dirname -- "$0")/../../.." && pwd)"
G="$ROOT/scripts/guards/check-guard-empty-root.py"
T="${TMPDIR:-/tmp}/guard-empty-root-selftest.$$"
FAILED=0
ok()  { echo "  ok: $1"; }
bad() { echo "  FAIL: $1"; FAILED=$((FAILED+1)); }
mkdir -p "$T"
trap 'rm -rf "$T"' EXIT

if [ ! -f "$G" ]; then
    echo "test-check-guard-empty-root: FAIL - нет самого стража $G" >&2
    exit 1
fi

# --- фикстурные стражи ----------------------------------------------------------
# ЛГУЩИЙ: на пустом корне выходит нулём и печатает `ok` с числом. Ровно та форма,
# которой охота открыла №911.
mk_liar() {
    printf '#!/bin/sh\necho "check-novac-liar ok: файлов 0, нарушений 0"\nexit 0\n' \
        > "$1/check-novac-liar.sh"
}
# ЧЕСТНЫЙ: тоже выходит нулём, но говорит о пустоте словами.
mk_honest() {
    printf '#!/bin/sh\necho "check-novac-honest ok: судить нечего (нет каталога)"\nexit 0\n' \
        > "$1/check-novac-honest.sh"
}
# ОТКАЗЫВАЮЩИЙ: ненулевой код — самый честный ответ на пустоту.
mk_refuser() {
    printf '#!/bin/sh\necho "check-novac-refuser: FAIL — мишень потеряна" >&2\nexit 1\n' \
        > "$1/check-novac-refuser.sh"
}

run() {  # $1 = каталог стражей, $2 = файл базы
    GUARD_EMPTY_ROOT_BASELINE="$2" GUARD_EMPTY_ROOT_TIMEOUT=20 \
        python "$G" "$T" "$1" >"$T/out" 2>"$T/err"
}

printf 'lying=0\njudged=2\n' > "$T/base0"
printf 'lying=1\njudged=3\n' > "$T/base1"
printf 'lying=0\njudged=100\n' > "$T/base-wide"
printf '# ни одного ключа\n' > "$T/base-broken"

# --- 1. чистый вход: честный + отказывающий -------------------------------------
mkdir -p "$T/clean"
mk_honest "$T/clean"
mk_refuser "$T/clean"
if run "$T/clean" "$T/base0"; then
    ok "честный и отказывающий при базе 0 — зелёный"
else
    bad "чистый вход, а страж красный: $(cat "$T/err")"
fi

# --- 2. лгущий сверх базы: КРАСНЫЙ, адрес назван --------------------------------
mkdir -p "$T/grow"
mk_honest "$T/grow"
mk_refuser "$T/grow"
mk_liar "$T/grow"
if run "$T/grow" "$T/base0"; then
    bad "лгущий страж прошёл зелёным: $(cat "$T/out")"
else
    if grep -q "check-novac-liar" "$T/err"; then
        ok "лгущий красный, и назван по имени"
    else
        bad "красный, но без адреса: $(cat "$T/err")"
    fi
fi

# --- 3. лгущий ровно на базе -----------------------------------------------------
if run "$T/grow" "$T/base1"; then
    ok "лгущий ровно на базе (lying=1) — зелёный"
else
    bad "на базе, а красный: $(cat "$T/err")"
fi

# --- 4. мишень потеряна: ноль подсудных ------------------------------------------
mkdir -p "$T/none"
if run "$T/none" "$T/base0"; then
    bad "ноль стражей под судом — а мета-страж зелёный"
else
    if grep -q "мишень" "$T/err"; then
        ok "ноль подсудных — красный, назван потерей мишени"
    else
        bad "красный, но не про мишень: $(cat "$T/err")"
    fi
fi

# --- 5. семья ужалась больше чем вдвое -------------------------------------------
if run "$T/grow" "$T/base-wide"; then
    bad "семья с 100 до 3 — а страж зелёный"
else
    if grep -q "мишень" "$T/err"; then
        ok "усохшая семья — красный, назван потерей мишени"
    else
        bad "красный, но не про мишень: $(cat "$T/err")"
    fi
fi

# --- 6. база без ключей -----------------------------------------------------------
if run "$T/clean" "$T/base-broken"; then
    bad "база без ключей — а страж зелёный"
else
    if grep -q "lying=N" "$T/err"; then
        ok "база без ключей — красный, ключ назван"
    else
        bad "красный, но не про ключ базы: $(cat "$T/err")"
    fi
fi

echo "итог: FAIL $FAILED"
if [ "$FAILED" -eq 0 ]; then
    echo "test-check-guard-empty-root ok: зелёный ноль на пустом корне краснеет с именем стража, потеря мишени и усохшая семья краснеют, честная оговорка и отказ законны"
    exit 0
fi
exit 1
