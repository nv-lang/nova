# PROGRESS — окно p338-defaultfn (№338)

Модель: sonnet. Ветка `p338-defaultfn` (worktree `d:/Sources/nv-lang/nova-p338`),
на `main`, НЕ вливалась и НЕ пушилась.

## Итог

✅ Основная поломка (№338) починена. Фикс в чекер-канале (1 строка + комментарий),
`emit_c.rs` не тронут. Все требуемые фикстуры зелёные на `nova check` И `nova test`.

Дополнительно найден (но НЕ чинился — вне мандата, отдельная, более узкая проблема,
подробности ниже) смежный баг: вызов результата default-arg-десугаренного HOF
в форме двойного вызова `f(...)( ...)` (без промежуточного связывания) падает
Rust-паникой `[P67-LEGACY] Path call return type unknown (no parts)` —
**подтверждено ПРЕ-СУЩЕСТВУЮЩИМ** (воспроизведено на debug-бинаре, собранном
ДО моего фикса callnorm.rs).

## Корень

`compiler-codegen/src/callnorm.rs::try_normalize_call` десугарит ЛЮБОЙ вызов
с default/named-арг в двухфазный `Block` (Plan 46, D102). Синтезированный
внутренний `Call`-узел получал `id: ExprId::UNSET`.

`emit_c.rs::emit_block_expr` типизирует temp-переменную блока через
`infer_expr_c_type(block.trailing)`. Оба checker-канала (Channel 1:
`resolved_callees`→`fn_ret_by_span`; Channel 2: `resolved_types`→
`resolved_type_to_c`) гейтятся на `expr.id.is_set()` — с UNSET id молча
пропускались. Легаси-fallback (`infer_call_ret_c`, ветка B10f для
`Ident`-callee, `user_fn_sigs`) НАРОЧНО отвергает ответ `"void*"` (общая
erasure для fn-типов, `resolved_type_to_c`'s `R::Func => "void*"`) без
альтернативы — `block_ty` оставался `""`.

Итог в C: `_nv_tmp_N;` (без типа) и `_nv_tmp_N = ()(callee(...))` (пустой
каст) → `error: use of undeclared identifier '_nv_tmp_N'`. `nova check`
зелёный — чекер честно резолвил ВОЗВРАТ исходного (недесугаренного) вызова,
но эта резолюция никогда не доходит до эмиссии desugared-Block'а.

## Фикс

`callnorm.rs`, `try_normalize_call`, финальный `new_call`: `id:
crate::ast::ExprId::UNSET` → `id: e.id`.

`e` — тот самый ОРИГИНАЛЬНЫЙ `Call`, который переписывается; `normalize_expr`
делает `e.kind = new_kind` (Block), но `e.id` НЕ трогает — он survive'ит с
тем самым id, что чекер уже аннотировал ДО десугаринга. Переиспользование
`e.id` на синтезированном `new_call` внутри Block'а — честное отражение
факта «это тот же call-site, аргументы просто переставлены в param-order» —
и открывает оба канала для `block.trailing`. Emission-код (`emit_call` и
далее) не тронут — только откуда берётся ExprId у одного AST-узла.

1 файл изменён (`callnorm.rs`), emit_c.rs НЕ тронут (временный debug-trace,
добавлявшийся при расследовании, откачен ДО финального коммита — см. коммит
`35575a403`).

## Расследование (для протокола)

1. Репро с `check` зелёным / `test` красным подтверждено дословно как в
   брифе: `_nv_tmp_435;` без типа, `_nv_tmp_435 = ()(nova_fn_make_adder(step));`.
2. Трассировка через `NOVA_TRACE_ICR=1` (debug-сборка) + временный точечный
   `eprintln!` в `Stmt::Let`'s HOF-канале (`emit_c.rs:~31730`, откачен) —
   подтвердила, что для НЕ-desugared (без default-аргумента) вызова с
   ПРИМИТИВНЫМ (`Scalar`) параметром/возвратом fn-типа (`fn(int)->int`)
   регистрация `fn_param_sigs` для let-биндинга ТОЖЕ не срабатывает — но
   ПО ДРУГОЙ причине (`resolved_type_to_typeref_named` не знает вариант
   `ResolvedType::Scalar`/`Bool`/`Float` — комментарий в коде ошибочно
   утверждает, что параметры HOF-сигнатуры «никогда не бывают» ничем, кроме
   `Named`/`Func`/`Unit`). Это **ОТДЕЛЬНЫЙ, более широкий баг**,
   НЕ специфичный для default-параметров: `ro f = plain_hof(3); f(5)` (БЕЗ
   default, БЕЗ callnorm) уже ломается на `main` сегодняшним тегом, если
   `fn(int)->int` — подтверждено на существующем регресс-файле
   `nova_tests/plan70/f2_fn_forward_decl_hof_pos.nv` (там же в тесте
   `ro plus5 = make_adder(5); assert(plus5(7)==12)`), который падает и на
   `main`'ском собственном бинаре (`nova-cli/target/release/nova.exe` из
   `d:/Sources/nv-lang/nova`, коммит `f249d733` на момент проверки):
   `CC-FAIL ... undefined symbol: nova_fn_plus5`.
   С Named-типом (не scalar) та же форма (без default) РАБОТАЕТ — проверено
   отдельно.
