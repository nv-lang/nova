#!/bin/sh
# Н5 форма 1: 'ro x = 1; x = 2' принимается молча.
# Запуск: sh cmd.sh из ЭТОГО каталога.
. ../env.sh || exit 1
cd "$ROOT" || exit 1
P="$HUNT/p11-ro-assign-accepted/probe.nv"
echo "=== SUBJECT: novac check (ожидается МОЛЧАНИЕ)"
"$NOVAC" check "$P"; echo "novac rc=$?"
echo "=== ORACLE: nova check"
"$NOVA" check "$P"; echo "oracle rc=$?"
echo "=== ЧТО ЭМИТИТСЯ (обычный изменяемый C-локал: молчаливое принятие)"
"$NOVAC" emit "$P" | sed -n '/nova_fn_main_impl/,/^}/p' | head -8
