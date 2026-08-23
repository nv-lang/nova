#!/bin/sh
# scripts/tools/novac-e1-smoke.sh — the E1/E2 vertical smoke: a file compiled
# by novac must BEHAVE identically to the oracle's binary (plan 274 §9 core).
#
# Cost discipline (plan 274 §1.4 «цена итерации», owner 2026-08-15): the
# oracle's `nova build` spends ~20s OUTSIDE the compiler; the smoke pays it
# ONCE per file+oracle and never again:
#   1. the ORACLE binary is cached by (file content, oracle mtime);
#   2. novac's C is linked DIRECTLY by clang with the oracle's exact argv,
#      captured ONCE through the NOVA_CLANG interception door on the same
#      build that produced the reference binary; -g dropped, lld linker;
#   3. the runtime headers (nova_rt.h -> gc/libuv, ~82k preprocessed lines,
#      0.43s of the 0.5s compile) are a PCH built once per oracle;
#   4. the hot path forks as few processes as MSYS allows: no git/cygpath/
#      md5sum per run — the cache key is the file's bytes via cksum, paths
#      are computed once into the argv cache.
# Warm target: <= 2s per file (emit + clang ~0.5s + two runs).
#
# Usage: sh scripts/tools/novac-e1-smoke.sh [file.nv]   (default: hello)
# Проверялся: Windows (Git Bash), 2026-08-15.
export LC_ALL=C
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
FILE="${1:-examples/basics/hello.nv}"
NOVAC="$ROOT/novac/target/novac.exe"
CACHE="${NOVAC_SMOKE_CACHE:-${TMPDIR:-/tmp}/novac-smoke-cache}"
T="${TMPDIR:-/tmp}/novac-smoke.$$"
mkdir -p "$CACHE" "$T"
trap 'rm -rf "$T"' 0
fail() { echo "novac-e1-smoke: FAIL — $1" >&2; exit 1; }
[ -f "$NOVAC" ] || fail "нет $NOVAC (собери novac)"
cd "$ROOT" || exit 2

# ---- oracle location & stamp: computed once per cache dir ---------------
if [ ! -f "$CACHE/oracle.path" ]; then
    ORACLE_MAIN=$(git -C "$ROOT" rev-parse --path-format=absolute --git-common-dir 2>/dev/null)
. "$(CDPATH= cd -- "$(dirname -- "$0")/../guards" && pwd)/lib/novac.sh"
ORACLE="$(novac_find_oracle "$(pwd)" || true)"
    [ -f "$ORACLE" ] || ORACLE="$ROOT/nova-cli/target/release/nova.exe"
    [ -f "$ORACLE" ] || fail "нет оракула"
    printf '%s\n' "$ORACLE" > "$CACHE/oracle.path"
fi
read -r ORACLE < "$CACHE/oracle.path"
ORACLE_STAMP=$(stat -c %Y "$ORACLE")
# КАКОЙ clang НАСТОЯЩИЙ. Путь Windows — это ДЕФОЛТ WINDOWS, а не факт мира:
# на Linux его нет, и инструмент молча уезжал в «нет такого файла». Признак
# системы здесь — `cygpath`: он есть в MSYS и его нет нигде больше.
if command -v cygpath >/dev/null 2>&1; then
    REAL_CLANG="${NOVA_CLANG:-C:/Program Files/LLVM/bin/clang.exe}"
else
    REAL_CLANG="${NOVA_CLANG:-$(command -v clang || printf 'clang')}"
fi

