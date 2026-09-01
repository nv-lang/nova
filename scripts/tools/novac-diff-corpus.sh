#!/bin/sh
# scripts/tools/novac-diff-corpus.sh — полнокорпусный дифф-прогон novac
# против оракула (план 274 §9/Э1 ОБВЯЗКА; с Э2 — механизм храповика §10.4).
#
# Прогоняет ОБЕ реализации по корпусу (по умолчанию examples/**/*.nv) и
# классифицирует исходы по коду возврата:
#   совпали-приняли · совпали-отвергли ·
#   subset  (novac отверг, оракул принял) — ожидаемое отставание novac;
#           раскладывается по корзинам §10.4:
#             «вне точки»   — файл ДОБАВЛЕН в git после spec-point И отвергнут
#                             (двухчастный прокси bootstrap §3; спорные — руками);
#             «заблокировано оракулом» — носители [LEGACY-#NNN]/EXPECT_CC_ERROR;
#             остальное     — честное отставание подмножества;
#   DANGER  (novac ПРИНЯЛ, оракул отверг) — класс К7; красный вне allow;
#   PANIC   (274.3/F3, одно определение на все механизмы — lib/novac.sh:
#           код возврата не 0/1 (вердикт) и не 2 (честный отказ двери с
#           сообщением), либо 'panic' в stderr) — всегда красный.
#
# Второе монотонное число (274 §9/Э2): файлы «совпали-приняли», собранные
# ОБОИМИ компиляторами с поведенческим совпадением (exit+stdout байт-в-байт)
# — через novac-e1-smoke.sh; без него регресс кодогена невидим до Э5.
#
# Расстояние до самосборки (§10.4): novac/src/**/*.nv через novac check;
# отвергнутые — отдельное число, в знаменатель храповика НЕ входят.
#
# Шапка прогона несёт ревизию оракула и режим сборки novac (§10.3 — иначе
# классификация невоспроизводима через неделю). Последняя строка — машинная,
# её парсит check-novac-differential.sh для сверки с novac-corpus.baseline.
#
# Usage: sh scripts/tools/novac-diff-corpus.sh [corpus-dir]
# Бюджет: examples (60 файлов) ~2–4 мин под нагрузкой; полный корпус
# (std+nova_tests+spec_tests, ~2.8k) — только кнопкой/ночью (bootstrap §3).
# Проверялся: Windows (Git Bash), 2026-08-14.
export LC_ALL=C
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
. "$ROOT/scripts/guards/lib/novac.sh"   # novac_is_panic_rc (274.3/F3)
CORPUS="${1:-$ROOT/examples}"
NOVAC="$ROOT/novac/target/novac.exe"
ALLOW="$ROOT/novac/divergences.allow"
T="${TMPDIR:-/tmp}/novac-diff-corpus.$$"
mkdir -p "$T"
trap 'rm -rf "$T"' 0

[ -f "$NOVAC" ] || { echo "novac-diff-corpus: нет $NOVAC" >&2; exit 2; }
. "$(CDPATH= cd -- "$(dirname -- "$0")/../guards" && pwd)/lib/novac.sh"
ORACLE="$(novac_find_oracle "$ROOT" || true)"
if [ ! -f "$ORACLE" ]; then
    MAINROOT=$(git -C "$ROOT" rev-parse --path-format=absolute --git-common-dir 2>/dev/null)
    [ -n "$MAINROOT" ] && ORACLE="$MAINROOT/../nova-cli/target/release/nova.exe"
fi
[ -f "$ORACLE" ] || { echo "novac-diff-corpus: оракул не собран" >&2; exit 2; }

# Строго hex-ревизия: в том же файле есть ПРОЗА со словом «oracle-pin:»
# (комментарий-контракт) — свободный якорь цеплял и её.
PIN=$(tr -d '\r' < "$ROOT/novac/nova.toml" | sed -n 's/^#[[:space:]]*oracle-pin:[[:space:]]*\([0-9a-f][0-9a-f]*\)$/\1/p')
SPEC_POINT=$(tr -d '\r' < "$ROOT/novac/nova.toml" | sed -n 's/^#[[:space:]]*spec-point:[[:space:]]*\([0-9-]*\)$/\1/p')
ORACLE_REV=$(git -C "$(dirname "$ORACLE")" rev-parse --short HEAD 2>/dev/null)
# spec-queue (274.3/F13): отставание от спеки — МАШИННОЕ число, а не запись
# рукой в nova.toml. Считается как число коммитов в spec/decisions после
# закреплённой точки; расхождение с toml — красный (план §1.1: «лаг — число,
# не ощущение»).
# СЧИТАЕМ ПО ЛИНИИ MAIN, А НЕ ПО СВОЕМУ ДЕРЕВУ (2026-08-23, класс К8 плана 274
# §9.1д). Прежняя форма считала коммиты в ТЕКУЩЕМ дереве, и число зависело от
# того, кто смотрит: в ветке — 22, в CI (ветка, слитая с main) — 24, при одной и
# той же спеке. Отставание от СПЕКИ — свойство спеки, а не чекаута. Плюс коммит,
# который лишь ПРИНЁС чужие решения (слияние), больше не считается решением:
# `--no-merges`.
# Порядок опроса: `origin/main` — если реф есть (обычный клон); иначе HEAD — в
# PR-чекауте origin/main может не быть, но сам чекаут УЖЕ содержит main.

