#!/usr/bin/env bash
# scripts/guards/check-commit-language.sh — сообщения коммитов пишутся
# по-английски.
#
# Норма: docs/dev/dev-workflow.md §7 (стиль); реестр 221.1 №518 (дефект
# самого этого стража: байтовый диапазон ловил тире как кириллицу).
#
# ЗАЧЕМ. Решение владельца 2026-08-09: репозиторий публичный и зеркалится на три
# площадки (github/gitverse/sourcecraft), историю читают снаружи. Прежняя норма
# требовала русского (`docs/dev/dev-workflow.md`), и практика ей следовала: 194
# из 200 последних коммитов кириллицей. Норма сменилась — значит нужен страж,
# иначе она проживёт ровно до первого забывшего.
#
# ГРАНИЦА. Проверяются ТОЛЬКО коммиты после точки перехода: история не
# переписывается, и старые русские сообщения законны. Точка перехода — первый
# коммит, где правило записано; она берётся из файла ниже, а не из памяти.
#
# ЧТО СЧИТАЕТСЯ НАРУШЕНИЕМ. Кириллица в теме или теле коммита. Исключение —
# merge-коммиты, чью тему пишет git, и цитаты: строка, начинающаяся с `>` или
# с четырёх пробелов (цитата чужого текста, например реплики владельца, —
# законна, её перевод исказил бы источник).
#
# ИСПОЛЬЗОВАНИЕ:
#   bash scripts/guards/check-commit-language.sh [<корень>] [<база>]
# База по умолчанию — точка перехода. Самопроверка:
#   bash scripts/guards/check-commit-language.sh --selftest
set -u
export LC_ALL=C

CUTOVER_FILE_REL="scripts/guards/commit-language.cutover"

# --------------------------------------------------------------------------
if [ "${1:-}" = "--selftest" ]; then
    SELF="$0"
    fails=0
    TMP=$(mktemp -d) || exit 1
    trap 'rm -rf "$TMP"' EXIT

    git -C "$TMP" init -q 2>/dev/null
    git -C "$TMP" config user.email t@t && git -C "$TMP" config user.name t
    mkdir -p "$TMP/scripts/guards"
    : > "$TMP/f"
    git -C "$TMP" add f >/dev/null 2>&1
    git -C "$TMP" commit -q -m "base commit in english" 2>/dev/null
    BASE=$(git -C "$TMP" rev-parse HEAD)
    echo "$BASE" > "$TMP/$CUTOVER_FILE_REL"

    # 1. Английский коммит после точки перехода — ПРОХОДИТ.
    echo a >> "$TMP/f"; git -C "$TMP" add f >/dev/null 2>&1
    git -C "$TMP" commit -q -m "add a thing in english" 2>/dev/null
    if ! bash "$SELF" "$TMP" >/dev/null 2>&1; then
        echo "selftest FAIL: английский коммит отвергнут" >&2
        fails=$((fails + 1))
    fi

    # 2. Русский коммит после точки перехода — ОТВЕРГАЕТСЯ.
    echo b >> "$TMP/f"; git -C "$TMP" add f >/dev/null 2>&1
    git -C "$TMP" commit -q -m "добавил ещё одну вещь" 2>/dev/null
    if bash "$SELF" "$TMP" >/dev/null 2>&1; then
        echo "selftest FAIL: русский коммит пропущен" >&2
        fails=$((fails + 1))
    fi

    # 3. Цитата в теле — НЕ нарушение (перевод исказил бы источник).
    git -C "$TMP" reset -q --hard HEAD~1
    echo c >> "$TMP/f"; git -C "$TMP" add f >/dev/null 2>&1
    git -C "$TMP" commit -q -m "quote the owner verbatim

The owner asked for this:

