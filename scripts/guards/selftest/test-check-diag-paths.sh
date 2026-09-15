#!/bin/sh
# scripts/guards/selftest/test-check-diag-paths.sh — самотест стража
# check-diag-paths.py (реестр 221.1 №1109: текст компилятора ссылает на файлы,
# которых нет, и это не сверяется ничем).
#
# СЛУЧАИ — каждый отвечает на свой вопрос; счёт печатает сам самотест, а не эта
# строка: число, написанное рукой, расходится с числом случаев молча.
#   1. живой путь в прозе — ЗЕЛЁНЫЙ (ложняка нет);
#   2. мёртвый путь в прозе — КРАСНЫЙ, и в отказе назван АДРЕС (файл:строка);
#   3. мёртвый путь, РАЗРЕЗАННЫЙ продолжением строки (`\` в конце строки), —
#      КРАСНЫЙ. Это тот самый случай, на котором ослепла построчная регулярка
#      первого замера: пятый носитель (`W_FFI_BARE_HANDLE`) нашёлся руками, а не
#      измерением. Случай, из-за которого страж вообще разбирает литералы;
#   4. мёртвый путь в комментарии Rust — ЗЕЛЁНЫЙ: пользователю его не показывают;
#   5. путь-ЗНАЧЕНИЕ (литерал равен ровно пути, прозы нет) — ЗЕЛЁНЫЙ: это вход
#      функции или путь записи, а не утверждение «файл существует». Без этого
#      различия страж краснеет на законном коде — замер дал 13 таких из 30;
#   6. мёртвый путь в raw-литерале (r#"..."#) — КРАСНЫЙ: вторая форма литерала
#      не должна быть слепой зоной;
#   7. НОЛЬ путей под судом — КРАСНЫЙ как потеря мишени, а не «мёртвых 0»:
#      сломанный разбор литералов обязан выглядеть отказом, а не успехом;
#   8. база без ключей — КРАСНЫЙ: судить нечем != зелено.
#
# Фикстурное дерево — своё, во временном каталоге; настоящее дерево самотест не
# читает (кроме самого файла стража, который он и проверяет).
set -u
export LC_ALL=C

ROOT="$(CDPATH= cd -- "$(dirname -- "$0")/../../.." && pwd)"
G="$ROOT/scripts/guards/check-diag-paths.py"
T="${TMPDIR:-/tmp}/diag-paths-selftest.$$"
FAILED=0
# СЧЁТЧИК, А НЕ ЧИСЛО В СТРОКЕ (страж check-selftest-honest-count, правило: число
# случаев, написанное рукой, расходится с числом случаев МОЛЧА). Здесь же сегодня
# такое и случилось в шапке соседнего стража: «шесть случаев» при семи.
CASES=0
ok()  { CASES=$((CASES+1)); echo "  ok: $1"; }
bad() { CASES=$((CASES+1)); echo "  FAIL: $1"; FAILED=$((FAILED+1)); }
trap 'rm -rf "$T"' EXIT

if [ ! -f "$G" ]; then
    echo "test-check-diag-paths: FAIL - нет самого стража $G" >&2
    exit 1
fi

# --- фикстурное дерево ----------------------------------------------------------
# mk_root <каталог> — пустое дерево с корнями, живым файлом и базой dead=0.
mk_root() {
    r="$1"
    rm -rf "$r"
    mkdir -p "$r/compiler-codegen/src" "$r/docs/guide" "$r/scripts/guards"
    printf 'live\n' > "$r/docs/guide/live.md"
    printf 'dead=0\njudged=1\n' > "$r/scripts/guards/diag-paths.baseline"
}

run() { python "$G" "$1" 2>&1; }

# --- 1. живой путь в прозе -> зелёный -------------------------------------------
mk_root "$T/c1"
cat > "$T/c1/compiler-codegen/src/a.rs" <<'EOF'
fn f() { emit(format!("consume protocol: see docs/guide/live.md for the template.")); }
EOF
OUT="$(run "$T/c1")"; RC=$?
if [ "$RC" -eq 0 ]; then ok "1 живой путь в прозе — зелёный"
else bad "1 живой путь дал отказ: $OUT"; fi

# --- 2. мёртвый путь в прозе -> красный, с адресом -------------------------------
mk_root "$T/c2"
cat > "$T/c2/compiler-codegen/src/a.rs" <<'EOF'
fn f() { emit(format!("consume protocol: see docs/guide/gone.md for the template.")); }
EOF
OUT="$(run "$T/c2")"; RC=$?
if [ "$RC" -ne 0 ]; then
    if echo "$OUT" | grep -q "docs/guide/gone.md" && echo "$OUT" | grep -q "a.rs:1"; then
        ok "2 мёртвый путь в прозе — красный, адрес назван"
    else bad "2 красный, но без адреса пути: $OUT"; fi
