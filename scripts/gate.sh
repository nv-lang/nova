#!/bin/sh
# scripts/gate.sh — ЕДИНЫЙ авторитетный гейт (план 231 трек Д п.1).
# Запуск из корня целевого дерева (main-репа или worktree):  bash scripts/gate.sh
# Ассерты делает СКРИПТ, не интегратор: любой провал = exit 1 с внятной строкой.
#
# Состав (CLAUDE.md/dev-workflow):
#   1) cargo build --release (nova-cli)
#   2) мега-CU spec_tests/conformance ОДНИМ CU: exit=0 И строка "PASS: N  FAIL: 0" присутствует
#   3) nova check std/src (БЕЗ NOVA_STD_PATH): канон "PASS: 147  FAIL: 26  WARN: 1078"
#      (ассертится ТОЛЬКО FAIL — см. ~:212; PASS/WARN растут от новых файлов законно)
#   4) nova lint --deny std/src: канон 0 находок
#   5) nova lint --deny spec_tests: канон 0 находок (221.1 №416 хвост)
#   6) флагман examples/flagship/aggregator --strict-effects: строка "built:"
set -u
ROOT="$(pwd)"
MAIN_REPO="d:/Sources/nv-lang/nova"
export NOVA_GC_LIB_DIR="D:\\Sources\\nv-lang\\nova\\compiler-codegen\\vcpkg_installed\\x64-windows-static\\lib"
export NOVA_INCLUDE_DIR="D:\\Sources\\nv-lang\\nova\\compiler-codegen\\vcpkg_installed\\x64-windows-static\\include"
unset NOVA_STD_PATH 2>/dev/null || true

# КОПИМ отказы вместо выхода на первом (2026-08-09, требование владельца
# «почти каждый шаг можно делать в фоне» + наблюдение: гейт падал на ПЕРВОМ же
# страже и не доходил до остальных, из-за чего интегратор четырежды подряд
# чинил по одной находке, тратя по 40-минутному прогону на каждую).
#
# Теперь стражи-проверки копят ошибки и гейт сообщает ВСЕ разом; выход
# происходит один раз, перед дорогими шагами (мега-CU) — незачем тратить
# 37 минут, если дерево уже не проходит дешёвые проверки.
GATE_FAILS=""
GATE_FAIL_N=0

# ОТМЕТКИ ВРЕМЕНИ — свойство ГЕЙТА, а не способа запуска (2026-08-09).
#
# Раньше время в лог ставила обёртка gate-bg.sh, и прогон, запущенный напрямую,
# оказывался непрофилируемым: владелец просил профиль, а данных не было вовсе.
# Профиль не должен зависеть от того, кто как позвал скрипт, поэтому отметку
# ставит сам шаг. Формат читает scripts/tools/gate-profile.sh.
# ESC не литералом (2026-08-12, страж check-no-control-chars): невидимый
# управляющий байт внутри регулярки — форма, которую съедает любой копипаст,
# и тогда снятие цветовых кодов молча перестаёт работать, а `PASS:` перестаёт
# находиться. Байт задаётся явно и один раз.
ESC=$(printf '\033')
GATE_T0=$(date +%s)
step() {
    printf '[%5ds] == gate: %s ==\n' "$(( $(date +%s) - GATE_T0 ))" "$1"
}
fail() {
    echo "GATE FAIL: $1" >&2
    GATE_FAILS="$GATE_FAILS
  * $1"
    GATE_FAIL_N=$((GATE_FAIL_N + 1))
}
# Барьер: вызывается там, где продолжать бессмысленно либо дорого.
gate_barrier() {
    if [ "$GATE_FAIL_N" -gt 0 ]; then
        echo "" >&2
        echo "GATE: отказов на этом рубеже — $GATE_FAIL_N:$GATE_FAILS" >&2
        exit 1
    fi
}
# `fatal` — для случаев, где продолжать НЕЛЬЗЯ (нет бинаря, сломано окружение).
fatal() { echo "GATE FATAL: $1" >&2; exit 1; }

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

step "arch-ratchet"
bash "$ROOT/scripts/guards/arch-ratchet.sh" || fail "arch-ratchet (emit_c growth)"

# Реестр 221.1 №138 (урок 2026-07-27): копия рантайма внутри пакетной репы/
# worktree не под git → её протухание невидимо, и она ШАДОВИТ настоящий
# рантайм. Реальная цена промаха: полтора часа диагностики «регрессии
# компилятора» в nova-http, которой не было (устаревшие заголовки копии не
# объявляли символ из фикса №108), плюс >1 ГБ мусора по репам. Копия НЕ нужна —
# есть штатные NOVA_RT_DIR/NOVA_CG_INCLUDE (см. шапку самого стража).
step "no-runtime-copy"
bash "$ROOT/scripts/guards/check-no-runtime-copy.sh" || fail "копия рантайма в пакетной репе/worktree (№138)"

# Трек Ж (231): страж без самотеста — доверие на слово. Самотесты дешёвые
# (секунды) и проверяют ОБА свойства: ловит нарушение / не даёт ложняка.
# Реестр 221.1 №155/№161 (урок повторился ДВАЖДЫ за день): маркер [M-...] в коде без
# записи в реестре = невидимый долг — обход живёт, а дефекта для планирования нет.
# Первый прогон стража нашёл 59 таких; ручные аудиты видели 8. Храповик: расти нельзя.
step "marker-registry-sync"
bash "$ROOT/scripts/guards/check-marker-registry-sync.sh" "$ROOT" || fail "маркеры в коде без записи в реестре (№155/№161)"
step "bug-number-sync (№217 — каждый новый маркер нумерован в 221.1)"
bash "$ROOT/scripts/guards/check-bug-number-sync.sh" "$ROOT" || fail "новый маркер без № в 221.1 (правило владельца №217)"

