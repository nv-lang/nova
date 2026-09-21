#!/bin/sh
# Н3 контроль: мёртвая строка после if делает тот же файл законным.
# Запуск: sh cmd.sh из ЭТОГО каталога.
. ../env.sh || exit 1
cd "$ROOT" || exit 1
P="$HUNT/p8-dead-code-makes-it-legal/probe.nv"
echo "=== SUBJECT: novac check (ожидается ЧИСТО)"
"$NOVAC" check "$P"; echo "novac rc=$?"
echo "=== ORACLE: nova check"
"$NOVA" check "$P"; echo "oracle rc=$?"
echo "=== ЧТО ЭМИТИТСЯ (форма компилируется правильно — значит отказ Н3 не про эмиттер)"
"$NOVAC" emit "$P" | sed -n '/novac_fn_p_f__nova_int__to_nova_int/,/^}/p'
