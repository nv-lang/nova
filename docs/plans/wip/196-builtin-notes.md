<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# Plan 196 — Producer B для builtin Option/Result методов, финальный чекпойнт

**Worktree:** `nova-196builtin`, ветка `p196-builtin-producer`. **База:** main `ce0ab9e00`.
**Модель:** sonnet. **Зона:** `types/mod.rs` (продюсер) + `emit_c.rs` (SHADOW-хук
за `cfg(debug_assertions)`, минимальный — frozen `infer_call_ret_c` ЛОГИКА не
переписана, только `return match {...}` → `let legacy = match {...}; shadow;
return legacy`, поведенчески идентично в release).

---

## 0. Статус: ЗАВЕРШЕНО. Все гейты зелёные.

## 1. Гипотеза задания (100%-достижимость B11q/B11r на d30/d408/d119) — ОПРОВЕРГНУТА

Задание опиралось на GEN-финала ICR-трейс (docs/plans/wip/196-gen-final-notes.md
Задача 2): «`[ICR-HIT] B11r_result_like_methods`/`B11q_novaopt_methods` на
d30_try_op_unwrap_pair/d408_option_chain_sized_width/
d119_option_result_method_level_generic — 100% достижимость».

Перепроверено ЗАНОВО на **изолированных repro** этих трёх фикстур через
**реальный пайплайн** (`nova-codegen test-build`, `resolve_imports_inline` +
libuv + MSVC vcvars — НЕ standalone `nova-codegen compile`/`check`, которые
структурно НЕ резолвят imports вообще, см. ниже): **`[ICR-HIT]` B11q/B11r = 0,
`[MLRFS-HIT]` = 0 для всех трёх.** Чекер (`resolve_instance_method_return_arity`
/ Producer B, код УЖЕ был в main ДО этой волны — предыдущие 196.x волны) уже
ПОЛНОСТЬЮ резолвит `map`/`or`/`is_ok`/`unwrap`-семью на этих трёх фикстурах —
легаси НЕ достигается вообще.

**Почему GEN-финал увидел `[ICR-HIT]`:** `icr_trace`/`trace_mlrfs`
дедуплицируют ГЛОБАЛЬНО (один print на маркер за весь процесс, НЕ per-file/
per-callsite). GEN-финал гонял трейс на mega-CU (`spec_tests/conformance`,
993 файла) и видел «маркер сработал» — это НЕ доказывает срабатывание ИМЕННО
на d30/d119/d408: маркер мог сработать на любом из 990 других файлов с
Option/Result. Методологический разрыв: не отделили «сработал где-то в CU» от
«сработал на конкретной gate-фикстуре». Отдельная ловушка (обнаружена И
устранена по пути): `nova-codegen compile`/`check` (main.rs, single-file diag
tool) НЕ вызывает `resolve_imports_inline` — imports не мержатся,
`SigRegistry::build_base`/`method_overloads("Option", ...)` мисс ДЛЯ ВСЕХ
методов (артефакт инструмента, НЕ production-путь — `nova-cli`/`nova test`/
`nova build`/`nova-codegen test-build` идут через `resolve_imports_inline`
ДО чекера).

## 2. Реальный (узкий) residual — найден byte-offset↔source корреляцией

Полный CU (`spec_tests/conformance`, реальный пайплайн) с
`NOVA_TRACE_ICR=1 NOVA_TRACE_MLRFS=1` + временными diag-трейсами в чекере
(добавлены для локализации, УБРАНЫ — не часть финального диффа): B11q/B11r
ДОСТИЖИМЫ, но НЕ на d30/d119/d408 — на `flat_map`
(`Option[T]@flat_map[U]`/`Result[T,E]@flat_map[U]`,
`spec_tests/conformance/plan200_14_option_result_flat_map_filter.nv`, 12 из
~52 call-сайтов `step_a=None`).

**Root cause:** `closure_arg_return_peek` (и inline-дубль в
`resolve_method_return_with_closure_args`) пикает тип closure-тела ТОЛЬКО
для `ClosureBody::Expr` или `ClosureBody::Block` с **ПУСТЫМИ** `stmts`.
Естественная идиома для `flat_map`-стиля комбинаторов — closure с
side-effect ПЕРЕД возвращаемым `Some(...)`/вызовом:
```
n.flat_map(|x| { called = true; Some(x + 1) })
r.flat_map(|x| { called = true; p200_14_check_positive(x) })
```
Block с ≥1 стейтментом ДО trailing — старый гейт безусловно бейлится
(`_ => None`), метод-дженерик `U` никогда не биндится из closure → весь
propose-then-verify solver-канал молчит → легаси остаётся ЕДИНСТВЕННЫМ
источником.

