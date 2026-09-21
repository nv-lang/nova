#!/bin/sh
# Перепрогон ВСЕХ проб охоты: каждая своей командой, вывод в её же run.out.
# Запуск из ЭТОГО каталога: sh rerun.sh
# Бинари берутся из env.sh; если они собраны в другом дереве —
#   NOVAC=<...>/novac/target/novac.exe NOVA=<...>/nova.exe sh rerun.sh
here=$(pwd)
for d in "$here"/p*/; do
    b=$(basename "$d")
    ( cd "$d" && sh cmd.sh > run.out 2>&1 )
    if [ ! -s "$d/run.out" ]; then
        echo "$b: ПУСТОЙ run.out — проба не отработала" >&2
        exit 1
    fi
    echo "########## $b"
    cat "$d/run.out"
done
