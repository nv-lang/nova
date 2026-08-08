#!/bin/sh
# scripts/gate.sh — ЕДИНЫЙ авторитетный гейт (план 231 трек Д п.1).
# Запуск из корня целевого дерева (main-репа или worktree):  bash scripts/gate.sh
# Ассерты делает СКРИПТ, не интегратор: любой провал = exit 1 с внятной строкой.
#
# Состав (CLAUDE.md/dev-workflow):
#   1) cargo build --release (nova-cli)
#   2) мега-CU spec_tests/conformance ОДНИМ CU: exit=0 И строка "PASS: N  FAIL: 0" присутствует
#   3) nova check std/src (БЕЗ NOVA_STD_PATH): канон "PASS: 144  FAIL: 27  WARN: 1057"
#   4) nova lint --deny std/src: канон 0 находок
#   5) nova lint --deny spec_tests: канон 0 находок (221.1 №416 хвост)
#   6) флагман examples/flagship/aggregator --strict-effects: строка "built:"
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
bash "$ROOT/scripts/guards/selftest/test-arch-ratchet.sh" >/dev/null || fail "селфтест arch-ratchet (аудит стражей 2026-08-08 — был единственным без селфтеста)"

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

# Реестр 221.1 №446/№447 (окно presume-cas-gate, 2026-08-08): единственный
# mco_resume в рантайме держит структурный инвариант «ни одно действие над
# co не выполняется вне выигранного CAS» — было соглашением на 4 сайтах,
# два из которых несли живой дефект (двойной destroy+sweep дубликата
# мёртвого co). Страж ловит новый resume-сайт, открытый в обход
# nova_resume_fiber (fibers.h).
echo "== gate: expect-markers (неизвестный EXPECT_* раннер молча игнорирует — №453) =="
bash "$ROOT/scripts/guards/check-expect-markers.sh" "$ROOT" || fail "неизвестный EXPECT_* в тесте"
bash "$ROOT/scripts/guards/selftest/test-check-expect-markers.sh" >/dev/null || fail "селфтест expect-markers"

echo "== gate: invariant-discipline (норма об инвариантах — всеобъемлюща) =="
bash "$ROOT/scripts/guards/check-invariant-discipline.sh" "$ROOT" || fail "новый инвариант на честном слове (conventions-governance)"
bash "$ROOT/scripts/guards/selftest/test-check-invariant-discipline.sh" >/dev/null || fail "селфтест стража инвариантов"
echo "== gate: sync-guards (копии стражей в пакетных репах не разошлись) =="
bash "$ROOT/scripts/tools/sync-guards-to-packages.sh" || fail "копии стражей в пакетных репах разошлись с эталоном"

echo "== gate: no-path-deps (D420 — path только под [replace]; №444) =="
bash "$ROOT/scripts/guards/check-no-path-deps.sh" "$ROOT" || fail "path-зависимость в коммитящемся манифесте/локе (D420)"

echo "== gate: single-mco-resume (№446/№447 — единственный resume-сайт в Vela) =="
bash "$ROOT/scripts/guards/check-single-mco-resume.sh" "$ROOT" || fail "посторонний mco_resume() вне fibers.h::nova_resume_fiber (№446/№447)"


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

echo "== gate: doc-examples (снятые формы в nova-примерах публикуемой доки, окно p-example-guard) =="
DOC_EXAMPLES_SHOW_MATCHES=0 bash "$ROOT/scripts/guards/check-doc-examples.sh" "$ROOT" || fail "doc-examples (дока учит снятому синтаксису — let/readonly/*ro T/*unsafe T/постфикс-!/trait-impl-throws/ref-формы/external fn/addr_of/null <тип>/#impl(<старое имя>) — см. вывод выше)"
bash "$ROOT/scripts/guards/selftest/test-check-doc-examples.sh" || fail "doc-examples selftest"

