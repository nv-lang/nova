#!/bin/sh
# scripts/guards/check-novac-selftest-proves-red.sh — самотест обязан
# ДОКАЗЫВАТЬ, что его страж ловит (конвенция П16, требование владельца
# 2026-08-16: «страж обязан доказать, что ловит то, что требуется»).
#
# ЗАЧЕМ ИМЕННО МУТАЦИЯ. Самотест, в котором есть только зелёная сторона
# («чистый вход не краснеет»), остаётся зелёным и над стражем, который
# вообще ничего не делает. Заявить «у меня есть красный случай» может любой
# файл; доказать — только опыт. Поэтому проверка одна: подменяем стража
# заглушкой (`#!/bin/sh` + `exit 0`) и гоняем его самотест. Самотест обязан
# упасть. Прошёл над заглушкой — значит он не судит стража, а рапортует.
#
# ПРОВЕРЯЕТ: для каждого scripts/guards/check-novac-*.sh, у которого есть
#   scripts/guards/selftest/test-<имя>.sh, — что самотест НАД ЗАГЛУШКОЙ
#   возвращает ненулевой код. Печатает числом, сколько доказали.
# НЕ ПРОВЕРЯЕТ: качество зелёной стороны (ложняки — дело самого самотеста и
#   приёмки); стражей вне семьи novac (у них своя норма 254 и свой реестр);
#   групповые самотесты без одноимённого стража (test-check-novac-binary-guards
#   судит четвёрку сразу) — они считаются отдельным числом и не краснеют.
#
# БЕЗОПАСНОСТЬ. Подмена делается по одному файлу за раз, оригинал кладётся
# рядом (<страж>.proving-backup) и возвращается сразу же; trap возвращает
# всё при любом выходе, включая Ctrl-C. В конце страж СВЕРЯЕТ контрольные
# суммы всех тронутых файлов с исходными — расхождение само по себе красное.
#
# $1 — корень репозитория.
# $2 — override каталога стражей (шов самотеста; самотесты берутся из $2/selftest).
# env NOVAC_PROVE=0 — пропустить (дешёвая выборка; гейт гоняет всегда).
# env NOVAC_PROVE_DEADLINE — секунд на один самотест (по умолчанию 150).
# Проверялся: Windows (Git Bash), 2026-08-16.
export LC_ALL=C
ROOT="${1:-$(cd "$(dirname "$0")/../.." && pwd)}"
GUARDS="${2:-$ROOT/scripts/guards}"
SELF="$GUARDS/selftest"
NAME=check-novac-selftest-proves-red
DEADLINE="${NOVAC_PROVE_DEADLINE:-150}"

if [ "${NOVAC_PROVE:-1}" = "0" ]; then
    echo "$NAME ok: пропущено по NOVAC_PROVE=0 (гейт гоняет полностью)"
    exit 0
fi
if [ ! -d "$GUARDS" ]; then
    echo "$NAME ok: судить нечего (нет $GUARDS)"
    exit 0
fi

T="${TMPDIR:-/tmp}/novac-prove.$$"
mkdir -p "$T" || exit 1
restore() {
    for b in "$GUARDS"/*.proving-backup; do
        [ -f "$b" ] || continue
        mv "$b" "${b%.proving-backup}"
    done
    rm -rf "$T"
}
trap restore 0 INT TERM

PROVED=0
BLIND=""
GROUPED=0
NOSELF=""

for g in "$GUARDS"/check-novac-*.sh; do
    [ -f "$g" ] || continue
    base=$(basename "$g" .sh)
    st="$SELF/test-$base.sh"
    if [ ! -f "$st" ]; then
        NOSELF="$NOSELF $base"
        continue
    fi
    # свой собственный файл судим тоже: процесс уже прочитан, подмена
    # на диске на текущий прогон не влияет (П16 п.2 — правило без исключений)
    cksum < "$g" > "$T/before.$base"
    cp "$g" "$g.proving-backup"
    printf '#!/bin/sh\nexit 0\n' > "$g"
    ( NOVAC_CORPUS=0 NOVAC_COST=0 NOVAC_PROVE=0 timeout "$DEADLINE" sh "$st" ) > "$T/out" 2>&1
    rc=$?
    mv "$g.proving-backup" "$g"
    cksum < "$g" > "$T/after.$base"
    if ! cmp -s "$T/before.$base" "$T/after.$base"; then
        echo "$NAME: FAIL — $base не восстановлен после подмены (сверка контрольных сумм)" >&2
        exit 1
    fi
    if [ "$rc" -eq 124 ]; then
        echo "$NAME: FAIL — самотест $base не уложился в ${DEADLINE}с под заглушкой: доказательство не получено" >&2
        echo "  подними NOVAC_PROVE_DEADLINE либо разбей самотест на дешёвую и живую половины (П16 п.4)" >&2
        exit 1
    fi
    if [ "$rc" -eq 0 ]; then
        BLIND="$BLIND $base"
    else
        PROVED=$((PROVED + 1))
    fi
done

# групповые самотесты (без одноимённого стража) — считаем отдельно
for st in "$SELF"/test-check-novac-*.sh; do
    [ -f "$st" ] || continue
    b=$(basename "$st" .sh); b=${b#test-}
    [ -f "$GUARDS/$b.sh" ] || GROUPED=$((GROUPED + 1))
done

if [ -n "$BLIND" ]; then
    echo "$NAME: FAIL — самотест проходит над ЗАГЛУШКОЙ, значит ничего не доказывает (П16):" >&2
    for b in $BLIND; do echo "  $b — подмени стража на 'exit 0' и убедись сам: самотест обязан упасть" >&2; done
    echo "  почини: впрысни нарушение в подложку (\$2-шов стража) и ассертируй ненулевой код + имя виновника в stderr" >&2
    exit 1
fi
if [ -n "$NOSELF" ]; then
    echo "$NAME: FAIL — страж без самотеста (П16 п.5: одной волной, не «потом»):" >&2
    for b in $NOSELF; do echo "  $b — нет $SELF/test-$b.sh" >&2; done
    exit 1
fi

echo "$NAME ok: самотестов доказали красноту мутацией: $PROVED; групповых (без одноимённого стража): $GROUPED"
exit 0