# Реестр 221.1 №446/№447 (окно presume-cas-gate, 2026-08-08): единственный
# mco_resume в рантайме держит структурный инвариант «ни одно действие над
# co не выполняется вне выигранного CAS» — было соглашением на 4 сайтах,
# два из которых несли живой дефект (двойной destroy+sweep дубликата
# мёртвого co). Страж ловит новый resume-сайт, открытый в обход
# nova_resume_fiber (fibers.h).
step "expect-markers (неизвестный EXPECT_* раннер молча игнорирует — №453)"
step "накопление несведённых веток (никогда не копи)"
step "форма записей реестра (класс, приоритет, оговорка)"
bash "$ROOT/scripts/guards/check-registry-entry-shape.sh" "$ROOT" || fail "запись реестра без класса/приоритета/оговорки"

bash "$ROOT/scripts/guards/check-no-accumulation.sh" "$ROOT" || fail "накопление выросло: замершие несведённые ветки"

bash "$ROOT/scripts/guards/check-expect-markers.sh" "$ROOT" || fail "неизвестный EXPECT_* в тесте"

step "nova:expect — храповик разметки негативных фикстур (план 262 Б)"
# Файловый EXPECT_COMPILE_ERROR говорит «где-то в этом файле ошибка», и
# фикстура остаётся зелёной, даже когда ошибка переехала на другую строку
# по другой причине. `nova:expect` пришпиливает ожидание к МЕСТУ. Разом
# разметить 550 фикстур нельзя — храповик не даёт их числу расти.
bash "$ROOT/scripts/guards/check-nova-expect-ratchet.sh" "$ROOT" \
    || fail "неразмеченных негативных фикстур стало больше (план 262 Б)"

step "doc-truth (нормативная дока врёт именем EXPECT_* или неисполнимой командой — №455)"
bash "$ROOT/scripts/guards/check-doc-truth.sh" "$ROOT" || fail "неизвестный EXPECT_* или неисполнимая команда nova в AGENTS.md/docs/dev(/docs/guide для маркеров)"

step "invariant-discipline (норма об инвариантах — всеобъемлюща)"
# №475: ИМЕННО этот страж завис 2026-08-08 на 4 часа (grep-цикл по всему
# дереву, включая target/ и чужие worktree) и держал gate.sh мёртвым, пока
# никто не смотрел. `with-deadline.sh` появился как раз ради этого класса,
# но применён был только к сетевым/дорогим шагам ниже — сам виновник
# инцидента остался незавёрнутым. Предел 300с — тот же бюджет, что у
# per-guard селфтестов ниже (:259/:463) и тот, в который страж после
# переписи на awk укладывается за секунды на реальном дереве.
bash "$ROOT/scripts/tools/with-deadline.sh" 300 \
    bash "$ROOT/scripts/guards/check-invariant-discipline.sh" "$ROOT" || fail "новый инвариант на честном слове (conventions-governance), ЛИБО страж не уложился в 300с (№475 — зависший страж ничего не доказал)"
step "sync-guards (копии стражей в пакетных репах не разошлись)"
bash "$ROOT/scripts/tools/sync-guards-to-packages.sh" || fail "копии стражей в пакетных репах разошлись с эталоном"

step "no-path-deps (D420 — path только под [replace]; №444)"
bash "$ROOT/scripts/guards/check-no-path-deps.sh" "$ROOT" || fail "path-зависимость в коммитящемся манифесте/локе (D420)"

step "single-mco-resume (№446/№447 — единственный resume-сайт в Vela)"
bash "$ROOT/scripts/guards/check-single-mco-resume.sh" "$ROOT" || fail "посторонний mco_resume() вне fibers.h::nova_resume_fiber (№446/№447)"



step "doc-hygiene (язык/чистота публичной доки, правило владельца 2026-07-31)"
bash "$ROOT/scripts/guards/check-doc-hygiene.sh" "$ROOT" || fail "doc-hygiene (кириллица/внутренние ссылки в /// или линте — рост запрещён)"

step "doc-conventions (docs/dev/doc-conventions.md enforcement, Plan 242)"
# №322: вторым аргументом — база сравнения для подпроверки same-commit
# pairing (без неё она физически не выполняется). Локально берём
# предыдущий коммит; в CI база передаётся явно (PR base / event.before).
# №586: раньше при неудаче вычисления HEAD~1 сюда молча уезжала пустая
# строка, check-doc-conventions.sh легитимно пропускал guide_same_commit
# (для СЕБЯ это «не передали диапазон» — нормальный кросс-репный случай),
# и гейт печатал зелёный пропуск там, где вызывающий обязан был дать базу.
# require-diff-base.sh делает эту неудачу ОТКАЗОМ шага, а не тихим пропуском.
DOC_GUARD_BASE="$(bash "$ROOT/scripts/tools/require-diff-base.sh" "$ROOT")" \
    || fail "doc-conventions: не вычислить diff-base для guide_same_commit (см. scripts/tools/require-diff-base.sh) — подпроверка не может выполниться"