echo "== gate: test-fixture-coverage (правила 1/5 test-conventions.md — neg-фикстура на новый E_*/W_*, регресс-фикстура на закрытие маркера; реестр 221.1 №399) =="
TFC_BASE="$(git -C "$ROOT" rev-parse --verify -q HEAD~1 2>/dev/null || true)"
bash "$ROOT/scripts/guards/check-test-fixture-coverage.sh" "$ROOT" "$TFC_BASE" || fail "test-fixture-coverage (новый E_*/W_*-код без neg-фикстуры, ИЛИ строка реестра 221.1 закрыта без .nv-ссылки — см. вывод выше; WARN про registry/backlog-расхождение НЕ роняет гейт)"
bash "$ROOT/scripts/guards/selftest/test-check-test-fixture-coverage.sh" || fail "test-fixture-coverage selftest"

echo "== gate: ci-status (внешний авторитетный гейт — GitHub Actions; реестр 221.1 №395/№401/№402) =="
# НЕ блокирующий: внешний сервис бывает недоступен, и падение сети не должно
# ронять локальный гейт. Блокирует отправку хук `pre-push` (--strict).
# Смысл шага — чтобы «локально зелено» и «снаружи красно» не могли разойтись
# молча, как расходились сутки 2026-08-05/06 (24 слияния подряд без взгляда в CI).
bash "$ROOT/scripts/guards/check-ci-status.sh" || true

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
_MEGA_T0=$(date +%s)
"$NOVA" test --positive --compile-error "$ROOT/spec_tests/conformance" >"$MEGA_LOG" 2>&1
MEGA_EXIT=$?
_MEGA_SEC=$(( $(date +%s) - _MEGA_T0 ))
# ── Храповик ВРЕМЕНИ мега-CU (реестр 221.1 №437) ─────────────────────────────
# ЗАЧЕМ: №437 (замедление чекера ~4x) и №429 (регресс латентности) оба прожили
# незамеченными, потому что скорость мы меряем только когда случайно заглянем.
# Этот шаг делает время ВИДИМЫМ на каждом прогоне.
# ДИЗАЙН — ИНФОРМАЦИОННЫЙ, НЕ блокирующий: wall-time шумит (скорость машины,
# фоновая нагрузка), и жёсткий порог ложно ронял бы гейт. Урок 2026-08-07:
# «деградация чекера» оказалась частично артефактом замера под нагрузкой
# конкурирующих окон (p-stability подтвердил паритет латентности в ЧИСТОМ замере).
# Поэтому: печатаем время и дельту ВСЕГДА; ГРОМКО предупреждаем при росте >50%;
# гейт НЕ роняем (никакой fail). Порог 1.5x ловит алгоритмический скачок (×4 у
# №437 закричал бы), но переживает 20-30% шума машины.
_MEGA_TIME_BASE="$ROOT/scripts/guards/mega-cu-time.baseline"
_MEGA_BASE=$(grep -E '^seconds=' "$_MEGA_TIME_BASE" 2>/dev/null | head -1 | cut -d= -f2)
if [ -n "$_MEGA_BASE" ] && [ "$_MEGA_BASE" -gt 0 ] 2>/dev/null; then
    _MEGA_PCT=$(( (_MEGA_SEC - _MEGA_BASE) * 100 / _MEGA_BASE ))
    echo "mega-CU wall-time: ${_MEGA_SEC}s (baseline ${_MEGA_BASE}s, дельта ${_MEGA_PCT}%)"
    if [ "$_MEGA_SEC" -gt $(( _MEGA_BASE * 3 / 2 )) ]; then
        echo "  ⚠️  МЕГА-CU МЕДЛЕННЕЕ БАЗЫ БОЛЕЕ ЧЕМ НА 50% (№437-класс)." >&2
        echo "  ⚠️  Проверь нагрузку машины (tasklist | grep cargo/clang); если чисто —" >&2
        echo "  ⚠️  это НАСТОЯЩИЙ регресс скорости, замерь профиль. Гейт НЕ роняю (шум)." >&2
    fi