# СЧИТАЕМ ДО ТОГО МЕСТА MAIN, КОТОРОЕ ВЕТКА УЖЕ СОДЕРЖИТ, а не до вершины
# `origin/main` (правка 2026-08-30 по замеру окна 274, реестр №843).
#
# ЗАЧЕМ. Правило требует, чтобы `novac` ЗНАЛ, что язык сдвинулся. Прежняя
# форма считала до ВЕРШИНЫ main, то есть требовала знать о коммитах, КОТОРЫХ В
# ВЕТКЕ ЕЩЁ НЕТ. Догнать такое число нельзя по построению: оно протухает
# через минуты после чужого пуша. ЗАМЕР ОКНА 274 (2026-08-30): четыре
# обновления строки за вечер (31→32→33→34→35), каждое — прогон яруса novac
# (девять минут) плюс разбор отказа; около сорока минут на бухгалтерию, и ни один
# из четырёх D-коммитов не менял для `novac` НИЧЕГО.
#
# СМЫСЛ ПРАВИЛА СОХРАНЁН ЦЕЛИКОМ: знание фиксируется в момент, когда сдвиг
# РЕАЛЬНО ПРИЕЗЖАЕТ в ветку — то есть при синхронизации, осознанно. Теряется
# только требование быть в курсе чужих пушей, которых у тебя ещё нет.
#
# ПОПРАВКА 2026-08-31 (реестр №846, форма предложена окном 274). Счёт шёл до
# merge-base(HEAD, origin/main), и это освободило ВЕТКИ, но оставило ДЫРУ НА main:
# там origin/main ОТСТАЁТ на ещё не запущенный коммит, то есть ДО пуша
# счётчик требовал одно число, а CI на ТОМ ЖЕ коммите — другое (замер
# 2026-08-30: 35 против 36). Одно дерево — два ответа.
#
# СЧИТАЕМ ДО HEAD, И ЭТО НЕ УПРОЩЕНИЕ, А УСТРАНЕНИЕ ПРИЧИНЫ. Формула
# больше НЕ ССЫЛАЕТСЯ НА УДАЛЁННУЮ ВЕТКУ ВОВСЕ, значит зависеть от того,
# запущено ли уже, она не может ПО ПОСТРОЕНИЮ — это сильнее любого замера.
# СМЫСЛ ПРАВИЛА ЦЕЛ: `git log HEAD -- spec/decisions` считает ровно те D-коммиты,
# КОТОРЫЕ ДЕРЕВО СОДЕРЖИТ. Для ветки это то же послабление, что и раньше
# (чужих невлитых коммитов в её истории нет), ПЛЮС её СОБСТВЕННЫЕ спек-коммиты,
# о которых она точно знает, — то есть строго точнее merge-base.
QUEUE_BASE=HEAD
QUEUE_REAL=$(git -C "$ROOT" log --since="$SPEC_POINT 23:59:59" --no-merges --format=%h "$QUEUE_BASE" -- spec/decisions 2>/dev/null | wc -l | tr -d " ")
QUEUE_TOML=$(tr -d '\r' < "$ROOT/novac/nova.toml" | sed -n 's/^#[[:space:]]*spec-queue:[[:space:]]*\([0-9][0-9]*\)$/\1/p')
echo "novac-diff-corpus: oracle-pin=$PIN oracle-HEAD=$ORACLE_REV spec-point=$SPEC_POINT spec-queue=$QUEUE_REAL (в nova.toml: $QUEUE_TOML) сборка novac=single-file корпус=$CORPUS"
if [ -z "$QUEUE_TOML" ]; then
    echo "novac-diff-corpus: FAIL — строка spec-queue в novac/nova.toml не распознана (строгий формат «#   spec-queue: N», без хвостовых комментариев). Пустой якорь = тихо отключённая проверка (274.3/F13)." >&2
    exit 1
