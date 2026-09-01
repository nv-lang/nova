#!/bin/sh
# scripts/guards/check-spec-retired-names.sh — спека не смеет НАРАЩИВАТЬ упоминания
# имени, которое сама же сняла (реестр №845 в docs/plans/221.1-bug-sweep.md).
#
# ЗАЧЕМ. Класс «спека позади собственной реализации» подтверждён ЧАСТОТОЙ, а не
# одним наблюдением: пять случаев за вечер 2026-08-30 (№830, №840, №841, №842,
# №844). Худший из них — D55 требовал `static with_capacity(n int) -> Self`,
# снятый амендментом D372 от 2026-07-06: тип, написанный ПО СПЕКЕ, реализовал бы
# несуществующий контракт и получил CC-FAIL на ровном месте. Нашлось это тем,
# что окно скопировало строку спеки в свой план как факт о реализации.
#
# ПОЧЕМУ ХРАПОВИК, А НЕ ЗАПРЕТ. Первый живой прогон (2026-08-30) дал 58 попаданий
# на одно имя: снятая форма расползлась по спеке за месяцы до того, как её сняли,
# и половина мест — законная история, амендменты и цитаты из Rust. Страж-запрет
# тут краснел бы на ВСЁМ, что написано до него, и был бы снят первым же окном,
# которое его встретит, — а вместе с ним перестали бы ловиться настоящие новые
# случаи. Поэтому предмет надзора — не «ноль упоминаний», а «не БОЛЬШЕ, чем
# вчера»: база по файлам, краснота на РОСТ, движение вниз — молча.
#
# ЧТО СЧИТАЕТСЯ. Число в базе — УПОМИНАНИЯ, а не строки. Счёт по строкам
# давал дыру, найденную самотестом 2026-08-30: второе требование, дописанное
# в конец СУЩЕСТВУЮЩЕЙ строки, не меняло числа и проходило мимо храповика.
#
# БАЗА ХОДИТ ТОЛЬКО ВНИЗ, и опускается ТЕМ ЖЕ слиянием, что снимает упоминание,
# строкой-летописью в самой базе. Оставленная высокой база разрешает откатиться
# ровно на столько, на сколько ты продвинулся, и никто не заметит.
#
# ПОЧЕМУ СПИСКОМ ИМЁН, А НЕ ГРЕПОМ ПО ПРОЗЕ. Замер 2026-08-30: регэксп
# «removed|ex-|RETRACTED + имя в бэктиках» по `std/src` и `compiler-codegen/src`
# даёт 33 имени, и половина — мусор (`char`, `int`, `usize`, `Self`, `span`).
# Список КУРИРУЕТСЯ, и каждая строка несёт, ЧЕМ имя снято, — иначе это не факт,
# а подозрение. Что в список НЕ кладётся — сказано в шапке самого списка.
#
# ЧТО НЕ СЧИТАЕТСЯ УПОМИНАНИЕМ. Спека обязана иметь право ГОВОРИТЬ о снятом —
# иначе нельзя записать саму ретракцию. Строка не считается, если зачёркнута
# (`~~`) либо несёт слово ретракции: АМЕНДМЕНТ, СНЯТ, РЕТРАКТ, отвергнут,
# removed, ex-`, RETRACTED.
#
# ПОЧЕМУ `history/` НЕ СУДИТСЯ. Каталог `spec/decisions/history/` — ДОМ снятых
# имён: там записано, что и почему отвергнуто. Каждая будущая ретракция ЗАКОННО
# добавляет туда упоминание — то есть страж, судящий `history/`, краснел бы на
# ПРАВИЛЬНОЙ работе и учил бы её не делать. Замер 2026-08-30: четыре из 57
# попаданий были ровно такими записями в `rejected.md`/`evolution.md`.
#
# Самотест: scripts/guards/selftest/test-check-spec-retired-names.sh
export LC_ALL=C
ROOT="${1:-$(cd "$(dirname "$0")/../.." && pwd)}"
GUARDS_DIR=$(cd "$(dirname "$0")" && pwd)
LIST="${NOVA_RETIRED_NAMES:-$GUARDS_DIR/spec-retired-names.list}"
BASE="${NOVA_RETIRED_BASELINE:-$GUARDS_DIR/spec-retired-names.baseline}"
SPEC_REL="${NOVA_SPEC_REL:-spec/decisions}"

[ -f "$LIST" ] || {
    echo "check-spec-retired-names: FAIL — нет списка $LIST: пустой вход = тихо снятая проверка" >&2
    exit 1
}
[ -f "$BASE" ] || [ "${NOVA_RETIRED_EMIT:-}" = "1" ] || {
    echo "check-spec-retired-names: FAIL — нет базы $BASE: без неё храповик не храповик" >&2
    exit 1
}
[ -d "$ROOT/$SPEC_REL" ] || {
    echo "check-spec-retired-names: FAIL — нет каталога решений $ROOT/$SPEC_REL" >&2
    exit 1
}

TMPD="${TMPDIR:-/tmp}/spec-retired.$$"
mkdir -p "$TMPD" || exit 1
trap 'rm -rf "$TMPD"' 0 2 15