else
    echo "mega-CU wall-time: ${_MEGA_SEC}s (baseline не задан — см. scripts/guards/mega-cu-time.baseline)"
fi
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

# Реестр 221.1 №416 (2026-08-07): `.github/workflows/nova-lint.yml`
# (`nova-lint-std-gate`) уже гоняет `nova lint --deny std` как ЖЁСТКИЙ
# гейт на CI, но локальный gate.sh его не гонял вовсе — красный CI
# оставался невидимым до самого PR (третий случай «локальный гейт слабее
# внешнего», см. PROGRESS-p416.md). `--deny`: W→E, находки = exit≠0 самим
# CLI (не парсингом кода возврата) — см. `nova-cli/src/main.rs::cmd_lint`.
echo "== gate: nova lint --deny std/src (0 findings — 221.1 №416) =="
LINT_LOG="${TMPDIR:-/tmp}/gate_lint_$$.log"
"$NOVA" lint --deny "$ROOT/std/src" >"$LINT_LOG" 2>&1
LINT_EXIT=$?
LINT_LINE=$(sed -e "s/\[[0-9;]*m//g" "$LINT_LOG" | grep -E "^lint: .* finding\(s\)" | tail -1)
echo "lint std/src :: $LINT_LINE"
# Строка "lint: N file(s), M finding(s), K denied (--deny, exit 1)" ОБЯЗАНА
# присутствовать — краш линтера её не печатает вовсе, и голая проверка
# exit-кода молча считала бы такой прогон непроверенным «зелёным» (то же
# правило, что у mega-CU выше: PASS/FAIL строка ассертится ЯВНО).
echo "$LINT_LINE" | grep -qE "finding\(s\)" \
    || fail "nova lint std/src: строки 'N finding(s)' нет вовсе (краш? см. $LINT_LOG)"
echo "$LINT_LINE" | grep -qE ", 0 finding\(s\)" \
    || fail "nova lint std/src: находки > 0, ожидался канон 0 (см. $LINT_LOG): '$LINT_LINE'"
[ "$LINT_EXIT" -eq 0 ] || fail "nova lint --deny std/src: exit=$LINT_EXIT (см. $LINT_LOG)"

# Реестр 221.1 №416 (хвост, 2026-08-07, окно p416b): `.github/workflows/
# nova-lint.yml` также гоняет `nova lint --deny spec_tests` (89 находок были
# красными на CI, локальный gate.sh их не проверял вовсе — тот же класс
# слепоты, что уже был закрыт для std/src выше в этом же гейте). spec_tests —
# корпус ФИКСТУР: часть находок в исходных 83 была НАМЕРЕННОЙ (неканоничная
# форма — сам предмет теста), закрыта через `// nova:allow RULE -- причина`
# с обоснованием (PROGRESS-p416b.md), не игнором строки/файла целиком.
echo "== gate: nova lint --deny spec_tests (0 findings — 221.1 №416 хвост) =="
LINT_LOG2="${TMPDIR:-/tmp}/gate_lint_spec_$$.log"
"$NOVA" lint --deny "$ROOT/spec_tests" >"$LINT_LOG2" 2>&1
LINT_EXIT2=$?
LINT_LINE2=$(sed -e "s/\[[0-9;]*m//g" "$LINT_LOG2" | grep -E "^lint: .* finding\(s\)" | tail -1)
echo "lint spec_tests :: $LINT_LINE2"
echo "$LINT_LINE2" | grep -qE "finding\(s\)" \
    || fail "nova lint spec_tests: строки 'N finding(s)' нет вовсе (краш? см. $LINT_LOG2)"
echo "$LINT_LINE2" | grep -qE ", 0 finding\(s\)" \
    || fail "nova lint spec_tests: находки > 0, ожидался канон 0 (см. $LINT_LOG2): '$LINT_LINE2'"
[ "$LINT_EXIT2" -eq 0 ] || fail "nova lint --deny spec_tests: exit=$LINT_EXIT2 (см. $LINT_LOG2)"

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
