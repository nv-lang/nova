#!/bin/sh
# Самотест check-doc-conventions.sh (Plan 242): проверяет ВСЕ шесть проверок
# стража на fixture-дереве (не на настоящей репе): (1) ловит нарушение,
# (2) не ложнит на норме, (3) храповик-метрики пропускают долг В ПРЕДЕЛАХ
# baseline и красят рост НАД ним. LC_ALL=C (урок msys2 2026-07-31).
set -u
export LC_ALL=C
GUARD_SRC="$(cd "$(dirname "$0")/.." && pwd)/check-doc-conventions.sh"
TMP="${TMPDIR:-/tmp}/dc_selftest_$$"
fails=0
note_fail() { echo "SELFTEST FAIL: $1" >&2; fails=$((fails + 1)); }

setup_tree() {  # корень, куда кладём guard+baseline co-located (как в реальной репе)
    # ---------------------------------------------------------------------
# 7 (№290). РЕАЛЬНЫЙ файл базы, не синтетический: (а) каждый ключ обязан
# разбираться в ЦЕЛОЕ; (б) комментарий, потерявший ведущую '#', НЕ должен
# приниматься за значение. Прежний селфтест строил свою базу и потому не
# видел, что настоящая сломана — страж молча уходил в ветку «ok».
# ---------------------------------------------------------------------
(
    real_base="$(dirname "$0")/../doc-conventions.baseline"
    for k in plan_missing_status dev_links code_block_mismatch_pairs; do
        v=$(grep -E "^$k=[0-9]+[[:space:]]*$" "$real_base" | tail -1 | cut -d= -f2 | tr -d '[:space:]')
        case "$v" in
            ''|*[!0-9]*)
                echo "SELFTEST FAIL: 7 — ключ '$k' в РЕАЛЬНОЙ базе не разбирается в целое (получено: '$v')" >&2
                exit 1 ;;
        esac
    done
    # комментарий без решётки не должен проходить как значение
    if printf 'dev_links=7: пояснение
' | grep -qE "^dev_links=[0-9]+[[:space:]]*$"; then
        echo "SELFTEST FAIL: 7 — строка-комментарий без '#' принята за значение" >&2
        exit 1
    fi
) || fails=$((fails + 1))

# ---------------------------------------------------------------------
# 8. readme_pair: README пакета обязан быть парой en+ru (решение владельца
#    2026-08-03). Проверяем: одиночный README.md ловится; пара проходит;
#    репа без README вакуумно-зелёная.
# ---------------------------------------------------------------------
(
    RT=$(mktemp -d)
    printf '# Pkg

```
code
```
' > "$RT/README.md"
    sh "$(dirname "$0")/../check-doc-conventions.sh" "$RT" >/tmp/dc_8a_$$ 2>&1
    grep -q "readme_pair: есть README.md, нет README.ru.md" /tmp/dc_8a_$$ || {
        echo "SELFTEST FAIL: 8a — одиночный README.md не пойман" >&2; rm -rf "$RT"; exit 1; }

    printf '# Пакет

```
code
```
' > "$RT/README.ru.md"
    sh "$(dirname "$0")/../check-doc-conventions.sh" "$RT" >/tmp/dc_8b_$$ 2>&1
    grep -q "readme_pair — README.md + README.ru.md" /tmp/dc_8b_$$ || {
        echo "SELFTEST FAIL: 8b — корректная пара README не принята" >&2; rm -rf "$RT"; exit 1; }

    printf '# Пакет

```
другой-код
```
' > "$RT/README.ru.md"
    sh "$(dirname "$0")/../check-doc-conventions.sh" "$RT" >/tmp/dc_8c_$$ 2>&1
    grep -q "код-блоки README.md и README.ru.md расходятся" /tmp/dc_8c_$$ || {
        echo "SELFTEST FAIL: 8c — расхождение код-блоков README не поймано" >&2; rm -rf "$RT"; exit 1; }

    rm -f "$RT/README.md" "$RT/README.ru.md"
    sh "$(dirname "$0")/../check-doc-conventions.sh" "$RT" >/tmp/dc_8d_$$ 2>&1
    grep -q "README в корне нет" /tmp/dc_8d_$$ || {
        echo "SELFTEST FAIL: 8d — репа без README не вакуумно-зелёная" >&2; rm -rf "$RT"; exit 1; }
    rm -rf "$RT" /tmp/dc_8a_$$ /tmp/dc_8b_$$ /tmp/dc_8c_$$ /tmp/dc_8d_$$
) || fails=$((fails + 1))