> добавь страж" 2>/dev/null
    if ! bash "$SELF" "$TMP" >/dev/null 2>&1; then
        echo "selftest FAIL: цитата под '>' сочтена нарушением" >&2
        fails=$((fails + 1))
    fi

    # 4. Английское сообщение С ДЛИННЫМ ТИРЕ — ПРОХОДИТ. Регрессия на реальный
    #    промах: байтовый диапазон [А-Яа-я] под LC_ALL=C ловил тире как
    #    кириллицу, и страж краснел на собственных английских коммитах.
    git -C "$TMP" reset -q --hard "$BASE"
    echo e >> "$TMP/f"; git -C "$TMP" add f >/dev/null 2>&1
    git -C "$TMP" commit -q -m "english subject with an em-dash — like this

Body also has a dash — and a quote character 'x'." 2>/dev/null
    if ! bash "$SELF" "$TMP" >/dev/null 2>&1; then
        echo "selftest FAIL: английское сообщение с тире сочтено кириллицей" >&2
        fails=$((fails + 1))
    fi

    # 5. Коммиты ДО точки перехода не проверяются вовсе.
    git -C "$TMP" reset -q --hard "$BASE"
    echo d >> "$TMP/f"; git -C "$TMP" add f >/dev/null 2>&1
    git -C "$TMP" commit -q -m "русский до перехода" 2>/dev/null
    NEWBASE=$(git -C "$TMP" rev-parse HEAD)
    echo "$NEWBASE" > "$TMP/$CUTOVER_FILE_REL"
    if ! bash "$SELF" "$TMP" >/dev/null 2>&1; then
        echo "selftest FAIL: коммит ДО точки перехода проверен" >&2
        fails=$((fails + 1))
    fi

    if [ "$fails" -eq 0 ]; then
        echo "check-commit-language selftest: OK (5 проверок)"
        exit 0
    fi
    echo "check-commit-language selftest: ПРОВАЛ, отказов $fails" >&2
    exit 1
fi

# --------------------------------------------------------------------------
ROOT="${1:-$(pwd)}"
CUTOVER_FILE="$ROOT/$CUTOVER_FILE_REL"

if [ ! -f "$CUTOVER_FILE" ]; then
    echo "check-commit-language: нет файла точки перехода $CUTOVER_FILE" >&2
    echo "  Он и есть граница: без него страж не знает, где кончается законная" >&2
    echo "  русская история и начинается норма 2026-08-09." >&2
    exit 1
fi
BASE="${2:-$(head -1 "$CUTOVER_FILE" | tr -d '[:space:]')}"

# Именованные исключения: `<sha> <причина>` по строке. Точка перехода
# объявлена окончательной, поэтому единичный коммит, пришедший слиянием
# после неё, закрывается ЗДЕСЬ — с причиной, а не сдвигом границы.
EXEMPT_FILE="$ROOT/scripts/guards/commit-language.exempt"

git -C "$ROOT" rev-parse --verify -q "$BASE" >/dev/null 2>&1 || {
    echo "check-commit-language: точка перехода $BASE не найдена в репозитории" >&2
    exit 1
}

# Норма применяется по ДАТЕ АВТОРСТВА, а не по достижимости от точки перехода.
# Иначе ветка, начатая ДО нормы и влитая ПОСЛЕ, краснит гейт задним числом —
# и единственным выходом становится переписывание чужой истории. Дата — это
# «когда правило действовало», достижимость — «когда ветку влили»; правило
# привязано к первому.
CUT_TS=$(git -C "$ROOT" log -1 --format='%at' "$BASE")

