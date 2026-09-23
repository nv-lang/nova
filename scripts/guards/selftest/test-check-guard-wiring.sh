#!/usr/bin/env bash
# test-check-guard-wiring.sh — САМОТЕСТ мета-стража `check-guard-wiring.sh`.
#
# Мета-страж проверяет, что каждый страж документирован/подключён/покрыт. Сам он
# тоже страж, поэтому обязан доказать те же два свойства: ЛОВИТ нарушение и НЕ
# даёт ложного срабатывания. Иначе получилась бы рекурсия доверия на слово.
#
# Дополнительно этот самотест ПРОГОНЯЕТ мета-страж на РЕАЛЬНОЙ репе — так
# правило владельца («нет толку, если не подключён») энфорсится на каждом гейте
# без отдельного шага в gate.sh: цикл `scripts/guards/selftest/test-*.sh`
# подхватывает этот файл автоматически.
#
# Запуск: scripts/guards/selftest/test-check-guard-wiring.sh
# Выход: 0 — мета-страж исправен И реальная репа чиста; 1 — иначе.
#
# План: docs/plans/231-bug-cycle-exit.md §4в.

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]:-$0}")" && pwd)"
# Скрипт живёт в scripts/guards/selftest/ — корень репы на три уровня выше.
REPO_ROOT="$(cd "$SCRIPT_DIR/../../.." && pwd)"
GUARD="$REPO_ROOT/scripts/guards/check-guard-wiring.sh"

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

fails=0
check() { # имя, ожидаемый_код, фактический_код
    if [ "$2" -eq "$3" ]; then
        echo "  ok: $1"
    else
        echo "  ПРОВАЛ: $1 — ожидался код $2, получен $3" >&2
        fails=$((fails + 1))
    fi
}

# Собрать игрушечную репу: scripts/guards/ + scripts/guards/selftest/ + gate.sh
# с циклом самотестов (та же трёхуровневая форма, что настоящая scripts/).
make_repo() { # каталог
    mkdir -p "$1/scripts/guards/selftest" "$1/docs/plans"
    # Страж теперь требует не только ССЫЛКУ на план, но и чтобы названный путь
    # СУЩЕСТВОВАЛ (2026-09-07, предложение окна 274). В игрушечной репе плана
    # не было вовсе, и «корректно оформленный страж» перестал быть таковым —
    # фикстура описывала мир, которого больше нет. Кладём план рядом.
    : > "$1/docs/plans/231-bug-cycle-exit.md"
    cat > "$1/scripts/gate.sh" <<'EOG'
#!/usr/bin/env bash
for st in "$ROOT"/scripts/guards/selftest/test-*.sh; do bash "$st"; done
EOG
}

good_header() { # файл, имя
    cat > "$1" <<EOH
#!/usr/bin/env bash
# $2 — учебный страж для самотеста.
# ПОЧЕМУ: проверяем, что мета-страж принимает корректно оформленного стража.
# ЧТО ПРОВЕРЯЕТ: ничего, это фикстура.
# ИСПОЛЬЗОВАНИЕ: $2
# Коды: 0 — ок.
# Ещё строка шапки, чтобы набрать минимум.
# И ещё одна.
# План: docs/plans/231-bug-cycle-exit.md §4в.
exit 0
EOH
}

echo "самотест check-guard-wiring:"

# (1) ЛОВИТ: страж без самотеста.
r1="$tmp/r1"; make_repo "$r1"
good_header "$r1/scripts/guards/check-foo.sh" "check-foo.sh"
bash "$GUARD" "$r1" >/dev/null 2>&1
check "ловит стража без самотеста" 1 $?

# (2) ЛОВИТ: страж с тонкой шапкой (самотест есть).
r2="$tmp/r2"; make_repo "$r2"
printf '#!/usr/bin/env bash\n# коротко\nexit 0\n' > "$r2/scripts/guards/check-bar.sh"
touch "$r2/scripts/guards/selftest/test-check-bar.sh"
bash "$GUARD" "$r2" >/dev/null 2>&1
check "ловит тонкую шапку" 1 $?

# (3) ЛОВИТ: шапка есть, самотест есть, но нет ссылки на план.
r3="$tmp/r3"; make_repo "$r3"
{ printf '#!/usr/bin/env bash\n'; for i in $(seq 1 10); do printf '# строка шапки %s\n' "$i"; done; printf 'exit 0\n'; } > "$r3/scripts/guards/check-baz.sh"
touch "$r3/scripts/guards/selftest/test-check-baz.sh"
# С 2026-08-27 это свойство — ХРАПОВИК (реестр 221.1 №785), поэтому судится
# НЕ сам факт, а РОСТ над базой. База берётся от каталога САМОГО стража, а не
# подопытного дерева, поэтому подложному дереву база задаётся переменной —
# иначе один нарушитель сравнивался бы с настоящей базой репы и молча проходил.
wb0="$tmp/wb0"; printf 'no_plan_ref=0
' > "$wb0"
NOVA_WIRING_BASELINE="$wb0" bash "$GUARD" "$r3" >/dev/null 2>&1
check "ловит рост безадресных над базой 0" 1 $?