fi
if [ "$QUEUE_REAL" != "$QUEUE_TOML" ]; then
    echo "novac-diff-corpus: FAIL — spec-queue в novac/nova.toml ($QUEUE_TOML) расходится с фактом ($QUEUE_REAL D-коммитов в spec/decisions после $SPEC_POINT). Обнови строку или двигай spec-point сознательно (274.3/F13, план §1.1)." >&2
    exit 1
fi

find "$CORPUS" -type f -name '*.nv' | sort > "$T/list"
N=$(wc -l < "$T/list" | tr -d ' ')
[ "$N" -gt 0 ] || { echo "novac-diff-corpus: корпус пуст" >&2; exit 2; }

acc=0; rej=0; subset=0; outpoint=0; blocked=0; danger=0; panic=0; allowed=0
t_novac=0; t_oracle=0
wall0=$(date +%s%N)
# Bucket «вне точки» needs each file's git ADD date. One bulk `git log
# --name-only` over the corpus (0.35s) instead of `--follow` per file
# (0.46s × ~50 files = 25s of the old 6-minute run — plan 274.2 §3.1).
# Format: lines of "<date>" then the paths added by that commit; we keep
# the OLDEST date per path (files listed under several commits keep the
# first-seen = most recent in log order, so process reversed).
git -C "$ROOT" log --diff-filter=A --format='%as' --name-only -- "$CORPUS" 2>/dev/null     | awk 'NF==0{next} /^[0-9]{4}-[0-9]{2}-[0-9]{2}$/{d=$0; next} {print $0"	"d}'     | tac | awk -F'	' '!seen[$1]++' > "$T/added_dates" 2>/dev/null || : > "$T/added_dates"
# --- ПАЧКА: один процесс novac на весь корпус -----------------------------
# Замер 2026-08-17: чистый старт novac 149 мс против ~50 мс работы на файле,
# то есть три четверти цены пофайлового прохода — это запуск. Шестьдесят один
# файл = девять секунд стартов. Пачкой платится один.
#
# Вердикт по файлу берётся из диагностик: каждая несёт своё поле `file`
# (схема §7), поэтому файл с диагностикой = отвергнут, без неё = принят.
# Если пачка УМЕРЛА (код вне 0/1/2 или слово panic в выводе), вердикты по ней
# недостоверны — откатываемся на пофайловый проход и ничего не выдумываем.
NOVAC_BATCH=1
: > "$T/novac_rejected"
s=$(date +%s%N)
# OTNOSITELNYE puti: MSYS converts an absolute /d/... into D:\... at the
# exec boundary, and novac prints in its diagnostic the path it RECEIVED, so
# the strings stop matching our list (caught 2026-08-17: the batch counted 56
# refusals and the loop found none). A relative path crosses the boundary
# unchanged, so the verdict matches the same `rel` string the loop computes.
sed "s|^$ROOT/||" "$T/list" > "$T/list.rel"
eval "cd \"$ROOT\" && timeout 300 \"$NOVAC\" check $(sed 's/^/"/;s/$/"/' "$T/list.rel" | tr '\n' ' ')" \
    > "$T/batch.out" 2> "$T/batch.err" </dev/null
brc=$?
t_novac=$(( ( $(date +%s%N) - s ) / 1000000 ))
# ДЫРА, ЗАКРЫТАЯ 2026-08-22 (волна В8): пачка, УМЕРШАЯ на середине, читалась как
# «все остальные файлы приняты». Замер: novac упал на снятом инварианте после
# третьего файла из 61, вышел с кодом 2 — а условие смотрело `> 2` и слово
# `panic`, которого в тексте ICE нет («internal compiler error»). Итог: раннер
# насчитал 10 ЛОЖНЫХ «поведение разошлось», и ни одно из них не было правдой —
# правдой было, что вердиктов вообще нет. Ложно-зелёное здесь ещё хуже: если бы
# файлы, которые novac ОТВЕРГАЕТ, были в корпусе «приняли-приняли», гейт бы
# позеленел на мёртвой пачке.
#
# Признак смерти пачки теперь ТРИ, и все три об одном: код выхода не 0/1
# (novac отвечает 1 «есть диагностики»), слово `panic`, и маркер ICE. Любой из
# них — откат на пофайловый проход, у которого вердикт на файл свой.
if [ "$brc" -gt 1 ] \
    || grep -qi "panic" "$T/batch.out" "$T/batch.err" 2>/dev/null \
    || grep -q "E_NOVAC_ICE" "$T/batch.out" "$T/batch.err" 2>/dev/null; then
    NOVAC_BATCH=0
    t_novac=0
    echo "DEBUG batch DEAD rc=$brc (ice=$(grep -c E_NOVAC_ICE "$T/batch.out" 2>/dev/null)) -- per-file fallback" >&2