rm -rf "$TMP"
    mkdir -p "$TMP/spec" "$TMP/docs/guide" "$TMP/docs/plans" "$TMP/scripts/guards"
    cp "$GUARD_SRC" "$TMP/scripts/guards/check-doc-conventions.sh"
    printf 'plan_missing_status=0\ndev_links=0\ncode_block_mismatch_pairs=0\nmixed_language_files=99\ncode_comment_ru_files=99\n' > "$TMP/scripts/guards/doc-conventions.baseline"
}
run_guard() { sh "$TMP/scripts/guards/check-doc-conventions.sh" "$TMP" 2>"$TMP/.stderr"; }

# ============================================================
# 1. spec_en_header
# ============================================================
setup_tree
printf '# Working glossary\nno ru pair, not a translation\n' > "$TMP/spec/GLOSSARY.en.md"
run_guard || { note_fail "1a: ложняк на GLOSSARY.en.md без ru-пары (не перевод, exempt)"; }

printf '# Overview\ncontenu ru\n' > "$TMP/spec/overview.md"
printf '# Overview\nno header, no frontmatter\n' > "$TMP/spec/overview.en.md"
run_guard && note_fail "1b: не поймал spec/overview.en.md без шапки/frontmatter"
grep -q "overview.en.md" "$TMP/.stderr" || note_fail "1b: сообщение не называет файл"

cat > "$TMP/spec/overview.en.md" <<'EOF'
<!-- source_rev: abc1234; source_date: 2026-08-02 -->
> Informative translation; the Russian text is normative.

# Overview
EOF
run_guard || note_fail "1c: ложняк на корректной шапке+frontmatter"

# ============================================================
# 2. guide_pairing (PUBLISHED.list)
# ============================================================
run_guard || note_fail "2a: ложняк без PUBLISHED.list (должен быть вакуумно-зелен)"

printf 'bar\n' > "$TMP/docs/guide/PUBLISHED.list"
run_guard && note_fail "2b: не поймал bar в PUBLISHED.list без пары вообще"

printf '# Bar EN\n' > "$TMP/docs/guide/bar.md"
run_guard && note_fail "2c: не поймал bar.md без bar.ru.md (частичная пара)"
grep -q "bar.ru.md" "$TMP/.stderr" || note_fail "2c: сообщение не называет отсутствующую сторону"

printf '# Bar RU\n' > "$TMP/docs/guide/bar.ru.md"
run_guard || note_fail "2d: ложняк на полной паре bar.md/bar.ru.md"

# ============================================================
# 3. plan_status (ratchet)
# ============================================================
printf '# Plan 01\nno status line\n' > "$TMP/docs/plans/01-foo.md"
run_guard && note_fail "3a: не поймал рост plan_missing_status (0 -> 1, baseline=0)"

printf 'plan_missing_status=1\ndev_links=0\ncode_block_mismatch_pairs=0\nmixed_language_files=99\ncode_comment_ru_files=99\n' > "$TMP/scripts/guards/doc-conventions.baseline"
run_guard || note_fail "3b: храповик не пропустил долг в пределах baseline=1"

printf '# Plan 02\n**Статус:** DONE\n' > "$TMP/docs/plans/02-bar.md"
run_guard || note_fail "3c: ложняк — 02-bar.md со статусом не должен считаться нарушением"

printf '# Plan 03\nno status either\n' > "$TMP/docs/plans/03-baz.md"
run_guard && note_fail "3d: не поймал рост plan_missing_status (1 -> 2, baseline=1)"

printf 'plan_missing_status=2\ndev_links=0\ncode_block_mismatch_pairs=0\nmixed_language_files=99\ncode_comment_ru_files=99\n' > "$TMP/scripts/guards/doc-conventions.baseline"
run_guard || note_fail "3e: храповик не пропустил после легитимного повышения baseline до 2"

# ============================================================
# 4. dev_links (ratchet)
# ============================================================
printf 'See [dev](../dev/x.md).\n' > "$TMP/docs/guide/refs1.md"
run_guard && note_fail "4a: не поймал рост dev_links (0 -> 1, baseline=0)"

