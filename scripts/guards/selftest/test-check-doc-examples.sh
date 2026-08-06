#!/bin/sh
# Самотест check-doc-examples.sh (окно p-example-guard): на fixture-дереве
# (не на настоящей репе) проверяет — (1) каждый из 11 классов снятых форм
# ловится, (2) корректный (канонический) код НЕ ложнит, (3) ```rust/```sh
# блоки игнорируются целиком даже если внутри `let`, (4) блок-гранулярное
# исключение («RETIRED form:»/E_*-код где-то в блоке — исключён весь блок),
# (5) ratchet пропускает долг в пределах baseline и красит рост над ним,
# (6) spec/open-questions.md выведен из периметра. LC_ALL=C (урок msys2).
set -u
export LC_ALL=C
GUARD_SRC="$(cd "$(dirname "$0")/.." && pwd)/check-doc-examples.sh"
TMP="${TMPDIR:-/tmp}/dex_selftest_$$"
fails=0
note_fail() { echo "SELFTEST FAIL: $1" >&2; fails=$((fails + 1)); }

zero_baseline='retired_kw_let=0
retired_kw_readonly=0
retired_pointer_ro=0
retired_unsafe_type_modifier=0
retired_postfix_bang=0
retired_trait_impl_throws=0
retired_ref_form=0
retired_external_fn=0
retired_addr_of=0
retired_null_ptr=0
retired_protocol_renamed=0
'

setup_tree() {
    rm -rf "$TMP"
    mkdir -p "$TMP/docs/guide" "$TMP/spec" "$TMP/scripts/guards"
    cp "$GUARD_SRC" "$TMP/scripts/guards/check-doc-examples.sh"
    printf '%s' "$zero_baseline" > "$TMP/scripts/guards/doc-examples.baseline"
}
run_guard() { DOC_EXAMPLES_SHOW_MATCHES=0 bash "$TMP/scripts/guards/check-doc-examples.sh" "$TMP" >"$TMP/.stdout" 2>"$TMP/.stderr"; }

# ============================================================
# 0. Пустое/чистое дерево — вакуумно-зелёное.
# ============================================================
setup_tree
run_guard || note_fail "0: ложняк на пустом дереве (без docs/guide-файлов вовсе)"

# ============================================================
# 1. Каждый из 11 классов — отдельным фикстур-файлом, ловится ratchet'ом.
# ============================================================
check_class() {  # label key nova_body
    local label="$1" key="$2" body="$3"
    setup_tree
    printf '# Fixture\n\n```nova\n%s\n```\n' "$body" > "$TMP/docs/guide/f.md"
    run_guard && { note_fail "$label: не поймал рост ($key должен был вырасти 0 -> N)"; return; }
    grep -q "^$key=" "$TMP/.stdout" 2>/dev/null
    if ! grep -qE "DOC-EXAMPLES FAIL: $key=" "$TMP/.stderr"; then
        note_fail "$label: FAIL-строка не назвала ключ $key (см. .stderr)"
    fi
}

check_class "1a let"            retired_kw_let               'ro x = 1
mut y = x
let z = 2'
check_class "1b if-let"         retired_kw_let                'if let Some(v) = opt { }'
check_class "1c while-let"      retired_kw_let                'while let Some(v) = it.next() { }'
check_class "1d readonly"       retired_kw_readonly           'fn f(x readonly int) -> int => x'
check_class "1e *ro T"          retired_pointer_ro             'fn f(p *ro int) -> int => p.read()'
check_class "1f *unsafe T"      retired_unsafe_type_modifier   'fn f(p *unsafe int) -> int => 0'
check_class "1g postfix !"      retired_postfix_bang           'ro v = compute()!'
check_class "1h trait"          retired_trait_impl_throws      'trait Foo {
    fn bar() -> int
}'
check_class "1i impl-for"       retired_trait_impl_throws      'impl Foo for Bar {
    fn bar() -> int => 1
}'
check_class "1j throws"         retired_trait_impl_throws      'fn risky(x int) throws MyError -> int => x'
check_class "1k ref param"      retired_ref_form               'fn f(ref x int) -> int => x'
check_class "1l ref call"       retired_ref_form               'ro y = f(ref x)'
check_class "1m external fn"    retired_external_fn            'external fn c_strlen(s str) -> int'
check_class "1n addr_of"        retired_addr_of                'unsafe {
    ro p = addr_of(x)
}'
check_class "1o null ptr"       retired_null_ptr               'ro p = null int'
check_class "1p protocol renamed" retired_protocol_renamed     '#impl(Hashable)
type Foo { ro v int }'

[ "$fails" -eq 0 ] && echo "selftest check-doc-examples: часть 1 (11 классов ловятся) OK"

# ============================================================
# 2. Канонический (текущий) код — НЕ ложнит ни на одном классе.
# ============================================================
setup_tree
cat > "$TMP/docs/guide/canon.md" <<'EOF'
# Canon

```nova
module demo

fn f(p *int, q *uninit int) -> int {
    ro x = 1
    mut y = x
    consume z = y
    if Some(v) = try_thing() {
        y = v
    }
    while cond() {
        y = y + 1
    }
    ro r = risky()!!
    ro n: Option[*int] = None
    ro w = &x
    y
}

#impl(Hash)
type Foo { ro v int }

extern "nova" fn c_strlen(s str) -> int
extern "C" fn c_open(path str) -> int
```
EOF
run_guard || note_fail "2: канонический код ложнит хотя бы на одном классе (см. .stderr)"