else bad "2 мёртвый путь прошёл зелёным: $OUT"; fi

# --- 3. мёртвый путь, РАЗРЕЗАННЫЙ продолжением строки -> красный -----------------
# Ровно форма, на которой ослепла построчная регулярка: ни одна строка исходника
# не содержит пути целиком.
mk_root "$T/c3"
cat > "$T/c3/compiler-codegen/src/a.rs" <<'EOF'
fn f() {
    emit(format!(
        "an FFI handle never travels bare - declare a newtype \
         (module-conventions section 4a; reference: docs/guide/\
         gone-split.md). A legitimate exception needs a marker."
    ));
}
EOF
OUT="$(run "$T/c3")"; RC=$?
if [ "$RC" -ne 0 ] && echo "$OUT" | grep -q "docs/guide/gone-split.md"; then
    ok "3 путь, разрезанный переносом, — найден и красный"
else bad "3 разрезанный путь НЕ найден (страж слеп там же, где первый замер): $OUT"; fi

# --- 4. мёртвый путь в комментарии Rust -> зелёный -------------------------------
mk_root "$T/c4"
cat > "$T/c4/compiler-codegen/src/a.rs" <<'EOF'
// historical note: docs/guide/gone-comment.md described the old rule
/* also docs/guide/gone-block.md in a block comment */
fn f() { emit(format!("see docs/guide/live.md now.")); }
EOF
OUT="$(run "$T/c4")"; RC=$?
if [ "$RC" -eq 0 ]; then ok "4 путь в комментарии не судится — зелёный"
else bad "4 комментарий покраснел: $OUT"; fi

# --- 5. путь-значение (литерал = ровно путь) -> зелёный --------------------------
mk_root "$T/c5"
cat > "$T/c5/compiler-codegen/src/a.rs" <<'EOF'
fn f() {
    let p = PathBuf::from("docs/guide/never-written.md");
    let q = root.join("docs/guide/output-target.md");
    emit(format!("see docs/guide/live.md now."));
}
EOF
OUT="$(run "$T/c5")"; RC=$?
if [ "$RC" -eq 0 ]; then ok "5 путь-значение не судится — зелёный"
else bad "5 путь-значение покраснел (страж красит законный код): $OUT"; fi

# --- 6. мёртвый путь в raw-литерале -> красный -----------------------------------
mk_root "$T/c6"
cat > "$T/c6/compiler-codegen/src/a.rs" <<'EOF'
fn f() { emit(r#"migration guide: docs/guide/gone-raw.md has the table."#); }
EOF
OUT="$(run "$T/c6")"; RC=$?
if [ "$RC" -ne 0 ] && echo "$OUT" | grep -q "docs/guide/gone-raw.md"; then
    ok "6 raw-литерал судится — красный"
else bad "6 raw-литерал — слепая зона: $OUT"; fi

# --- 7. НОЛЬ путей под судом -> красный как потеря мишени ------------------------
mk_root "$T/c7"
cat > "$T/c7/compiler-codegen/src/a.rs" <<'EOF'
fn f() { emit(format!("nothing to see here at all.")); }
EOF
OUT="$(run "$T/c7")"; RC=$?
if [ "$RC" -ne 0 ] && echo "$OUT" | grep -qi "мишень"; then
    ok "7 нулевая мишень — отказ, а не зелёный ноль"
else bad "7 пустая мишень прошла зелёной (страж лжёт на сломанном разборе): $OUT"; fi

# --- 8. база без ключей -> красный ------------------------------------------------
mk_root "$T/c8"
cat > "$T/c8/compiler-codegen/src/a.rs" <<'EOF'
fn f() { emit(format!("see docs/guide/live.md now.")); }
EOF
printf '# letopis bez klyuchey\n' > "$T/c8/scripts/guards/diag-paths.baseline"
OUT="$(run "$T/c8")"; RC=$?
if [ "$RC" -ne 0 ]; then ok "8 база без ключей — отказ"
else bad "8 база без ключей прошла зелёной: $OUT"; fi

# --- итог -------------------------------------------------------------------------
if [ "$FAILED" -ne 0 ]; then
    echo "test-check-diag-paths: FAIL - провалено случаев: $FAILED из $CASES" >&2
    exit 1
fi
echo "test-check-diag-paths ok: $CASES/$CASES"
exit 0
