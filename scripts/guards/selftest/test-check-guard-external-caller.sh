#!/bin/sh
# Самотест check-guard-external-caller (П16: страж без доказательства красноты
# запрещён). Красноту доказываем МУТАЦИЕЙ ПОДЛОЖНОГО ДЕРЕВА: у каждого случая
# отнимается ровно одна часть, и ждётся красный с названной причиной.
#
# Главный случай — пятый: упоминание стража в КОММЕНТАРИИ рабочего потока CI
# вызовом не является. На нём попалась первая проба этого правила: регексп нашёл
# строку «they used to live in scripts/gate.sh» и ответил «CI зовёт». Если этот
# случай зазеленеет, страж снова начнёт считать разговоры о вызове вызовом.
export LC_ALL=C
GD="$(cd "$(dirname "$0")/.." && pwd)"
ROOT="$(cd "$GD/../.." && pwd)"
G="$GD/check-guard-external-caller.py"
T="${TMPDIR:-/tmp}/guard-external-caller-selftest.$$"
mkdir -p "$T"
trap 'rm -rf "$T"' 0
fails=0
ok()  { echo "  ok: $1"; }
bad() { echo "  FAIL: $1" >&2; fails=$((fails+1)); }

# Подложное дерево: два стража, поток CI и гейт — по вкусу случая.
mk_tree() {
    rm -rf "$T/tree"
    mkdir -p "$T/tree/scripts/guards" "$T/tree/.github/workflows"
    printf '#!/bin/sh\necho "check-a ok: nothing"\n' > "$T/tree/scripts/guards/check-a.sh"
    printf '#!/bin/sh\necho "check-b ok: nothing"\n' > "$T/tree/scripts/guards/check-b.sh"
}
mk_base() { printf 'without-external-caller %s\n' "$1" > "$T/base"; }

# ── 1. живое дерево — зелёный со строкой ok: ─────────────────────────────
if python "$G" "$ROOT" > "$T/o1" 2> "$T/e1"; then
    grep -q "^check-guard-external-caller ok:" "$T/o1" \
        && ok "живое дерево — зелёный со строкой ok:" \
        || bad "зелёный без строки ok:"
else
    bad "живое дерево покраснело: [$(head -n 2 "$T/e1")]"
fi

# ── 2. оба стража без вызывающего, база 2 — зелёный ──────────────────────
mk_tree; mk_base 2
python "$G" "$T/tree" "$T/base" > "$T/o2" 2>&1 \
    && ok "число сошлось с базой — зелёный" \
    || bad "совпадение с базой покраснело: [$(head -n 2 "$T/o2")]"

# ── 3. РОСТ выше базы — красный ──────────────────────────────────────────
mk_tree; mk_base 1
if python "$G" "$T/tree" "$T/base" > "$T/o3" 2> "$T/e3"; then
    bad "рост числа прошёл зелёным"
else
    grep -q "РОСТ" "$T/e3" && ok "рост — красный, причина названа" || bad "красный, но не про рост"
fi

# ── 4. ПРОГРЕСС без опускания базы — тоже красный ────────────────────────
mk_tree; mk_base 5
if python "$G" "$T/tree" "$T/base" > "$T/o4" 2> "$T/e4"; then
    bad "прогресс без опускания базы прошёл зелёным"
else
    grep -q "ПРОГРЕСС без опускания" "$T/e4" \
        && ok "прогресс без опускания базы — красный" \
        || bad "красный, но не про базу"
fi

# ── 5. ГЛАВНЫЙ: упоминание в комментарии CI — НЕ вызов ───────────────────
mk_tree; mk_base 2
printf 'jobs:\n  x:\n    steps:\n      # check-a.sh used to run here\n      - run: echo hi\n' \
    > "$T/tree/.github/workflows/w.yml"
if python "$G" "$T/tree" "$T/base" > "$T/o5" 2>&1; then
    ok "упоминание в комментарии вызовом не считается"
else
    bad "комментарий засчитан за вызов — страж считает разговоры о вызове вызовом"
fi

# ── 6. настоящий вызов из CI — страж перестаёт быть сиротой ──────────────
mk_tree; mk_base 1
printf 'jobs:\n  x:\n    steps:\n      - run: bash scripts/guards/check-a.sh\n' \
    > "$T/tree/.github/workflows/w.yml"
python "$G" "$T/tree" "$T/base" > "$T/o6" 2>&1 \
    && ok "прямой вызов из CI засчитан" \
    || bad "прямой вызов из CI не засчитан: [$(head -n 2 "$T/o6")]"

# ── 7. гейт засчитывается, только если его ЗАПУСКАЕТ CI ──────────────────
mk_tree; mk_base 2
printf '#!/bin/sh\nbash scripts/guards/check-a.sh\nbash scripts/guards/check-b.sh\n' > "$T/tree/scripts/gate-x.sh"
printf 'jobs:\n  x:\n    steps:\n      - run: echo hi\n' > "$T/tree/.github/workflows/w.yml"
python "$G" "$T/tree" "$T/base" > "$T/o7" 2>&1 \
    && ok "гейт, который CI не запускает, вызывающим не считается" \
    || bad "незапускаемый гейт засчитан: [$(head -n 2 "$T/o7")]"

mk_base 0
printf 'jobs:\n  x:\n    steps:\n      - run: bash scripts/gate-x.sh\n' > "$T/tree/.github/workflows/w.yml"
python "$G" "$T/tree" "$T/base" > "$T/o8" 2>&1 \
    && ok "гейт, который CI запускает, засчитан за обоих" \
    || bad "запускаемый гейт не засчитан: [$(head -n 2 "$T/o8")]"

# ── 8. нет базы — красный, а не «нечего судить» ──────────────────────────
mk_tree
if python "$G" "$T/tree" "$T/nosuch" > "$T/o9" 2> "$T/e9"; then
    bad "отсутствие базы прошло зелёным"
else
    grep -q "нет базы" "$T/e9" && ok "нет базы — красный" || bad "красный, но не про базу"
fi

echo "итог: FAIL $fails"
if [ "$fails" -eq 0 ]; then
    echo "test-check-guard-external-caller ok: все случаи, включая комментарий вместо вызова"
    exit 0
fi
exit 1