bash "$ROOT/scripts/guards/check-doc-conventions.sh" "$ROOT" "$DOC_GUARD_BASE" || fail "doc-conventions (шапка/frontmatter spec en, guide-парность, статус-строка плана, dev-ссылки, код-блоки пар — см. вывод выше)"

step "doc-examples (снятые формы в nova-примерах публикуемой доки, окно p-example-guard)"
DOC_EXAMPLES_SHOW_MATCHES=0 bash "$ROOT/scripts/guards/check-doc-examples.sh" "$ROOT" || fail "doc-examples (дока учит снятому синтаксису — let/readonly/*ro T/*unsafe T/постфикс-!/trait-impl-throws/ref-формы/external fn/addr_of/null <тип>/#impl(<старое имя>) — см. вывод выше)"

step "test-fixture-coverage (правила 1/5 test-conventions.md — neg-фикстура на новый E_*/W_*, регресс-фикстура на закрытие маркера; реестр 221.1 №399)"
# №586-класс: та же ошибка, что у DOC_GUARD_BASE выше — пустая база молча
# уезжала в подпроверку, которая для СЕБЯ легитимно пропускает rule5/rule1,
# и отказ вызывающего тонул в чужом легитимном пропуске.
TFC_BASE="$(bash "$ROOT/scripts/tools/require-diff-base.sh" "$ROOT")" \
    || fail "test-fixture-coverage: не вычислить diff-base (см. scripts/tools/require-diff-base.sh) — rule5/rule1 не могут выполниться"
bash "$ROOT/scripts/guards/check-test-fixture-coverage.sh" "$ROOT" "$TFC_BASE" || fail "test-fixture-coverage (новый E_*/W_*-код без neg-фикстуры, ИЛИ строка реестра 221.1 закрыта без .nv-ссылки — см. вывод выше; WARN про registry/backlog-расхождение НЕ роняет гейт)"

step "ci-status (внешний авторитетный гейт — GitHub Actions; реестр 221.1 №395/№401/№402)"
# НЕ блокирующий: внешний сервис бывает недоступен, и падение сети не должно
# ронять локальный гейт. Блокирует отправку хук `pre-push` (--strict).
# Смысл шага — чтобы «локально зелено» и «снаружи красно» не могли разойтись
# молча, как расходились сутки 2026-08-05/06 (24 слияния подряд без взгляда в CI).
# Предел 120с: это сетевой запрос, а не вычисление. Недоступный GitHub не
# должен превращаться в зависший гейт.
bash "$ROOT/scripts/tools/with-deadline.sh" 120 \
    bash "$ROOT/scripts/guards/check-ci-status.sh" || true

# Срок 60→240 (2026-08-11): шаг упёрся в предел и стал отказом «шаг ничего не
# доказал» (№475) — не из-за кириллицы, а потому что история, которую он
# просматривает, выросла.
#
# ДОЛГ ЗАКРЫТ 2026-08-12: страж СДЕЛАН ИНКРЕМЕНТАЛЬНЫМ, а срок в третий раз НЕ
# поднят. 2026-08-12 он снова упёрся (4 мин 23 с при пределе 240 с), и это был
# ровно тот случай, о котором предупреждала прежняя запись. Теперь отметка
# «проверено до» лежит в `.git/` (не версионируется) и ставится ТОЛЬКО на
# зелёном исходе; если отметка не предок HEAD — диапазон берётся полный.
# Замер: полный прогон 4 мин 23 с, следующий за ним — 0,7 с. Логика проверки
# не изменилась ни на букву, самотест стража 5/5.
#
# Срок 240 оставлен как ЗАПАС на первый прогон в свежем клоне, где отметки
# ещё нет: там честно нужны минуты, и это не повод считать шаг недоказанным.
step "язык сообщений коммитов (норма 2026-08-09 — по-английски)"
bash "$ROOT/scripts/tools/with-deadline.sh" 240 \
    bash "$ROOT/scripts/guards/check-commit-language.sh" "$ROOT" \
    || fail "кириллица в сообщениях коммитов после точки перехода"

step "устаревшие пометки «не реализован» в спеке (№557)"
bash "$ROOT/scripts/guards/check-stale-unimplemented.sh" "$ROOT" \
    || fail "спека объявляет нереализованным то, что план считает сделанным"

step "рабочие деревья только в d:/Sources/nv-lang (№561)"
bash "$ROOT/scripts/guards/check-worktree-location.sh" "$ROOT" \
    || fail "worktree вне дозволенного корня"

step "страница правил называет всех стражей (№560)"
bash "$ROOT/scripts/guards/check-rules-page-complete.sh" "$ROOT" \
    || fail "страж без объяснения на странице правил"

# №565: локальный гейт судит НЕ ТО ДЕРЕВО, что судит внешний мир — локально
# активен `nova.override.toml`, которого в коммите нет. Шаг собирает флагман во
# временном дереве из HEAD. Дорогой (минуты), поэтому под сроком.
step "флагман собирается на ЧИСТОМ дереве из HEAD (№565)"
bash "$ROOT/scripts/tools/with-deadline.sh" 600 \
    bash "$ROOT/scripts/guards/check-clean-checkout-build.sh" "$ROOT" \
    || fail "на чистом дереве флагман не собирается (dev-override прячет расхождение)"

