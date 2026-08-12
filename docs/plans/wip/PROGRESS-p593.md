<!-- SPDX-License-Identifier: CC-BY-4.0 -->
# PROGRESS — окно `p593-gc-premature`

Чекпоинт расследования. Каждая строка — УСТАНОВЛЕННЫЙ факт с командой и
дословным выводом. Гипотезы помечены словом «гипотеза».

## Ф0. Стартовые условия

- Дерево: `d:/Sources/nv-lang/nova-p553`, ветка `p593-gc-premature`.
- HEAD на старте: `aaabcb4a8` (= `main~1`; в `main` сверх ветки только коммит
  `9d365f52e` с брифами, кода не трогает).
- Прочитано: бриф, `docs/dev/debugging-races.md` целиком, записи №593/№605.
- Окружение: `NOVA_GC_LIB_DIR`/`NOVA_GC_INCLUDE_DIR` на vcpkg главного дерева
  (в worktree подмодуль `gc` пуст, `vcpkg_installed` нет).
- Компилятор пересобран: `bash scripts/tools/build-compiler.sh` → `ok`.

## Ф1. ФАКТ 1 БРИФА НЕ ВОСПРОИЗВОДИТСЯ (установлено)

Команда — дословно из брифа, голая оболочка, `NOVA_FIBER_STACK` не выставлен
(проверено печатью: `FIBER_STACK=[UNSET] GC_DONT_GC=[UNSET] AUTOARM=[UNSET]
MAXPROCS=[UNSET]`):

    ./nova-cli/target/release/nova.exe test spec_tests/conformance/groupm_tu_linkage --jobs 1

Шесть прогонов подряд, все шесть:

    PASS           spec_tests/conformance/groupm_tu_linkage/p577_reflect
    PASS           spec_tests/conformance/groupm_tu_linkage/p581_nested
    PASS           spec_tests/conformance/groupm_tu_linkage/p581_probe
    PASS           spec_tests/conformance/groupm_tu_linkage/p583_probe

    ===== SUMMARY =====
    PASS: 4  FAIL: 0

Ожидалось (факт 1 брифа): `PASS: 1 FAIL: 3`, все три —
`nova: fiber stack overflow in slot 0 (STATUS_STACK_OVERFLOW)`. Не наблюдалось
ни разу.

## Ф2. Прямой стресс собранных .exe — 320 прогонов, ноль отказов (установлено)

`nova test … --keep-artifacts`, затем каждый `.exe` гоняется напрямую (приём
§3.3 плейбука, чтобы не платить перекомпиляцией рантайма за прогон):

    p583_probe.exe:   PASS=100 FAIL=0 /100
    p581_nested.exe:  PASS=100 FAIL=0 /100
    p577_reflect.exe: PASS=60  FAIL=0 /60
    p581_probe.exe:   PASS=60  FAIL=0 /60

Итого 320 прогонов ARMED (прямой запуск .exe = armed M:N), ни одного ненулевого
кода возврата. При частоте отказа 100% (как в факте 1) вероятность такого исхода
нулевая; даже при p=2% — 0.15%.

## Ф3. Побочное наблюдение (НЕ факт про №593)

Прогон с `NOVA_FIBER_STACK=8388608` (факт 2 брифа) дал:

    TIMEOUT        spec_tests/conformance/groupm_tu_linkage/p577_reflect  # killed after 64382ms
    PASS: 3  FAIL: 1

То есть «лечащая» переменная из факта 2 в моём дереве не лечит, а мешает. Это
согласуется с §2.1.0 плейбука (TIMEOUT раннера ≠ hang рантайма), но отдельно не
разбиралось — гипотеза, не факт.

## Гипотеза Г1 (проверяется): носитель починен на main ПОСЛЕ снятия дискриминаторов

Дискриминаторы №593 сняты интегратором коммитом `cf7a99401` (2026-08-11 20:30).
В ТОТ ЖЕ ВЕЧЕР, ПОЗЖЕ, в main влиты:

    9af7c76d3 2026-08-11 21:08 merge(#592): monomorphise the static array-ext generic per element
    bf4ba4a64 2026-08-11 21:11 merge(#592, #577): the erased body is gone, and with it the question of which element to guess

`p577_reflect` — фикстура ровно про этот стёртый (erased) статический
array-ext generic body. Гипотеза: наблюдавшийся отказ нёс именно этот body, и он
физически удалён из компилятора. Проверка — прогон фикстур на `cf7a99401`.
