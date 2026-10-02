#!/usr/bin/env bash
# Самотест check-novac-bin-door.py и самой двери novac_bin (реестр 221.1 №1607).
# Страж -- клетки ПАРАМИ (то же место: имя файла -> красный, дверь -> зелёный), плюс
# настоящее дерево: ok как есть, FAIL после возврата одного места к имени файла.
# Дверь -- на ДВУХ файлах сразу (новее из двух, в обе стороны), NOVAC вызывающего первым,
# без файлов -- путь сборки этой платформы. Файлы создаёт python: в MSYS `touch novac`
# при живом `novac.exe` попадает в сам `novac.exe`.
set -u
export LC_ALL=C

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
G="$ROOT/scripts/guards/check-novac-bin-door.py"
FAILED=0
ok()  { echo "  ok   $1"; }
bad() { echo "  PROVAL $1" >&2; FAILED=1; }

TMP="$(mktemp -d)"; trap 'rm -rf "$TMP"' EXIT
run() { python "$G" "$1" > "$TMP/.out" 2> "$TMP/.err"; }

doors() {   # $1 каталог: дверь на месте -- иначе страж честно отказывает «мишень потеряна»
    mkdir -p "$1/scripts/guards/lib"
    : > "$1/scripts/guards/lib/novac.sh"; : > "$1/scripts/guards/lib/novac_bin.py"
}
mk() {   # $1 каталог, $2 относительный путь файла, $3 текст
    rm -rf "$1"; mkdir -p "$1/$(dirname "$2")"
    printf '%s\n' "$3" > "$1/$2"
    doors "$1"
}
pair() {  # $1 имя клетки, $2 путь, $3 красный текст, $4 зелёный текст
    mk "$TMP/r" "$2" "$3"
    run "$TMP/r"; rc=$?
    [ $rc -ne 0 ] && grep -q "$2:1:" "$TMP/.err" && ok "$1: red" || bad "$1: expected FAIL at $2:1"
    mk "$TMP/g" "$2" "$4"
    run "$TMP/g" && grep -q "ok:" "$TMP/.out" && ok "$1: green" || bad "$1: expected ok"
}

pair "guard default .exe"  scripts/guards/check-x.sh 'BIN="${2:-$ROOT/novac/target/novac.exe}"' \
                                                     'BIN="${2:-$(novac_bin "$ROOT")}"'
pair "tool bare path"      scripts/tools/t.sh 'NOVAC="novac/target/novac"' 'NOVAC="$(novac_bin .)"'
pair "build output"        scripts/gate-x.sh '"$NOVA_BIN" build novac/src/main.nv -o "$ROOT/novac/target/novac.exe"' \
                                             '"$NOVA_BIN" build novac/src/main.nv -o "$(novac_bin_out "$ROOT")"'
pair "python parts"        scripts/tools/t.py 'novac = root / "novac" / "target" / "novac"' 'novac = novac_bin(root)'
pair "exempt sample"       scripts/tools/s.py 'SAMPLE = "C:\\novac\\target\\novac.exe emit"' \
                                              'SAMPLE = "C:\\novac\\target\\novac.exe emit"  # novac-bin: not a selection -- data'

# Not judged: a comment, a self-test building a fixture tree.
mk "$TMP/n" scripts/guards/selftest/test-y.sh 'touch "$FIX/novac/target/novac.exe"'
printf '%s\n' '# the stale novac/target/novac.exe used to win' > "$TMP/n/scripts/c.sh"
run "$TMP/n" && grep -q "ok:" "$TMP/.out" && ok "not judged: selftest fixture, comment" || bad "not judged: expected ok"

# A lost target: an empty root is a FAIL with its reason, never a green zero (#911).
mkdir -p "$TMP/e"
run "$TMP/e"; rc=$?
[ $rc -ne 0 ] && grep -q "target is lost" "$TMP/.err" && ok "empty root: FAIL, target lost" || bad "empty root: expected FAIL"

# The real tree: ok as is, FAIL after one door call turns back into the file name.
run "$ROOT" && grep -q "ok:" "$TMP/.out" && ok "real tree: ok" || bad "real tree: expected ok ($(tail -1 "$TMP/.err"))"
F="$ROOT/scripts/guards/check-novac-no-panic.sh"
if grep -q 'BIN="${2:-$(novac_bin "$ROOT")}"' "$F"; then
    mkdir -p "$TMP/t/scripts/guards"; doors "$TMP/t"
    sed 's|BIN="${2:-$(novac_bin "$ROOT")}"|BIN="${2:-$ROOT/novac/target/novac.exe}"|' "$F" > "$TMP/t/scripts/guards/check-novac-no-panic.sh"
    run "$TMP/t"; rc=$?
    [ $rc -ne 0 ] && grep -q "check-novac-no-panic.sh" "$TMP/.err" && ok "real carrier reverted: red" || bad "real carrier reverted: expected FAIL"
else
    bad "real carrier: the door call in check-novac-no-panic.sh was not found"
fi

# The door itself.
. "$ROOT/scripts/guards/lib/novac.sh"
D="$TMP/d"; mkdir -p "$D/novac/target"
DW="$(cd "$D" && pwd -W 2>/dev/null || pwd)"
unset NOVAC
case "$(novac_bin "$D")" in */novac/target/novac.exe|*/novac/target/novac) ok "door: no file -> a build path" ;; *) bad "door: no file" ;; esac
python - "$DW" <<'EOF'
import os, sys, time
t = os.path.join(sys.argv[1], "novac", "target")
open(os.path.join(t, "novac.exe"), "w").write("old")
time.sleep(1.2)
open(os.path.join(t, "novac"), "w").write("new")
EOF
case "$(novac_bin "$D")" in */novac/target/novac) ok "door: bare newer -> bare" ;; *) bad "door: bare newer, got $(novac_bin "$D")" ;; esac
python - "$DW" <<'EOF'
import os, sys, time
time.sleep(1.2)
os.utime(os.path.join(sys.argv[1], "novac", "target", "novac.exe"))
EOF
case "$(novac_bin "$D")" in */novac/target/novac.exe) ok "door: .exe newer -> .exe" ;; *) bad "door: .exe newer, got $(novac_bin "$D")" ;; esac
[ "$(NOVAC=/x/carina novac_bin "$D")" = "/x/carina" ] && ok "door: NOVAC of the caller first" || bad "door: NOVAC ignored"
PY="$(python -c "import sys; sys.path.insert(0, sys.argv[1]); import novac_bin as n; print(n.novac_bin(sys.argv[2]).name)" "$ROOT/scripts/guards/lib" "$DW")"
[ "$PY" = "novac.exe" ] && ok "door (python): the same choice" || bad "door (python): got $PY"

[ $FAILED -eq 0 ] && echo "test-check-novac-bin-door: ok" || { echo "test-check-novac-bin-door: FAIL" >&2; exit 1; }