# (3а) НЕ ЛОВИТ: тот же один безадресный при базе 1 — долг не вырос.
# С 2026-09-20 база ИМЕННАЯ: одного счёта мало, нарушитель должен быть НАЗВАН,
# иначе страж не отличит «тот же самый» от «другой вместо него».
wb1="$tmp/wb1"; printf 'no_plan_ref=1
no_plan_ref_name=check-baz
' > "$wb1"
NOVA_WIRING_BASELINE="$wb1" bash "$GUARD" "$r3" >/dev/null 2>&1
check "равенство базе — не ложное срабатывание" 0 $?

# ── (3б) ЗАМЕНА нарушителя: счёт тот же, имя ДРУГОЕ — красный ────────────
# Ровно то, чего счёт не умеет: одного безадресного починили, другого завели,
# число не дрогнуло. Именно этот случай стоял в дереве 2026-09-20 — база 48
# при долге 50 и четырёх новых безадресных.
wbsub="$tmp/wbsub"; printf 'no_plan_ref=1
no_plan_ref_name=check-someone-else
' > "$wbsub"
NOVA_WIRING_BASELINE="$wbsub" bash "$GUARD" "$r3" >/dev/null 2>&1
check "замена нарушителя при том же счёте — красный" 1 $?

# ── (3в) СУЖЕНИЕ ПРЕДИКАТА: имя пропало из набора, файл на месте — красный ─
# Требование интегратора 2026-09-20 и сильнейший повод именной базы. Мутируем
# СУДЬЮ: копия стража перечисляет только `*.sh`. Питоновский нарушитель из базы
# исчезает из набора, ЧИСЛО ПАДАЕТ — и это прочлось бы как погашенный долг.
# Имя же исчезнуть молча не может: файл на диске есть, значит сузился предикат.
r6="$tmp/r6"; make_repo "$r6"
{ printf '# -*- coding: utf-8 -*-\n'; printf 'u"""check-pyoffender\n';
  for i in $(seq 1 8); do printf 'строка шапки %s\n' "$i"; done; printf '"""\n'; } \
    > "$r6/scripts/guards/check-pyoffender.py"
touch "$r6/scripts/guards/selftest/test-check-pyoffender.py"
wbpy="$tmp/wbpy"; printf 'no_plan_ref=1
no_plan_ref_name=check-pyoffender
' > "$wbpy"
NOVA_WIRING_BASELINE="$wbpy" bash "$GUARD" "$r6" >/dev/null 2>&1
check "питоновский нарушитель виден широкому предикату — не ложняк" 0 $?

narrowed="$tmp/narrowed.sh"
sed 's|/scripts/guards/check-\*\.py||' "$GUARD" > "$narrowed"
NOVA_WIRING_BASELINE="$wbpy" bash "$narrowed" "$r6" > "$tmp/o3v" 2>&1
rc=$?
if [ "$rc" -eq 0 ]; then
    check "сужение предиката — красный" 1 0
elif grep -q "предикат сузился" "$tmp/o3v"; then
    check "сужение предиката — красный" 1 1
else
    check "сужение предиката — красный ИМЕННО про предикат" 1 0
fi

# ── (3г) ПОГАШЕНИЕ долга — зелёный с советом, а не красный ───────────────
# Обратная сторона (3в): имя ушло из набора ЗАКОННО — страж обзавёлся ссылкой.
# Красить это значило бы наказывать за починку.
r7="$tmp/r7"; make_repo "$r7"
good_header "$r7/scripts/guards/check-paid.sh" "check-paid.sh"
touch "$r7/scripts/guards/selftest/test-check-paid.sh"
wbpaid="$tmp/wbpaid"; printf 'no_plan_ref=1
no_plan_ref_name=check-paid
' > "$wbpaid"
NOVA_WIRING_BASELINE="$wbpaid" bash "$GUARD" "$r7" > "$tmp/o3g" 2>&1
check "погашенный долг — зелёный" 0 $?
grep -q "долг СНИЗИЛСЯ" "$tmp/o3g" \
    && echo "  ok: погашение названо вслух, с просьбой опустить базу" \
    || { echo "  ПРОВАЛ: погашение прошло молча" >&2; fails=$((fails + 1)); }

# ── (3д) База ПРИЗНАЁТ долг числом, но не называет имён — красный ────────
wbnames="$tmp/wbnames"; printf 'no_plan_ref=1
' > "$wbnames"
NOVA_WIRING_BASELINE="$wbnames" bash "$GUARD" "$r3" > "$tmp/o3d" 2>&1
rc=$?
if [ "$rc" -ne 0 ] && grep -q "БЕЗ ИМЁН" "$tmp/o3d"; then
    check "счёт без имён — красный" 1 1