else
    cat "$T/batch.out" "$T/batch.err" 2>/dev/null \
        | grep -o '"file":"[^"]*"' | sed 's/.*:"//;s/"$//' | sort -u > "$T/novac_rejected"
    echo "DEBUG batch rc=$brc rejected=$(wc -l < "$T/novac_rejected") out=$(wc -c < "$T/batch.out")" >&2
fi

while IFS= read -r f; do
    rel=${f#"$ROOT"/}
    if [ "$NOVAC_BATCH" = "1" ]; then
        # вердикт из пачки: путь в списке отвергнутых => отверг
        if grep -Fxq "$rel" "$T/novac_rejected" 2>/dev/null; then rn=1; else rn=0; fi
        : > "$T/err"
    else
        s=$(date +%s%N)
        timeout 10 "$NOVAC" check "$f" >/dev/null 2>"$T/err" </dev/null
        rn=$?
        t_novac=$(( t_novac + ( $(date +%s%N) - s ) / 1000000 ))
    fi
    s=$(date +%s%N)
    "$ORACLE" check "$f" >/dev/null 2>&1 </dev/null
    ro=$?
    t_oracle=$(( t_oracle + ( $(date +%s%N) - s ) / 1000000 ))
    if novac_is_panic_rc "$rn" || grep -qi "panic" "$T/err"; then
        panic=$((panic+1))
        echo "  PANIC/HANG: $rel (novac rc=$rn)" >> "$T/red"
        head -1 "$T/err" | sed 's/^/    /' >> "$T/red"
    elif [ "$rn" -eq 0 ] && [ "$ro" -ne 0 ]; then
        if [ -f "$ALLOW" ] && grep -Fxq "$rel" "$ALLOW"; then
            allowed=$((allowed+1))
        else
            danger=$((danger+1))
            echo "  DANGER (К7): $rel — novac принял, оракул отверг (rc=$ro)" >> "$T/red"
        fi
    elif [ "$rn" -ne 0 ] && [ "$ro" -eq 0 ]; then
        # Корзины §10.4 — принадлежность решает МАШИНА (bootstrap §3).
        if grep -q 'LEGACY-#\|EXPECT_CC_ERROR' "$f"; then
            blocked=$((blocked+1))
        else
            added=$(awk -F'	' -v f="$rel" '$1==f{print $2; exit}' "$T/added_dates")
            if [ -n "$added" ] && [ -n "$SPEC_POINT" ] && [ "$added" \> "$SPEC_POINT" ]; then
                outpoint=$((outpoint+1))
                echo "$rel ($added)" >> "$T/outpoint"
            else
                subset=$((subset+1))
                echo "$rel" >> "$T/subset"
            fi
        fi
    elif [ "$rn" -eq 0 ]; then
        acc=$((acc+1)); echo "$rel" >> "$T/acc"
    else
        rej=$((rej+1))
    fi
done < "$T/list"

# Поведенческое число: каждый совпали-принятый файл через смоук (эмиссия
# novac + релинк драйвером + побайтовый дифф stdout/exit против оракула).
# СТРОКА allow ДЕЙСТВУЕТ И ЗДЕСЬ (2026-08-27). Её контракт (шапка
# `novac/divergences.allow`) говорит: «одна строка = одна фикстура, на которой
# novac и оракул расходятся НАРОЧНО», а реестр расхождений велит заводить строку
# ровно в тот день, когда расхождение стало ВИДИМЫМ ПОВЕДЕНИЕМ. Код же спрашивал
# allow только в ветке «novac принял, оракул отверг» — то есть про поведение
# контракт обещал то, чего не делал, и первое же сознательное поведенческое
# расхождение (вместимость литерала, амендмент D239) красило прогон.
# Сознательное расхождение считается ОТДЕЛЬНО и в behavior-match не входит:
# «сошлись байт-в-байт» и «разошлись, и мы правы» — разные факты.
beh=0; behfail=0; behallow=0
if [ -f "$T/acc" ]; then
    while IFS= read -r rel; do
        if sh "$ROOT/scripts/tools/novac-e1-smoke.sh" "$rel" >/dev/null 2>&1; then
            beh=$((beh+1))
        elif [ -f "$ALLOW" ] && grep -Fxq "$rel" "$ALLOW"; then
            behallow=$((behallow+1))
            echo "  РАЗОШЛИСЬ СОЗНАТЕЛЬНО (allow): $rel — история в docs/dev/novac-divergences.md" >> "$T/note"
        else
            echo "  ПОВЕДЕНИЕ РАЗОШЛОСЬ: $rel (оба check-принимают, но бинарь novac != оракула)" >> "$T/red"
            behfail=$((behfail+1))
        fi
    done < "$T/acc"
fi

# Расстояние до самосборки: собственный исходник через novac check.
#
# ОДНИМ процессом на все файлы (владелец 2026-08-17: «пусть делают пачкой»).
# Замер: чистый старт novac 149 мс против 50 мс работы на файле, то есть
# три четверти цены — это запуск. Двадцать один файл по вызову = три секунды
# стартов; пачкой — один.
#
# Вердикт по файлу берётся из ДИАГНОСТИК: каждая несёт своё поле `file`
# (схема §7), поэтому «файл с хотя бы одной диагностикой» = отвергнут, а
# остальные приняты. Если пачка УМЕРЛА (паника — код вне 0/1/2), вердикты
# по ней недостоверны, и мы честно откатываемся на пофайловый проход, а не
# гадаем.
self_files=""
self_total=0
for f in "$ROOT"/novac/src/*/*.nv "$ROOT"/novac/src/*.nv; do
    [ -f "$f" ] || continue
    self_total=$((self_total+1))
    self_files="$self_files \"$f\""
done
self_rej=0
if [ "$self_total" -gt 0 ]; then
    eval "timeout 60 NOVAC_SELF_PATH=novac/src \"$NOVAC\" check $self_files" > "$T/self.out" 2> "$T/self.err" </dev/null
    src=$?
    # ПАЧКА С ICE НЕДОСТОВЕРНА (замер 2026-08-30): ice обрывает процесс кодом 2,
    # который ПРОХОДИТ порог «код вне 0/1/2», и файлы ПОСЛЕ точки смерти выходят
    # «чистыми», не будучи досуженными вовсе. Так родилось ложное 41/53 волны В6:
    # двенадцать «принятых» стояли в списке позади interop.nv, чей ice убил
    # пачку. ICE в выводе — тот же откат на пофайловый проход, что и смерть.
    if grep -q "E_NOVAC_ICE" "$T/self.out" "$T/self.err" 2>/dev/null; then
        src=99
    fi
    if [ "$src" -le 2 ]; then
        self_rej=$(cat "$T/self.out" "$T/self.err" 2>/dev/null \
            | grep -o '"file":"[^"]*"' | sort -u | wc -l | tr -d '[:space:]')
    else
        for f in "$ROOT"/novac/src/*/*.nv "$ROOT"/novac/src/*.nv; do
            [ -f "$f" ] || continue
            timeout 10 "$NOVAC" check "$f" >/dev/null 2>&1 </dev/null || self_rej=$((self_rej+1))
        done
    fi
fi
wall=$(( ( $(date +%s%N) - wall0 ) / 1000000 ))

echo "novac-diff-corpus: файлов $N — совпали-приняли $acc · совпали-отвергли $rej · отставание $subset · вне-точки $outpoint · заблокировано-оракулом $blocked · DANGER $danger · PANIC $panic · allow $allowed"
echo "novac-diff-corpus: поведенчески совпали $beh из $acc · самосборка: отвергнуто $self_rej из $self_total"
echo "novac-diff-corpus: цена прогона — novac ${t_novac}ms, оракул ${t_oracle}ms, стена ${wall}ms"
if [ -f "$T/acc" ]; then
    echo "  в подмножестве (оба приняли):"
    sed 's/^/    /' "$T/acc"
fi
if [ -f "$T/outpoint" ]; then
    echo "  вне точки (добавлены после $SPEC_POINT, спорные разбираются поимённо):"
    sed 's/^/    /' "$T/outpoint"
fi
if [ "$danger" -gt 0 ] || [ "$panic" -gt 0 ] || [ "$behfail" -gt 0 ]; then
    echo "novac-diff-corpus: FAIL" >&2
    cat "$T/red" >&2
    exit 1
fi
# Машинная строка — её парсит check-novac-differential.sh (храповик §10.4).
if [ -s "$T/note" ]; then
    cat "$T/note"
fi
echo "novac-diff-corpus baseline-numbers: contract-match=$((acc+rej)) behavior-match=$beh behavior-allowed=$behallow out-of-point=$outpoint oracle-blocked=$blocked self-distance=$self_rej/$self_total"
echo "novac-diff-corpus ok"
exit 0
