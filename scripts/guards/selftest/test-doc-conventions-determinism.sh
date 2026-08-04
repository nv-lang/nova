#!/bin/sh
# Селфтест детерминизма стража doc-conventions (№321).
# Отдельный быстрый скрипт: основной селфтест и без того идёт минуты, а эта
# проверка обязана быть дешёвой, чтобы её гоняли на каждом гейте.
# Суть: три прогона стража по НЕИЗМЕННОМУ дереву обязаны дать одинаковый
# ответ. Прежняя схема сравнивала код-блоки через mktemp (по два файла на
# пару) и на Windows/MSYS плавала — страж краснел на чистом дереве примерно
# в трети прогонов, то есть симметрично мог и пропустить настоящую поломку.
export LC_ALL=C
set -u
GUARD="$(dirname "$0")/../check-doc-conventions.sh"
BASE="$(dirname "$0")/../doc-conventions.baseline"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT
mkdir -p "$TMP/docs/guide" "$TMP/docs/plans" "$TMP/spec" "$TMP/scripts/guards"
cp "$GUARD" "$TMP/scripts/guards/"
cp "$BASE" "$TMP/scripts/guards/"
# пара с идентичными код-блоками + пара с расхождением: обе стороны метрики
printf '# Pair\n\n```\ncode\n```\n' > "$TMP/docs/guide/same.md"
printf '# Пара\n\n```\ncode\n```\n' > "$TMP/docs/guide/same.ru.md"
printf '# Other\n\n```\ncode-en\n```\n' > "$TMP/docs/guide/diff.md"
printf '# Другая\n\n```\ncode-ru\n```\n' > "$TMP/docs/guide/diff.ru.md"
first=""
i=0
while [ "$i" -lt 3 ]; do
    out=$(sh "$TMP/scripts/guards/check-doc-conventions.sh" "$TMP" 2>&1 \
          | grep -oE 'code_block_mismatch_pairs=[0-9]+' | head -1)
    [ -z "$first" ] && first="$out"
    if [ "$out" != "$first" ]; then
        echo "SELFTEST FAIL (детерминизм): прогон дал '$out', первый — '$first'" >&2
        exit 1
    fi
    i=$((i + 1))
done
echo "selftest determinism: OK (3 прогона по неизменному дереву совпали: $first)"
