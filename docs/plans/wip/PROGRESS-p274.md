# PROGRESS — окно p274-277

Компиляторное окно двух пунктов реестра 221.1 одним заходом: №274 (auto-by-ref
операторные стабы) + №277 (Time vtable typedef redefinition, standalone-CU) +
П.2 брифа `scratch38/BRIEF_cu_fixpack.md` (P67 Duration.zero() ICE).

Worktree: `d:/Sources/nv-lang/nova-p274`, ветка `p274-277`, от main `60f33db08`.
Модель: sonnet.

## Задача А (№274) — ЗАКРЫТО

Коммит `53464e1ba`.

Корень: операторный desugar (`nova_vr_binop_*`/`nova_vr_ueq_*`, emit_c.rs)
всегда передавал RHS-операнд в реальный метод ПО ЗНАЧЕНИЮ. С Plan 172.14
auto-by-ref, `ro`-параметр value-struct'а >16Б по C-ABI эмитится как `T*`
(`param_is_auto_byref`) — стабы этот флаг не консультировали (равенство —
только через устаревший `param_c_types`-суффикс `*`, который auto-byref
пасс не проставляет; арифметика/ordering — вообще никак).

Фикс: все три сайта-обёртки (равенство, арифметика, ordering) теперь ИЛИ
консультируют `method_byref_flag` (единственный источник правды 172.14)
ИЛИ старый pointer-suffix check. Общая логика форматирования обёртки
вынесена в `compiler-codegen/src/codegen/operator_dispatch.rs::emit_vr_wrapper`
(+ 2 unit-теста) — emit_c.rs не растёт сверх ratchet (64311, δ0).

Фикстура: `spec_tests/conformance/d274_binop_stub_byref.nv` (value-record
32Б, `+`/`-`/`==`/`!=`) — зелёная в мега-CU.

Верификация НА НОСИТЕЛЕ (`nova-bigint-p240`, ветка `p240`, НЕ коммичено):
- `nova test src` (env NOVA_STD_PATH/NOVA_RT_DIR/NOVA_GC_LIB_DIR/NOVA_CG_INCLUDE
  на главную репу) — CC-FAIL `src/bigrat_test` ПОЛНОСТЬЮ УШЁЛ.
  **`PASS: 8  FAIL: 0  SKIP: 3`** — не «другой дефект», а зелёный полностью.
- `nova check src --strict-effects` — **`PASS: 11  FAIL: 0  WARN: 24`**.

## Задача Б П.1 (№277) — ЗАКРЫТО

Коммит `69fc790d6`.

Корень: ДВА независимых emission-сайта для `NovaVtable_Time`. Первый
(`emit_effect_type`, `name != "Time"` skip) — безусловный, работал всегда.
Второй (`vtable_names` forward-decl пре-пасс, ~emit_c.rs:6499) полагался на
`local_effects` (`Item::Type` в `module.items`), которая НЕ безусловна:
standalone-CU, использующий `Time` только как bare-эффект-аннотацию
(`fn f() Time -> T`), никогда не тянет `type Time effect {...}` (лежит в
`std/time/duration/time_effect.nv`, не в auto-import prelude) — но
`vtable_names` всё равно видит имя (безусловный скан `f.effects`). Второй
сайт эмитил именованный `typedef struct NovaVtable_Time NovaVtable_Time;`,
конфликтующий с анонимным hand-written `typedef struct {...} NovaVtable_Time;`
из effects.h.

Фикс: вернул "Time" в `BUILTIN_VTABLE_NAMES` (тот же безусловный
skip-канал, что уже используют "Fail"/"TimerMetrics" — НЕ новый point-if
по имени в новом месте).

Фикстура: `spec_tests/conformance/standalone/d277_time_vtable_typedef_standalone.nv`
(6-строчный репро брифа, адаптированный под реальный синтаксис
`fn main() Time -> ()`) — зелёная.

4-пробная матрица Os/Fs/Net/Io (та же standalone-CU форма, bare effect-row
без импорта) — прогнана ДО и ПОСЛЕ фикса, НЕ репродюсит баг ни разу
(подтверждает диагноз: Time — единственное затронутое исключение). Матрица
не коммичена (верификация only, временные файлы удалены).