# №578: флаг, который никто не взводит и нигде не описывает, снаружи
# неотличим от несделанной работы (прецедент — №575, много-TU).
# №612: второго индекса планов не бывает — рукописная сводка расходится молча.
step "второго индекса планов нет (№612)"
bash "$ROOT/scripts/guards/check-no-handwritten-plan-index.sh" "$ROOT" \
    || fail "рукописная сводка планов вернулась"

# №608/№609: публичная страница на двух языках, и имя стороны не лжёт.
step "публичные страницы парны и на своих языках (№608, №609)"
bash "$ROOT/scripts/guards/check-doc-language-pairs.sh" "$ROOT" \
    || fail "страницы без пары или сторона не на своём языке"

# №TBD-plan-dup: один и тот же абзац в двух разделах плана — план,
# который начнёт врать по частям.
step "дословные повторы между разделами планов"
python "$ROOT/scripts/guards/check-plan-duplication.py" "$ROOT" \n    || fail "дословный повтор между разделами плана вырос"

# №TBD-secrets: секрет вреден самим фактом попадания в историю.
step "секреты в дереве (ключи, токены, пароль внутри URL)"
bash "$ROOT/scripts/guards/check-staged-secrets.sh" --tree "$ROOT" \n    || fail "секрет в дереве"

# №TBD-control-chars: невидимый управляющий байт в исходнике — код, который
# читается верным и работает неверным (перенесено из соседнего проекта).
step "невидимые управляющие байты в исходниках"
bash "$ROOT/scripts/guards/check-no-control-chars.sh" "$ROOT" \
    || fail "управляющие байты в исходниках"

# №607: корень публичного репозитория — витрина, а не стол.
step "в корне репозитория только предусмотренное (№607)"
bash "$ROOT/scripts/guards/check-repo-root-clean.sh" "$ROOT" \
    || fail "в корне лежит непредусмотренное"

# №597: код возврата обёртки — не код возврата сборки.
step "фоновая сборка проверяет результат, а не код обёртки (№597)"
bash "$ROOT/scripts/guards/check-background-build-verified.sh" "$ROOT" \
    || fail "фоновая сборка без проверки результата"

# №594: реестр говорит «закрыто», а ветки в `main` нет — следующий будет
# искать несуществующее. 2026-08-11 так пролежала ветка плана 270.
step "принятая в реестре работа влита (№594)"
bash "$ROOT/scripts/guards/check-accepted-branch-merged.sh" "$ROOT" \
    || fail "работа принята в реестре, но её ветка не влита"

step "у каждого флага NOVA_* есть вызывающий или описание (№578)"
bash "$ROOT/scripts/guards/check-flag-has-caller.sh" "$ROOT" \
    || fail "флаг NOVA_* без вызывающего и без описания"

step "лицензионная гигиена (манифесты объявляют лицензию, подмодули названы — №556)"
bash "$ROOT/scripts/guards/check-license-hygiene.sh" "$ROOT" \
    || fail "манифест без лицензии либо вендоренный подмодуль без уведомления"

step "язык манифестов (nova.toml / nova.lock.toml — по-английски, норма 2026-08-10)"
bash "$ROOT/scripts/guards/check-manifest-language.sh" "$ROOT" \
    || fail "кириллица в манифесте пакета"

step "самотесты стражей (все из каталога, по одному разу)"
# ЕДИНСТВЕННОЕ место, где они запускаются. Каталог обходится целиком,
# поэтому новый самотест подхватывается сам — дописывать его в gate.sh
# не нужно и, значит, нельзя забыть.
for st in "$ROOT"/scripts/guards/selftest/test-*.sh; do
    [ -e "$st" ] || continue
    # Самотест — проверка проверки. Не уложился в срок — он не медленный,
    # он сломан (№475).
    #
    # 180 -> 300 (2026-08-10, реестр №558). Мера ВРЕМЕННАЯ и названа как долг,
    # а не как новая норма: `test-check-doc-examples.sh` идёт 207 секунд на
    # ненагруженной машине и потому падал по сроку ВНУТРИ полного прогона,
    # оставаясь зелёным поодиночке. Ложно-красный гейт хуже отсутствующего:
    # его начинают читать выборочно. Приёмка №558 — ни один самотест не
    # требует больше минуты, и это ИЗМЕРЕНО; тогда срок вернётся вниз.
    bash "$ROOT/scripts/tools/with-deadline.sh" 300 bash "$st" \
        || fail "самотест стража: $(basename "$st")"
done

step "cargo build --release"
( cd "$ROOT/nova-cli" && cargo build --release ) || fail "cargo build"
NOVA="$ROOT/nova-cli/target/release/nova.exe"
[ -x "$NOVA" ] || fail "nova.exe not found: $NOVA"

# РУБЕЖ ПЕРЕД ДОРОГИМ ШАГОМ. Всё выше — дешёвые стражи (секунды); мега-CU
# идёт около 37 минут. Если дерево не прошло дешёвое, тратить их незачем,
# но и обрывать на ПЕРВОЙ находке незачем тоже — здесь сообщаются ВСЕ.
gate_barrier

