# [M-p67-path-call-const-receiver-method-ice] — чекпоинт

**Worktree:** `nova-p67const` (ветка `p-fix-p67-const`, от `main`@4acdd33cc).
**Модель:** sonnet. Синхронно, без суб-агентов.

## Механизм бага

`const BUDGET_MS int = 120` + `BUDGET_MS.to_millis()` → ICE
`[P67-LEGACY] Path call return type unknown for method=to_millis`
(emit_c.rs:52930).

Корень — ПАРСЕР (`compiler-codegen/src/parser/mod.rs`, ~8636-8730):
Path-vs-Member решается ТОЛЬКО по регистру первой буквы идентификатора
(`starts_uppercase`). Nova-конвенция для констант — SCREAMING_SNAKE_CASE
(начинается с заглавной) → `BUDGET_MS.to_millis` жадно сворачивается в
2-сегментный `ExprKind::Path(["BUDGET_MS", "to_millis"])` вместо
`Member{obj: Ident("BUDGET_MS"), name: "to_millis"}`, который получает
обычная lowercase-переменная того же метода.

Чекер материализует return-тип метод-вызова в `resolved_types_buf`
(`compiler-codegen/src/types/mod.rs`, `infer_method_call_channel_type`,
вызывается из `f1_expr`'s Call-арма ~8001) — ДО фикса функция матчила
ТОЛЬКО `ExprKind::Member{obj, name}`. Path-форма (parts.len()==2) в
`f1_expr` (~8247, `else if let ExprKind::Path(parts) = &func.kind`) имеет
только несколько узких спецкейсов (`deserialize`, `ChanReader.
close_after`) — общего "ресивер — bound const/переменная" фолбэка не
было. Поэтому Path-форма с const-ресивером падала мимо ВСЕХ producer'ов
Channel-2 в финальную ICE-панику emit_c.rs (легаси `infer_call_ret_c`
трактует parts[0] исключительно как имя типа/static-namespace).

Брат [M-flagship-monotonic-now-bare-binding-ice] (`ro t0 = Monotonic.
now()`) закрыт ПОПУТНО 67717dcb1 (M-176 variant-chain) — но тот фикс
покрывал ТОЛЬКО 3-сегментный `[Type, Variant, method]` shape (nullary-
variant chain-call без промежуточного binding). 2-сегментный
`[CONST_IDENT, method]` с const-ресивером — другой AST-shape, тот фикс
его не касался. Подтверждено ревизией 83f61f1fd.

## Фикс

`compiler-codegen/src/types/mod.rs`:

1. Новое поле `TypeCheckCtx::const_types: HashMap<String, TypeRef>` —
   имя top-level `const NAME TYPE = value` → declared TYPE (только
   явно аннотированные консты; без аннотации — вне рамок фикса).
   Строится один раз в `TypeCheckCtx::build` проходом по `module.items`
   (`Item::Const`).
2. `infer_method_call_channel_type` (~16515) — рефакторинг: единый
   `match &func.kind` с ДВУМЯ рукавами вместо одного:
   - `ExprKind::Member{obj, name}` — байт-идентичная старая логика
     (просто перенесена внутрь match-рукава).
   - НОВЫЙ `ExprKind::Path(parts) if parts.len()==2` — резолвит
     `recv_ty` через `self.const_types.get(&parts[0])`; если const с
     таким именем не найден — `None` (честный фолбэк на легаси, как
     раньше). Типы и консты — раздельные namespace'ы в Nova, поэтому
     совпадение с реальным static-namespace Path (`Monotonic.now()`)
     невозможно.
   - Дальше — ОБЩИЙ хвост (`resolve_instance_method_return_arity` +
     `resolve_method_return_with_closure_args` fallback), одинаковый
     для обоих рукавов.

emit_c.rs НЕ трогал (не моя зона — parfor-агент). f1_check_call/
node_substs/resolve_instance_method_return_arity — НЕ трогал (Producer
B зона, параллельный агент в том же файле).

## Фикстура

`examples/flagship/aggregator/regressions/const_receiver_generic_ext_ice/
const_receiver_generic_ext_ice.nv` — по образцу
`monotonic_now_bare_binding` (тот же folder-module паттерн, соседи в
той же директории).

## Вердикты (ДО/ПОСЛЕ)

- ДО фикса: `nova test examples/flagship/aggregator/regressions/
  const_receiver_generic_ext_ice` → ICE
  `internal error ... emit_c.rs:52930: [P67-LEGACY] Path call return
  type unknown for method=to_millis`.
- ПОСЛЕ фикса: `PASS: 1 FAIL: 0`.
- Все 8 flagship-regressions (включая `monotonic_now_bare_binding`,
  брат-фикстура): `PASS: 8 FAIL: 0`.
- `nova test std/src/time`: `PASS: 6 FAIL: 0 SKIP: 1` (SKIP — cron,
  нет test-блоков, норма, известно из baseline).
- `nova build examples/flagship/aggregator/src/main.nv --strict-effects`:
  собрался чисто (только baseline-предупреждения, ни одной ошибки).
- `spec_tests/conformance` (главный гейт, один CU,
  `--positive --compile-error --timeout 300 --jobs 4`):
  **PASS: 509 FAIL: 0 SKIP: 14** (0 строк `^FAIL` в полном логе).
  Мега-CU НЕ гонял (не требовалось гейтом фикса).

## Сборка / окружение worktree

- libuv submodule скопирован из main (`.git` внутри удалён).
- `target/libuv-cache` скопирован из main (кэш собранного libuv.lib) —
  иначе `test-build`/`nova test` на Windows требует vcvars для
  пересборки libuv "с нуля" (в этом окружении недоступно).
- `NOVA_GC_LIB_DIR`/`NOVA_GC_INCLUDE_DIR` → main repo
  `compiler-codegen/vcpkg_installed/x64-windows-static/{lib,include}`.
- Свой бинарь: `compiler-codegen/target/debug/nova-codegen.exe` +
  `nova-cli/target/release/nova.exe`, оба пересобраны после фикса.
