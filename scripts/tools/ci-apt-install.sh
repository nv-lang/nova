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

PKGS="$*"
OPTS=(-o Acquire::Retries=3 -o Acquire::http::Timeout=30 -o Acquire::https::Timeout=30 -o DPkg::Lock::Timeout=120)

# Hosted build jobs already carry the C toolchain and the pinned GC source.
# Build the one missing dependency locally when that covers the entire request.
install_hosted_build_deps() {
    [ "${RUNNER_ENVIRONMENT:-}" = github-hosted ] || return 1
    local pkg gc_requested=0
    for pkg do
        case "$pkg" in
            libgc-dev) gc_requested=1 ;;
            clang|cmake|make) command -v "$pkg" >/dev/null || return 1 ;;
            build-essential)
                command -v gcc >/dev/null && command -v g++ >/dev/null &&
                    command -v make >/dev/null || return 1 ;;
            *) return 1 ;;
        esac
    done
    [ "$gc_requested" -eq 1 ] || return 1

    local gc_source="$(dirname "${BASH_SOURCE[0]}")/../../compiler-codegen/nova_rt/gc"
    [ -f "$gc_source/CMakeLists.txt" ] && command -v cmake >/dev/null || return 1
    local build_dir
    build_dir=$(mktemp -d) || return 1
    echo "ci-apt-install: building Boehm GC from the pinned submodule"
    cmake -S "$gc_source" -B "$build_dir" -DCMAKE_BUILD_TYPE=Release \
        -DCMAKE_INSTALL_PREFIX=/usr/local -DCMAKE_INSTALL_LIBDIR=lib \
        -Denable_docs=OFF -Dbuild_cord=OFF &&
        cmake --build "$build_dir" --parallel 2 &&
        $SUDO cmake --install "$build_dir" &&
        $SUDO ldconfig
    local rc=$?
    rm -rf "$build_dir"
    return "$rc"
}

if install_hosted_build_deps "$@"; then
    exit 0
fi


# GitHub's Azure mirror timed out on every retry. Use Ubuntu's signed
# archive over HTTPS on hosted runners, keeping their suites and components.
ubuntu_sources="${CI_APT_UBUNTU_SOURCES:-/etc/apt/sources.list.d/ubuntu.sources}"
if [ "${RUNNER_ENVIRONMENT:-}" = github-hosted ] &&
    [ -r "$ubuntu_sources" ] &&
    grep -Eq '^URIs: (mirror\+file:/etc/apt/apt-mirrors\.txt|https?://azure\.archive\.ubuntu\.com/ubuntu)/?[[:space:]]*$' "$ubuntu_sources"; then
    apt_sources=$(mktemp --suffix=.sources) || exit 1
    trap 'rm -f "$apt_sources"' EXIT
    sed -E 's@^URIs: (mirror\+file:/etc/apt/apt-mirrors\.txt|https?://azure\.archive\.ubuntu\.com/ubuntu)/?[[:space:]]*$@URIs: https://archive.ubuntu.com/ubuntu@' \
        "$ubuntu_sources" > "$apt_sources" || exit 1
    chmod a+r "$apt_sources" || exit 1
    OPTS+=(-o "Dir::Etc::sourcelist=$apt_sources" -o Dir::Etc::sourceparts=-)
    echo "ci-apt-install: using the Ubuntu archive over HTTPS"
fi

# Одна попытка: update и install под ОДНИМ пределом, чтобы три попытки уложились в предел шага.
# Пакеты идут ПОЗИЦИОННЫМИ аргументами после "--": sudo (env_reset) сбрасывает окружение, и переменная с пакетами
# терялась — `apt-get install -y` без пакетов завершается 0 и не ставит НИЧЕГО.
one_attempt() {
    $SUDO env DEBIAN_FRONTEND=noninteractive timeout -k 10 "$LIMIT" \
        bash -c 'g="$1"; n="$2"; shift 2; o=("${@:1:n}"); shift "$n"; "$g" "${o[@]}" update && "$g" "${o[@]}" install -y "$@"' \
        _ "$APT_GET" "${#OPTS[@]}" "${OPTS[@]}" "$@"
    rc=$?
    # rc=124 -- СНЯТ ПРЕДЕЛОМ (137 -- убит после -k): попытка не вердикт о пакетах, её повторяют.
    [ "$rc" -eq 124 ] || [ "$rc" -eq 137 ] && echo "ci-apt-install: СНЯТ ПРЕДЕЛОМ ${LIMIT}с" >&2
    return "$rc"
}

attempt=1
while [ "$attempt" -le "$MAX" ]; do
    echo "ci-apt-install: попытка $attempt из $MAX (предел $LIMIT с): $PKGS"
    one_attempt "$@"
    rc=$?
    if [ "$rc" -eq 0 ]; then
        echo "ci-apt-install ok: попытка $attempt"
        exit 0
    fi
    echo "ci-apt-install: попытка $attempt не удалась (rc=$rc)" >&2
    $SUDO "$DPKG" --configure -a >/dev/null 2>&1 || true
    attempt=$((attempt + 1))
    [ "$attempt" -le "$MAX" ] && sleep "$PAUSE"
done
echo "ci-apt-install: FAIL — $MAX попытки исчерпаны: $PKGS" >&2
exit 1