step "mega-CU (spec_tests/conformance, one CU)"
MEGA_LOG="${TMPDIR:-/tmp}/gate_mega_$$.log"
_MEGA_T0=$(date +%s)
# `--jobs` = половина ядер, а НЕ все (разведка p259-gate-speed, 2026-08-09).
# Замер на одном и том же чанке из 193 работ: `--jobs 8` — стенка 160 с при
# сумме занятости 1049 с; `--jobs 16` — стенка 154 с при сумме 2054 с. То есть
# удвоение воркеров даёт +4% пропускной способности и РОВНО ВДВОЕ худшую
# латентность каждой работы. А длительность шага определяется длинным полюсом —
# одной работой, — и его шестнадцать воркеров тормозят вдвое ни за что.
MEGA_JOBS="${NOVA_GATE_JOBS:-$(( $(nproc 2>/dev/null || echo 8) / 2 ))}"
[ "$MEGA_JOBS" -ge 1 ] 2>/dev/null || MEGA_JOBS=4
echo "mega-CU :: --jobs $MEGA_JOBS"
"$NOVA" test --positive --compile-error --jobs "$MEGA_JOBS" "$ROOT/spec_tests/conformance" >"$MEGA_LOG" 2>&1
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
MEGA_LINE=$(sed -e "s/${ESC}\[[0-9;]*m//g" "$MEGA_LOG" | grep -E "PASS: [0-9]+ +FAIL: [0-9]+" | tail -1)
echo "mega-CU exit=$MEGA_EXIT :: $MEGA_LINE"
# -- Храповик ЧИСЛА SKIP мега-CU (реестр 221.1 №453б) -------------------------
# ЗАЧЕМ: гейт ассертил только PASS/FAIL -- уехавшая из корпуса фикстура (не
# компилируется вовсе, лейн-исключена без видимой строки, неизвестный
# EXPECT_* тихо не проверяет ничего) не даёт ни PASS, ни FAIL и была НЕВИДИМА
# ПРИНЦИПИАЛЬНО (№453). SKIP-строка печатается ("PASS: N  FAIL: M  SKIP: K
# (skipped)") ТОЛЬКО когда K>0 -- при K==0 хвост "SKIP: ..." в строке
# отсутствует вовсе, поэтому явно дефолтим на 0, а не считаем это ошибкой.
# В ОТЛИЧИЕ от mega-cu-time.baseline (шум машины -> только предупреждение) --
# рост SKIP ЖЁСТКО роняет гейт: снижение (фикстуру осознанно убрали/перевели
# лейн) принимается, база обновляется вручную с летописью в baseline-файле.
_MEGA_SKIP=$(echo "$MEGA_LINE" | grep -oE "SKIP: [0-9]+" | grep -oE "[0-9]+" | head -1)
[ -n "$_MEGA_SKIP" ] || _MEGA_SKIP=0
_MEGA_SKIP_BASE_FILE="$ROOT/scripts/guards/mega-cu-skip.baseline"
_MEGA_SKIP_BASE=$(grep -E '^skips=' "$_MEGA_SKIP_BASE_FILE" 2>/dev/null | head -1 | cut -d= -f2)
if [ -n "$_MEGA_SKIP_BASE" ] && [ "$_MEGA_SKIP_BASE" -ge 0 ] 2>/dev/null; then
    echo "mega-CU SKIP: ${_MEGA_SKIP} (baseline ${_MEGA_SKIP_BASE})"
    if [ "$_MEGA_SKIP" -gt "$_MEGA_SKIP_BASE" ]; then
        fail "mega-CU SKIP вырос: ${_MEGA_SKIP} > baseline ${_MEGA_SKIP_BASE} (№453-класс -- фикстуры молча уехали из гейта; см. $_MEGA_SKIP_BASE_FILE и $MEGA_LOG). Если рост осознанный (лейн/маркер сменился законно) -- обнови skips= в baseline-файле с летописью."
    fi
else
    fail "mega-CU SKIP: baseline не задан или некорректен ($_MEGA_SKIP_BASE_FILE) -- храповик №453(б) не может проверить рост SKIP"
fi
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

step "check std/src (byte-canon)"
STD_LINE=$("$NOVA" check "$ROOT/std/src" 2>&1 | sed -e "s/${ESC}\[[0-9;]*m//g" | grep -E "^PASS" | tail -1)
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
# Реестр 221.1 №591/№402 (2026-08-11): локальный гейт гонял ТОЛЬКО
# `nova check std`, а CI гоняет `nova test std` — и сегодня CI показал
# `PASS: 67 FAIL: 8` там, где локальный видел свои двадцать шесть
# neg-фикстур и считал дерево здоровым. Два гейта проверяли разное и
# расходились молча; красное на `main` держалось неизвестно сколько
# прогонов, пока не заговорил `check-ci-status` (№585).
#
# ПОЧЕМУ ХРАПОВИК, А НЕ НОЛЬ. Восемь отказов существуют прямо сейчас, и
# среди них есть настоящие дефекты рантайма (`std/src/net/addr`,
# `d324_os_env_args_cwd_test`). Гейт, красный с самого рождения шага,
# начинают читать выборочно — ровно то, чем кончился №475. Храповик
# запрещает РОСТ и требует, чтобы число падало.
#
# ПОЧЕМУ НЕ `--jobs 1`: CI гоняет со своим параллелизмом, и мы ловим
# именно расхождение с CI, а не собственный удобный режим.
# Реестр 221.1 №591/№402 (2026-08-11): локальный гейт гонял ТОЛЬКО
# `nova check std`, а CI гоняет `nova test std` — и CI показал отказы там,
# где локальный видел здоровое дерево. Два гейта проверяли разное.
#
# СРАВНИВАЕМ ИМЕНА, А НЕ СЧИТАЕМ ШТУКИ. Счётчик прячет ПОДМЕНУ, и это не
# теория: в первый же прогон локально оказалось семь отказов против восьми на
# CI — но пересечение неполное (два отказа только локальные, два только на CI).
# Счётчик сказал бы «7 <= 8, всё хорошо» и скрыл бы `reflect_test`, который
# до слияния группы M не падал вовсе.
step "nova test std/src (то же, что гоняет CI — №591/№402)"
STD_TEST_BASE_FILE="$ROOT/scripts/guards/std-test-fail.baseline"
STD_TEST_LOG="${TMPDIR:-/tmp}/gate_std_test_$$.log"
"$NOVA" test "$ROOT/std/src" > "$STD_TEST_LOG" 2>&1
STD_TEST_LINE=$(sed -e "s/\[[0-9;]*m//g" "$STD_TEST_LOG" | grep -E "^PASS: " | tail -1)
echo "std test :: $STD_TEST_LINE"
if [ -z "$STD_TEST_LINE" ]; then
    fail "nova test std: нет строки итога — шаг ничего не доказал (№475)"
