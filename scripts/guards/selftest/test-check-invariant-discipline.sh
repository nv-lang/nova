#!/usr/bin/env bash
# Селфтест scripts/guards/check-invariant-discipline.sh.
#
# Страж без селфтеста не работает — норма проекта, и сам этот страж её энфорсит,
# так что отсутствие селфтеста было бы прямым лицемерием.
set -u
export LC_ALL=C

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
G="$ROOT/scripts/guards/check-invariant-discipline.sh"
FAILED=0
CASES=0
ok()  { CASES=$((CASES+1)); echo "  ok: $1"; }
bad() { CASES=$((CASES+1)); echo "  ПРОВАЛ: $1" >&2; FAILED=1; }

echo "== селфтест check-invariant-discipline =="

T=$(mktemp -d)
git -C "$T" init -q 2>/dev/null
git -C "$T" -c user.email=t@t -c user.name=t commit -q --allow-empty -m base 2>/dev/null
mkdir -p "$T/scripts/guards/selftest"

# Файл обязан быть В ИНДЕКСЕ: страж проверяет то, что коммитится, а не мусор
# рабочего дерева (иначе он тянул 175 посторонних файлов и не укладывался в
# таймаут). Это же ближе к реальности: pre-commit смотрит именно индекс.
run_guard() { ( cd "$T" && git add -A >/dev/null 2>&1; bash "$G" "$T" HEAD >/dev/null 2>&1 ); }

# 1. Ловит подпись инварианта-на-договорённости в добавленном коде.
printf 'struct S {\n    // mutable и consume взаимоисключающие — parser enforce\x27ит\n    x: bool,\n}\n' > "$T/a.rs"
if run_guard; then
    bad "НЕ поймал подпись «взаимоисключающие / parser enforce'ит»"
else
    ok "ловит инвариант на честном слове (код 1)"
fi

# 1-bis. Ловит подпись группы 2 — «энфорса НЕТ» (правило названо и тут же
#        признано непроверяемым). Это РЕАЛЬНЫЙ случай: комментарий
#        «move-out-запрет не вводился» у `spawn consume a, b`
#        (parser/mod.rs:10978) прошёл мимо первой редакции стража, и итогом
#        стало двойное закрытие сокета с падением цикла событий (№456).
#        Вопрос владельца «как наши новые правила поймали бы такую недоделку?»
#        показал, что НЕ поймали бы. Этот случай — чтобы не вернулось.
printf 'fn f() {
    // у spawn-формы свои правила (move-out-запрет не вводился)
}
' > "$T/a.rs"
if run_guard; then
    bad "НЕ поймал подпись «энфорса нет» (не вводился/не реализован)"
else
    ok "ловит подпись «энфорса нет» (группа 2)"
fi

# 2. Пометка [INV-PROPERTY] снимает претензию.
printf 'struct S {\n    // взаимоисключающие по типу [INV-PROPERTY]\n    x: bool,\n}\n' > "$T/a.rs"
if run_guard; then
    ok "принимает [INV-PROPERTY]"
else
    bad "ложно краснит на [INV-PROPERTY]"
fi

# 3. [INV-GUARD: X] с НЕсуществующим стражем — нарушение.
printf 'struct S {\n    // взаимоисключающие [INV-GUARD: net-takoy-strazh]\n    x: bool,\n}\n' > "$T/a.rs"
if run_guard; then
    bad "принял ссылку на несуществующего стража"
else
    ok "ловит [INV-GUARD:] на несуществующего стража"
fi

# 4. [INV-GUARD: X] с существующим стражем, но БЕЗ селфтеста — нарушение.
printf '#!/bin/sh\nexit 0\n' > "$T/scripts/guards/real-guard.sh"
printf 'struct S {\n    // взаимоисключающие [INV-GUARD: real-guard.sh]\n    x: bool,\n}\n' > "$T/a.rs"
if run_guard; then
    bad "принял стража без селфтеста"
else
    ok "ловит стража без селфтеста"
fi

# 5. Тот же страж + селфтест — принимается.
printf '#!/bin/sh\nexit 0\n' > "$T/scripts/guards/selftest/test-real-guard.sh"
if run_guard; then
    ok "принимает [INV-GUARD:] со стражем и селфтестом"
else
    bad "ложно краснит на корректном [INV-GUARD:]"
fi

# 6. [INV-TODO: №NNN] — признанный долг, принимается.
printf 'struct S {\n    // взаимоисключающие [INV-TODO: №462]\n    x: bool,\n}\n' > "$T/a.rs"
if run_guard; then
    ok "принимает [INV-TODO: №NNN] как признанный долг"
else
    bad "ложно краснит на [INV-TODO:]"
fi

# 7. Документация НЕ проверяется — там об инвариантах пишут.
rm -f "$T/a.rs"
mkdir -p "$T/docs"
printf '# Норма\n\nВзаимоисключающие поля — это инвариант, parser enforce\x27ит.\n' > "$T/docs/x.md"
if run_guard; then
    ok "не трогает docs/ (там об инвариантах пишут)"
else
    bad "ложно краснит на документации"
fi

rm -rf "$T"

if [ "$FAILED" -eq 0 ]; then
    echo "селфтест check-invariant-discipline: $CASES/$CASES ok"
    exit 0
fi
echo "селфтест check-invariant-discipline: ЕСТЬ ПРОВАЛЫ" >&2
exit 1
