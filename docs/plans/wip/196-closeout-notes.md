<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# Plan 196 — ФИНАЛЬНАЯ CLOSEOUT-ВОЛНА, чекпойнт (пошагово)

**Worktree:** `nova-196close`, ветка `p196-closeout`. **База:** main `58804953d`.
**Модель:** sonnet.

---

## 0. Окружение — готово

- Worktree создан из main HEAD `58804953d`.
- libuv submodule скопирован из main (`compiler-codegen/nova_rt/libuv`, `.git` удалён),
  `target/libuv-cache` скопирован из main **корневого** `target/libuv-cache` (НЕ
  `compiler-codegen/target/libuv-cache` — той папки в main нет; кэш живёт в root).
- `NOVA_GC_LIB_DIR`/`NOVA_GC_INCLUDE_DIR` → main repo vcpkg (`x64-windows-static/{lib,include}`).
- Собраны РЕЛИЗ-бинари ИЗ ЭТОГО worktree:
  - `compiler-codegen` (`cargo build --release` из `<wt>/compiler-codegen`) — 1m11s, 0 errors.
  - `nova-cli` (`cargo build --release` из `<wt>/nova-cli`) — 2m49s, 0 errors.
  - `nova.exe` → `<wt>/nova-cli/target/release/nova.exe`.

## 1. Baseline conformance gate — СИНХРОННО (foreground), не фоном

Команда (из корня worktree, `cwd=<wt>`):
```
NOVA_GC_LIB_DIR=<main>/compiler-codegen/vcpkg_installed/x64-windows-static/lib
NOVA_GC_INCLUDE_DIR=<main>/compiler-codegen/vcpkg_installed/x64-windows-static/include
./nova-cli/target/release/nova.exe test spec_tests/conformance --jobs 12
```
**ВАЖНО (замер):** `--jobs 4` (как раньше в прошлых волнах) НЕ укладывается в
10-минутный потолок Bash-тула на этой машине (пробовал дважды — оба раза убито
таймаутом на ~130-141 строке вывода из ~141 итоговых). `--jobs 12` (16 логических
CPU) укладывается: **8m56s**, полный вывод.

**Результат (baseline, ДО правок этой волны):** `PASS: 125  FAIL: 0  SKIP: 16`.
Это ЗЕЛЁНЫЙ гейт — совпадает по PASS/FAIL с прошлыми волнами (125/0), SKIP чуть
больше (16 vs 14 у прошлых волн — вероятно новые d78_dup_decl/d424 neg-фикстуры,
добавленные ПОСЛЕ builtin-волны, см. main log 30fdd2b9f/d4af18030 — не регрессия).

## 2. Реестр остатков (после чтения 196-builtin-notes/196-gen-final-notes/196-prodb-notes)

Подтверждено чтением: producer-b + gen-final + builtin-producer УЖЕ СЛИТЫ в main
(коммиты `142f81b1b`/`4343b48c3`/`545961e59`/`fb231f76a` все в `git log main`).
Текущий актуальный остаток по карте задания:

1. **If-body closure peek** — `closure_arg_return_peek` (`types/mod.rs:16921`)
   делегирует `ClosureBody::Expr(be)` в `self.infer_expr_type(be, &cscope)`.
   `infer_expr_type` УЖЕ ИМЕЕТ `ExprKind::If` арм (с 2026-07-02, `fc5f78b4f`,
   Plan 125/172.1, D275 unit-domination) — НЕ новый арм с нуля. НО этот арм не
   может резолвить `if x==0 {None} else {Some(x)}`: `None` (bare Ident) не
   резолвится generic-Option-вариантом (`infer_expr_type`'s Ident-fallback
   ГЕЙТИТ `td.generics.is_empty()` — Option ИСКЛЮЧЁН), а `Some(x)` (Call) —
   у `infer_expr_type`'s `ExprKind::Call` арме нет ветки для builtin
   Some/Ok/Err ctor БЕЗ expected-типа (та логика — `materialize_literal_coercion`,
   ~13724 — работает ТОЛЬКО с `expected`-типом на входе, реверс-направление,
   не применимо к peek).
   **Живой corpus-сайт:** `spec_tests/conformance/plan200_14_option_result_flat_map_filter.nv:44`
   `ro r = a.flat_map(|x| if x == 0 { None } else { Some(x) })` — тест
   "f itself can return None (real bind, not just map)" — РЕАЛЬНЫЙ, не
   синтетика. Работает СЕЙЧАС через legacy fallback (`infer_method_level_return_for_sum`
   B11q), не регрессия, просто не покрыт каналом.
   **План фикса:** узкий локальный helper ВНУТРИ `closure_arg_return_peek`
   (НЕ трогать общий `infer_expr_type` — 249 консьюмеров, риск слишком широк),
   распознающий структуру `If{then, else_: Some(Block)}` где обе стороны —
   простой trailing (`Stmt`-пусто или peek-safe), с спец-разбором
   `Ident("None")`/`Call(Some/Ok/Err, [x])` → комбинирует в Option[T]/Result[T,E].
   Статус: В РАБОТЕ.