# --- имена из списка: каждое обязано нести объяснение после «#» -------------
rc=0
NAMES=0
: > "$TMPD/names"
while IFS= read -r line; do
    case "$line" in ''|\#*) continue ;; esac
    name=${line%%#*}
    why=${line#*#}
    name=$(printf '%s' "$name" | sed 's/[[:space:]]*$//')
    [ -n "$name" ] || continue
    if [ "$why" = "$line" ] || [ -z "$(printf '%s' "$why" | tr -d '[:space:]')" ]; then
        echo "check-spec-retired-names: FAIL — имя «$name» без объяснения после «#»: чем оно снято? Без этого строка — подозрение, а не факт" >&2
        rc=1
        continue
    fi
    NAMES=$((NAMES + 1))
    printf '%s\n' "$name" >> "$TMPD/names"
done < "$LIST"
[ "$rc" -eq 0 ] || exit 1
[ "$NAMES" -gt 0 ] || {
    echo "check-spec-retired-names: FAIL — список пуст: проверка снята бы молча" >&2
    exit 1
}

# --- ОДИН проход грепа по всем именам сразу (замер: 58 попаданий за ~2с
#     против 17.6с на грепе-в-цикле) ---------------------------------------
PAT=$(tr '\n' '|' < "$TMPD/names" | sed 's/|$//')
( cd "$ROOT" && grep -rnE --exclude-dir=history -- "$PAT" "$SPEC_REL" 2>/dev/null ) > "$TMPD/raw" || true

# --- отсеять законные упоминания и посчитать по файлам ----------------------
awk -F: -v PATRE="$PAT" '
{
    file = $1
    text = $0
    sub(/^[^:]*:[0-9]+:/, "", text)
    if (text ~ /~~/) next
    if (text ~ /RETRACTED|removed|ex-`/) next
    next_ru = 0
    if (index(text, "АМЕНДМЕНТ")) next_ru = 1
    if (index(text, "СНЯТ")) next_ru = 1
    if (index(text, "снят")) next_ru = 1
    if (index(text, "РЕТРАКТ")) next_ru = 1
    if (index(text, "ретракт")) next_ru = 1
    if (index(text, "отвергнут")) next_ru = 1
    if (next_ru) next
    probe = text
    hits = gsub(PATRE, "&", probe)
    if (hits < 1) hits = 1
    cnt[file] += hits
    if (!(file in first)) first[file] = ""
    lines[file] = lines[file] "\n    " $1 ":" $2 " " substr(text, 1, 96)
}
END { for (f in cnt) printf "%s %d%s\n", f, cnt[f], lines[f] }
' "$TMPD/raw" > "$TMPD/percounts"

grep -E '^[^ ]+ [0-9]+$' "$TMPD/percounts" | sort > "$TMPD/now"

# NOVA_RETIRED_EMIT=1 -- print the per-file counts and stop. This is HOW THE
# BASELINE IS REGENERATED; it deliberately prints nothing else, so the output
# can be appended under the chronicle line by hand.
if [ "${NOVA_RETIRED_EMIT:-}" = "1" ]; then
    cat "$TMPD/now"
    exit 0
fi

# --- база -------------------------------------------------------------------
grep -E '^[^#][^ ]* [0-9]+$' "$BASE" | sort > "$TMPD/base"

TOTAL_NOW=$(awk '{s+=$2} END {print s+0}' "$TMPD/now")
TOTAL_BASE=$(awk '{s+=$2} END {print s+0}' "$TMPD/base")

while IFS=' ' read -r f n; do
    [ -n "$f" ] || continue
    b=$(awk -v k="$f" '$1==k {print $2}' "$TMPD/base")
    [ -n "$b" ] || b=0
    if [ "$n" -gt "$b" ]; then
        echo "check-spec-retired-names: FAIL — в «$f» упоминаний снятого имени стало $n, в базе $b." >&2
        awk -v k="$f" '$1 == k {found=1; next} /^[^ ]+ [0-9]+$/ {found=0} found {print}' "$TMPD/percounts" >&2
        echo "    Снятое имя требуется как живое. Либо назови в строке, что оно снято (АМЕНДМЕНТ/СНЯТ/removed/ex-\`), либо зачеркни её (~~), либо перепиши на живую форму." >&2
        echo "    Чем снято — сказано в $(basename "$LIST")." >&2
        rc=1
    fi
done < "$TMPD/now"

if [ "$rc" -eq 0 ]; then
    if [ "$TOTAL_NOW" -lt "$TOTAL_BASE" ]; then
        echo "check-spec-retired-names ok: имён под надзором $NAMES, упоминаний $TOTAL_NOW (база $TOTAL_BASE) — храповик пора опустить ТЕМ ЖЕ слиянием, строкой-летописью в $(basename "$BASE")"
    else
        echo "check-spec-retired-names ok: имён под надзором $NAMES, упоминаний $TOTAL_NOW при базе $TOTAL_BASE — роста нет"
    fi
fi
exit "$rc"