else
    STD_TEST_NOW="${TMPDIR:-/tmp}/gate_std_now_$$.txt"
    sed -e "s/\[[0-9;]*m//g" "$STD_TEST_LOG" \
        | grep -aE "^(RUN-FAIL|CC-FAIL)" \
        | awk '{print $2}' | sort -u > "$STD_TEST_NOW"
    if [ ! -f "$STD_TEST_BASE_FILE" ]; then
        fail "nova test std: нет базы $STD_TEST_BASE_FILE"
    else
        # Каждое имя в базе обязано нести номер записи (`№NNN`): имя без
        # номера — отложенный дефект без следа, а не фон (см. №599: №559 был
        # пронумерован сутками раньше и всё равно оказался невидим).
        UNLINKED=$(grep -vE '^[[:space:]]*#' "$STD_TEST_BASE_FILE" \
                   | grep -vE '^[[:space:]]*$' | grep -v '№' || true)
        if [ -n "$UNLINKED" ]; then
            echo "$UNLINKED" | sed 's/^/    БЕЗ НОМЕРА ЗАПИСИ: /'
            fail "nova test std: имя в базе без ссылки на запись реестра"
        fi
        grep -vE '^[[:space:]]*#' "$STD_TEST_BASE_FILE" | grep -vE '^[[:space:]]*$' \
            | sed 's/[[:space:]]*#.*//' | sed 's/[[:space:]]*$//' | sort -u \
            > "${TMPDIR:-/tmp}/gate_std_base_$$.txt"
        NEWLY=$(comm -23 "$STD_TEST_NOW" "${TMPDIR:-/tmp}/gate_std_base_$$.txt")
        GONE=$(comm -13 "$STD_TEST_NOW" "${TMPDIR:-/tmp}/gate_std_base_$$.txt")
        if [ -n "$NEWLY" ]; then
            echo "$NEWLY" | sed 's/^/    НОВЫЙ ОТКАЗ: /'
            fail "nova test std: отказ, которого нет в базе (подмена или регресс)"
        fi
        [ -n "$GONE" ] && echo "$GONE" | sed 's/^/    почищено (убери из базы): /'
        rm -f "${TMPDIR:-/tmp}/gate_std_base_$$.txt"
    fi
    rm -f "$STD_TEST_NOW"
fi
rm -f "$STD_TEST_LOG"

step "nova lint --deny std/src (0 findings — 221.1 №416)"
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
step "nova lint --deny spec_tests (0 findings — 221.1 №416 хвост)"
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

step "flagship examples --strict-effects (цели из общего с CI списка)"
# ПЯТЬ целей, а не одна. 2026-08-09: локальный гейт собирал только aggregator,
# CI собирает пять — и первым же прогоном покраснел на examples/tls/echo_server.nv
# (`undefined identifier session`, остаток переименования). Гейт, который слабее
# внешнего, не гейт: 147 коммитов ушли «на зелёном».
#
# Проверка — по КОДУ ВОЗВРАТА. Прежняя искала подстроку `built:` в ПОСЛЕДНЕЙ
# строке вывода, а вывод кончается предупреждениями чаще, чем строкой сборки.
# Та же ловушка стоила интегратору ложного «все четыре ОК»: `rc=$?` после
# конвейера с `sed` возвращает код sed, а не компилятора.
# Список — ФАЙЛОМ, общим с CI (2026-08-10). До этого он был записан дважды —
# здесь и в workflow — и ничто не проверяло совпадение копий; тот же класс, что
# №509/№524. Теперь добавление цели — одна строка в одном файле.
FLAGSHIP_LIST="$ROOT/scripts/guards/flagship-targets.txt"
[ -f "$FLAGSHIP_LIST" ] || fail "нет списка флагман-целей $FLAGSHIP_LIST"
FLAG_FAILED=""
FLAG_N=0
tr -d "$(printf '\r')" < "$FLAGSHIP_LIST" > "${TMPDIR:-/tmp}/gate_flagship_list_$$.txt"
while read -r _fname _t _rest; do
    case "$_fname" in ''|\#*) continue ;; esac
    [ -n "$_t" ] || { echo "flagship :: строка без пути: $_fname"; FLAG_FAILED="$FLAG_FAILED $_fname"; continue; }
    [ -f "$ROOT/$_t" ] || { echo "flagship :: НЕТ ФАЙЛА $_t"; FLAG_FAILED="$FLAG_FAILED $_fname"; continue; }
    FLAG_N=$((FLAG_N + 1))
    _flog="${TMPDIR:-/tmp}/gate_flagship_$$.log"
    "$NOVA" build "$ROOT/$_t" --strict-effects >"$_flog" 2>&1
    if [ $? -eq 0 ]; then
        echo "flagship ok :: $_t"
    else
        echo "flagship FAIL :: $_t"
        grep -m3 "error:" "$_flog" | sed 's/^/    /'
        FLAG_FAILED="$FLAG_FAILED $_t"
    fi
    rm -f "$_flog"