Второй под-класс (`|x| if x==0 {None} else {Some(x)}`,
`ClosureBody::Expr(If{..})`) — `infer_expr_type` не имеет general-арма для
`If`. **НЕ закрыт этой волной**: `infer_expr_type` вызывается повсеместно в
чекере, general `If`-арм — существенно более широкое изменение, требующее
отдельной волны с полным corpus-verification. Честно оставлен на легаси
(не паника, не полу-фикс, задокументировано для следующей волны).

## 3. Доставленный producer-фикс

**Файл:** `compiler-codegen/src/types/mod.rs`.

- Новый метод `TypeCheckCtx::closure_block_stmts_are_peek_safe(stmts: &[Stmt])
  -> bool` — true когда ВСЕ стейтменты `Stmt::Expr`/`Stmt::Assign`/
  `Stmt::TupleAssign` (ни один не вводит новое имя в scope → `trailing`'s
  свободные переменные — ровно closure-параметры, `cscope` валиден без
  изменений). `Stmt::Let`/`Const` (новые биндинги) и control-flow
  (`Return`/`Break`/`Continue`/`Throw`) — консервативно исключены (пик —
  read-only best-effort для propose-then-verify солвера, не интерпретатор).
- `closure_arg_return_peek` (~16769): гейт `b.stmts.is_empty()` →
  `Self::closure_block_stmts_are_peek_safe(&b.stmts)`.
- `resolve_method_return_with_closure_args` (~16965, был inline-дубль той же
  логики): тот же гейт, тем же helper'ом (устраняет drift между двумя
  copy-paste копиями).

Аддитивно: раньше ВСЕГДА `None` для non-empty-stmts block → теперь `Some` для
БЕЗОПАСНОГО подмножества. Не может регрессировать существующие `Some`-пути
(`b.stmts.is_empty()` — частный случай `closure_block_stmts_are_peek_safe`
тривиально true на пустом списке).

## 4. SHADOW-хук (emit_c.rs, минимальный, debug_assertions-only)