(продолжение по мере выполнения — п.2/3/4/5/6 ниже)

## 3. П1 — If-body closure peek: ✅ ЗАКРЫТО

**Реализация** (`compiler-codegen/src/types/mod.rs`, коммит `0d4ee870d`):
- Новый enum `ClosureIfCtorBranch` (module-level, рядом с `ctor_payload_expected`) —
  `Option(Option<TypeRef>)` / `Result(Option<TypeRef>, Option<TypeRef>)` — частичное
  знание про то, какой builtin sum и какой generic-слот знает ОДНА ветка If.
- `closure_if_ctor_branch_peek` — пикает ОДНУ ветку: bare `Ident("None")` →
  `Option(None)`; single-arg `Call(Some/Ok/Err, [x])` → инферит `x` через
  существующий `infer_expr_type` и заворачивает в нужный слот; иначе — делегат в
  `infer_expr_type` + `typeref_as_ctor_branch` (конкретный `Option[T]`/`Result[T,E]`
  тоже засчитывается).
- `closure_if_ctor_peek` — главная точка входа: гейт `If{then, else_:
  Some(Block)}` (без elif-цепочек — реальный corpus-шейп простой двусторонний),
  обе стороны обязаны быть `closure_block_stmts_are_peek_safe` (тот же гейт, что
  builtin-волна), комбинирует результаты обеих веток (конфликт слотов/разные суммы
  → `None`, безопасный legacy-фоллбек).
- Оба call-сайта (`closure_arg_return_peek` ~16921 и inline-дубль в
  `resolve_method_return_with_closure_args` ~17140) подключены через
  `.or_else(|| self.closure_if_ctor_peek(...))` — АДДИТИВНО (существующий
  `infer_expr_type`-путь пробуется ПЕРВЫМ, новый peek только страхует то, что
  раньше давало `None`).

**Гейты (release nova-cli, собран из ЭТОГО worktree):**
- `nova-codegen`/`nova-cli` — `cargo build --release` — 0 errors (оба).
- **Авторитетный гейт** `nova test spec_tests/conformance --jobs 12` (СИНХРОННО,
  foreground) — **PASS: 126  FAIL: 0  SKIP: 16**. ЗЕЛЁНЫЙ (baseline был PASS 125 —
  дельта не регрессия: сравнивал по `grep -c "^PASS"`, который ПОВТОРНО считал
  строку `===== SUMMARY ===== PASS: N ...` как ещё один "PASS"-хит — истинное
  сравнение по SUMMARY-строке; FAIL=0 в обоих прогонах, что и есть красная линия).
  Живой corpus-сайт `plan200_14_option_result_flat_map_filter.nv:44` (`if x==0
  {None} else {Some(x)}`) участвует в мега-CU (top-level loose-файлы без своего
  `fn main` агрегируются В ОДИН runnable-юнит `app_effect_basic_t8_1`, самый
  медленный тест — 269s, что и есть весь мега-CU целиком; отдельной
  PASS/FAIL-строки на `plan200_14` нет, но 0 FAIL для всего агрегата = его тесты
  тоже все PASS).
- **⚠ ЛОВУШКА ОКРУЖЕНИЯ (эта машина/сессия, НЕ связано с правкой):** `nova check
  --strict-effects examples/flagship/aggregator/src/main.nv` БЕЗ
  `NOVA_OFFLINE=1` падает `FAIL: 1` — «git-зависимость `tls`: fetch... nova-tls» —
  агент-сендбокс не имеет исходящего сетевого доступа, а resolve_git_dep пытается
  живой fetch несмотря на то, что нужный commit УЖЕ есть в глобальном
  `~/.nova/git` кэше (`nova-tls-768a12b7c05ddb78/910e14be86c3690f4b5ddd1d30d365437336f910`
  присутствует). **Фикс окружения:** `NOVA_OFFLINE=1` — тогда кэш используется
  без сети. С этим флагом: `nova check --strict-effects
  examples/flagship/aggregator/src/main.nv` → **PASS: 1  FAIL: 0  WARN: 33** (все
  warning — unused-import, косметика). `nova build --strict-effects --mode
  release ... -o aggregator.exe` → **built (34.10s)**, 0 ошибок. Записать в
  чекпойнт для следующих пунктов этой же волны — всегда экспортировать
  `NOVA_OFFLINE=1` для флагман-гейта в ЭТОЙ среде.

