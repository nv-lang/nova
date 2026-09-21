#!/bin/sh
# Н2: литерал в образце match не судится НИКЕМ; эмиссия даёт невалидный C.
# Запуск: sh cmd.sh из ЭТОГО каталога.
. ../env.sh || exit 1
cd "$ROOT" || exit 1
P="$HUNT/p6-match-literal-unjudged/probe.nv"
echo "=== SUBJECT: novac check (ожидается молчание)"
"$NOVAC" check "$P"; echo "novac rc=$?"
echo "=== ORACLE: nova check"
"$NOVA" check "$P"; echo "oracle rc=$?"
echo "=== ЧТО ЭМИТИТСЯ (строка сравнения в сгенерированном C)"
"$NOVAC" emit "$P" | grep -n "99999999999999999999999"
echo "=== ЧТО СКАЖЕТ СИШНЫЙ КОМПИЛЯТОР на эту строку"
CC=/c/Program\ Files/LLVM/bin/clang.exe
if [ -x "$CC" ]; then
    "$CC" -fsyntax-only "$HUNT/p6-match-literal-unjudged/emitted_line.c" 2>&1 | head -4
else
    echo "(clang не найден по $CC — шаг ПРОПУЩЕН, и это не зелёный)"
fi