done < "${TMPDIR:-/tmp}/gate_flagship_list_$$.txt"
rm -f "${TMPDIR:-/tmp}/gate_flagship_list_$$.txt"
echo "flagship :: собрано $FLAG_N"
[ -z "$FLAG_FAILED" ] || fail "flagship examples не собрались:$FLAG_FAILED"

step "flagship smoke (не собрать, а ЗАПУСТИТЬ — реестр 221.1 №548)"
# Сборка ловит синтаксис и типы, но НЕ то, работает ли программа. Флагман-мост
# вошёл в гейт 2026-08-10 и в тот же день оказался неработающим: туннель
# открывался, и мост тут же сбрасывал соединение (корень — №552, дизарм не знал
# встроенных конструкторов). Гейт этого не видел, потому что собирал и не
# запускал. Родня №402 (гоняли `nova check std`, но не `nova test std`).
#
# Смоук обязан быть БЕЗ СЕКРЕТОВ: прежний требовал живого прокси с паролем и
# потому не гонялся никогда — о поломке узнали от постороннего наблюдения.
# Каждый скрипт ниже поднимает своё окружение сам (локальные стенды, loopback).
FLAG_SMOKES="
examples/flagship/http_proxy_chain/tools/smoke.sh
"
SMOKE_FAILED=""
SMOKE_N=0
for _s in $FLAG_SMOKES; do
    [ -f "$ROOT/$_s" ] || { echo "smoke :: НЕТ ФАЙЛА $_s"; SMOKE_FAILED="$SMOKE_FAILED $_s"; continue; }
    SMOKE_N=$((SMOKE_N + 1))
    _slog="${TMPDIR:-/tmp}/gate_smoke_$$.log"
    bash "$ROOT/scripts/tools/with-deadline.sh" 300 bash "$ROOT/$_s" >"$_slog" 2>&1
    if [ $? -eq 0 ]; then
        echo "smoke ok :: $_s"
    else
        echo "smoke FAIL :: $_s"
        tail -5 "$_slog" | sed 's/^/    /'
        SMOKE_FAILED="$SMOKE_FAILED $_s"
    fi
    rm -f "$_slog"
done
echo "smoke :: прогнано $SMOKE_N"
[ -z "$SMOKE_FAILED" ] || fail "флагман-смоук красный:$SMOKE_FAILED"

# ── ПАРИТЕТ С CI (реестр 221.1 №516) ────────────────────────────────────────
# Ниже — шаги, которые CI считает блокирующими, а локальный гейт не делал.
# Пока их не было, «локально зелено» означало меньше, чем выглядело: 147
# коммитов ушли на зелёном локальном гейте и покраснели в CI первым прогоном.

step "examples anti-rot (весь examples/** по списку 197 Ф.5, как в CI)"
# Список целей читается ИЗ ТОГО ЖЕ файла, что использует CI, — иначе гейт и CI
# разойдутся молча, а это ровно тот дефект, который здесь и чинится.
ANTIROT_LIST="$ROOT/docs/plans/wip/197-f5-gate-list.txt"
if [ ! -f "$ANTIROT_LIST" ]; then
    fail "нет списка anti-rot $ANTIROT_LIST (CI читает его же)"
else
    tr -d "$(printf '\r')" < "$ANTIROT_LIST" > "${TMPDIR:-/tmp}/gate_antirot_list_$$.txt"
    ANTIROT_FAILED=""
    ANTIROT_N=0
    while read -r _kind _path; do
        case "$_kind" in BUILD|CHECK) ;; *) continue ;; esac
        ANTIROT_N=$((ANTIROT_N + 1))
        _alog="${TMPDIR:-/tmp}/gate_antirot_$$.log"
        if [ "$_kind" = "BUILD" ]; then
            "$NOVA" build "$ROOT/$_path" --strict-effects \
                -o "${TMPDIR:-/tmp}/ex_$(basename "$_path" .nv).exe" >"$_alog" 2>&1
        else
            "$NOVA" check "$ROOT/$_path" --strict-effects >"$_alog" 2>&1
        fi
        if [ $? -ne 0 ]; then
            echo "anti-rot FAIL :: $_kind $_path"
            grep -m2 "error:" "$_alog" | sed 's/^/    /'
            ANTIROT_FAILED="$ANTIROT_FAILED $_path"
        fi
        rm -f "$_alog"
    # `tr -d` обязателен: у списка СМЕШАННЫЕ окончания строк, и без очистки
    # `$_path` приходит с невидимым возвратом каретки — nova отвечает
    # `path not found` на путь, который в выводе выглядит совершенно верным.
    # Поймано 2026-08-09: все 32 цели падали в гейте и проходили вручную,
    # потому что вручную я читал грепнутое подмножество, а не файл целиком.
    done < "${TMPDIR:-/tmp}/gate_antirot_list_$$.txt"
    rm -f "${TMPDIR:-/tmp}/gate_antirot_list_$$.txt"
    echo "anti-rot :: целей проверено $ANTIROT_N"
    [ -z "$ANTIROT_FAILED" ] || fail "examples anti-rot:$ANTIROT_FAILED"