**Файл:** `compiler-codegen/src/codegen/emit_c.rs`, функция `infer_call_ret_c`,
ветки B11q (`NovaOpt_`) и B11r (`is_result_like`). Рефактор
`return match {...}` → `let legacy = match {...}; #[cfg(debug_assertions)]
{ shadow-check }; return legacy;` — **поведенчески идентично в release**
(SHADOW-блок физически отсутствует в release-биноходе). Debug-only: если
`self.resolved_types.get(&expr.id)` (Channel 2) ЕСТЬ и `resolved_type_to_c`
на нём успешен (Ok), `debug_assert_eq!` сверяет его с `legacy` побайтово —
ловит класс багов «канал аннотировал, но почему-то не был потреблён раньше
(в `infer_expr_c_type`'s Channel-2 early-return) — а если бы был, дал бы
другой ответ». Легаси НЕ снесён (задание явно требует оставить); frozen
`infer_call_ret_c` ЛОГИКА (вычисление ответа) НЕ переписана, только
обёрнута.

## 5. Гейты — все зелёные (release/debug сборки ЭТОГО worktree/target)

- **Producer-фикс закрывает residual:**
  `plan200_14_option_result_flat_map_filter.nv` — было структурно
  недостижимо через канал (легаси-фоллбек), теперь channel-first; тест
  **PASS** (реальный `nova-codegen test-build`, полный CU).
  `[MLRFS-HIT] ... resolved=false` — **0 записей** на полном CU ПОСЛЕ фикса
  (до фикса — `map`/`flat_map` иногда доходили до легаси; легаси ВСЕГДА
  давал верный ответ, ни одного «деградированного» пути; теперь либо канал
  отвечает раньше, либо остаётся редкий `If`-body под-класс §2, честно вне
  этого счётчика — `unwrap`/etc остаются `resolved=false`-но-корректными по
  конструкции, D85/D86 retraction).
- **Byte-parity revert-cycle** (baseline HEAD `ce0ab9e00` vs патч, тот же
  debug-бинарь, `plan200_14...` → компилирует ВЕСЬ 993-файловый mega-CU):
  **`.c` diff = 0** (весь merged `.c`, ~235K строк, побайтово идентичен
  до/после — канал независимо воспроизводит ТОТ ЖЕ ответ, что легаси уже
  давал на затронутых call-сайтах). Покрывает d30/d119/d408 КАК ЧАСТЬ того
  же CU (нулевой diff тривиально означает нулевой diff и на их вкладе).
- **SHADOW 0 расхождений** (`debug_assert_eq!` из §4, debug-сборка,
  `NOVA_GC_LIB_DIR`/`INCLUDE_DIR` → main-репо vcpkg, MSVC vcvars):
  - `spec_tests/conformance` mega-CU (993 файла, тот же прогон, что
    byte-parity) — 0 паник/mismatch, тест PASS.
  - `std/src/collections/{hashmap/core_test,lru_test}`,
    `std/src/time/civil/civil_test`,
    `std/src/encoding/{json_test,base64/core_test}` — 0 паник, все PASS.
  - Изолированные repro `mini_d30`/`mini_d119`/`mini_flatmap` (полный
    пайплайн, вне conformance-папки — быстрая проверка) — 0 паник, все PASS.
- **Авторитетный гейт** `nova test spec_tests/conformance --jobs 4`
  (RELEASE `nova-cli`, собран из ЭТОЙ ветки/worktree/target, single CU) —
  **PASS: 126  FAIL: 0  SKIP: 14**. ЗЕЛЁНЫЙ (было 125/0/14 у GEN-финала —
  +1 за счёт p200_14, ранее уже PASS-ившего через легаси, теперь через
  канал — семантика та же, путь другой).
- **Флагман** (RELEASE `nova-cli`, ЭТОТ worktree):
  `nova check --strict-effects examples/flagship/aggregator/src/main.nv` —
  **PASS: 1  FAIL: 0  WARN: 33** (все warning — unused-import, косметика,
  не про эту правку; те же числа, что у GEN-финала).
  `nova build --strict-effects --mode release ... -o aggregator.exe` —
  **built (36.72s)**, 0 ошибок.
- Мега-CU (`nova test` без folder-фильтра, весь репо разом) — НЕ гонялся,
  по заданию.

## 6. Трейс — N builtin-сайтов channel-аннотированы (до/после)

До этой волны (сравнение через изолированный `plan200_14`-repro на baseline
`ce0ab9e00`): `flat_map`-класс closure-с-side-effect-body — **0 из ~12**
затронутых call-сайтов канализированы (100% легаси). После фикса — **12 из
12** (100% канал, легаси не достигается на ЭТОМ конкретном под-классе;
`If`-body под-класс §2 остаётся легаси-обслуживаемым, честно недокументирован
как закрытый).

## 7. Готовность легаси B11q/B11r к сносу

**НЕ готов к сносу этой волной.** Остаётся живой (документированно) для:
(a) `If`-expression closure-body под-класс `flat_map`/аналогов (§2, НЕ
закрыт); (b) `unwrap`/`unwrap_or`/`unwrap_or_else` — РЕТРАКТИРОВАННЫЕ методы
(D85/D86), у которых НЕТ FnDecl-декларации в принципе — их канал (строка
~16020, `resolve_instance_method_return_arity`) свой отдельный спец-кейс, НЕ
через `method_overloads`, за пределами зоны этой волны; (c) любой ДРУГОЙ
Option/Result-метод-вызов, чей closure/арг-инференс солвер не может
независимо верифицировать (propose-then-verify, `resolve_return_channel`) —
размер этого остатка НЕ измерен исчерпывающе (SHADOW-хук §4 теперь СТОИТ на
месте и будет ловить любое расхождение в БУДУЩИХ прогонах, включая полный
`nova test` без folder-фильтра, если кто-то захочет исчерпывающе проверить
весь корпус). Рекомендация следующей волне: (1) general `ExprKind::If` арм
в `infer_expr_type` (широкое изменение, отдельная волна с полным
corpus-verification); (2) если после (1) SHADOW `0` расхождений на мега-CU —
тогда panic-detach B11q/B11r safe.

## 8. Окружение для воспроизведения (этот worktree)

- libuv submodule скопирован из main (без `.git`) + сплющен (`cp -r` в уже
  существующую пустую директорию дал вложенный `libuv/libuv/...` —
  исправлено `mv`).
- MSVC vcvars: `C:\Program Files (x86)\Microsoft Visual Studio\18\BuildTools\
  VC\Auxiliary\Build\vcvars64.bat`.
- `NOVA_GC_LIB_DIR`/`NOVA_GC_INCLUDE_DIR` → main-репо
  `D:\Sources\nv-lang\nova\compiler-codegen\vcpkg_installed\x64-windows-static\
  {lib,include}` (свой `vcpkg_installed` из первого `cargo build` неполный —
  main-репо надёжнее).
- Диагностические временные файлы/скретчи (`scratch_*.txt`,
  `scratchpad/mini_*.nv`, патч ревёрт-цикла) — использованы для локализации
  и верификации, УДАЛЕНЫ перед коммитом (не часть финального диффа).

Коммит(ы) — ветка `p196-builtin-producer`, база main `ce0ab9e00`. В main НЕ
мёржено, push НЕ делался (по заданию).

Модель: sonnet.