# ---- 1. oracle binary + captured argv, cached ---------------------------
KEY=$(cksum < "$FILE" | cut -d' ' -f1)-$ORACLE_STAMP
ORACLE_EXE="$CACHE/oracle-$KEY.exe"
LINKCMD="$CACHE/link-$ORACLE_STAMP.argv"
CFLAGS="$CACHE/cflags-$ORACLE_STAMP.argv"
PCH="$CACHE/prelude-$ORACLE_STAMP.pch"
if [ ! -f "$ORACLE_EXE" ] || [ ! -f "$LINKCMD" ]; then
    STEM=$(basename "$FILE" .nv)
    mkdir -p "$T/pkgless"
    sed "s/^module [a-zA-Z_.]*${STEM}\$/module ${STEM}/" "$FILE" > "$T/pkgless/$STEM.nv"
    LOG="$T/cc.log"; : > "$LOG"
    # ПЕРЕХВАТ clang-argv — ОДНА идея в ДВУХ формах, и вторая появилась по
    # красному CI 2026-08-23 (класс К3, план 274 §9.1д): обёртка была только
    # `.cmd`, то есть Windows-only, и на Linux инструмент печатал `cygpath:
    # command not found` на каждой фикстуре, а гейт объявлял «перехват не
    # сработал» — отказ, по которому идут искать поломку в компиляторе.
    # Форма разная, потому что оболочка разная; смысл один: записать каждый
    # аргумент строкой, поставить `__END__` и позвать настоящий clang.
    if command -v cygpath >/dev/null 2>&1; then
        WIN_T=$(cygpath -w "$T"); WIN_LOG=$(cygpath -w "$LOG")
        printf '@echo off\r\nsetlocal\r\n:loop\r\nif "%%~1"=="" goto run\r\necho %%1>> "%s"\r\nshift\r\ngoto loop\r\n:run\r\necho __END__>> "%s"\r\n"%s" %%*\r\n' "$WIN_LOG" "$WIN_LOG" "$(cygpath -w "$REAL_CLANG")" > "$T/clang-log.cmd"
        WRAPPER="$WIN_T\\clang-log.cmd"
    else
        printf '#!/bin/sh\nfor a in "$@"; do printf "%%s\\n" "$a" >> "%s"; done\nprintf "__END__\\n" >> "%s"\nexec "%s" "$@"\n' "$LOG" "$LOG" "$REAL_CLANG" > "$T/clang-log.sh"
        chmod +x "$T/clang-log.sh"
        WRAPPER="$T/clang-log.sh"
    fi
    NOVA_CLANG="$WRAPPER" "$ORACLE" build "$T/pkgless/$STEM.nv" -o "$T/oracle.exe" >"$T/oracle.out" 2>&1 \
        || fail "оракул не собрал $FILE: $(tail -3 "$T/oracle.out")"
    cp "$T/oracle.exe" "$ORACLE_EXE"
    if [ ! -f "$LINKCMD" ]; then
        # link argv: everything but -o/<exe>/<input>; -g dropped; lld added
        awk 'BEGIN{skip=0} /^__END__/{exit} skip{skip=0; next} /^-o$/{skip=1; next} /\.c"?$/{next} /^-g$/{next} {print}' "$LOG" \
            | tr -d '\r' | sed 's|\\|/|g' > "$LINKCMD"
        printf '%s\n' "-fuse-ld=lld" >> "$LINKCMD"
        grep -q "libnova_rt" "$LINKCMD" || fail "перехват clang-argv не сработал"
        # compile-only flags (for the PCH and the -c step): no libs/linker flags
        grep -vE '\.lib$|^-l|^-L$|/lib$|Wl,|^-ffunction-sections|^-fdata-sections|^-fuse-ld' "$LINKCMD" > "$CFLAGS"
    fi
fi

# ---- 2. PCH of the runtime prelude, once per oracle ----------------------
if [ ! -f "$PCH" ]; then
    # the PCH records its source header's path — keep it in the cache too
    printf '#include "nova_rt/nova_rt.h"\n' > "$CACHE/prelude-$ORACLE_STAMP.h"
    # ОТКАЗ ОБЯЗАН ПОКАЗАТЬ ПЕРЕХВАЧЕННЫЕ ФЛАГИ (2026-08-23). На CI (Linux) этот
    # шаг ответил «cannot specify -o when generating multiple output files» — то
    # есть в CFLAGS попал ВХОД, а не только флаги, — и по одному этому
    # сообщению нельзя сказать, какой именно: argv оракула на Linux другой, а
    # машины под рукой нет. Теперь отказ несёт первые строки CFLAGS, и разбор
    # идёт по факту, а не по догадке.
    eval "\"$REAL_CLANG\" $(tr '\n' ' ' < "$CFLAGS") -x c-header \"$CACHE/prelude-$ORACLE_STAMP.h\" -o \"$PCH\"" > "$T/pch.out" 2>&1 \
        || fail "PCH не собрался: $(head -3 "$T/pch.out") | перехваченные CFLAGS ($(grep -c '' "$CFLAGS") строк): $(tr '\n' ' ' < "$CFLAGS" | head -c 400)"
fi

# ---- 3. novac emit -> compile against PCH -> link with the oracle's argv --
"$NOVAC" emit "$FILE" > "$T/novac.c" 2>"$T/emit.err" || fail "novac emit упал: $(cat "$T/emit.err")"
# the emission's first include IS the PCH prelude; drop that one line
sed '0,/^#include "nova_rt\/nova_rt.h"$/{//d}' "$T/novac.c" > "$T/body.c"
eval "\"$REAL_CLANG\" $(tr '\n' ' ' < "$CFLAGS") -include-pch \"$PCH\" -c \"$T/body.c\" -o \"$T/body.o\"" > "$T/cc.out" 2>&1 \
    || fail "clang -c упал: $(head -5 "$T/cc.out")"
eval "\"$REAL_CLANG\" $(tr '\n' ' ' < "$LINKCMD") -o \"$T/novac.exe\" \"$T/body.o\"" > "$T/link.out" 2>&1 \
    || fail "clang не слинковал: $(head -5 "$T/link.out")"

# ---- 4. behavior diff ---------------------------------------------------
"$ORACLE_EXE" > "$T/out.oracle" 2>&1; e_o=$?
"$T/novac.exe"  > "$T/out.novac"  2>&1; e_n=$?
cmp -s "$T/out.oracle" "$T/out.novac" || fail "stdout расходится: $(diff "$T/out.oracle" "$T/out.novac" | head -3)"
[ "$e_o" -eq "$e_n" ] || fail "exit-коды расходятся: oracle=$e_o novac=$e_n"
echo "novac-e1-smoke ok: $FILE — поведение идентично оракулу (stdout байт-в-байт, exit $e_o)"
exit 0
