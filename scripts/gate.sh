#!/bin/sh
# scripts/gate.sh — ЕДИНЫЙ авторитетный гейт (план 231 трек Д п.1).
# Запуск из корня целевого дерева (main-репа или worktree):  bash scripts/gate.sh
# Ассерты делает СКРИПТ, не интегратор: любой провал = exit 1 с внятной строкой.
#
# Состав (CLAUDE.md/dev-workflow):
#   1) cargo build --release (nova-cli)
#   2) мега-CU spec_tests/conformance ОДНИМ CU: exit=0 И строка "PASS: N  FAIL: 0" присутствует
#   3) nova check std/src (БЕЗ NOVA_STD_PATH): канон "PASS: 144  FAIL: 27  WARN: 1057"
#   4) флагман examples/flagship/aggregator --strict-effects: строка "built:"
set -u
ROOT="$(pwd)"
MAIN_REPO="d:/Sources/nv-lang/nova"
export NOVA_GC_LIB_DIR="D:\\Sources\\nv-lang\\nova\\compiler-codegen\\vcpkg_installed\\x64-windows-static\\lib"
export NOVA_INCLUDE_DIR="D:\\Sources\\nv-lang\\nova\\compiler-codegen\\vcpkg_installed\\x64-windows-static\\include"
unset NOVA_STD_PATH 2>/dev/null || true

fail() { echo "GATE FAIL: $1" >&2; exit 1; }

echo "== gate: arch-ratchet =="
bash "$ROOT/scripts/guards/arch-ratchet.sh" || fail "arch-ratchet (emit_c growth)"

# Реестр 221.1 №138 (урок 2026-07-27): копия рантайма внутри пакетной репы/
# worktree не под git → её протухание невидимо, и она ШАДОВИТ настоящий
# рантайм. Реальная цена промаха: полтора часа диагностики «регрессии
# компилятора» в nova-http, которой не было (устаревшие заголовки копии не
# объявляли символ из фикса №108), плюс >1 ГБ мусора по репам. Копия НЕ нужна —
# есть штатные NOVA_RT_DIR/NOVA_CG_INCLUDE (см. шапку самого стража).
echo "== gate: no-runtime-copy =="
bash "$ROOT/scripts/guards/check-no-runtime-copy.sh" || fail "копия рантайма в пакетной репе/worktree (№138)"

# Трек Ж (231): страж без самотеста — доверие на слово. Самотесты дешёвые
# (секунды) и проверяют ОБА свойства: ловит нарушение / не даёт ложняка.
# Реестр 221.1 №155/№161 (урок повторился ДВАЖДЫ за день): маркер [M-...] в коде без
# записи в реестре = невидимый долг — обход живёт, а дефекта для планирования нет.
# Первый прогон стража нашёл 59 таких; ручные аудиты видели 8. Храповик: расти нельзя.
echo "== gate: marker-registry-sync =="
bash "$ROOT/scripts/guards/check-marker-registry-sync.sh" "$ROOT" || fail "маркеры в коде без записи в реестре (№155/№161)"

echo "== gate: selftests стражей =="
for st in "$ROOT"/scripts/guards/selftest/test-*.sh; do
    [ -e "$st" ] || continue
    bash "$st" || fail "самотест стража: $(basename "$st")"
done

echo "== gate: cargo build --release =="
( cd "$ROOT/nova-cli" && cargo build --release ) || fail "cargo build"
NOVA="$ROOT/nova-cli/target/release/nova.exe"
[ -x "$NOVA" ] || fail "nova.exe not found: $NOVA"

echo "== gate: mega-CU (spec_tests/conformance, one CU) =="
MEGA_LOG="${TMPDIR:-/tmp}/gate_mega_$$.log"
"$NOVA" test --positive --compile-error "$ROOT/spec_tests/conformance" >"$MEGA_LOG" 2>&1
MEGA_EXIT=$?
MEGA_LINE=$(sed -e "s/\[[0-9;]*m//g" "$MEGA_LOG" | grep -E "PASS: [0-9]+ +FAIL: [0-9]+" | tail -1)
echo "mega-CU exit=$MEGA_EXIT :: $MEGA_LINE"
# Строка PASS/FAIL ОБЯЗАНА присутствовать: краш компилятора её не печатает вовсе,
# и наивный фильтр молча роняет — тогда «зелено» означает «ничего не увидел».
echo "$MEGA_LINE" | grep -qE "PASS: [0-9]+ +FAIL: [0-9]+" \
    || fail "mega-CU: строки PASS/FAIL нет вовсе (краш не печатает её — см. $MEGA_LOG)"
