#!/usr/bin/env bash
# scripts/tools/sync-guards-to-packages.sh — раздать общепроектные стражи в
# пакетные репозитории и проверить, что копии не разошлись.
#
# ЗАЧЕМ (напоминание владельца 2026-08-08): «не забывай, что у нас много реп,
# пакеты, например, живут в отдельных репах». Норма об инвариантах объявлена
# ВСЕОБЪЕМЛЮЩЕЙ — значит и энфорс обязан работать в `nova-tls`, `nova-http`,
# `nova-polaris`, `nova-compress`, `nova-socks`, `nova-bignum`, а не только в
# `nova`. Гейт `nova` их не видит: у каждой репы свой гейт и свой pre-commit.
#
# ПОЧЕМУ КОПИЯ, А НЕ ССЫЛКА. Пакетная репа обязана собираться и проверяться
# САМА ПО СЕБЕ — на CI соседних каталогов нет (тот же урок, что и №444: путь за
# границу репозитория нерезолвим на чистом клоне). Значит копия. А раз копия —
# обязателен контроль расхождения, иначе через месяц у шести реп будет шесть
# разных стражей.
#
# РЕЖИМЫ:
#   bash scripts/tools/sync-guards-to-packages.sh          # проверить (не менять)
#   bash scripts/tools/sync-guards-to-packages.sh --write  # раздать/обновить
#
# КОДЫ: 0 — копии совпадают с эталоном; 1 — расхождение либо отсутствие
# (в режиме проверки), 0 — после успешной раздачи (в режиме --write).

set -u
export LC_ALL=C

NOVA="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
SIBLINGS_DIR="$(dirname "$NOVA")"
WRITE=0
[ "${1:-}" = "--write" ] && WRITE=1

# Общепроектные стражи: имя в scripts/guards/ + его селфтест.
GUARDS="check-invariant-discipline.sh"

PACKAGES="nova-tls nova-http nova-polaris nova-compress nova-socks nova-bignum"

DRIFT=0
MISSING=0
SYNCED=0
# №767: СКОЛЬКО РЕП ДЕЙСТВИТЕЛЬНО СРАВНИЛИ. Без этого счётчика итог
# говорил «копии совпадают с эталоном» и тогда, когда не открыл ни одного файла:
# на CI ни одной пакетной репы нет, все уходят в «пропуск», DRIFT и MISSING
# остаются нулями. Гейт видел зелёный шаг, утверждающий факт о файлах,
# которых не читал (Г15).
COMPARED=0

for pkg in $PACKAGES; do
    dst="$SIBLINGS_DIR/$pkg"
    [ -d "$dst/.git" ] || { echo "  $pkg: репы нет локально — пропуск"; continue; }
    COMPARED=$((COMPARED + 1))

    for g in $GUARDS; do
        src="$NOVA/scripts/guards/$g"
        st_src="$NOVA/scripts/guards/selftest/test-${g%.sh}.sh"
        [ -f "$src" ] || { echo "  ЭТАЛОН ОТСУТСТВУЕТ: $src" >&2; exit 1; }

        mkdir -p "$dst/scripts/guards/selftest" 2>/dev/null

        for pair in "$src:$dst/scripts/guards/$g" \
                    "$st_src:$dst/scripts/guards/selftest/test-${g%.sh}.sh"; do
            from="${pair%%:*}"; to="${pair#*:}"
            [ -f "$from" ] || continue
            if [ ! -f "$to" ]; then
                if [ "$WRITE" -eq 1 ]; then
                    cp "$from" "$to"; SYNCED=$((SYNCED+1))
                    echo "  $pkg: РАЗДАН $(basename "$to")"
                else
                    echo "  $pkg: НЕТ $(basename "$to")" >&2; MISSING=$((MISSING+1))
                fi
                continue
            fi
            if ! cmp -s "$from" "$to"; then
                if [ "$WRITE" -eq 1 ]; then
                    cp "$from" "$to"; SYNCED=$((SYNCED+1))
                    echo "  $pkg: ОБНОВЛЁН $(basename "$to")"
                else
                    echo "  $pkg: РАСХОЖДЕНИЕ $(basename "$to")" >&2; DRIFT=$((DRIFT+1))
                fi
            fi
        done
    done
done

if [ "$WRITE" -eq 1 ]; then
    echo "sync-guards: роздано/обновлено файлов: $SYNCED"
    echo "sync-guards: подключить в каждом пакете — строка в scripts/githooks/pre-commit:"
    echo "    bash \"\$(git rev-parse --show-toplevel)/scripts/guards/check-invariant-discipline.sh\" \\"
    echo "         \"\$(git rev-parse --show-toplevel)\" HEAD || exit 1"
    exit 0
fi

if [ "$DRIFT" -gt 0 ] || [ "$MISSING" -gt 0 ]; then
    echo "sync-guards: FAIL — расхождений $DRIFT, отсутствует $MISSING" >&2
    echo "sync-guards: раздать эталон — bash scripts/tools/sync-guards-to-packages.sh --write" >&2
    exit 1
fi
if [ "$COMPARED" -eq 0 ]; then
    # НЕ ОТКАЗ: шести соседних реп на раннере и не бывает, а красный шаг
    # каждый прогон учит его не читать. ГГ ГОВОРИТ ПРАВДУ ВСЛУХ — тогда
    # видно, что здесь проверки не было, и что её надо где-то сделать.
    echo "sync-guards: ПРОВЕРЯТЬ БЫЛО НЕЧЕГО — ни одной пакетной репы рядом (сравнено 0 из $(printf '%s\n' $PACKAGES | grep -c .))"
    echo "sync-guards: это НЕ подтверждение совпадения копий; настоящая сверка идёт там, где репы склонированы"
    exit 0
fi
echo "sync-guards ok: копии стражей в пакетных репах совпадают с эталоном (сравнено реп: $COMPARED)"
exit 0
