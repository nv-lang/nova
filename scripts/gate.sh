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

# --- gate: dev-override visibility (221.1 №283, владелец 2026-08-04) -------
# `nova.override.toml`/`nova.local.toml` ([replace], D420) — gitignored,
# законный инструмент владельца для локальной разработки: путь-оверрайд
# поверх запиненной git-зависимости, без re-tag/push на каждой итерации.
# Опасность — не в самом файле, а в том, что он МОЛЧА подменяет резолв
# для перекрытых пакетов: гейт может показать `built:` на дереве с
# override, хотя тот же граф по `nova.lock.toml` (то, что реально видит
# любой чистый checkout — worktree/clone/CI) не соберётся вовсе. Ровно
# так был замаскирован №283 (tls запинен на коммите ДО переименования
# read_to_vec -> read_bytes, флагман уже звал новое имя — main собирался
# только потому, что override тихо подставлял живую соседнюю nova-tls).
#
# Выбор — ПРЕДУПРЕЖДЕНИЕ, не отказ. Override — рабочий инструмент
# владельца для повседневной разработки; гейт гоняется с главного репо
# постоянно (сегодня — пять раз), и жёсткий отказ означал бы либо вечно
# красный гейт на обычном рабочем дереве владельца, либо (реалистичнее)
# что первый же отказ научит держать override временно переименованным
# на время гейта — то есть ту же слепоту, только руками, а не автоматикой.
# Дыра была не в СУЩЕСТВОВАНИИ override, а в его МОЛЧАНИИ: «built:» и
# «GATE OK» выглядели одинаково что с override, что без него. Здесь чиним
# именно это — предупреждение печатается дважды (в начале прогона И рядом
# с финальным вердиктом, чтобы не потерялось при `tail`), перечисляет
# перекрытые пакеты и НЕ трогает exit code гейта.
OVERRIDE_FILES=$(find "$ROOT" \
    \( -path "*/target" -o -path "*/.git" -o -path "*/vcpkg_installed" \
       -o -path "*/node_modules" -o -path "*/.claude" \) -prune \
    -o \( -iname "nova.override.toml" -o -iname "nova.local.toml" \) -print \
    2>/dev/null)
print_override_warning() {
    echo "!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!"
    echo "!!! GATE WARNING: dev-override (nova.override.toml/nova.local.toml) активен."
    echo "!!! Этот прогон НЕ доказывает, что дерево собирается на чистом checkout'е"
    echo "!!! (worktree/clone/CI) — перекрытые пакеты резолвятся МИМО nova.lock.toml."
    echo "$OVERRIDE_FILES" | while IFS= read -r f; do
        [ -n "$f" ] || continue
        echo "!!!   файл: $f"
        grep -v '^[[:space:]]*#' "$f" | grep -v '^[[:space:]]*$' | sed 's/^/!!!     /'
    done
    echo "!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!"
}
if [ -n "$OVERRIDE_FILES" ]; then
    print_override_warning
fi

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
echo "== gate: bug-number-sync (№217 — каждый новый маркер нумерован в 221.1) =="
bash "$ROOT/scripts/guards/check-bug-number-sync.sh" "$ROOT" || fail "новый маркер без № в 221.1 (правило владельца №217)"


echo "== gate: selftests стражей =="

echo "== gate: doc-hygiene (язык/чистота публичной доки, правило владельца 2026-07-31) =="
bash "$ROOT/scripts/guards/check-doc-hygiene.sh" "$ROOT" || fail "doc-hygiene (кириллица/внутренние ссылки в /// или линте — рост запрещён)"
bash "$ROOT/scripts/guards/selftest/test-check-doc-hygiene.sh" || fail "doc-hygiene selftest"

echo "== gate: doc-conventions (docs/dev/doc-conventions.md enforcement, Plan 242) =="
# №322: вторым аргументом — база сравнения для подпроверки same-commit
# pairing (без неё она физически не выполняется). Локально берём
# предыдущий коммит; в CI база передаётся явно (PR base / event.before).
DOC_GUARD_BASE="$(git -C "$ROOT" rev-parse --verify -q HEAD~1 2>/dev/null || true)"
bash "$ROOT/scripts/guards/check-doc-conventions.sh" "$ROOT" "$DOC_GUARD_BASE" || fail "doc-conventions (шапка/frontmatter spec en, guide-парность, статус-строка плана, dev-ссылки, код-блоки пар — см. вывод выше)"
bash "$ROOT/scripts/guards/selftest/test-check-doc-conventions.sh" || fail "doc-conventions selftest"
bash "$ROOT/scripts/guards/selftest/test-doc-conventions-determinism.sh" || fail "doc-conventions determinism selftest (№321)"
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
if [ -n "$OVERRIDE_FILES" ]; then
    print_override_warning
    echo "GATE OK (final) [DEV-OVERRIDE ACTIVE — не доказательство чистого дерева, см. предупреждение выше]"
else
    echo "GATE OK (final)"
fi
