#!/usr/bin/env bash
# ci-apt-install.sh — `apt-get update && apt-get install` для шагов CI с пределом и повтором (задача #37).
#
# ЗАЧЕМ. 2026-10-07 шаг «Install system deps» в nova-gate.yml зависал на apt дважды (run 37686972126,
# 84+ минуты без движения; ручной full по #34 — отмена по пределу 120 минут). Нормальное время шага —
# одна-две минуты. Голый `sudo apt-get update && sudo apt-get install` не имеет ни предела, ни повтора:
# зависшее зеркало съедало весь предел задания.
#
# ЧТО ДЕЛАЕТ. До 3 попыток; каждая — `timeout -k 10 $CI_APT_ATTEMPT_TIMEOUT` (по умолчанию 270 с) вокруг
# `apt-get update` + `apt-get install -y`. У самого apt свои пределы: Acquire::Retries=3 (сетевые сбои
# отдельных файлов), Acquire::http::Timeout / Acquire::https::Timeout=30 (молчащее соединение),
# DPkg::Lock::Timeout=120 (занятый замок dpkg вместо мгновенного отказа). Между попытками — `dpkg --configure -a`:
# убитый посреди установки apt оставляет пакеты недонастроенными, повтор без этого падает о другом.
# Повтор безопасен: install -y идемпотентен, update просто перечитывает индексы.
#
# ПОЧЕМУ 270 с, А НЕ 600. Предел шага в workflow — `timeout-minutes: 15` (900 с); три попытки по 600 с в него не
# входят: третья не успела бы начаться, а вторую оборвал бы предел шага. 3x270 + паузы = ~13.5 мин < 15.
# ПОЧЕМУ SHELL-ЦИКЛ, А НЕ РЕТРАЙ-ДЕЙСТВИЕ (nick-fields/retry и подобные): сторонний код с доступом к раннеру
# требует пина по SHA и проверки; цикл на двадцать строк — нет, и он же правится вместе со стражем.
#
# Запуск: bash scripts/tools/ci-apt-install.sh pkg1 pkg2 ...
# Швы самотеста: CI_APT_SUDO (по умолчанию sudo; пусто — без sudo), CI_APT_GET, CI_APT_DPKG,
#                CI_APT_ATTEMPT_TIMEOUT, CI_APT_PAUSE.
set -u
SUDO="${CI_APT_SUDO-sudo}"
APT_GET="${CI_APT_GET:-apt-get}"
DPKG="${CI_APT_DPKG:-dpkg}"
LIMIT="${CI_APT_ATTEMPT_TIMEOUT:-270}"
PAUSE="${CI_APT_PAUSE:-10}"
MAX=3

if [ "$#" -eq 0 ]; then
    echo "ci-apt-install: FAIL — не названо ни одного пакета" >&2
    exit 2
fi

export CI_APT_PKGS="$*"
OPTS=(-o Acquire::Retries=3 -o Acquire::http::Timeout=30 -o Acquire::https::Timeout=30 -o DPkg::Lock::Timeout=120)

# Одна попытка: update и install под ОДНИМ пределом, чтобы три попытки уложились в предел шага.
one_attempt() {
    # shellcheck disable=SC2086
    $SUDO env DEBIAN_FRONTEND=noninteractive timeout -k 10 "$LIMIT" \
        bash -c 'g="$1"; shift; "$g" "$@" update && "$g" "$@" install -y $CI_APT_PKGS' _ "$APT_GET" "${OPTS[@]}"
}

attempt=1
while [ "$attempt" -le "$MAX" ]; do
    echo "ci-apt-install: попытка $attempt из $MAX (предел $LIMIT с): $CI_APT_PKGS"
    one_attempt
    rc=$?
    if [ "$rc" -eq 0 ]; then
        echo "ci-apt-install ok: попытка $attempt"
        exit 0
    fi
    echo "ci-apt-install: попытка $attempt не удалась (rc=$rc; 124/137 — предел)" >&2
    $SUDO "$DPKG" --configure -a >/dev/null 2>&1 || true
    attempt=$((attempt + 1))
    [ "$attempt" -le "$MAX" ] && sleep "$PAUSE"
done
echo "ci-apt-install: FAIL — $MAX попытки исчерпаны: $CI_APT_PKGS" >&2
exit 1