else
    check "счёт без имён — красный" 1 0
fi

# ── (3е) Счёт и число имён РАСХОДЯТСЯ — красный ──────────────────────────
# Два источника одного значения; расхождение молчит, если его не спросить.
wbdrift="$tmp/wbdrift"; printf 'no_plan_ref=5
no_plan_ref_name=check-baz
' > "$wbdrift"
NOVA_WIRING_BASELINE="$wbdrift" bash "$GUARD" "$r3" > "$tmp/o3e" 2>&1
rc=$?
if [ "$rc" -ne 0 ] && grep -q "противоречит себе" "$tmp/o3e"; then
    check "расхождение счёта и имён — красный" 1 1
else
    check "расхождение счёта и имён — красный" 1 0
fi

# ── (3ж) Адрес ТОЛЬКО В ТЕЛЕ, после шапки — это НЕ адрес шапки: красный ──
# 2026-09-23: предикат судил весь файл, и комментарий посреди кода («волна E4
# (план 274.11)» в check-novac-row-fields, строка 86) гасил долг. Свойство —
# «шапка называет план», поэтому адрес засчитывается только в ведущем блоке.
r8="$tmp/r8"; make_repo "$r8"
{ printf '#!/usr/bin/env bash\n'; for i in $(seq 1 10); do printf '# строка шапки %s\n' "$i"; done
  printf 'set -u\n'; printf '# случайное упоминание: план 274 — не адрес шапки\n'; printf 'exit 0\n'; } \
    > "$r8/scripts/guards/check-bodyref.sh"
touch "$r8/scripts/guards/selftest/test-check-bodyref.sh"
NOVA_WIRING_BASELINE="$wb0" bash "$GUARD" "$r8" > "$tmp/o3zh" 2>&1
check "адрес только в теле — красный при базе 0" 1 $?

# ── (3з) ДЛИННАЯ шапка: адрес после 20-й строки, но внутри шапки — зелёный ─
# Граница — конец ведущего блока, а не «первые 20 строк»: у самого
# check-guard-wiring адрес стоит в строке 35, и он законен.
r9="$tmp/r9"; make_repo "$r9"
{ printf '#!/usr/bin/env bash\n'; for i in $(seq 1 24); do printf '# строка шапки %s\n' "$i"; done
  printf '#\n# План: docs/plans/231-bug-cycle-exit.md §4в.\n\n'; printf 'set -u\nexit 0\n'; } \
    > "$r9/scripts/guards/check-longhdr.sh"
touch "$r9/scripts/guards/selftest/test-check-longhdr.sh"
NOVA_WIRING_BASELINE="$wb0" bash "$GUARD" "$r9" > "$tmp/o3z" 2>&1
check "адрес в длинной шапке после 20-й строки — не ложняк" 0 $?

# ── (3и) Питоновская шапка: адрес внутри докстринга — зелёный ───────────
r10="$tmp/r10"; make_repo "$r10"
{ printf '#!/usr/bin/env python3\n'; printf '"""check-pyaddr — учебный страж.\n'
  for i in $(seq 1 8); do printf 'строка шапки %s\n' "$i"; done
  printf 'План: docs/plans/231-bug-cycle-exit.md §4в.\n"""\n'; printf 'import sys\n'; } \
    > "$r10/scripts/guards/check-pyaddr.py"
touch "$r10/scripts/guards/selftest/test-check-pyaddr.py"
NOVA_WIRING_BASELINE="$wb0" bash "$GUARD" "$r10" > "$tmp/o3i" 2>&1
check "адрес внутри питоновского докстринга — не ложняк" 0 $?

# (4) НЕ ловит: полностью корректный страж (шапка + план + самотест + цикл в gate).
r4="$tmp/r4"; make_repo "$r4"
good_header "$r4/scripts/guards/check-good.sh" "check-good.sh"
touch "$r4/scripts/guards/selftest/test-check-good.sh"
bash "$GUARD" "$r4" >/dev/null 2>&1
check "НЕ ловит корректно оформленного стража" 0 $?

# (5) НЕ ловит: стражей нет вовсе — нечего проверять.
r5="$tmp/r5"; make_repo "$r5"
bash "$GUARD" "$r5" >/dev/null 2>&1
check "НЕ ловит пустой набор стражей" 0 $?

# (6) РЕАЛЬНАЯ РЕПА: все существующие стражи обязаны быть в порядке.
#     Это и есть подключение правила к автопроверке — падает гейт, а не «когда-нибудь заметим».
bash "$GUARD" >/dev/null 2>&1
check "реальная репа nova: все стражи документированы/подключены/покрыты" 0 $?

if [ "$fails" -ne 0 ]; then
    echo "самотест ПРОВАЛЕН: $fails свойств(а) не выполняются" >&2
    exit 1
fi
echo "самотест ok: мета-страж ловит нарушения, не даёт ложняка, реальная репа чиста"