printf 'plan_missing_status=2\ndev_links=1\ncode_block_mismatch_pairs=0\nmixed_language_files=99\ncode_comment_ru_files=99\n' > "$TMP/scripts/guards/doc-conventions.baseline"
run_guard || note_fail "4b: храповик не пропустил dev_links=1 в пределах baseline=1"

printf 'Another link docs/dev/y.md here.\n' >> "$TMP/docs/guide/refs1.md"
run_guard && note_fail "4c: не поймал рост dev_links (1 -> 2, baseline=1)"

printf 'plan_missing_status=2\ndev_links=2\ncode_block_mismatch_pairs=0\nmixed_language_files=99\ncode_comment_ru_files=99\n' > "$TMP/scripts/guards/doc-conventions.baseline"
run_guard || note_fail "4d: храповик не пропустил после легитимного повышения baseline до 2"

# ============================================================
# 5. code_block_identity (ratchet)
# ============================================================
cat > "$TMP/docs/guide/bar.md" <<'EOF'
# Bar EN

```
english comment
code_here()
```
EOF
cat > "$TMP/docs/guide/bar.ru.md" <<'EOF'
# Bar RU

```
english comment
code_here()
```
EOF
run_guard || note_fail "5a: ложняк на байт-идентичных код-блоках"

cat > "$TMP/docs/guide/bar.ru.md" <<'EOF'
# Bar RU

```
русский комментарий
code_here()
```
EOF
run_guard && note_fail "5b: не поймал расхождение код-блоков (0 -> 1, baseline=0)"

printf 'plan_missing_status=2\ndev_links=2\ncode_block_mismatch_pairs=1\nmixed_language_files=99\ncode_comment_ru_files=99\n' > "$TMP/scripts/guards/doc-conventions.baseline"
run_guard || note_fail "5c: храповик не пропустил долг code_block_mismatch_pairs=1 в пределах baseline=1"

# ============================================================
# 2b. guide_same_commit (best-effort, требует git + diff-base)
# ============================================================
GTMP="${TMPDIR:-/tmp}/dc_selftest_git_$$"
rm -rf "$GTMP"; mkdir -p "$GTMP/docs/guide" "$GTMP/scripts/guards"
cp "$GUARD_SRC" "$GTMP/scripts/guards/check-doc-conventions.sh"
printf 'plan_missing_status=0\ndev_links=0\ncode_block_mismatch_pairs=0\nmixed_language_files=99\ncode_comment_ru_files=99\n' > "$GTMP/scripts/guards/doc-conventions.baseline"
(
    cd "$GTMP" || exit 1
    git init -q .
    git config user.email selftest@example.com
    git config user.name selftest
    printf '# Pair EN v1\n' > docs/guide/pair.md
    printf '# Pair RU v1\n' > docs/guide/pair.ru.md
    git add docs/guide/pair.md docs/guide/pair.ru.md scripts/guards
    git commit -q -m base
    base_sha=$(git rev-parse HEAD)
    printf '# Pair EN v2 changed\n' > docs/guide/pair.md
    git add docs/guide/pair.md
    git commit -q -m "only en side"
    sh scripts/guards/check-doc-conventions.sh . "$base_sha" >/tmp/dc_2b_out_$$ 2>&1
    code=$?
    if [ "$code" -eq 0 ]; then
        echo "SELFTEST FAIL: 2b не поймал однобокую правку пары (en изменён, ru нет)" >&2
        exit 1
    fi
    grep -q "same-commit pairing" /tmp/dc_2b_out_$$ || { echo "SELFTEST FAIL: 2b сообщение не про same-commit pairing" >&2; exit 1; }
    rm -f /tmp/dc_2b_out_$$
) || fails=$((fails + 1))
rm -rf "$GTMP"

rm -rf "$TMP"

if [ "$fails" -ne 0 ]; then
    echo "selftest check-doc-conventions: FAIL ($fails провал(ов))" >&2
    exit 1
fi
echo "selftest check-doc-conventions: OK (все 8 проверок (7 — разбор РЕАЛЬНОЙ базы, №290): ловят нарушение / не ложнят / храповики пропускают долг в пределах baseline)"
