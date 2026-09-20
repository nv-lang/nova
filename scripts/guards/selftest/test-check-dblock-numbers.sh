#!/usr/bin/env bash
# Самотест check-dblock-numbers.sh — песочница с НАСТОЯЩИМИ ветками.
#
# Почему песочница, а не фикстуры-файлы: страж судит ИСТОРИЮ (что ветка
# ДОБАВИЛА относительно точки расхождения), и проверить это можно только
# настоящим git. Подделка деревьев доказала бы разбор заголовков и ничего — про
# признак «добавлено веткой».
#
# Клетки ПАРАМИ: у каждой «ждём красного» есть «ждём зелёного» на почти том же
# входе. Страж, который краснеет всегда, снимут первым днём; который никогда —
# неотличим от отсутствующего.
set -u
export LC_ALL=C

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
G="$ROOT/scripts/guards/check-dblock-numbers.sh"
FAILED=0
OKN=0
ok()  { OKN=$((OKN + 1)); echo "  ok   $1"; }
bad() { echo "  ПРОВАЛ $1" >&2; FAILED=1; }

TMP="$(mktemp -d)"; trap 'rm -rf "$TMP"' EXIT
R="$TMP/repo"

git_q() { git -C "$R" "$@" >/dev/null 2>&1; }

setup_base() {   # общая история: один блок D100
    rm -rf "$R"; mkdir -p "$R/spec/decisions"
    git -C "$TMP" init -q repo -b main
    git_q config user.email t@t; git_q config user.name t
    printf '## D100. Base block\n\ntext\n' > "$R/spec/decisions/02-types.md"
    git_q add spec/decisions/02-types.md
    git_q commit -qm base
}

branch_adds() { # $1 имя ветки, $2 номер, $3 файл
    git_q checkout -q -b "$1" main
    printf '\n## D%s. Block of %s\n\ntext\n' "$2" "$1" >> "$R/spec/decisions/$3"
    git_q add "spec/decisions/$3"
    git_q commit -qm "add D$2"
    git_q checkout -q main
}

run() { ( cd "$R" && bash "$G" . ) > "$TMP/.out" 2> "$TMP/.err"; }

# --- пара 1: две ветки добавили ОДИН номер ----------------------------------
setup_base
branch_adds win-a 999 02-types.md
branch_adds win-b 999 02-types.md
run
if [ $? -ne 0 ] && grep -q "D999" "$TMP/.err"; then
    ok "две ветки с одним D999 — красный, номер назван"
else
    bad "1: столкновение двух веток не поймано: $(cat "$TMP/.out") $(cat "$TMP/.err")"
fi

# --- пара 1б: номера разведены — зелено -------------------------------------
setup_base
branch_adds win-a 999 02-types.md
branch_adds win-b 998 02-types.md
run
if [ $? -eq 0 ]; then
    ok "номера разведены — зелёный"
else
    bad "1б: ложный отказ на разных номерах: $(cat "$TMP/.err")"
fi

# --- пара 2: ветка заняла номер, УЖЕ занятый общей историей -----------------
setup_base
branch_adds win-a 100 03-syntax.md 2>/dev/null || true
run
if [ $? -ne 0 ] && grep -q "D100" "$TMP/.err"; then
    ok "ветка заняла номер из базы — красный"
else
    bad "2: занятый базой номер не пойман: $(cat "$TMP/.out") $(cat "$TMP/.err")"
fi

# --- пара 3: ТАБЛИЦА против ветки -------------------------------------------
# Номер выдан одной ветке, а блок стоит в другой: выдача разошлась с исполнением.
setup_base
branch_adds win-a 977 02-types.md
printf '| номер | предмет | окно / ветка | выдан | состояние |\n|---|---|---|---|---|\n| D977 | нечто | окно / win-b | 2026-09-20 | ждёт |\n' \
    > "$R/spec/decisions/ISSUED-NUMBERS.md"
git_q add spec/decisions/ISSUED-NUMBERS.md; git_q commit -qm table
run
if [ $? -ne 0 ] && grep -q "D977" "$TMP/.err"; then
    ok "таблица называет другую ветку — красный"
