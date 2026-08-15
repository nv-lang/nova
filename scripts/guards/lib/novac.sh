#!/bin/sh
# scripts/guards/lib/novac.sh — общие функции стражей novac (274.3, класс К-A).
# Подключение: . "$(dirname "$0")/lib/novac.sh"
#
# novac_require_bin NAME ROOT BIN — F1: «судить нечего» законно ТОЛЬКО пока
#   novac/src/main.nv не существует. Как только исходник есть, отсутствие
#   бинаря = гейт его не собрал (или сборка упала) — красный, не «ok».
# novac_is_panic_rc RC — F3: контракт §7 «на любом входе novac завершается
#   кодом 0 или 1»; всё иное (2 — usage/IO, 101 — panic, 124 — timeout,
#   >=128 — сигнал, 3 — abort рантайма на Windows) — паника/крэш для стражей
#   ноль-паник. Единая константа вместо трёх порогов (>=124/>=128).
novac_require_bin() {
    _name="$1"; _root="$2"; _bin="$3"
    if [ -f "$_bin" ]; then return 0; fi
    if [ -f "$_root/novac/src/main.nv" ]; then
        echo "$_name: FAIL — novac/src существует, а бинаря $_bin нет: гейт обязан собрать novac шагом novac-build (274.3/F1); «судить нечего» больше не законно" >&2
        exit 1
    fi
    echo "$_name ok: судить нечего (novac/src ещё нет)"
    exit 0
}
novac_is_panic_rc() {
    [ "$1" -ne 0 ] && [ "$1" -ne 1 ]
}
