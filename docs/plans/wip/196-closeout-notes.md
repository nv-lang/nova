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

## Статус пунктов (сводка, обновляется)

- П1 (If-body peek): В РАБОТЕ.
- П2 (снос B11q/B11r): ОЖИДАЕТ П1.
- П3 (re-trace resolve_result_option_ret / B06a-B10j): НЕ НАЧАТО.
- П4 (re-trace rt_slots_from_args): НЕ НАЧАТО.
- П5 (терминал-фиксы по зондам wip/): НЕ НАЧАТО.
- П6 (реестр 196-one-truth-closeout.md): НЕ НАЧАТО.