fi

step "lint registry self-test (правило срабатывает И не даёт ложняка, как в CI)"
# ДВЕ стороны, и вторая важнее: страж, переставший ловить, выглядит так же,
# как страж, которому нечего ловить.
# `--deny` ОБЯЗАТЕЛЕН: без него находки информационные и код возврата 0 — так
# устроен инструмент (nova-cli/src/main.rs:2866). Шаг CI требовал код 1 от
# команды БЕЗ `--deny` и потому был красным с самого введения флага: самотест
# реестра не проверял ничего, и под ним спокойно жило ложное срабатывание
# (реестр 221.1 №519 и №520).
"$NOVA" lint --deny "$ROOT/spec_tests/conformance/lint/conv_pos.nv" >/dev/null 2>&1
if [ $? -eq 0 ]; then
    fail "conv_pos.nv БОЛЬШЕ НЕ даёт находок — conv-правило перестало срабатывать"
fi
"$NOVA" lint --deny "$ROOT/spec_tests/conformance/lint/conv_clean.nv" >/dev/null 2>&1 \
    || fail "conv_clean.nv даёт находки — ложное срабатывание conv-правила (№520)"

step "nova build smoke (ICE-храповик плана 196, как в CI)"
SMOKE_NV="${TMPDIR:-/tmp}/nova_build_smoke_$$.nv"
printf 'fn main() {\n    println("hello, nova build")\n}\n' > "$SMOKE_NV"
"$NOVA" build "$SMOKE_NV" -o "${TMPDIR:-/tmp}/nova_build_smoke_$$.exe" >/dev/null 2>&1 \
    || fail "nova build smoke не собрался (ICE-регресс, план 196)"
rm -f "$SMOKE_NV" "${TMPDIR:-/tmp}/nova_build_smoke_$$.exe"

step "lint W_LEADING_BINOP_CONTINUATION по nova_tests (как в CI)"
"$NOVA" lint --rule W_LEADING_BINOP_CONTINUATION "$ROOT/nova_tests" >/dev/null 2>&1 \
    || fail "W_LEADING_BINOP_CONTINUATION даёт находки в nova_tests"

# ОСТАВШИЙСЯ ПРОБЕЛ, названный вслух: `nova doc --check` / `nova doc --test`
# (workflow nova-doc.yml) локально НЕ прогоняются. Этот job в CI сейчас красный,
# и прежде чем ставить его в гейт, надо разобрать его отказ — иначе гейт
# краснеет по чужому долгу и перестаёт быть сигналом. Записано в
# docs/dev/prompts/integrator-queue.md.

step "пакетные репозитории (план 261 Ф.3 — №524)"
# Список — ФАЙЛОМ, общим с CI. Перечисление в скрипте
# протекает (№509).
PKG_LIST="$ROOT/scripts/guards/package-repos.txt"
if [ ! -f "$PKG_LIST" ]; then
    fail "нет списка пакетов $PKG_LIST (план 261 Ф.0)"
else
    # Окружение задаётся ЯВНО: установка компилятора НЕ
    # самодостаточна — он выводит пути к std и рантайму из
    # каталога проекта (№530). Когда №530 закроется — снять
    # эти три строки и убедиться, что шаг всё ещё зелёный.
    PKG_ENV_STD="$ROOT/std/src"
    PKG_ENV_RT="$ROOT/compiler-codegen/nova_rt"
    PKG_ENV_CG="$ROOT/compiler-codegen"
    PKG_FAILED=""
    PKG_N=0
    tr -d "$(printf '\r')" < "$PKG_LIST" > "${TMPDIR:-/tmp}/gate_pkg_list_$$.txt"
    while read -r _pname _ppath _pcmd; do
        case "$_pname" in ''|\#*) continue ;; esac
        [ -d "$_ppath" ] || { echo "пакеты :: НЕТ КАТАЛОГА $_pname ($_ppath)"; PKG_FAILED="$PKG_FAILED $_pname"; continue; }
        PKG_N=$((PKG_N + 1))
        _plog="${TMPDIR:-/tmp}/gate_pkg_$$.log"
        ( cd "$_ppath" \
          && NOVA_STD_PATH="$PKG_ENV_STD" NOVA_RT_DIR="$PKG_ENV_RT" NOVA_CG_INCLUDE="$PKG_ENV_CG" \
             "$NOVA" test src ) >"$_plog" 2>&1
        if [ $? -eq 0 ]; then
            echo "пакеты ok :: $_pname"
        else
            echo "пакеты FAIL :: $_pname"
            grep -m2 -E "PASS:|error" "$_plog" | sed 's/^/    /'
            PKG_FAILED="$PKG_FAILED $_pname"
        fi
        rm -f "$_plog"
    done < "${TMPDIR:-/tmp}/gate_pkg_list_$$.txt"
    rm -f "${TMPDIR:-/tmp}/gate_pkg_list_$$.txt"
    echo "пакеты :: проверено $PKG_N"
    [ -z "$PKG_FAILED" ] || fail "пакетные репозитории красные:$PKG_FAILED"
fi

step "D-number uniqueness"
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

# Итоговый рубеж: сюда доходим, если мега-CU прошёл.
gate_barrier