# ============================================================
# 3. ```rust блок с `let` — НЕ считается нарушением (язык не nova).
# ============================================================
setup_tree
cat > "$TMP/docs/guide/other-lang.md" <<'EOF'
# Other language

```rust
let x = 5;
external fn foo();
```

```sh
let x=5
```
EOF
run_guard || note_fail "3: блок «rust»/«sh» с let/external fn ложно посчитан нарушением"

# ============================================================
# 4. Блок-гранулярное исключение: «RETIRED form:» на ОДНОЙ строке блока,
#    сама снятая форма — на ДРУГОЙ строке того же блока (реальный случай
#    docs/guide/typed-pointers.md, найденный при вводе стража) — весь блок
#    исключён, включая строку без самого маркера.
# ============================================================
setup_tree
cat > "$TMP/docs/guide/retired-table.md" <<'EOF'
# Pointer forms

```nova
// RETIRED form:           FINAL canonical equivalent:
ro * T                  // *T
unsafe * T               // *uninit T  — for a UNINIT pointee (§10a rename,
                        //   was `*unsafe T`); for a NULLABLE pointer use Option[*T]
```
EOF
run_guard || note_fail "4a: блок-исключение по маркеру RETIRED не сработало (строка без маркера в том же блоке дала ложный красный)"

# тот же принцип, но маркер — код диагностики E_* на отдельной строке.
setup_tree
cat > "$TMP/docs/guide/e-code-note.md" <<'EOF'
# Historical note

```nova
// see E_KW_REMOVED_LET for background
let x = 1
```
EOF
run_guard || note_fail "4b: блок-исключение по коду E_* не сработало"

# контрольный отрицательный: маркер стоит СНАРУЖИ блока (в прозе) — блок
# внутри БЕЗ маркера обязан ловиться как обычно (маркер не «протекает»
# из прозы в код-блок).
setup_tree
cat > "$TMP/docs/guide/marker-outside.md" <<'EOF'
# Note

`let` was removed (see below for an unrelated snippet):

```nova
let x = 1
```
EOF
run_guard && note_fail "4c: маркер СНАРУЖИ nova-блока ошибочно исключил блок (должен был поймать retired_kw_let)"

# ============================================================
# 5. Ratchet: долг в пределах baseline — зелёный; рост над baseline —
#    красный; легитимное повышение baseline снова даёт зелёный.
# ============================================================
setup_tree
printf '# F1\n\n```nova\nexternal fn a() -> int\n```\n' > "$TMP/docs/guide/f1.md"
printf 'retired_kw_let=0\nretired_kw_readonly=0\nretired_pointer_ro=0\nretired_unsafe_type_modifier=0\nretired_postfix_bang=0\nretired_trait_impl_throws=0\nretired_ref_form=0\nretired_external_fn=1\nretired_addr_of=0\nretired_null_ptr=0\nretired_protocol_renamed=0\n' > "$TMP/scripts/guards/doc-examples.baseline"
run_guard || note_fail "5a: храповик не пропустил retired_external_fn=1 в пределах baseline=1"

printf '# F2\n\n```nova\nexternal fn b() -> int\n```\n' > "$TMP/docs/guide/f2.md"
run_guard && note_fail "5b: не поймал рост retired_external_fn (1 -> 2, baseline=1)"

printf 'retired_kw_let=0\nretired_kw_readonly=0\nretired_pointer_ro=0\nretired_unsafe_type_modifier=0\nretired_postfix_bang=0\nretired_trait_impl_throws=0\nretired_ref_form=0\nretired_external_fn=2\nretired_addr_of=0\nretired_null_ptr=0\nretired_protocol_renamed=0\n' > "$TMP/scripts/guards/doc-examples.baseline"
run_guard || note_fail "5c: храповик не пропустил после легитимного повышения baseline до 2"

# ============================================================
# 6. spec/open-questions.md выведен из периметра (см. шапку стража).
# ============================================================
setup_tree
printf '# Open questions\n\n```nova\nexternal fn legacy() -> int\n```\n' > "$TMP/spec/open-questions.md"
run_guard || note_fail "6: spec/open-questions.md не исключён из периметра (ложный красный на историческом журнале)"

# ============================================================
# 7. Отсутствие строки в baseline — красный с внятным сообщением
#    (не «страж ушёл в ветку ok», см. №290-урок check-doc-conventions.sh).
# ============================================================
setup_tree
printf 'retired_kw_let=0\n' > "$TMP/scripts/guards/doc-examples.baseline"
run_guard && note_fail "7: не поймал отсутствие ключей в неполной baseline (должен упасть — страж не может ratchet-ить без базы)"
grep -q "нет строки" "$TMP/.stderr" || note_fail "7: сообщение не объясняет отсутствие ключа в baseline"

rm -rf "$TMP"

if [ "$fails" -ne 0 ]; then
    echo "selftest check-doc-examples: FAIL ($fails провал(ов))" >&2
    exit 1
fi
echo "selftest check-doc-examples: OK (11 классов ловятся / канон не ложнит / чужой язык игнорируется / блок-исключение RETIRED+E_* / ratchet растёт-красный/долг-зелёный / open-questions.md вне периметра / неполная baseline красная)"