**Вывод:** producer gap №1 закрыт. Легаси (`infer_method_level_return_for_sum`
B11q/B11r) для этого шейпа теперь НЕ единственный путь — канал (`resolve_return_channel`
через `resolve_instance_method_return_arity`/`node_substs`) отвечает раньше для
`If`-body Option/Result combinator-closures. Легаси-ветка САМА НЕ снесена этим
пунктом (это — П2, следующий).

## 4. П2 — Снос B11q/B11r: ❌ НЕ СНОШЕНО (честный отрицательный вердикт, ПОДТВЕРЖДЁН заново)

**Методология:** временный env-gated detach-panic (`NOVA_196_DETACH_B11=1`,
`#[cfg(debug_assertions)]`, НЕ оставлен в дереве) в ОБЕИХ ветках B11q/B11r,
собран `nova-cli --release` с `RUSTFLAGS="-C debug-assertions=on"` (быстрый
ОПТИМИЗИРОВАННЫЙ бинарь, но с `debug_assertions` включённым — компромисс:
компилятор `compiler-codegen`'а собирается со своим `[profile.release]
opt-level=0` (bootstrap-профиль, см. Cargo.toml), а `nova-cli`'s профиль
release — `opt-level=2 lto=thin`; т.к. общего workspace нет, профиль-настройки
корневого пакета инвокации управляют ВСЕМ деревом зависимостей — собирать
`nova-cli` напрямую даёт быстрый рантайм; чистый `cargo build` (dev) внутри
`compiler-codegen` — на порядки медленнее на реальном корпусе, single-file
`nova test` через него не уложился в 5 мин даже на одном файле).

**Изолированный repro через РЕАЛЬНЫЙ пайплайн** (`nova test
spec_tests/conformance/d30_try_op_unwrap_pair.nv` — single-file path идёт через
`resolve_imports_inline`, тянет prelude/std транзитивно):

**РЕЗУЛЬТАТ — детач-паника СРАЗУ сработала** (это ОПРОВЕРГАЕТ гипотезу задания,
что П1 мог обнажить B11q/B11r как мёртвые):
```
nova: internal error at emit_c.rs:52996: [M-196-closeout-detach]
B11q_novaopt_methods reached: method=debug obj_ty=NovaOpt_nova_int
```
**Root cause находки:** `Option[T Debug]@debug(mut f Fmt) -> ()`
(`std/src/prelude/protocols.nv:732`) — ОБЫЧНЫЙ Nova-body метод с КОНКРЕТНЫМ
(`()`/Unit) возвратом, **БЕЗ единого closure/generic** во всём вызове. Это
доказывает: B11q/B11r — доставочный механизм ДЛЯ ЛЮБОГО
Option/Result-instance-метода, который чекер's Channel 2 (`resolved_types`) НЕ
материализует независимо (гораздо ШИРЕ, чем closure-peek residual, который
закрыл П1). `Result[T Debug, E Debug]@debug` (`protocols.nv:753`) — тот же
класс для B11r (не проверялся отдельным прогоном — идентичная форма,
симметричный вывод очевиден и не требует отдельного детач-цикла).

**Действие:** детач-panic КОД УДАЛЁН из дерева (temporary trial only, mirrors
GEN-final's own methodology — trial-then-revert-if-live). Заменён на
doc-комментарии над ОБЕИМИ ветками (`[M-196-closeout]`), фиксирующие ЭТОТ
вердикт + конкретную улику (файл/строка/метод), чтобы будущая волна не
повторяла ту же гипотезу без перепроверки — ровно тот же паттерн, что оставила
GEN-final волна для своего собственного (независимо подтверждённого) вердикта.
SHADOW-хуки (`debug_assert_eq!`) НЕ трогались — остаются как есть (0
расхождений, как и раньше).

**Гейт:** `cargo build --release` (compiler-codegen, обычный проф.,
БЕЗ `RUSTFLAGS`) — 0 errors, чистый компайл после ревёрта. Полный
conformance-мега-CU НЕ перегонялся для ЭТОГО шага — чистый diff = удаление
временного детач-кода (net поведение идентично состоянию П1, которое уже
прошло 126/0/16 + флагман зелёным); ревёрт не меняет ни одной исполняемой
инструкции относительно П1-коммита.

**Вывод:** B11q/B11r остаются ЖИВЫМИ. Полный физический снос ЭТОЙ волной —
НЕ безопасен и НЕ выполнен. Маркер `[M-196-closeout]` задокументирован для
следующей волны (потребовалась бы отдельная работа — материализовать
`Option/Result@debug`-и-подобные КОНКРЕТНО-типизированные builtin-sum-методы в
Channel 2 в чекере, вне периметра этой волны).

## 5. П3 — re-trace `resolve_result_option_ret` (B06a/B10j): ❌ НЕ СНОШЕНО (1 живой класс найден)

**Методология:** ПРАВИЛЬНАЯ (изолированные single-file repro через реальный
пайплайн, `nova test <file>.nv`/`nova test <folder>`, `NOVA_TRACE_ICR=1`,
optimized nova-cli binary собран с `RUSTFLAGS="-C debug-assertions=on"` —
быстрый рантайм + `cfg(debug_assertions)` трейсы активны). Маркеры внутри самой
`resolve_result_option_ret` (`emit_c.rs:19487`) — `GEN196_legacy_resolve_result_
option_ret_RESULT`/`_OPTION` (не путать с B06a/B10j — те трассируют ВЫЗЫВАЮЩИЕ
ветки, которые могут дойти до этой fn и получить `None` без трассировки самой
fn — только внутренние маркеры доказывают, что функция РЕАЛЬНО произвела
ответ).

**Прогон (8 карта-фикстур, ПОШТУЧНО, изолированно):**
`d85_question_return`, `d85_result_payload_width`, `d30_try_op_unwrap_pair`,
`d408_option_chain_sized_width`, `d30_result_option_ret_generic`,
`d88_default_generic_params`, `m196_facetc_generic_static_typaram`,
`d119_option_result_method_level_generic` — **0 хитов** `GEN196_legacy_
resolve_result_option_ret_*` на ВСЕХ восьми (B06a/B10j caller-ветки САМИ
срабатывают на некоторых из них, но не доходят до трассируемого пути внутри
`resolve_result_option_ret` — берут другой возврат раньше `?`).

**Прогон std/{collections,time,encoding}** (папками, folder-CU):
`std/src/collections` — 0 хитов; `std/src/time` — 0 хитов; **`std/src/encoding`
— 1 хит** `GEN196_legacy_resolve_result_option_ret_RESULT`. Бисекция сузила до
`std/src/encoding/serde` (изолированный прогон папки — тот же 1 хит,
`PASS: 1 FAIL: 0`).

**Точный класс (найден чтением, не измерен построчно — `icr_trace` дедуплицирует
булево per-marker-per-process, не считает call-сайты):** generic array/slice
serde-методы `[]T@serialize[S Serializer](mut s S) -> Result[(), SerError]` /
`[]T.deserialize[D Deserializer](mut d D) -> Result[[]T, DeError]`
(`std/src/encoding/serde/serde.nv:299,307`), вызываемые на конкретном
element-типе — напр. `@tags.serialize(s)` где `tags: []str`
(`std/src/encoding/serde/manual_roundtrip_test.nv:43`). Method-level generic
(`S`/`D` Serializer/Deserializer) + `Result[(), SerError]`/`Result[[]T, DeError]`
возврат → `B06a_method_overload_sentinel_mono` → `resolve_result_option_ret`
резолвит РЕАЛЬНО (не откатывается раньше).

**Вывод:** `resolve_result_option_ret` ОСТАЁТСЯ ЖИВОЙ — снос НЕ выполнен
(инструкция задания: «если жив — точный класс+число, доложи (НЕ снос)»).
Число: **1 подтверждённый живой класс** (generic slice-serde
serialize/deserialize) в изолированном std-корпусе; 0 хитов на всех 8
карта-фикстурах. Producer B (turbofish instance-method node_substs) закрыл
СВОЙ класс (explicit-turbofish instance-methods), но НЕ покрывает
generic-slice-extension-method + Result-return без турбофиша — за пределами
периметра Producer B (см. `docs/plans/wip/196-prodb-notes.md` §1: «Producer B
целится в generic INSTANCE-методы user-типов… B11q/B11r обслуживают BUILTIN
Option/Result» — этот serde-класс третий: generic SLICE-extension-метод,
ни то ни другое). Маркер для будущей волны: доресолвить `[]T@serialize`/
`[]T.deserialize`-класс в Channel 2 (чекер), тогда `resolve_result_option_ret`
можно будет пере-детачить.

## Статус пунктов (сводка, обновляется)

- П1 (If-body peek): ✅ ЗАКРЫТО (коммит `0d4ee870d`, гейты зелёные).
- П2 (снос B11q/B11r): ❌ НЕ СНОШЕНО — честный отрицательный вердикт (живой
  corpus-хит `Option@debug`, вне closure/generic периметра П1), doc-маркер
  `[M-196-closeout]` оставлен, детач-код удалён.
- П3 (re-trace resolve_result_option_ret / B06a-B10j): ❌ НЕ СНОШЕНО — 1 живой
  класс (generic slice-serde serialize/deserialize, std/src/encoding/serde),
  0 хитов на 8 карта-фикстурах. Снос НЕ выполнен (жив).
- П4 (re-trace rt_slots_from_args): В РАБОТЕ.
- П5 (терминал-фиксы по зондам wip/): НЕ НАЧАТО.
- П6 (реестр 196-one-truth-closeout.md): НЕ НАЧАТО.