## Задача Б П.2 (P67 Duration.zero()) — ДУБЛЬ, НЕ ФОРСИРОВАН

Документация: коммит `c09346969` + правки реестра (`docs/plans/backlog-followups.md`
`[M-missing-static-method-p67-ice]`, `docs/plans/221.1-bug-sweep.md` №83).

Репро воспроизведено буквально: `Duration.zero()` (несуществующий метод —
у Duration есть только `Duration.ZERO`, константа) в standalone-CU даёт
ICE `[P67-LEGACY] Path call return type unknown for method=zero`
(emit_c.rs:59434). `nova check` на этом же файле говорит `ok` — чекер
МОЛЧА пропускает вызов несуществующего static-метода (тихая дыра),
падение только на codegen-стадии.

Дубль-проверка (по инструкции брифа): корень СОВПАДАЕТ с уже
зарегистрированным `[M-missing-static-method-p67-ice]` (P2, найден
2026-07-31 на `SocketAddr.from_str`), который сам — дубль ЕЩЁ более
раннего `[M-p81-unknown-static-receiver-silent-p67]` (№83, 2026-07-24):
чекер никогда не валидирует существование static Path-вызова; §81-прототип
такой проверки был написан и ОТКАЧЕН из-за 42-файловой регрессии
(cross-module const-приёмники + intrinsic-namespace без `.nv`-декла).

Ретрай в этом окне: более узкий гейт (только когда `parts[0]` уже
`self.types`-известный declared-тип — структурно исключает ОБА
§81-класса) + исключения assoc-const/sum-variant/builtin-ctor имён.
`nova check std/src` **регрессировал 147/26/60 → 115/58/43** (+32 файла) —
ТРЕТИЙ, ранее не задокументированный false-positive класс: effect-операции
через ИДЕНТИЧНУЮ `Type.op()`-форму (`Time.sleep`/`Time.now`/
`Time.now_monotonic`/`Random.u64` — резолвятся через `effect_schemas`, не
`method_overloads`). Изменение немедленно отревертировано
(`git checkout -- compiler-codegen/src/types/mod.rs`), канон 147/26/60
подтверждён восстановленным.

**Решение: НЕ форсировать фикс.** Правильное решение нуждается в ТРЕТЬЕМ
пререквизите (effect-op-aware исключение) вдобавок к двум §81-шным
(cross-module const-индекс, intrinsic-namespace oracle) — это генуинно
отдельное окно с собственным бюджетом на полную верификацию, не
«точечный фикс по имени zero», как явно запрещал бриф. Находка
задокументирована в обоих реестрах для следующего захода.

## Гейты — итог

| Гейт | Вердикт |
|---|---|
| `cargo build --release` (nova-cli) | чистый, только pre-existing dead-code warnings |
| Новые фикстуры (274, 277) | зелёные (conformance CU `PASS: 157 FAIL: 0`) |
| `nova check std/src` | **байт-в-байт канон: PASS: 147 FAIL: 26 WARN: 60** |
| `arch-ratchet.sh` | `lines=64311 <= 64311` (δ0), `infer=348 <= 349` (-1) |
| Мега-CU / флагман | за интегратором (не в скоупе этого окна) |

## Реестры

- №274 → ✅ ЗАКРЫТ (docs/plans/221.1-bug-sweep.md)
- №277 → ✅ ЗАКРЫТ (docs/plans/221.1-bug-sweep.md)
- П.2 (Duration.zero) — дубль-проверен, НЕ закрывался (уже отслеживается
  `[M-missing-static-method-p67-ice]` / №83, оба обогащены находкой)

## Спек-амендмент

Не потребовался — оба фикса ABI/эмиссии (codegen-уровень), язык не менялся.
D431 Ф.2-v3-текст про Time-исключение (эффекты.md) остаётся корректным:
"Time — единственное hand-written исключение" — фикс №277 НЕ меняет это
решение, только чинит ВТОРОЙ emission-путь, который его не соблюдал.

## Коммиты этого окна (ветка `p274-277`)

1. `53464e1ba` — fix(codegen): №274
2. `69fc790d6` — fix(codegen): №277
3. `c09346969` — docs: P.2 investigation findings
4. (этот) — docs: закрытие реестров №274/№277 + PROGRESS