else
    bad "3: расхождение таблицы с веткой не поймано: $(cat "$TMP/.out") $(cat "$TMP/.err")"
fi

# --- пара 3б: таблица называет ТУ ЖЕ ветку — зелено -------------------------
printf '| номер | предмет | окно / ветка | выдан | состояние |\n|---|---|---|---|---|\n| D977 | нечто | окно / win-a | 2026-09-20 | ждёт |\n' \
    > "$R/spec/decisions/ISSUED-NUMBERS.md"
git_q add spec/decisions/ISSUED-NUMBERS.md; git_q commit -qm table2
run
if [ $? -eq 0 ]; then
    ok "таблица сходится с веткой — зелёный"
else
    bad "3б: ложный отказ при сходящейся таблице: $(cat "$TMP/.err")"
fi

# --- пара 4: СТАРЫЙ СЛУЧАЙ ГЕЙТА — два одинаковых номера в ОДНОМ дереве -----
# Шаг `D-number uniqueness` переезжает сюда, и вердикт обязан остаться тем же.
setup_base
printf '\n## D100. Second block with the same number\n\ntext\n' >> "$R/spec/decisions/02-types.md"
run
if [ $? -ne 0 ] && grep -q "100" "$TMP/.err"; then
    ok "два D100 в одном дереве — красный (поведение шага гейта сохранено)"
else
    bad "4: дубль в своём дереве не пойман: $(cat "$TMP/.out") $(cat "$TMP/.err")"
fi

# --- пара 4б: тот же файл без дубля — зелено --------------------------------
setup_base
run
if [ $? -eq 0 ]; then
    ok "дерево без дублей — зелёный"
else
    bad "4б: ложный отказ на чистом дереве: $(cat "$TMP/.err")"
fi

# --- 5: ЗНАМЕНАТЕЛЬ ВИДЕН -----------------------------------------------------
# Ноль добавленных ветками — это «судить нечего», а не «чисто», и страж обязан
# сказать это словами: иначе пустая мишень читается как чистая.
setup_base
run
if grep -q "судить нечего" "$TMP/.out"; then
    ok "пустая мишень названа словами, а не выдана за чистоту"
else
    bad "5: при нуле добавленных страж смолчал о знаменателе: $(cat "$TMP/.out")"
fi

# --- 6: реестр брони НЕ попадает в свою же выборку --------------------------
# Файл, который ГОВОРИТ о номерах, не объявляет их. Иначе страж ловит сам себя.
setup_base
printf '# Реестр\n\n## D100. Упоминание номера в реестре брони\n' \
    > "$R/spec/decisions/ISSUED-NUMBERS.md"
run
if [ $? -eq 0 ]; then
    ok "заголовок в ISSUED-NUMBERS не считается объявлением номера"
else
    bad "6: страж поймал сам себя на реестре брони: $(cat "$TMP/.err")"
fi

# --- 7: ОТСТАВАНИЕ ОТ БАЗЫ — НЕ ЗАЯВКА НА НОМЕР -----------------------------
# Поймано на себе: `main` поправил строку заголовка (добавил якорь), моё дерево
# стояло на прежней — и сравнение с ВЕРШИНОЙ базы показало её как добавленную,
# то есть страж объявил столкновением отставание. Сравнивать надо с точкой
# расхождения. Клетка пиннит именно это.
setup_base
git_q checkout -q main
printf '## D100. Base block {#d100}\n\ntext\n' > "$R/spec/decisions/02-types.md"
git_q add spec/decisions/02-types.md
git_q commit -qm "main правит заголовок"
git_q checkout -q -b behind HEAD~1      # ветка осталась на прежней строке
run
if [ $? -eq 0 ]; then
    ok "ветка отстала от базы — это не заявка на номер, зелёный"
else
    bad "7: отставание прочитано как столкновение: $(cat "$TMP/.err")"
fi

echo "селфтест check-dblock-numbers: $OKN/$OKN ok"
[ "$FAILED" -eq 0 ] || { echo "селфтест check-dblock-numbers: ЕСТЬ ПРОВАЛЫ" >&2; exit 1; }
