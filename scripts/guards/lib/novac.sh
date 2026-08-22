#!/bin/sh
# scripts/guards/lib/novac.sh — общие функции стражей novac (274.3, класс К-A).
# Подключение: . "$ROOT/scripts/guards/lib/novac.sh"
#
# novac_require_bin NAME ROOT BIN — F1: «судить нечего» законно ТОЛЬКО пока
#   novac/src/main.nv не существует. Как только исходник есть, отсутствие
#   бинаря = гейт его не собрал (или сборка упала) — красный, не «ok».
#
# novac_is_panic_rc RC — F3: ЕДИНОЕ определение паники вместо трёх разных
#   порогов (>=124 в фаззере, >=128 в страже, >=124 в дифф-раннере).
#   Уточнение контракта (274.3/F3, проверено живым фаззером 2026-08-15):
#   план §7 говорит «на любом входе novac завершается кодом 0 или 1» — это
#   про ВЕРДИКТ компилятора (принял / отверг с диагностикой). Кроме вердикта
#   у CLI есть ДВЕРЬ: файл не читается / не UTF-8 / неверный вызов — это
#   exit 2 с честным сообщением (документировано в novac/src/main.nv, носитель
#   контракта — маркер EXPECT_EXIT_CODE 2). Мутационный фаззер порождает
#   не-UTF-8 входы штатно: 15 таких кейсов из 192 честно отвечают «not UTF-8»
#   и выходят 2 — паникой это не является.
#   Итог: паника = код НЕ 0, НЕ 1 и НЕ 2 (сюда попадают 3 — abort рантайма на
#   Windows, 101 — паника Nova, 124 — таймаут, >=128 — сигнал), либо слово
#   'panic' в stderr (это проверяет вызывающий), либо МОЛЧАЛИВАЯ дверь — см.
#   novac_is_silent_door: exit 2 без единого байта вывода означает отказ без
#   причины, а это тот же класс «тихо», что и паника.
#
# novac_is_silent_door RC OUTFILE ERRFILE — exit 2 без вывода = красный.
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
    [ "$1" -ne 0 ] && [ "$1" -ne 1 ] && [ "$1" -ne 2 ]
}

novac_is_silent_door() {
    [ "$1" -eq 2 ] || return 1
    [ -s "$2" ] && return 1
    [ -s "$3" ] && return 1
    return 0
}

# novac_load_scale ROOT — F10: поправка на ЗАГРУЗКУ машины, общая для всех
# бюджетов времени. Печатает множитель в сотых (100 = машина как при записи
# базы, 300 = втрое медленнее). Считается по эталонному fork оболочки — под
# MSYS это главный источник шума — против записанного в базе `cal-ms`.
# Абсолютный wall-clock судить нельзя: один и тот же `novac check` даёт 150мс
# на тихой машине и 3300мс под полным прогоном гейта, а дифф-раннер — 50с и
# 146с (оба случая пойманы ложными красными 2026-08-15).
novac_load_scale() {
    _base="$1/scripts/guards/novac-iteration-cost.baseline"
    _cal_base=$(tr -d '\r' < "$_base" 2>/dev/null | sed -n 's/^cal-ms \([0-9][0-9]*\)$/\1/p' | head -n 1)
    [ -n "$_cal_base" ] && [ "$_cal_base" -gt 0 ] 2>/dev/null || { echo 100; return 0; }
    _b=999999
    for _i in 1 2 3; do
        _s=$(date +%s%N | cut -c1-13)
        sh -c : >/dev/null 2>&1
        _e=$(( $(date +%s%N | cut -c1-13) - _s ))
        [ "$_e" -lt "$_b" ] && _b=$_e
    done
    _sc=$(( _b * 100 / _cal_base ))
    [ "$_sc" -lt 100 ] && _sc=100
    [ "$_sc" -gt 2000 ] && _sc=2000
    echo "$_sc"
}

# novac_find_oracle ROOT — путь к бинарю ОРАКУЛА или пусто (rc=1).
#
# ОДНО место, знающее имя файла. 2026-08-18, разбор красного CI: дифф-страж и
# страж свежести оболочки печатали на Linux «судить нечего (оракул не собран)»,
# хотя job собирает его командой cargo. Причина — одна буква: искали
# `nova.exe`, а на Linux бинарь зовётся `nova`. Главное доказательство
# корректности проекта — дифф против оракула — не выполнялось на CI и
# держалось только на машине автора, причём ЗЕЛЁНЫМ молчанием, неотличимым
# от «проверено» (класс K-A, 274.3/F1).
novac_find_oracle() {
    _fo_root="$1"
    for _fo_c in \
        "$_fo_root/nova-cli/target/release/nova.exe" \
        "$_fo_root/nova-cli/target/release/nova"; do
        [ -f "$_fo_c" ] && { printf '%s\n' "$_fo_c"; return 0; }
    done
    # Worktree без своего target: сборка одна на все деревья (реестр #650).
    _fo_main=$(git -C "$_fo_root" rev-parse --path-format=absolute --git-common-dir 2>/dev/null)
    if [ -n "$_fo_main" ]; then
        for _fo_c in \
            "$_fo_main/../nova-cli/target/release/nova.exe" \
            "$_fo_main/../nova-cli/target/release/nova"; do
            [ -f "$_fo_c" ] && { printf '%s\n' "$_fo_c"; return 0; }
        done
    fi
    return 1
}