3. Из-за находки (2) фикстуры в `spec_tests/conformance/standalone/
   p338_default_arg_fn_type_return.nv` используют NAMED-тип `Box` (не
   `int`), чтобы не путать №338 (мой фикс) с этим отдельным
   scalar-конкретным багом — и чтобы pos-контроль B («без default,
   fn-тип возврата — не сломано») реально проходил ДО и ПОСЛЕ моего фикса.
4. Отдельно найден ПРЕ-СУЩЕСТВУЮЩИЙ (не регрессия от фикса) баг: двойной
   вызов `make_adder()(x)` (без промежуточного связывания) на
   default-arg-десугаренной функции падает Rust-паникой в `infer_call_ret_c`
   (`emit_c.rs:59672`, `[P67-LEGACY] Path call return type unknown (no
   parts)`) — легаси-ветка «callee этого вызова сам — Call» (форма `f(...)(
   ...)`, ищет `fn_returns_fn_sig` по имени) не распознаёт callee, ставший
   `Block` (desugar). Подтверждено на debug-бинаре, собранном ДО фикса
   callnorm.rs (тот же панике, только смещённая строка). НЕ в списке
   обязательных форм брифа буквально («прямым подвыражением» = передача
   РЕЗУЛЬТАТА как аргумента другому вызову — эта форма ПРОВЕРЕНА и
   РАБОТАЕТ, `call_fn(make_adder(), x)`); отдельная, более узкая, НЕ
   чинилась в этом окне — оставляю текстовой находкой для триажа
   интегратором, номер не присваиваю.

## Фикстура

`spec_tests/conformance/standalone/p338_default_arg_fn_type_return.nv` —
8 `test`-блоков:
- 4 формы вызова `make_adder` (одиночный fn-тип `fn(Box)->Box`): омитнутый
  default / именованный литерал / форвард переменной / прямое
  подвыражение (аргумент другого вызова, без биндинга);
- `mut`-привязка с явной аннотацией типа;
- Middleware-форма (`make_combiner`, `fn(Box,Box)->Box`, 2 параметра) —
  омитнутый default + именованный литерал;
- pos-контроль A: default-параметр + ОБЫЧНЫЙ (не fn) возврат;
- pos-контроль B: БЕЗ default-параметров + fn-тип возврата.

## Гейты — вердикты (буквально)

- `cargo build --release` (nova-cli, worktree `nova-p338`): чисто, `Finished
  release profile [optimized] target(s) in 2m 12s`, только pre-existing
  warnings (dead_code/unused), 0 errors.
- Своя фикстура, `nova check spec_tests/conformance/standalone/
  p338_default_arg_fn_type_return.nv`: `PASS: 1  FAIL: 0`.
- Своя фикстура, `nova test spec_tests/conformance/standalone/
  p338_default_arg_fn_type_return.nv` (C-codegen, реальная сборка): `PASS: 1
  FAIL: 0`.
- `nova check std/src`: `PASS: 148  FAIL: 26  WARN: 61` — канон 148/26/61
  байт-в-байт.
- polaris `./nova.sh test src --strict-effects` (запущено вручную через
  env-переменные на бинарь p338-worktree + `NOVA_STD_PATH`/`NOVA_RT_DIR` на
  p338, GC lib/include на main — см. `project-worktree-nova-test-setup`):
  `PASS: 37  FAIL: 0  SKIP: 18` — канон 37/0/18 байт-в-байт.
- `arch-ratchet` (`scripts/guards/arch-ratchet.sh`): `lines=64542 <= 64545`,
  `infer=348 <= 348` — EXIT=0, оба в пределах.
- `nova lint spec_tests/conformance/standalone/
  p338_default_arg_fn_type_return.nv`: `1 file(s), 0 finding(s)`.
- Мега-CU (канон 651/0/68) и флагман — НЕ гонял (по инструкции, у
  интегратора при приёмке).

## Файлы

- `compiler-codegen/src/callnorm.rs` — фикс (1 строка + комментарий),
  коммит `35575a403`.
- `spec_tests/conformance/standalone/p338_default_arg_fn_type_return.nv` —
  новая фикстура, коммит `df3fee79d`.
- `docs/plans/wip/PROGRESS-p338.md` — этот файл.

## Что НЕ сделано / для триажа

- Отдельный баг «двойной вызов `f(...)( ...)` на default-arg-десугаренной
  функции» (см. п.4 выше) — пре-существующий, не в мандате, номер не
  присвоен.
- Отдельный баг «`fn_param_sigs`-регистрация let-биндинга HOF-результата не
  срабатывает для ПРИМИТИВНЫХ (scalar/bool/float) параметров/возврата
  fn-типа даже БЕЗ default-параметров» (см. п.2 выше;
  `resolved_type_to_typeref_named`, `emit_c.rs:~11293-11327`, комментарий
  ошибочно утверждает «только Named/Func/Unit») — пре-существующий, не в
  мандате, номер не присвоен. Подтверждено на существующем regression-файле
  `nova_tests/plan70/f2_fn_forward_decl_hof_pos.nv`, который СЕГОДНЯ падает
  и на `main` (не в стандартном gate-прогоне — `nova_tests/` не входит в
  регресс, см. `feedback-large-tests-stored-not-in-regress`).
