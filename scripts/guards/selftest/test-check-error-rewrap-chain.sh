#!/usr/bin/env bash
# Самотест check-error-rewrap-chain.py -- клетки ПАРАМИ: у каждой «ждём красного» есть
# «ждём зелёного» на почти том же входе (тот же макрос, тот же аргумент, `{:#}` вместо
# `{}`), плюс проба на настоящем дереве: ok как есть, FAIL после возврата одного `{}`.
set -u
export LC_ALL=C

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
G="$ROOT/scripts/guards/check-error-rewrap-chain.py"
FAILED=0
ok()  { echo "  ok   $1"; }
bad() { echo "  PROVAL $1" >&2; FAILED=1; }

TMP="$(mktemp -d)"; trap 'rm -rf "$TMP"' EXIT
run() { python "$G" "$1" > "$TMP/.out" 2> "$TMP/.err"; }

mk() {   # $1 каталог, $2 текст файла nova-cli/src/x.rs
    rm -rf "$1"; mkdir -p "$1/nova-cli/src" "$1/compiler-codegen/src"
    printf '%s\n' "$2" > "$1/nova-cli/src/x.rs"
}
pair() {  # $1 имя клетки, $2 красный текст, $3 зелёный текст
    mk "$TMP/r" "$2"
    run "$TMP/r"; rc=$?
    [ $rc -ne 0 ] && grep -q "x.rs:1:" "$TMP/.err" && ok "$1: red" || bad "$1: expected FAIL at x.rs:1"
    mk "$TMP/g" "$3"
    run "$TMP/g" && grep -q "ok:" "$TMP/.out" && ok "$1: green" || bad "$1: expected ok"
}

pair "anyhow positional"  'let x = r.map_err(|e| anyhow!("resolve: {}", e));' \
                          'let x = r.map_err(|e| anyhow!("resolve: {:#}", e));'
pair "bail second arg"    'bail!("dep `{}`: {}", name, err);' \
                          'bail!("dep `{}`: {:#}", name, err);'
pair "inline capture"     'return Err(anyhow!("read {path}: {e}"));' \
                          'return Err(anyhow!("read {path}: {e:#}"));'
pair "format into field"  'Stage::Cc { error: format!("spawn cc: {}", e) }' \
                          'Stage::Cc { error: format!("spawn cc: {:#}", e) }'
pair "top printer"        'eprintln!("{} {}", bold(&red("error:")), e);' \
                          'eprintln!("{} {:#}", bold(&red("error:")), e);'
pair "suffix name"        'anyhow!("open: {}", io_err)' \
                          'anyhow!("open: {:?}", io_err)'
pair "to_string form"     'let vs = list_versions(url).map_err(|e| e.to_string())?;' \
                          'let vs = list_versions(url).map_err(|e| format!("{:#}", e))?;'

# A test reading the top message is not judged.
mk "$TMP/a" 'assert!(err.to_string().contains("clone"), "err: {}", err);'
run "$TMP/a" && grep -q "ok:" "$TMP/.out" && ok "not judged: assert line" || bad "not judged: assert line expected ok"

# Multi-line call: the format string and the error on later lines.
mk "$TMP/m" 'let x = r.map_err(|e| {
    anyhow!(
        "git: {}",
        e
    )
});'
run "$TMP/m"; rc=$?
[ $rc -ne 0 ] && grep -q "x.rs:2:" "$TMP/.err" && ok "multi-line call: red" || bad "multi-line call: expected FAIL at x.rs:2"

# Not judged: another name, a comment, a non-error placeholder beside `{:#}`.
mk "$TMP/n" '// anyhow!("text {}", e) in a comment
let a = anyhow!("value {}", count);
let b = format!("{}: {:#}", name, e);'
run "$TMP/n" && grep -q "ok:" "$TMP/.out" && ok "not judged: comment, other name, mixed" || bad "not judged: expected ok"

# The real tree: ok as is, FAIL after one `{:#}` turns back into `{}`.
run "$ROOT" && grep -q "ok:" "$TMP/.out" && ok "real tree: ok" || bad "real tree: expected ok ($(tail -1 "$TMP/.err"))"
F="$ROOT/compiler-codegen/src/lockfile.rs"
if grep -q 'git-зависимостей:\\n  {:#}' "$F" 2>/dev/null; then
    mkdir -p "$TMP/t/compiler-codegen/src" "$TMP/t/nova-cli/src"
    sed 's/git-зависимостей:\\n  {:#}/git-зависимостей:\\n  {}/' "$F" > "$TMP/t/compiler-codegen/src/lockfile.rs"
    run "$TMP/t"; rc=$?
    [ $rc -ne 0 ] && grep -q "lockfile.rs" "$TMP/.err" && ok "real carrier reverted: red" || bad "real carrier reverted: expected FAIL"
else
    bad "real carrier lockfile.rs: the rewrapped line was not found"
fi

[ $FAILED -eq 0 ] && echo "test-check-error-rewrap-chain: ok" || { echo "test-check-error-rewrap-chain: FAIL" >&2; exit 1; }