# ИНКРЕМЕНТАЛЬНОСТЬ (2026-08-12, долг из шапки шага гейта: «сделать страж
# инкрементальным, а не поднимать срок в третий раз»).
#
# Логика проверки НЕ меняется ни на букву — меняется только ДИАПАЗОН: уже
# проверенные коммиты не проверяются заново. Отметка лежит в `.git/` и НЕ
# версионируется: это локальное состояние прогона, а не факт о проекте.
# Если отметка не является предком HEAD (историю переписали, ветку сменили) —
# отметка игнорируется и диапазон берётся полный. Порча кэша может сделать
# прогон медленнее, но НЕ может сделать его слепым.
GIT_DIR=$(git -C "$ROOT" rev-parse --git-dir 2>/dev/null)
VERIFIED_FILE="${NOVA_COMMIT_LANG_VERIFIED:-$GIT_DIR/nova-commit-language-verified}"
SCAN_FROM="$BASE"
if [ -f "$VERIFIED_FILE" ]; then
    LAST=$(head -1 "$VERIFIED_FILE" | tr -d '[:space:]')
    if [ -n "$LAST" ]        && git -C "$ROOT" rev-parse --verify --quiet "$LAST^{commit}" >/dev/null 2>&1        && git -C "$ROOT" merge-base --is-ancestor "$LAST" HEAD 2>/dev/null        && git -C "$ROOT" merge-base --is-ancestor "$BASE" "$LAST" 2>/dev/null; then
        SCAN_FROM="$LAST"
    fi
fi

BAD=""
for sha in $(git -C "$ROOT" rev-list "$SCAN_FROM..HEAD" --no-merges 2>/dev/null); do
    ats=$(git -C "$ROOT" log -1 --format='%at' "$sha")
    [ "$ats" -ge "$CUT_TS" ] 2>/dev/null || continue
    if [ -n "$EXEMPT_FILE" ] && [ -f "$EXEMPT_FILE" ] \
       && grep -q "^$sha " "$EXEMPT_FILE" 2>/dev/null; then
        continue
    fi
    msg=$(git -C "$ROOT" log -1 --format='%B' "$sha")
    # Цитаты и отступ-блоки исключаем: чужой текст переводить нельзя.
    # Плюс ВСТРОЕННЫЕ цитаты — текст в обратных апострофах и одинарных
    # кавычках: английское сообщение, описывающее русский литерал (имя
    # маркера, форму разметки, текст диагностики), обязано его процитировать,
    # иначе оно говорит не о том, о чём говорит.
    stripped=$(printf '%s\n' "$msg" | grep -v '^>' | grep -v '^    ' \
        | sed "s/\`[^\`]*\`//g" | sed "s/'[^']*'//g")
    # ВАЖНО: класс Юникода, а НЕ байтовый диапазон `[А-Яа-я]`. Под LC_ALL=C
    # диапазон сравнивает БАЙТЫ, и длинное тире `—` (U+2014, байты E2 80 94)
    # попадает в те же границы — английское сообщение с тире объявлялось
    # кириллицей. Поймано 2026-08-09 на двух собственных коммитах интегратора.
    # Префикс `(*UTF8)` обязателен: страж работает под LC_ALL=C, и без него
    # PCRE не включает UTF-8 — класс молча не находит НИЧЕГО, то есть страж
    # пропускает всё. Ровно это и показала самопроверка сразу после правки.
    if printf '%s' "$stripped" | grep -Pq '(*UTF8)\p{Cyrillic}'; then
        subj=$(git -C "$ROOT" log -1 --format='%s' "$sha")
        BAD="$BAD
  ${sha%${sha#?????????}}  $subj"
    fi
done

if [ -n "$BAD" ]; then
    echo "check-commit-language: FAIL — кириллица в сообщениях коммитов" >&2
    echo "$BAD" >&2
    echo "" >&2
    echo "  Норма с 2026-08-09 (решение владельца): сообщения коммитов —" >&2
    echo "  по-английски. Репозиторий публичный и зеркалится на три площадки," >&2
    echo "  историю читают снаружи. Правило — docs/dev/dev-workflow.md." >&2
    echo "  Цитату чужого текста можно оставить как есть: строка с '>' в начале" >&2
    echo "  не проверяется." >&2
    exit 1
fi

# Отметка ставится ТОЛЬКО на зелёном исходе: красный прогон не должен
# «проглатывать» непроверенные коммиты.
git -C "$ROOT" rev-parse HEAD > "$VERIFIED_FILE" 2>/dev/null || true
echo "check-commit-language ok: кириллицы в сообщениях после точки перехода нет"
exit 0