# 2026-07-30: whitelist СНЯТ — мега-CU впервые полностью зелёный (591/0/67).
# История, чтобы не завели снова «на всякий случай»: whitelist допускал ровно один
# красный по ИМЕНИ `a_q3_println_debug_record`. Имя оказалось ЛОЖНЫМ — раннер
# приписывал падение первому по алфавиту файлу слитого CU, а сам `a_q3` был невиновен
# (изолированно 5/5). Настоящих виновников оказалось трое, слоями: `d62` (NULL-deref
# handler'а, реестр №158) прятал `Tagged` (гейт №136 бил по TurboFish-форме, №159),
# который прятал третий дефект. Каждый обрывал прогон раньше следующего.
# Мораль: whitelist по имени в слитом CU — не смягчение, а слепое пятно. Если красный
# вернётся — чинить, а не заносить в исключения.
[ "$MEGA_EXIT" -eq 0 ] || { grep -E "FAIL|TIMEOUT" "$MEGA_LOG" | grep -v "FAIL: 0" | head -10 >&2; fail "mega-CU exit=$MEGA_EXIT"; }
echo "$MEGA_LINE" | grep -qE "PASS: [0-9]+ +FAIL: 0([^0-9]|$)" || fail "mega-CU: FAIL != 0 (см. $MEGA_LOG)"

echo "== gate: check std/src (byte-canon) =="
STD_LINE=$("$NOVA" check "$ROOT/std/src" 2>&1 | sed -e "s/\[[0-9;]*m//g" | grep -E "^PASS" | tail -1)
echo "std :: $STD_LINE"
# Канон 2026-07-29: PASS 147 / FAIL 26 / WARN 1078. Прежний «144/27/1057» устарел
# и делал гейт красным на чистом main (факт до этого слияния — 146/27/1071: PASS/WARN
# росли от новых файлов, это законно). FAIL опустился 27→26 в этом слиянии:
# `testing/handlers/core.nv` получил недостающий `import std.time.duration`
# ([M-inline-cast-receiver-method-resolution]). Оставшиеся 26 — ВСЕ neg-фикстуры
# (проверено списком: ни одна не перестала падать). Ассертим ТОЛЬКО FAIL —
# рост FAIL = регресс; сдвиг PASS/WARN от новых файлов законен и гейт ронять не должен.
echo "$STD_LINE" | grep -qE "FAIL: 26\b" \
    || fail "check std: FAIL отклонился от канона 26 (все 26 — neg-фикстуры): '$STD_LINE'"

echo "== gate: flagship aggregator --strict-effects =="
FLAG_LINE=$("$NOVA" build "$ROOT/examples/flagship/aggregator/src/main.nv" --strict-effects 2>&1 | sed -e "s/\[[0-9;]*m//g" | tail -1)
echo "flagship :: $FLAG_LINE"
echo "$FLAG_LINE" | grep -q "built:" || fail "flagship not built: '$FLAG_LINE'"

echo "== gate: D-number uniqueness =="
# 2026-07-30: послабление под D431 СНЯТО — коллизия закрыта перенумерацией
# FixedArray-блока в D440 (реестр 221.1 №123).
#
# Регулярка была `^## D[0-9]+\.` — с ОБЯЗАТЕЛЬНОЙ точкой, и это слепое пятно:
# заголовок решения пишут и через тире (`## D435 — field-attribute …`), такие в
# скан не попадали ВООБЩЕ — 287 блоков из 349 видимых сейчас. Цена промаха
# замерена в тот же день: окно №162 завело новый блок под номером D435, уже
# занятым `## D435 —` в 02-types.md, и старый гейт бы промолчал — при том что
# auto_derive.rs и conformance-фикстуры уже ссылались на D435 в ДРУГОМ смысле.
#
# Заголовок НОВОГО решения = номер, за которым сразу `.` либо ` —`. Блоки-
# продолжения (`## D178 amend V2 — …`, `## D33-LEGACY (archived).`,
# `## D239-зеркало`) между номером и разделителем несут слово и в скан не идут —
# это тот же decision, а не второй под тем же номером. Сверяем ЧИСЛА, а не
# строки целиком: иначе `## D440.` и `## D440 —` разошлись бы как разные ключи.
DUPES=$(grep -rhoE "^## D[0-9]+(\.|[[:space:]]+—)" spec/decisions/*.md \
        | grep -oE "[0-9]+" | sort -n | uniq -d)
[ -n "$DUPES" ] && fail "дублирующиеся номера D-блоков: $(echo "$DUPES" | tr '\n' ' ')"
echo "GATE OK (final)"
