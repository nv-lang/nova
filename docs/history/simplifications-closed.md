# Архив: закрытые упрощения и хроники (docs/simplifications.md)

Архив снятых упрощений и исторических записей, вычищенных из
[`docs/simplifications.md`](../simplifications.md) 2026-07-18 (заказ владельца —
файл предназначался для действующих упрощений, но превратился в свалку всего
подряд). Порядок внутри каждой секции — исходный (как в simplifications.md на
момент переноса). Содержимое переносится **дословно**, без перефразирования
(кроме одной пометки уже устаревших хвостов «идёт работа X», если X завершена).

Начиная с этой чистки закрытые упрощения переносятся сюда **в момент закрытия**
(не копятся в живом списке под пометкой «ЗАКРЫТО»).

## Секция 1 — Закрытые упрощения

Записи, которые были осознанным упрощением (с rationale и условием снятия) и
впоследствии сняты/зафикшены. Формально — то же самое, что раньше жило в
simplifications.md под пометками ЗАКРЫТ/РЕШЕНО/✅.

[2026-07-06 Plan 180 Ф.1 — sum-type rich auto-derive, ✅ УПРОЩЕНИЕ СНЯТО] Закрыт `[M-126-sum-*-rich]` (equal/hash/clone/compare/display/debug). **Было:** sum-арм каждого из шести built-in-синтезаторов = заглушка (equal=identity `@ == other`, hash=literal 0, clone=self, compare=0, display/debug=typename) → `#impl(P)` на sum-типе давал МОЛЧА неверную семантику (все значения «равны», hash-коллизия, clone-алиасинг). **Стало:** `match @ { … }` с одной arm на variant, payload биндится в паттерне и рекурсится как record-поля (`auto_derive.rs::synth_{equal,hash,clone,compare,fmt}_sum_body` + helpers `variant_bind_pattern`/`variant_construct`). Инжектятся ПОСЛЕ type-check (как record-path) → codegen-annotation-free-инференс лаверит match/variant-паттерны/variant-конструкцию (scrutinee `@` + `other: Self` — типы известны). **Verify:** `nova_tests/plan180_f1/sum_rich_autoderive_ok.nv` (6 test-блоков, все PASS — assertions падали бы на заглушках); conformance 53/0; zero-regression (единственный behavior-delta — `plan91_14/pos_debug_sum_derive.nv`, пинил V1-typename-placeholder → обновлён на variant-name); 43 Rust-unit-теста auto_derive PASS (+t27 stale sb→w fix). **Known-limit (не заглушка):** метод на bare-unit-варианте (`Nought.hash()`) мис-инферится — аннотировать `Self`-локалом; pre-existing bidirectional-инференс-граница.
---

[2026-07-04 Plan 176 keystone — value-record в generic `Result`/vtable mono, ✅ УПРОЩЕНИЕ СНЯТО] Закрыт рекуррентный codegen-gap, ранее обходившийся heap-boxing'ом value-record-ошибок (176 `IoError`, потенц. 180 Ser/DeError, 178 HttpError). **Было:** `Result[T, <value-record E>]` в возврате protocol-метода / generic-fn / generic-wrapper-метода → CC-FAIL `unknown type name 'NovaRes_<ok>_NovaValue_<E>'`; обход — делать error-record heap (`type IoError { … }` вместо `value`), чтобы `type_ref_to_c` стирал его в рантайм-предопределённый `NovaRes_nova_int_nova_str`. **Root:** protocol-vtable struct (ранний буфер `user_type_fwd_decls`/`generic_type_defs`) ссылается на КОНКРЕТНЫЙ `NovaRes_<ok>_NovaValue_<E>*`, чей typedef splice'ится позже (`__NOVARES_TYPEDEFS__`). **Fix (аддитивный, phase-correct):** `emit_c.rs::emit_protocol_box_typedef` forward-declare'ит `typedef struct NovaRes_<n> NovaRes_<n>;` референсимых value-record-`NovaRes` monos в ту же раннюю зону ДО vtable-struct (pointer-field → tag достаточно; полное тело — прежним splice'ем; C11 6.7/3 redundant-typedef ок). **Снято:** `IoError` heap→`value` (D322 §3b канон), обход heap-record удалён из `std/io/error.nv`. **Verify:** conformance 38/0; io 19/19 (value `IoError`); repro `nova_tests/p176repro/g1` (Result+value-record через protocol-fn) + `g2x` (generic-wrapper+value-record, явные type-args); zero-regression vs parent e50fcc6d. **НЕ закрыто (отдельные корни, эмпирически разведены):** generic-wrapper INFERENCE-конструкция (`[M-176-generic-wrapper-mono-inference]`, value-record-НЕзависим — падает и на heap-error); method-generic метод на value-record-ресивере (`[M-valuerecord-receiver-generic-method]`); форвард bounded-generic bound (checker, `[M-176-io-forward-bounded-generic]`).
---

[2026-06-17 Plan 147 Ф.7] Checker enforcement gaps for D246 three-axis mutability CLOSED. (1) `[M-147-ro-binding-index-freeze]` CLOSED — `ro a = [...]; a[0] = x` now gives `E_READONLY_CONTENT`; `is_through_ro_binding` added to `check_target_readonly` Index arm with entry-code guard (avoids false positives in prelude/std). (2) `[M-147-param-index-freeze]` CLOSED — non-`mut` params inserted into `ro_binding_names` at fn entry (snapshot/restore); `v[i]=x` on plain param now gives `E_READONLY_CONTENT`. (3) `[M-147-ro-ro-redundant-binding]` CLOSED — `ro a ro T = ...` gives `E_REDUNDANT_TYPE_MODIFIER` (parser-level, pre-existing; oracle test f7_neg3 confirms). Oracle tests: 7 new fixtures (f7_pos1..f7_pos3, f7_neg1..f7_neg4). Result: 37/0 PASS.

---

### Plan 104.9 — nova-lsp language-sync: completion + quick-fixes (2026-06-17, ✅ CLOSED [M-104.9-completion-language-sync])

- **Где** — `nova-lsp/src/completion.rs`, `nova-lsp/src/code_actions.rs`, `nova-lsp/tests/completion.rs`.
- **Что сделано** — Полная синхронизация nova-lsp с актуальной поверхностью языка после ~50 изменений (Планы 114/133/139/147/152/160/161 и др.). (1) `completion.rs`: `let` удалён из всех keyword-списков → `ro`/`mut`/`extern`/`priv`/`reveal`/`value`; добавлен `while-let` сниппет; `collect_let_bindings` сканирует `ro`/`mut`; `float`/`usize`/`Map` удалены из prelude_items → добавлены `f64`/`f32`/`HashMap`/`Set`; `int_methods()` = только `min/max/compare`; `float_methods()` → `f64_methods()` с полным math.nv API; `str_methods()`: `len` → `byte_len()`, `chars()` → `as_chars()`, `split_lines()` → `lines()`, `to_int()` → `parse_int()`; `vec_methods()`: `iter()->VecIter[T]` (не `iterator()`), `lazy()`, `chunks()`, `append`, `flatten`, etc. (2) `code_actions.rs`: добавлены группы 104.5.6 (7 protocol-impl fixes), 104.5.7 (2 field/type fixes), 104.5.8 (2 str fixes: `E_STR_NO_LEN` → `.byte_len()` MachineApplicable), 104.5.9 (2 comparison fixes); `E_ADDR_OF_MUT_REQUIRES_MUT_BINDING` → `E_ADDR_OF_REMOVED` (Plan 118.6 followup). (3) Тесты: 255 unit + 13 integration tests PASS; `nova_tests/plan104_9/` 10/10 PASS через release compiler.
- **Что было упрощено** (в V1): completion.rs не имел механизма обновления при изменениях языка → hardcoded списки устарели. Теперь синхронизированы, но всё ещё статические.
- **Как чинить** (V2): dynamic completion — запрашивать методы через compiler API вместо статических таблиц. `[M-104.9-dynamic-method-completion]` — **CLOSED Plan 104.10 Ф.5 (2026-07-03):** статические таблицы методов удалены; type-driven completion резолвит методы из `ResolvedModule` (expr_types → тип ресивера → `module.items` scan, вкл. inline stdlib + cross-file peers) через `completion.rs::method_items_typed`. Никаких встроенных таблиц.
- **Приоритет** — L (CLOSED 2026-06-17).

---

### Plan 153 Phase B — step_by / chain / zip / flat_map + scalar @min/@max (2026-06-16)

- **Где** — `std/collections/vec_lazy.nv`, `std/collections/vec_iter.nv`, `std/runtime/defaults.nv`.
- **Что сделано** — (1) `[M-153-scalar-min-max]` CLOSED. (2) `step_by` + `chain` CLOSED. (3) `zip` CLOSED (`[M-153.2-tuple-elem-adapter]` FIXED: receiver typevar alias binding в emit_c dispatch). (4) `flat_map` CLOSED (`[M-153.2-flat-map-inner-option]` FIXED: VR typedef ordering via `novaopt_vr_typedefs_buf`). Тесты: `plan153_2/` 25 тестов PASS (zip_basic 9pos + zip_neg 3neg + zip_min 1 + flat_map_basic 7pos + flat_map_neg 4neg + step_by_zero_neg 1). D260 Phase B ЗАКРЫТА. Коммиты `d505c0e5`, `542a3db8`, `00b494d6`, `0e539ef3`, `8cf1d23a`.
- **Упрощение** — ~~`zip` и `flat_map` реализованы без тестов из-за closure-typing gaps.~~ ЗАКРЫТО — оба фикса в codegen, тесты зелёные.
- **Как чинить** — ✅ ЗАКРЫТО.
- **Приоритет** — ✅ ЗАКРЫТО (2026-06-16).

---

### Plan 91.12 Ф.9 — str @as_ptr + DnsNet V1 + consume value net types (2026-06-16)

- **Где** — `std/runtime/string/core.nv`, `std/net/{effect,ffi,dns,tcp,udp,mock}.nv`, `compiler-codegen/nova_rt/net.{h,c}`, `compiler-codegen/src/codegen/emit_c.rs`.
- **Что сделано** — (1) `str @as_ptr() -> *u8`: Nova body `=> @ptr`, используется в DNS handler для bytes-FFI. D294. (2) `DnsNet` effect; `real_dns_net()` реализован через `uv_getaddrinfo` + park/wake; TLS `_net_dns_addrs[]`. `SocketAddr.lookup()` wrapper обходит vtable type-erasure. D295. (3) `TcpListener`/`TcpStream`/`UdpSocket` стали `consume value` (было просто `value`). (4) Codegen fix: boxing `NovaValue_*` в `nova_int` slot при `push`. Тесты: 21/0 PASS.
- ~~**Упрощение 1**~~ ✅ **`[M-91.13-dns-iter-boxing]` CLOSED 2026-06-16** — Fix: `is_generic_stub_c` в `emit_c.rs` + DnsNet V2 `[]SocketAddr` API. Vtable type-erasure устранена: `&& !name.contains("____")` в `is_generic_stub_c`; монорфизованные generic-инстансы (`Nova_Vec____NovaValue_SocketAddr*`) больше не классифицируются как stubs. `DnsNet.lookup` теперь возвращает `Result[[]SocketAddr, NetError]`.
- ~~**Упрощение 2**~~ ✅ **`[M-91.13-real-dns-integration-test]` CLOSED 2026-06-16** — `net_v2_dns_real_slow.nv` добавлен (`_slow` suffix, `NOVA_SLOW_TESTS=1` opt-in). `assert(r.is_ok())` с реальным `localhost` resolver.
---

### Plan 91.13 — DNS vtable erasure fix + DnsNet V2 multi-address API (2026-06-16, ✅ CLOSED)

- **Где** — `compiler-codegen/src/codegen/emit_c.rs` (is_generic_stub_c), `std/net/{effect,dns,mock}.nv`, `nova_tests/plan91_12/`.
- **Что сделано** — (1) Codegen fix: `is_generic_stub_c` добавлена проверка `&& !name.contains("____")` — монорфизованные generic-инстансы больше не ошибочно классифицируются как stubs, OK-тип в Result-арме vtable не erases в `nova_int`. (2) DnsNet V2: `lookup` возвращает `Result[[]SocketAddr, NetError]`; `real_dns_net()` строит Vec через `dns_addr_at(0..count)`; `mock_dns_net()` возвращает `Ok([loopback(0)])`; `SocketAddr.lookup` wrapper обновлён. (3) Тесты: `net_v2_dns_smoke.nv` обновлён (Vec API: `addrs[0].is_v4()`, `addrs.len()`); `net_v2_dns_real_slow.nv` добавлен (opt-in, `_slow`). D295 AMENDED (V2). 21/0 plan91_12 PASS.
- **`[M-91.13-dns-iter-boxing]`** ✅ CLOSED — vtable erasure устранена одной строкой; V2 multi-address API реализован полностью.
- **`[M-91.13-real-dns-integration-test]`** ✅ CLOSED — `net_v2_dns_real_slow.nv` (real DNS, opt-in).

---

### Plan 163 Ф.1-Ф.3 — import/export glob hygiene (2026-06-16, ✅ CLOSED [M-import-glob-forbid])

- **Где** — `compiler-codegen/src/types/mod.rs` (E_REEXPORT_GLOB + E_IMPORT_GLOB checks); ~100 файлов std/ + nova_tests/ (import-migration).
- **Что сделано** — `E_REEXPORT_GLOB`: `export import m` без `.{}` → ошибка (нулевая миграция). `E_IMPORT_GLOB`: `import m` без `.{}`/`as` → ошибка (вариант a: запрет, НЕ qualified-redesign). ~100 файлов мигрированы `import X` → `import X as X`. plan163 5/0 PASS.
- **Выбор** — вариант (a) запрет, а не (b) qualified `import m → m.foo`. Вариант (a) проще и не требует изменений резолвера (Plan 162 уже делает резолвер-рефакторинг).
- **D-блоки** — D288 (E_REEXPORT_GLOB), D289 (E_IMPORT_GLOB option a). Q-import-glob-hygiene RESOLVED.
- **Приоритет** — L (CLOSED).
---

### Plan 162.2 — sig_table compile-path wiring (2026-06-16, ✅ CLOSED [M-162.2-sig-table-wiring])

- **Где** — `nova-cli/src/main.rs` (вызов `collect_all_signatures` + `build_with_sig_table`); `compiler-codegen/src/types/mod.rs` (`is_known_fn` как fallback в fn call resolution); `spec/decisions/07-modules.md` (D293).
- **Что сделано** — `collect_all_signatures()` подключена в продакшн compile path до `TypeCheckCtx`. `is_known_fn()` используется в fn call resolution, `#[allow(dead_code)]` снят. Two-pass resolver полностью замкнут. 4/4 PASS (план162_2).
- **D-блоки** — D293 (sig_table compile-path wiring). Маркер [M-162.2-sig-table-wiring] CLOSED.
- **Приоритет** — L (CLOSED).

### [ЗАКР] for-in только для Range (0..N или m..n) — [C5]
- **Закрыто:** Добавлена ветка для `NovaArray_T*` в `emit_for`.
  `for n in arr` генерирует `for (int64_t _i=0; _i<arr->len; _i++) { T n = arr->data[_i]; ... }`.
  Тип элемента выводится через `infer_expr_c_type`. Тест: `nova_tests/39_for_in_array.nv` (11 assert).
---

### [ЗАКР] Generics — полная мономорфизация (Plan 48 Ф.0-Ф.3) — [C6]
- **Закрыто (2026-05-15):** Plan 48 Ф.0-Ф.3 полностью завершён:
  - Ф.0: generic free functions → монаморфные специализации `fn_T` per call-site type
  - Ф.1: generic methods (instance + static) → `Nova_Type_method____nova_T`
  - Ф.2: замыкания в generic-функциях (basic case)
  - Ф.3: generic records/sum-types → конкретные `Nova_Type____nova_T` struct'ы:
    - `Stack[int]` → `Nova_Stack____nova_int` с полем `nova_int`
    - `Stack[str]` → `Nova_Stack____nova_str` с полем `nova_str*`
    - `Result2[T]` → tag-enum + union + конкретные constructor-функции
  - 393/393 PASS включая ранее падавшие `modules/stack_queue` и `types/self_universal`
- **Остаток (Plan 48 V2 followups):** `within[T]` / `race[T]` заблокированы
  spawn closure-capture в mono pipeline — [M-spawn-closure-capture-mono].
---

### [C7] Index выражения — прямое разыменование без bounds check ✅ RESOLVED Plan 96 Ф.1
- **Где:** `emit_c.rs` → `ExprKind::Index`
- **Что было:** `arr[i]` генерировался как голый `(arr)->data[i]` — controlled buffer overflow на запись, UB на чтение; противоречие D27 §1632 «panic при OOB».
- **Как починено (Plan 96 Ф.1, 2026-05-23):** `emit_bchk_array_access` хелпер — все 5 паттернов (self-access cast / array-of-arrays / array-of-record-ptrs / str-box / default + double-indexing) обернуты в GNU statement-expression с bounds-check; форма `*({ ... &_a->data[_i]; })` обходит Clang-ограничение «stmt-expr is not lvalue». `nv_panic_index_oob(idx, len)` wrapper в `array.h` форматирует сообщение «array: index N out of bounds for length L». 4 теста pos/neg в `nova_tests/plan96/`; full regression 1087 PASS / 0 FAIL.
---

### [C8] println — тип аргумента через infer_expr_c_type ✅ RESOLVED Plan 67
- **Где:** `emit_c.rs` → `make_print_call` / `infer_print_helper`
- **Что было:** Выбор `nova_print_int` vs `nova_print_str` vs `nova_print_bool` основан на
  ручном AST pattern matching — не покрывал `str.from(x)`, if/match expr, method chains.
- **Исправление (Plan 67):** `infer_print_helper` переписан на `infer_expr_c_type`-based
  dispatch (AD1). Добавлен `nova_print_char` + CharLit pre-check (AD3). 10 новых тестов.
- **Остаток:** `println(c)` где `c: char` — всё ещё `nova_print_int` (char stored as nova_int;
  fix requires `nova_char` distinct C type — Plan 67+1).
- **Приоритет:** RESOLVED
---

### [ЗАКР] parallel for — реализован — [R8]
- **Закрыто (2026-05-06):** keyword `parallel for x in iter { body }`.
  Десугарится в codegen в `supervised { for x in iter { spawn { body } } }`.
- **Закрыто (2026-05-06): array-mode `parallel for → []T`** (D71). Когда body имеет
  trailing-expression, форма возвращает `NovaArray_T*` (T ∈ {int, bool, f64, str}).
  Каждый fiber пишет результат в `result.data[idx]` по своему индексу — порядок
  записи в slots не зависит от порядка планирования. Реализация в `emit_parallel_for`:
  pre-allocate `NovaArray_T*` размера N (для Range — `end - start [+1]`; для ArrayLit —
  длина литерала), per-iteration ctx содержит `_nova_par_idx` + `_nova_par_result`,
  spawn body's trailing пишет в `_c->_nova_par_result->data[_c->_nova_par_idx]`.
  Без trailing — старая semantic (statement, unit). Spread в array literal не
  поддержан в v1 — degrade to unit.
- **Capture-by-value для immutable scalars:** spawn-capture теперь различает
  `let` (immutable) vs `let mut` (mutable). Immutable scalar (int/bool/f64/byte) →
  capture by value (snapshot в ctx struct). Всё остальное — by pointer (shared mut).
- **Heap-alloc ctx в supervised:** ctx-struct для spawn внутри supervised
  аллоцируется на куче (через nova_alloc), не на стеке — иначе все queued fibers
  внутри loop разделяют один stack-slot и видят последнее значение.
- **Loop-var регистрация:** range-loop в `emit_for` теперь регистрирует binding
  в `var_types` (как nova_int) — без этого capture не находил loop-переменную.
- **Тесты:** `nova_tests/41_parallel_for.nv` — 12 тестов statement-mode (interleaving,
  snapshot semantics). `nova_tests/50_parallel_for_array.nv` — 6 тестов array-mode
  (range/inclusive-range/array-lit → []int, yield-stable ordering, mix mut-capture +
  array-result, regression statement-mode).
---

### [M-125-type-checker-never-first-class] ✅ ЗАКРЫТО (Plan 125.1, 2026-06-05)
- **Где:** `compiler-codegen/src/types/mod.rs` — Ф.5 из основного Plan 125,
  который был deferred в V1 codegen-only. Type-checker side first-class
  `Ty::Never` subtype rule per spec D25.
- **Было:** Plan 125 V1 закрыл result-type inference на codegen-side
  (`emit_c.rs`), но type-checker до сих пор полагался на `TyCat::Other`
  escape-hatch — любой `throw`/`panic` в expression position silent'но
  проходил type-check. Spec D25 / 08-runtime.md:1018 обещал `never <: T`,
  но не было явной subtype-rule. Followup `[M-125-type-checker-never-first-class]`
  фиксировал implementation gap.
- **Стало:** 4 точечных additions в `types/mod.rs` (~50 LOC):
  - **Ф.1** `assignable()` — `if matches!(ty_of_ref(&found_tr), Ty::Never)
    { return Compat::Ok }` (never <: T для любого T)
  - **Ф.2** `infer_expr_type` propagates `never` для `ExprKind::Throw`,
    `ExprKind::Interrupt`, `Call(panic|exit|abort|unreachable, ...)`, и
    user fn'ов где ВСЕ overloads объявлены `-> never` (all-divergent
    guard избегает ambiguity при unresolved overload)
  - **Ф.3** `infer_block_trailing_typeref` — top-level shape check через
    helper `expr_diverges_at_top`; returns `Some(prim_ref("never"))` для
    trailing-divergent. Conservative — не walks preceding stmts.
  - **Ф.4** `detect_divergent_consumable` (D196 form 3) использует
    `block_diverges` для early-skip обеих веток вместо `?`-propagation
    abort; ЛЮБОЙ divergent путь → `None` (нет fake-conflict
    «Consumable[T] vs never»).
  - Pure-additive — `TyCat::Other` safety-net preserved (conservative
    addition, не subtraction; per Q1 resolution в plan-доке).
- **Tests:** `nova_tests/plan125_1/` — 12 positive (never_subtype_of_*,
  throw_in_let_typed, interrupt_in_let_typed, panic_in_match_arm_typed,
  never_arg_to_fn, never_in_return_position, d196_consume_divergent_branch,
  unreachable_call_typed) + 3 negative (let_never_no_context,
  never_no_assignment, d196_consume_required_in_live_branch) — 15/0 PASS.
- **Regression baselines preserved:** plan125 22/0, plan125_followups 9/0.
- **Что НЕ сделано:** ничего — full Ф.5 scope из Plan 125 закрыт.
---

### [M-91.13-if-expr-divergence-aware-inference] ✅ ЗАКРЫТО (Plan 125, 2026-06-05)
- **Где:** `compiler-codegen/src/codegen/emit_c.rs` — новый codegen-local
  helper `block_trailing_diverges` + `expr_diverges_125` (Plan 125 §Ф.1-Ф.4
  whitelist). Wired в `emit_if_expr`, `infer_expr_c_type::If`, `emit_match`
  (двух-проходный), `infer_expr_c_type::Match`. `emit_block_into` теперь
  skip'ает `emit_zero_assign` для divergent block (избегает CC-FAIL при
  non-trivial типах элемента).
- **Было:** Spec D25 (08-runtime.md:1018) обещал `never <: T`, но
  `infer_expr_c_type` для if-expr возвращал `nova_unit`, если then-trailing
  не имеет типа (а throw — `(nv_panic(...), 0LL)` comma-expr с infer-type
  `nova_int`). Результат: `let s = if c { throw E } else { "hello" }`
  эмитил `nova_str _nv_if = NOVA_UNIT` — CC-FAIL «incompatible type». Plan
  91 Ф.3 JSON conformance был заблокирован на этом.
- **Стало:** Whitelist trailing-only divergence detection: `throw`,
  `panic`/`exit` builtins, user `fn -> never` direct call, `interrupt`
  внутри handler, + recursive composition (if/if-let/match/block
  trailing). Helper **codegen-local** — НЕ переиспользует
  `block_diverges` из types/mod.rs (root cause 2026-06-03 24-regression
  revert). Trailing-only invariant: scan последнего stmt только если
  `b.trailing.is_none()`, иначе условный early-return середине блока
  flip'ит легитимный stdlib-idiom. Phase gates (Ф.1-Ф.4) сохранены в
  plan-доке для будущих расширений.
- **Все 7 followups закрыты (2026-06-05):**
  - `[M-125-type-checker-never-first-class]` ✅ CLOSED (Plan 125.1)
  - `[M-125-loop-no-break-divergence]` ✅ CLOSED (Plan 125.2)
  - `[M-125-stmt-position-divergence]` ✅ CLOSED (Plan 125.2)
  - `[M-125-while-true-divergence]` ✅ CLOSED (Plan 125.2)
- **Закрытые followups batch 1 (2026-06-05, branch `plan-125-followups`):**
  - `[M-125-unreachable-builtin]` ✅ CLOSED — `fn unreachable(reason str) -> never`
    добавлен в `std/prelude/runtime.nv` + re-export в `std/prelude.nv` +
    `std/prelude/e2026_05.nv`. Whitelist в `expr_diverges_125` рядом с
    panic/exit. 3 фикстуры PASS (basic / match-default / runtime-fires).
  - `[M-125-method-call-never-detection]` ✅ CLOSED — extended
    `expr_diverges_125` whitelist на `ExprKind::Member` calls (instance
    method `obj.method()` + static method `Type.method()` `-> never`).
    Registry `never_returning_methods: HashSet<(String, String)>`,
    populated during method/free-fn scan. 3 фикстуры PASS.
  - `[M-125-codegen-never-cast]` ✅ CLOSED — context-aware dummy для
    comma-expr `(side_effect, dummy)` обёртки divergent expressions.
    Replaces hardcoded `(nova_int)0LL` на target-typed zero (pointers →
    `(T)NULL`, ints → `(T)0`, floats → `(T)0.0`, structs → C99 compound
    literal `(T){0}`, unit → `NOVA_UNIT`). Wire site:
    `emit_expr_with_target_type`. 3 фикстуры PASS.
- **Закрытые followups batch 2 = Plan 125.2 (2026-06-05, branch `plan-125.2`):**
  - `[M-125-loop-no-break-divergence]` ✅ CLOSED — `ExprKind::Loop` без
    break, который targets THIS loop scope, признаётся divergent в
    `expr_diverges_125`. Helper `loop_body_has_break(&Block)` рекурсивно
    walk'ает Block stmts + trailing с scope stop-rules (не descend
    в inner Loop/While/For/ParallelFor + Lambda/Closure/HandlerLit).
    Continue НЕ считается break. 4 фикстуры PASS.
  - `[M-125-while-true-divergence]` ✅ CLOSED — `ExprKind::While` с
    `cond.kind == BoolLit(true)` AND `loop_body_has_break(body) == false`
    признаётся divergent. Strict literal match (НЕ const-fold). 4 фикстуры PASS.
  - `[M-125-stmt-position-divergence]` ✅ CLOSED — last-stmt `Stmt::Break`
    / `Stmt::Continue` теперь признаются divergent в
    `block_trailing_diverges` (parser+type-checker гарантируют
    syntax-валидность только внутри loop scope, scope-context не
    требуется). `Stmt::Return` уже handled в V1. 3 фикстуры PASS.
  - Negative regression guards (4): neg/loop_with_break_concrete,
    neg/while_var_cond, neg/break_in_outer_loop_only,
    neg/regression_concurrency_loop_pattern (Plan 83-style supervised
    worker loop with graceful shutdown).
  - Spec: `spec/decisions/08-runtime.md` D25 whitelist расширен (loop-no-break /
    while-true const-cond / Break|Continue last-stmt).
  - Test status: plan125 22/22 + plan125_followups 9/9 + plan125_2 15/15 PASS.
---

### [M-result-erased-no-mono] ✅ ЗАКРЫТО (Plan 63 Fix F + Fix F+, 2026-05-17)
- **Где:** `compiler-codegen/src/codegen/emit_c.rs` — Fix F base
  (2ae78c7ae8d) ввёл `result_ok_inner_types` + `pending_result_ok_inner_type`.
  Fix F+ (ca677dd2147) добавил per-fn registry `fn_result_ok_inner_types`
  + helper `try_get_result_ok_inner_type_for_expr` для propagation
  через function-call returns + inline match + pending leak fix через
  save/restore на boundary fn body.
- **Было:** Result[T, E] не mono'd как Option (Nova_Result hardcoded
  с nova_int payload slot). Tuple `(str, int)` не пролезал в nova_int
  (8 bytes), match destructure читал `_0.f0/f1` напрямую на int →
  CC-FAIL. Fix F закрыл только let-bound + homogeneous case через
  pending mechanism. Heterogeneous + inline match оставались broken
  (pending leak из internal `Ok((..))` без surrounding let перетирал
  правильный type).
- **Закрыто:** Production-grade extension — registry per fn signature
  знает Result's Ok payload mono'd type; helper resolves для Ident
  (var lookup) / Call (callee registry) / Block (trailing). Stmt::Let
  + emit_match wire через helper. Pending state save/restore на fn
  body boundary — internal `Ok` constructions больше не leak'ают
  в caller's let-binding. Все 4 case'а Result[(T, U), E] работают:
  let+inline × homogeneous+heterogeneous.
- **Tests:** [`f19_tuple_in_result.nv`](../nova_tests/plan59/f19_tuple_in_result.nv)
  (5 sub-tests: heterogeneous let+inline, homogeneous let+inline, Err),
  [`f20_result_method_with_tuple.nv`](../nova_tests/plan59/f20_result_method_with_tuple.nv)
  (instance method returning Result),
  [`f21_multiple_result_fns_no_pending_leak.nv`](../nova_tests/plan59/f21_multiple_result_fns_no_pending_leak.nv)
  (multiple fns с разными mono types validates pending fix),
  [`f22_result_block_scrutinee.nv`](../nova_tests/plan59/f22_result_block_scrutinee.nv)
  (Block trailing call),
  [`f23_result_ok_wrong_arity_rejected.nv`](../nova_tests/plan59/f23_result_ok_wrong_arity_rejected.nv)
  (negative wrong arity).
- **Out-of-scope (future, не блокер):** Полный mono'd Result
  (`NovaRes_<T>_<E>` typedefs per concrete combo analogous к Option) —
  ≈ Plan 56 vtable scope расширенный на variants. Fix F + Fix F+
  покрывают все наблюдаемые use-cases через targeted boxed-pointer
  tracking без системного refactor'а. Если в будущем понадобится
  arbitrary T в Result Ok (не только tuple/struct) — тогда mono'd path.
---

### [M-stdlib-iter-in-generic-method-body] ✅ ЗАКРЫТО (Plan 63 Fix E, 2026-05-17)
- **Где:** `compiler-codegen/src/codegen/emit_c.rs` (Plan 63 Fix E
  commit 66113a8d2db от другого agent'а) + `std/collections/hashmap.nv`
  (commit 36e215cce83 — workaround removal).
- **Было:** Попытка убрать workaround `for i in 0..@_buckets.len()` в
  HashMap.@merge_from/@filter и заменить на идиоматичный
  `for (k, v) in other` ломала @clone (cascade). Hypothesis было
  mono pass state leak между sibling methods. Plan 59 закрытие
  оставило это как deferred.
- **Закрыто:** Plan 63 Fix E (от другого agent'а) исправил три
  взаимосвязанных bug'а в emit_array_lit / emit_monomorphized_method /
  array_element_types tracking для array-of-tuple boxed-storage в
  generic method body. После Fix E идиоматичный `for (k, v) in other`
  / `for (k, v) in @iter()` работает без cascade.
- **Test:** plan56 6/6 PASS, plan59 19/19 PASS после удаления
  workaround.
---

### [M-match-variant-mono-tuple-payload] ✅ ЗАКРЫТО (Plan 59 Phase 6, 2026-05-17)
- **Где:** `compiler-codegen/src/codegen/emit_c.rs` — `pattern_bind_typed`
  Pattern::Variant handler (Option Some branch + sum_schemas branch).
- **Было:** `match Some((k, v))` для heterogeneous `Option[(str, int)]`
  падал CC-FAIL — inner Tuple destructure binds k, v как nova_int
  default (`_nv_scr.value.f0/f1` typed as nova_int), потому что
  `pattern_destructure_tuple` lookup'ит `tuple_element_types[scr]`
  но для variant-payload access (`scr.value` для Option,
  `scr->payload.Ok._0` для user sum-type) ключ не зарегистрирован.
  Homogeneous `Option[(int, int)]` случайно работал — все nova_int slot.
- **Закрыто:** В Pattern::Variant handler перед recurse'ом в inner
  Pattern: если payload type starts_with `"_NovaTuple_"` (mono'd) —
  parse elements через `parse_mono_tuple_elements` + insert в
  `tuple_element_types[raw_access_string]`. Inner Tuple destructure
  теперь видит mono'd element types через registry. Покрывает обе
  branches: Option Some (t_from_scr / novaopt_value_types) и user
  sum-type (sum_schemas variant fields).
- **Test:** [`nova_tests/plan59/f17_tuple_in_option.nv`](../nova_tests/plan59/f17_tuple_in_option.nv)
  — 4 sub-tests: Some((int,int)), None branch, Some((str,int)),
  chained Option[(K, V)] mix.
- **Update 2026-05-17 EOD+1:** [M-result-erased-no-mono] также ✅ ЗАКРЫТО
  через Plan 63 Fix F (другой agent) + Fix F+ extension (см. отдельную
  запись ниже). Изначально оставалось deferred — Plan 63 Fix F base +
  Fix F+ закрыли все наблюдаемые case'ы `Result[(T1, T2), E]` без
  полного mono'd Result rewrite.
---

### [M-tuple-mangle-nested-collision] ✅ ЗАКРЫТО (Plan 59 Phase 5, 2026-05-17)
- **Где:** `compiler-codegen/src/codegen/emit_c.rs` — `compute_mono_tuple_c_name`
  + `parse_mono_tuple_elements` + callsites в `emit_for`, `let_destructure`,
  `emit_tuple_return_stash`.
- **Было:** Mangle scheme `_NovaTuple____<T1>__<T2>__...` — prefix `____`
  (4 underscores), separator `__` (2 underscores). Когда element type сам
  `_NovaTuple____...` (nested mono'd tuple), его внутренние `____` collide
  с outer separator. `split("__")` распадается на garbage. Симптом:
  `let (left, right) = nested_pair` использовал legacy `_NovaTuple2`
  вместо реального mono'd type → CC-FAIL initializing _NovaTuple2 with
  incompatible expression. Closed коммитом d73a892f27b workaround'ом
  (registry lookup), не root cause.
- **Закрыто:** Length-prefixed encoding (Itanium ABI analog):
  `_NovaTuple_<arity>_<L1>_<T1>_<L2>_<T2>...` где `<Ln>` decimal byte
  length следующего sanitized element. Parser читает length → берёт
  exactly столько chars → next. Unambiguous для **любой** глубины nesting.
  Distinguishable от legacy `_NovaTupleN` по `_` после `NovaTuple`.
  Workaround registry-lookup в let-destructure удалён.
- **Test:** [`nova_tests/plan59/f10_deeply_nested_tuple_mangle.nv`](../nova_tests/plan59/f10_deeply_nested_tuple_mangle.nv)
  — 4-уровневый nested + let-destructure + mixed types.
---

### [M-plan-59-regression-suite] ✅ ЗАКРЫТО (Plan 59 fix d73a892f27b, 2026-05-17)
- **Где:** `compiler-codegen/src/codegen/emit_c.rs` (164 строки) +
  9 regression-guard тестов в `nova_tests/plan59/`.
- **Было:** Plan 59 closure (5b9f317452e — mono'd tuple structs)
  ввёл 9 регрессий: typedef order для user fwd decls (json_ast,
  xoshiro), topological sort nested tuples (basics/tuples, match_advanced,
  pattern_matching), `for-in` на mono'd elem-type custom iter
  (for_iter_tuple/typed), let-destructure hardcoded `_NovaTupleN`
  ломал mono'd RHS (basics/tuples nested), closure-tuple
  `nova_int`↔`void_p` struct copy fail (closure_mut_capture_escape),
  tuple-of-arrays потеря `NovaArray*` typing (types/arrays).
- **Закрыто (6 фиксов):**
  1. Preamble: `__USER_TYPE_FWD_DECLS__` теперь до `__MONO_TUPLE_TYPEDEFS__`.
  2. Topological sort tuple typedef'ов (B перед A если A ссылается на B).
  3. `emit_for` принимает `_NovaTuple_...` elem-type + populate registry.
  4. `let_destructure` infers actual RHS struct type через parse.
  5. `emit_tuple_return_stash` helper — field-wise copy с cast при mismatch.
  6. `ExprKind::TupleLit` mono path registers tuple_element_types + var_types.
- **Test:** 9 regression-guard'ов f1-f9 в `nova_tests/plan59/`
  (positive + negative-cc). Validation: 580 PASS / 0 FAIL after fix
  (vs 568 baseline до Plan 59 — net +12).
---

### [ЗАКР] Пустая ctx struct → MSVC C2016
- **Закрыто:** `char _dummy;` добавлен при пустом списке captures. MSVC требует ≥1 члена.
---

### [ЗАКР] Коллизия .obj файлов в build_c.bat
- **Закрыто:** Объектники кладутся в `%TEMP%\nova_build_%RANDOM%` с уникальными именами.
---

### [ЗАКР] Wildcard binding — C2374 при нескольких `let _ = spawn {}`
- **Закрыто:** `Pattern::Wildcard` → `fresh_tmp()` вместо фиксированного `"_nova_unused"`.
---

### [ЗАКР] Pre-scan не охватывал While/For/Loop/Match
- **Закрыто:** Добавлены все expression containers в `scan_expr_fwd`.
---

### [ЗАКР] examples/ разбросаны по compiler-codegen/ и compiler-bootstrap/
- **Закрыто:** Все .nv файлы перемещены в корневой `examples/`.
---

### [ЗАКР] GC и fibers не имели глубоких тестов
- **Закрыто:** `nova_rt/test_gc_deep.c` (23 assert, malloc+RC) и `nova_rt/test_fibers_deep.c`
  (31 assert) проверяют alloc_count/free_count/live_count, RC lifecycle, раздельность стеков
  fiber, yield/resume порядок, stack isolation, state machine через yield.
  На Nova-уровне: `nova_tests/37_deep_gc.nv` (18 тестов) и `38_deep_spawn.nv` (28 тестов).
---

### [ЗАКР] Spawn захватывал локальные переменные как внешние (C2065/C2020/C2440)
- **Закрыто:** Три бага исправлены при написании Nova-level deep тестов:
  1. `collect_bound_names`: имена из `let` внутри spawn, for-pattern, match-arm
     теперь исключаются из списка captures (были Nova_Point** вместо Nova_Point*).
  2. Поле результата ctx-struct переименовано `result` → `_nova_result` чтобы
     не конфликтовать с захваченной переменной пользователя named "result".
  3. `infer_expr_c_type`: добавлен кейс `ExprKind::If` — if без else → `nova_unit`
     (раньше → `nova_int`, что давало C2440 при cast результата spawn body).
---

### [ЗАКР 2026-05-07] Q-buffer — Buffer mutable byte accumulator
- **Где:** `nova_rt/buffer.h` + `emit_c.rs` (special-case dispatch для
  Buffer.new/.with_capacity/.from + receiver-typed instance methods).
- **Что реализовано:** unified Buffer для bytes-buffer и string-builder
  (унификация vs Go bytes.Buffer + strings.Builder, Rust Vec<u8> + String).
  API: Buffer.new() / .with_capacity / .from(s str) / .from(b []byte);
  add_str/add_bytes/add_byte/add_char (UTF-8 encode 1-4 байта); len /
  capacity / clone; into() → []byte / try_into() → str (UTF-8 валидация
  через Nova_Fail_fail при ошибке) / into_str_unchecked() — escape hatch.
- **Тесты:** `nova_tests/55_buffer.nv` (16 тестов: basic ops, grow,
  clone independence, UTF-8 1/2/4-byte, hot-loop 1000-add).
- **Закрывает Q-buffer** (open-questions.md).
---

### [ЗАКР 2026-05-07] Q-char-literals — char literals 'a' / '\n' / '\u{...}'
- **Где:** `lexer/mod.rs` (lex_char) + `lexer/token.rs` (Char(u32)) +
  `ast/mod.rs` (ExprKind::CharLit + Literal::Char) + `parser/mod.rs`
  (parse_primary + parse_pattern) + `codegen/emit_c.rs` (char как
  nova_int в bootstrap'е).
- **Что реализовано:** ASCII char-литералы ('a'), escape sequences
  (\n / \t / \r / \\ / \' / \" / \0), Unicode escapes (\u{HEX}, до
  6 hex digits). Validation: surrogate (0xD800..0xDFFF) и > 0x10FFFF
  отвергаются. Pattern matching: match c { 'a' => ... }.
- **Тесты:** `nova_tests/56_char_literals.nv` (16 тестов: ASCII,
  escape, Unicode, match-pattern, Buffer.add_char, range-check).
- **Закрывает Q-char-literals**, разблокирует stdlib examples
  (complex.nv: 317→560, json.nv: 163→98).
---

### [ЗАКР 2026-05-07] Trailing-block в head-позиции control-flow
- **Где:** `parser/mod.rs` (no_trailing_block flag).
- **Что было:** `match foo() { Some(i) => ... }` парсился как
  call-with-trailing-block (`foo()` + блок). Падало с
  `unexpected '=>' in expression`.
- **Фикс:** добавлен `with_no_struct_or_trailing` (комбинация
  no_struct_lit + no_trailing_block). Применён в head-позициях
  match/if/while/for scrutinee.
- **Разблокировало:** semver.nv (136→251), sql.nv (201→295).
---

### [ЗАКР 2026-05-07] D26 prelude API — Option/Result методы + str API
- **Где:** `nova_rt/array.h` (Nova_Option_method_*, Nova_Result_method_*)
  + `emit_c.rs` (special-case dispatch для NovaOpt_T*/Nova_Result*).
- **Что реализовано:** базовые методы Option (is_some/is_none/unwrap/
  unwrap_or/unwrap_or_else/map/ok_or/or) и Result (is_ok/is_err/ok/err/
  unwrap/unwrap_or/unwrap_or_else/map/map_err). unwrap для None/Err
  throw'ит Fail с сообщением.
- **Spec:** D26 (08-runtime.md) дополнен полным API + примерами;
  Q "полный API Option/Result" частично закрыт. Расширенный API
  (and_then, flatten) — Q-monadic-api отдельно.
- **Тесты:** `nova_tests/runtime/unwrap_or.nv` (14 тестов).
- Также формализованы string-методы в D26: find/rfind/contains/
  starts_with/ends_with/split/trim/to_lower/to_upper — все индексы
  byte-offset (consistent с slice).
---

### [ЗАКР 2026-05-07] Source annotations default-on
- **Где:** `compiler-codegen/src/main.rs` (CLI flag) +
  `emit_c.rs` (emit_source_annotation_for_stmt/expr/span).
- **Что:** `/* SRC: <Nova-line> */` комментарии перед каждым C-stmt
  включены **по умолчанию**. Opt-out через `--no-annotate-source`
  (раньше было opt-in `-a/--annotate-source`).
- **Покрытие:** Stmt::*, block.trailing, FnBody::Expr (4 места —
  обычные fn, generic, methods, main).
- **Sanitize:** `*/` → `* /` (escape comment-close); одинокие `*`/`/`
  сохраняются (multiplication / division читаемы); truncate до 120
  символов с " …" если урезано.
- **Q-source-annotations** — обновлён под default-on.
---

### [ЗАКР 2026-05-07] D77 4-way auto-derive (from/into/try_from/try_into)
- **Где:** spec/decisions/08-runtime.md (D73 + D77 disclaimer).
- **Что:** программист пишет ОДНУ из 4-х форм, компилятор синтезирует
  остальные. **Рекомендация:** реализовывать `try_from` (Result-стиль
  явный, error type first-class), использовать в коде `from`/`into`
  (короче, идиоматичнее).
- **Алгоритм синтеза** задокументирован в D73 «Auto-derive 4-way».
- **D25** — добавлена секция «Performance: насколько дорогой `throw`»
  с cost-model (~50-200ns в bootstrap, vs Java/C++/Rust/Go) и
  recommendation использовать Result-стиль для hot path.
---

### [ЗАКР 2026-05-07] `interrupt v` через mco-coroutine-boundary
- **Где:** `nova_rt/fibers.h` (NovaFiberQueue: fiber_interrupt_top[N],
  interrupt_pending/interrupt_value), `nova_rt/effects.c`
  (nova_interrupt с cross-boundary-path), `emit_c.rs` (spawn-entry
  catch detect'ит "__nova_interrupt__" sentinel).
- **Что было:** D61/D65 требует handler-method для Fail (`fail() ->
  Never`) завершаться через `interrupt v`. Когда with-frame на
  main-stack, а throw в spawn-body, longjmp пересекал mco-границу
  → UB. Тесты использовали bootstrap-leniency `return ()`.
- **Фикс:** per-fiber switching `_nova_interrupt_top` в supervised_step
  (как `_nova_fail_top`). Если nova_interrupt не находит fiber-local
  frame — записывает pending в scope, longjmp на fiber-local fail-frame
  с sentinel. supervised_run после drain re-issue'ит interrupt на
  main-flow.
- **Тесты:** все 14 occurrences `return ()` в 4 файлах
  (effects/fail_handler, syntax/throw_in_expression,
  concurrency/cancel_scope_test, runtime/unwrap_or) переведены на
  spec-correct `interrupt ()`.
---

### [ЗАКР 2026-05-07] Named tmps в сгенерированном C
- **Где:** `emit_c.rs::fresh_tmp_named(role)` + use-sites.
- **Что было:** `_nova_tmp0`, `_nova_tmp1`, ... — голый счётчик.
- **Что стало:** `_nv_<role>_<n>` — семантическая роль:
  scr/match/matched/if/if_let/while_let/while/loop/println/tmp.
- **Зона покрытия:** match, if, IfLet, WhileLet, While, Loop,
  println. Остальные ~40 fresh_tmp call-sites используют общий
  `_nv_tmp_<n>`.
---

### [ЗАКР 2026-05-07] D26 str API — школа B (codepoint-indexed)
- **Где:** `spec/decisions/08-runtime.md` (D26), `spec/open-questions.md`
  (Q-string-indexing → закрыта), `nova_rt/nova_rt.h::nova_str_slice`,
  `nova_rt/array.h::nova_str_find/rfind/byte_len`,
  `emit_c.rs::str_method_to_rt` + Member-handler.
- **Что было:** byte-indexed (Rust/Go-style) — `s.len` = bytes,
  slice/find возвращали byte-offsets. Логично для FFI, но нелогично
  для пользователя: `"мир".len` под byte-API даёт 6 (3 codepoint × 2),
  а ожидается 3; индексация в Cyrillic/emoji неинтуитивна.
- **Что стало:** codepoint-indexed (Python/Swift-style):
  - `s.len` — codepoints, O(n).
  - `s.byte_len()` — bytes, O(1).
  - `s.char_len()` — alias `len` (для явности).
  - `s.slice(a, b)` принимает codepoint-индексы.
  - `s.find(needle) / rfind(needle)` возвращают codepoint-offset.
  - Внутреннее хранение остаётся UTF-8. Для FFI/IO — `byte_len()`.
- **Trade-off:** O(n) на `len`/`slice` вместо O(1). Для real-world
  text-handling это не bottleneck (строки обычно небольшие, hot-path
  итерируется без `len`). Если станет проблемой — кэш codepoint-len
  на структуре `nova_str` (поле + invalidation на mutation).
- **Тесты:** `nova_tests/types/str_search.nv` обновлён (section 7
  переписан с `len == bytes` на `len == codepoints` + `byte_len()`,
  добавлена section 8 с Cyrillic/emoji find/rfind/slice).
- **Закрывает:** Q-string-indexing.
---

### [ЗАКР 2026-05-07] Bitwise операторы — реализованы
- **Где:** `compiler-codegen/src/lexer/{token.rs,mod.rs}`,
  `parser/mod.rs::parse_bit_or/xor/and/shift`,
  `codegen/emit_c.rs::Binary{,_op_str}`,
  `nova_tests/types/bitwise.nv` (28 тестов).
- **Что было:** lexer отвергал single `&` ("did you mean &&?") и `^`
  ("unexpected byte"). Spec D-operators (spec/03-syntax.md уровни 7-10)
  определяет `& | ^ << >>`, но bootstrap не реализовывал.
- **Что стало:** новые токены Amp, Caret, Shl, Shr; новые BinOp варианты
  BitAnd/BitOr/BitXor/Shl/Shr; парсер с правильными приоритетами
  (cmp(6) → bit-or(7) → bit-xor(8) → bit-and(9) → shift(10) → range/add(11)).
  Codegen emit'ит C-операторы 1:1 — биты тождественны.
- **Покрытие:** 28 тестов basic + precedence (5 кейсов проверки spec
  иерархии) + типичные паттерны (mask, set/toggle bit, even-check) +
  u64-литералы за i64::MAX.
---

### [ЗАКР 2026-05-07] u64 hex/bin литералы > i64::MAX wrapping в i64
- **Где:** `lexer/mod.rs::lex_radix_int`.
- **Причина:** Hash-константы FNV-64 (`0xCBF29CE484222325`),
  UUID-namespace, CRC требуют u64-битовых паттернов. У нас один тип i64
  (nova_int). Лексер падал на `invalid int: number too large to fit`.
- **Что стало:** Если `i64::from_str_radix` падает, пробуем
  `u64::from_str_radix` и приводим к i64 wrapping (`u as i64`). Биты
  тождественны — для bitwise/hash это корректно.
- **Trade-off:** В арифметических контекстах (e.g. `0xFFFFFFFFFFFFFFFF + 1`)
  результат будет арифметикой over signed i64, что отличается от u64
  semantics. Для будущей работы — введение типа `uint`/`u64` (отдельный
  open question; текущее поведение покрывает 95% use-cases).
- **Покрытие:** bitwise.nv section 8 (3 теста — wrapping → negative,
  all-ones = -1, high-bit set).
---

### [ЗАКР 2026-05-07] Handler-expr non-greedy в `with`-выражении
- **Где:** `parser/mod.rs::parse_expr_or_handler_lit`.
- **Причина:** Форма `with E = (e) => interrupt Err(e) { body }` —
  handler-lambda greedy ела `{ body }` как trailing-block после
  `interrupt Err(e)`. Парсер видел `interrupt Err(e) { body }` как
  call-with-trailing-block.
- **Что стало:** Перед fallback на `parse_expr` устанавливаем
  `no_trailing_block=true`. Теперь handler-выражение не захватывает
  следующий `{`-block — он достаётся внешнему with-парсеру.
- **Эффект:** ~10 stdlib-файлов продвинулись (complex/cron/duration/
  retry/semver/semver_range/snowflake/statistics/rate_limiter/ulid).
---

### [ЗАКР 2026-05-07] mut-маркер на параметре функции (D6)
- **Где:** `parser/mod.rs::parse_param`.
- **Причина:** D6 говорит, что `fn f(buf mut Buffer, ...)` означает
  внутри fn возможность мутировать значение. Bootstrap не парсил `mut`
  в позиции параметра.
- **Что стало:** После имени параметра optional `mut` ключевое слово
  съедается (игнорируется в семантике — у нас GC + reference, mut
  не меняет поведения). Это spec-faithful — позволяет писать код по
  стилю spec'а.
- **Эффект:** stdlib/uuid и stdlib/uuid_v3_v5 разблокированы.
---

### [ЗАКР 2026-05-07] D55 anonymous record literal с inferred type
- **Где:** `codegen/emit_c.rs::emit_record_lit` + `expected_record_type`
  state-поле + helper `struct_name_from_c_type`.
- **Причина:** Форма `fn make_point() -> Point => { x: 7, y: 11 }` —
  anonymous record без struct-name. Codegen падал с "anonymous record
  literal without spread not supported". Spec D55 описывает coercion
  в позиции с явным типом, но bootstrap-codegen не имел type-inference
  context.
- **Что стало:**
  - Новое state-поле `expected_record_type: Option<String>`.
  - В emit_method_body / emit_fn_body перед эмитом тела функции
    устанавливаем `expected_record_type` из declared return type
    (через helper `struct_name_from_c_type` извлекая имя из
    `Nova_Foo*`/`Nova_Foo`).
  - В emit_record_lit — новая ветка для случая "type_name=None +
    spread=None + expected_record_type=Some" — эмитит как для
    именованного record.
  - `Self` в expected_record_type разворачивается в
    current_receiver_type.
- **Покрытие:** records.nv — 2 новых теста.
- **Эффект:** stdlib/range, fnv, snowflake, statistics, rate_limiter,
  bloom_filter, ulid, semver — продвинулись на следующие блокеры.
- **Ограничение:** Только при declared return type в fn-сигнатуре.
  Inferred-type для let-bindings (`let p Point = { x:1, y:2 }`) не
  поддерживается — отдельная задача.
---

### [ЗАКР 2026-05-07] D79 Channel[T] base implementation (bootstrap)
- **Где:** `nova_rt/channels.h` (новый), `nova_rt.h` include,
  `emit_c.rs` dispatch, `nova_tests/runtime/channels.nv` (11 тестов).
- **Что было:** stdlib-агент закрыл D79 spec gap (channels формально
  декларированы); bootstrap-runtime отсутствовал.
- **Что стало:** bounded ring-buffer + send/recv yield + close+drain.
  Sequential-only в bootstrap (D71 single-threaded cooperative).
- **Ограничение:** spawn-block (`spawn { ch.recv() }`) упирается в
  существующий codegen-bug. Channel готов как только fix. Парсер
  `select { ... }` отложен — отдельная задача.
---

### [ЗАКР 2026-05-07] Lint pass + 2 правила (D65/D62)
- **Где:** новый модуль `src/lints.rs`, `lib.rs` pub mod,
  `main.rs` `--no-lint` флаг.
- **Что стало:** lint pass с двумя правилами:
  - `export-fail-untyped` (D65): `export fn ... Fail` без `[E]` →
    warning. `Fail[E]` typed и `Fail[any]` explicit erasure OK.
  - `protocol-in-effect-position` (D62 matrix): `fn f() Hash ->
    ()` → warning. Хардкод known protocols (Hash/Ord/Eq/Iter/
    From/Into/TryFrom/TryInto/ToStr).
- **Архитектура:** возвращает `Vec<LintWarning>`. main.rs выводит в
  stderr с правильным line:col + rule-name. Не блокирует compile.
  6 unit-тестов в lints.rs.
---

### [ЗАКР 2026-05-07] D28 effect inference для private fn (минимальная)
- **Где:** `types/mod.rs::infer_effects(&mut Module)`,
  `main.rs cmd_compile` вызывает после parse+check.
- **Причина:** D62 strict transitivity создаёт шум в private helper'ах.
  D28 говорит «private — выводится автоматически».
- **Что стало:** mutable walk. Для каждой private fn (`!is_export`):
  - has_throw_in_fn(f) — рекурсивный обход body (Stmt + Expr).
  - Если есть throw и нет Fail в effect-row → добавляем
    `TypeRef::Named "Fail"` (placeholder per D65).
- **Не реализовано в bootstrap'е (TODO production):**
  - Точный E через type-of(throw expr) — добавляем голый `Fail`.
  - Транзитивная inference (callee Fail → caller Fail).
  - Inference других эффектов (Db/Net/etc) — они resource-capability,
    D62 требует явной декларации.
  - Public fn не трогается — D62 strict; lint export-fail-untyped
    warning'ит вместо.
- **Тест:** throws.nv smoke — `fn validate_d28(n int) -> int { if n<0
  { throw "negative" } n*2 }` компилируется без явного Fail.
---

### [ЗАКР 2026-05-07] D78 path/module enforcement в codegen

- **Проблема:** D78 декларирует "module path = file path", но bootstrap
  не проверял это. Можно было скопировать `std/encoding/json.nv` в
  `examples/json.nv` с тем же `module std.encoding.json` — компилятор
  пропускал.
- **Где:** `compiler-codegen/src/manifest.rs` (новый), вызов в
  `cmd_check / cmd_run / cmd_compile / cmd_test` после parse, до
  type-check.
- **Что делает:** walks parent dirs от файла, ищет `nova.toml`. Из
  manifest извлекает `[package].name` и `[lib].src` (минимальный
  TOML-парсер, не тянем full crate). Source root = `<dir>/<src>`.
  Expected module = `<package>.<rel-path-from-src-without-ext>`. Если
  declared != expected → compile error с hints (move-file vs rename-
  module). Если nova.toml не найден — skip (file вне пакета, ad-hoc
  script).
- **Сразу после реализации:** `tests-nova/` → `nova_tests/`. Имя
  директории должно совпадать с `package.name` внутри `nova.toml`,
  иначе все declared `module nova_tests.<group>.<file>` мисматчат
  file path (строгое enforcement активировалось → проявило старое
  несоответствие).
---

### [ЗАКР 2026-05-07] tests-nova/ → nova_tests/

- **Причина:** `tests-nova/nova.toml` содержит `[package].name =
  "nova_tests"`. По D78 имя директории == package.name, иначе
  enforcement ругается.
- **Что сделано:** rename + apdate всех refs (root `nova.toml`
  workspace member, `run_tests.ps1`, `compiler-{bootstrap,codegen}/
  tests/spec_nova.rs`, `.gitignore`, спека D78 в 07-modules.md, docs).
  В spec/decisions/history/evolution.md ссылки на `tests-nova/` оставлены
  как frozen historical record.
---

### [ЗАКР 2026-05-07] D38 turbofish в expression-position

- **Проблема:** Spec D38 декларировала `Cache[K, V].new()` и `parse[T](x)`
  как канонический синтаксис в expression-position, но bootstrap-парсер
  при виде `Ident[...]` всегда трактовал `[` как Index. Падение на
  `expected ']' got ','` в 5+ stdlib-файлах (hashmap, lru, jwt, ini, toml).
- **Где:** `compiler-codegen/src/ast/mod.rs` (`ExprKind::TurboFish`),
  `parser/mod.rs` (`try_parse_turbofish_args` + ветка LBracket в
  `parse_postfix`), `codegen/emit_c.rs` (TurboFish-arm + unwrap helper),
  `interp/mod.rs`, `types/mod.rs`.
- **AST:** новый узел `ExprKind::TurboFish { base, type_args }`. Не
  выбрасываем `type_args` — сохраняем для будущих этапов real type
  inference / monomorphization.
- **Парсер (peek-disambiguation):** speculative-parse `[...]` как
  `parse_type_args`; если успешно И post-`]` token — `(` (call),
  `.IDENT(` (method-call) или `?` (Try) — TurboFish; иначе rollback к
  Index. Multi-arg внутри `[...]` — однозначно turbofish (Index никогда
  не имеет comma). Все edge-кейсы пройдены: `xs[i].field` остаётся
  Index→Member (`.field` без `(` после), `xs[i]` без continuation —
  Index, `Type[K, V].method(...)` — TurboFish, `parse[int]("42")?` —
  TurboFish.
- **Codegen / interp:** `Expr::unwrap_turbofish()` распаковывает в base;
  применяется в emit_call, infer_expr_c_type, emit_stmt (let-decl
  generic-fn-tuple-arity), evaluate_expr — везде, где downstream
  смотрит на конкретный `kind`.
- **Не реализовано:** type-checker не валидирует что `type_args`
  satisfy generic bounds (D72) — bootstrap erases generics.
- **Тесты:** `nova_tests/types/generics.nv` — 4 теста: single-arg
  function, two-arg function, Index-regression, arr[i].field-regression.
  Stdlib-files (hashmap/lru/jwt/ini/toml) теперь проходят
  turbofish-блокер (упираются в следующие — отдельные блокеры).
---

### [ЗАКР 2026-05-08] D54 `as`-cast — реализован narrowing в codegen [P-as-cast-wraparound]

- **Проблема:** `ExprKind::As(inner, _ty)` в codegen был **no-op** —
  игнорировал target-тип, эмитил inner expression «как есть».
  Narrowing работал только косвенно (через C-narrowing на push в
  uint8_t-слот / param-копировании) — не как следствие `as`.
- **Где:** `compiler-codegen/src/codegen/emit_c.rs`:
  - `ExprKind::As` в emit_expr — теперь `(({c_ty})({inner}))`
  - `ExprKind::As` в `infer_expr_c_type` — возвращает target type из
    annotation, не type-of(inner)
- **Семантика overflow (D54 не уточнял):** **wraparound** в стиле
  C-narrowing для int → меньший int (truncate младших битов).
  Согласовано с C/Go/Rust 1.45+. Checked-cast (panic-on-overflow) —
  отложен в Q-checked-cast / future D-decision.
- **Cases:**
  - int → byte / int → i32 / etc. — wraparound
  - int → f64 / byte → int — identity (numeric promotion)
  - f64 → int — truncate (как в C)
  - newtype-alias ↔ underlying — idempotent (одинаковое C-представление)
- **Тесты:** `nova_tests/syntax/as_cast.nv` — 8 тестов (narrowing,
  bitwise-mask cast, i32 from i64, f64 ↔ int, newtype identity,
  zero-byte, negative wraparound). 63/63 nova_tests PASS.
- **Stdlib regression-проверка:** crc32/fnv/bloom_filter (активно
  используют `as byte` / `as u32` в bitwise pack'ах) продолжают
  работать. Stdlib total: 66 PASS (+1 markdown_minimal который теперь
  считается RUN-FAIL).
- **План:** docs/plans/05-as-cast-codegen.md.
---

### [ЗАКР 2026-05-08] D54 float→int saturation [P-as-cast-float-saturation]

- **Проблема:** План 05 закрыл основной gap (`as` теперь C-cast), но
  для **float → int** narrowing'а оставил UB на out-of-range / NaN /
  ±Infinity (C-стандарт §6.3.1.4 не определяет behavior). Spec D54
  не специфицировал semantics narrowing — gap-by-omission.
- **Решение:** float → integer narrowing делает **saturation** через
  runtime helper'ы, NaN→0, ±∞→границы. Согласовано с Rust 1.45+
  (RFC #2484 «sealed casts»). `as` остаётся pure — throw-форма
  для checked-cast доступна через D77 `iN.try_from(f)?`.
- **Где:**
  - `compiler-codegen/nova_rt/cast.h` (новый, ~140 строк) — 16
    `static inline` helper'ов: `nova_f64_to_{i8,i16,i32,i64,u8,u16,u32,u64}`
    и аналог для f32. Все ветки: `isnan` → 0, range bounds →
    `INT_MAX/MIN`, иначе truncate towards zero (как C).
  - `compiler-codegen/src/codegen/emit_c.rs::ExprKind::As` —
    детектит `f64/f32 → integer` пару и эмитит helper-call;
    остальные cast'ы остаются прямым C-cast (плана 05).
  - `compiler-codegen/nova_rt/nova_rt.h` — `#include "cast.h"`.
- **Bonus fix:** `ExprKind::FloatLit` codegen теперь принудительно
  эмитит scientific notation (`{:e}`) для очень больших / малых
  значений и `.0`-суффикс для целых f64-литералов. Иначе
  `1e20` эмитился как integer-literal `100000000000000000000`,
  переполняя u64 (MSVC C2177).
- **Тесты:** `nova_tests/syntax/as_cast_float.nv` — 13 тестов:
  in-range, out-of-range positive/negative, NaN, ±Infinity, unsigned
  negative→0, INT64 boundary saturation, int wraparound regression.
  64/64 nova_tests PASS.
- **Spec D54** дополнен таблицей «Семантика narrowing-конверсий» —
  закрывает spec-долг плана 05.
- **Не реализовано:**
  - `unchecked_as` (zero-cost UB-cast) — отвергнут D9 «один путь»;
    если профайлер покажет потребность — escape hatch через FFI.
  - f128 / f16 / bfloat16 не покрыты (нет в bootstrap).
  - Generic `_Generic`-helper отвергнут: 16 явных функций читаются
    прямо.
- **План:** docs/plans/07-as-cast-saturation.md.
---

### [ЗАКР 2026-05-08] D26 prelude: Result/Option methods полностью покрыты в codegen

- **Что:** реализованы `map_err`, `map`, `unwrap_or_else`, `err()`
  для Result, и `unwrap_or_else`, `map`, `ok_or` для Option в codegen
  (раньше были только `is_ok/is_err/unwrap/unwrap_or/ok` для Result
  и `is_some/is_none/unwrap/unwrap_or` для Option).
- **Где:** `compiler-codegen/src/codegen/emit_c.rs::ExprKind::Member`
  call dispatch для `obj_ty == "Nova_Result*"` и
  `obj_ty.starts_with("NovaOpt_")`. Inline-эмит fresh tmp + tag-check
  + closure-call через NovaClos_ii / NovaClos_vi.
- **Bonus 1:** `emit_lambda` теперь принимает `return_type_ann:
  Option<&TypeRef>` — явная аннотация `(e str) -> str => ...` берётся
  из AST; раньше игнорировалась → C2440 mismatch на str-payload.
- **Bonus 2:** parser lookahead за `(` различает zero-arg lambda
  `() => expr` и unit-литерал `()`. Раньше `() => 0` не парсился —
  `()` сразу проглатывался как UnitLit.
- **Тесты:** `nova_tests/runtime/result_methods.nv` — 22 теста (10
  новых для unwrap/err/Option-методов). 65/65 nova_tests PASS,
  stdlib без регрессий.
- **Spec:** `spec/decisions/08-runtime.md` D26 секция дополнена
  таблицей Bootstrap status с явным мапингом реализованного.
- **Bootstrap-ограничения зафиксированы в spec'е:**
  Q-result-monomorphization (Result hardcoded на nova_int / nova_str),
  Q-closure-param-inference (lambda-параметры требуют явной
  аннотации для не-int типов).
---

### [ЗАКР 2026-05-08] D26 prelude: Error и RuntimeError встроены в runtime

- **Что:** добавлены prelude-типы `Error` (record с `msg`) и
  `RuntimeError` (sum со 6 вариантами: `DivByZero`, `Overflow`,
  `IndexOutOfBounds {index, length}`, `TypeMismatch(str)`,
  `AssertFailed(str)`, `NoHandler(str)`). Раньше были только в
  spec'е — реальной реализации в bootstrap не было.
- **Где:**
  - `compiler-codegen/nova_rt/array.h` (~80 строк): `Nova_Error`
    struct + `Nova_Error_static_new(msg)`; `Nova_RuntimeError`
    tag-union + 6 `nova_make_RuntimeError_<Variant>` конструкторов.
    Tag константы `NOVA_TAG_RuntimeError_*`.
  - `compiler-codegen/src/codegen/emit_c.rs` в `emit_module`
    pre-population: `record_schemas["Error"] = {msg: nova_str}`,
    `method_receivers["new"] = ("Error", false)`,
    `sum_schemas["RuntimeError"] = {DivByZero: [], ..., NoHandler: [str]}`.
    `record_variant_field_order["RuntimeError::IndexOutOfBounds"] =
    [index, length]`.
  - `infer_expr_c_type` дополнен: `Error.new(...)` → `Nova_Error*`.
- **Тесты:** `nova_tests/runtime/error_runtime_error.nv` — 11 тестов:
  Error.new (basic, empty, в throw), все 6 вариантов RuntimeError
  (unit, record, tuple), independence Error/RuntimeError.
  Используют `if let` extraction вместо assignment-style match
  (избегает nova_assert-void mismatch — bootstrap-codegen ограничение).
- **Spec:** `spec/decisions/08-runtime.md` D26 секция таблицы
  Bootstrap status дополнена 8 новыми строками.
- **Bootstrap-ограничения зафиксированы:**
  - `Error.msg` поле без enforce'а readonly (spec говорит readonly,
    bootstrap-grade compromise).
  - `RuntimeError` варианты доступны user-коду, но **встроенные
    операции** (`a/b`, `arr[i]`, NoHandler) всё ещё throw'ают
    `nova_str` через `Nova_Fail_fail`. Конверсия throw-points в
    `Nova_RuntimeError*` payload — отдельная задача (требует
    расширения fail-frame mechanism).
---

### [ЗАКР 2026-05-08] Plan 08 Ф.1+Ф.2: try_from / from для встроенных пар

- **Что:** `int.try_from("42")`, `f64.try_from("3.14")`,
  `bool.try_from("true")`, `char.try_from("A")`, `char.try_from(65)`,
  `str.from(42)`, `str.from(true)`, `str.from('A')` — работают через
  codegen-path. Раньше падали на parse-фазе или эмитили raw `int.try_from(...)`
  что не компилировалось C-компилятором.
- **Где:**
  - `compiler-codegen/nova_rt/conv.h` (новый, ~180 строк): runtime-helpers
    `nova_str_to_i64`, `nova_str_to_u64`, `nova_str_to_f64`, `nova_str_to_bool`,
    `nova_str_to_char`, `nova_int_to_char`, `nova_char_to_str` (UTF-8 encode),
    `nova_bool_to_str`, `nova_f64_to_str`. Все `static inline`.
  - `compiler-codegen/src/parser/mod.rs`: parse_primary дополнен — primitive
    type-names (int/i8-i64/u8-u64/f32/f64/byte/bool/char/str) могут
    инициировать Path-конструкцию (`int.try_from(s)` парсится как
    `Path(["int", "try_from"])` вместо `Member { Ident("int"), "try_from" }`).
    Раньше PascalCase-only — поэтому `int.try_from` не работало.
  - `compiler-codegen/src/codegen/emit_c.rs`: Path-call dispatch для
    `T.try_from(v)` (numeric/bool/char через runtime helper'ы) и `T.from(v)`
    (str.from для bool/char/numeric). Type-inference: `T.try_from(...)` →
    `Nova_Result*`, `str.from(...)` → `nova_str`.
- **Bootstrap-ограничение:** Result hardcoded на `(nova_int Ok, nova_str Err)`
  — все try_from эмитят payload как `nova_int` (для f64 это means
  bit-pattern truncation). Полный generic Result — отдельная задача
  Q-result-monomorphization.
- **CharLit detection:** `str.from(arg)` для CharLit-arg (например `'A'`)
  специально проверяется ДО общего numeric-arm, потому что char хранится
  как nova_int но семантика char→str (UTF-8 encode) ≠ int→str (decimal).
- **Тесты:** `nova_tests/runtime/from_into_basic.nv` — 26 тестов:
  int/f64/bool/char.try_from валидное+невалидное+overflow,
  char.try_from(int) range/surrogate, str.from(int/bool/char) ASCII+
  Cyrillic UTF-8. 67/67 nova_tests PASS.
- **Не сделано в этом коммите:** Ф.3 (4-way auto-derive synthesis),
  Ф.4 (strict if cond:bool), Ф.5 (as-cast restrictions), Ф.6 (generic-bound
  enforcement), Ф.7 (spec). Делаются отдельными коммитами.
- **План:** docs/plans/08-from-into-conversions.md.
---

### [ЗАКР 2026-05-08] Plan 06 Ф.1: Iter[T] protocol fallback в for-in

- **Что:** `emit_for` получил Case 3 — generic loop через
  `Nova_<T>_method_next(it)` для любого user-type'а с
  `mut @next() -> Option[T]`. Раньше падало с
  `for-in: unsupported iterator type 'Nova_X*'`.
- **Где:**
  - `compiler-codegen/src/codegen/emit_c.rs::emit_for`: Case 3 после
    Range и Array. Использует новый registry `all_methods:
    HashSet<(TypeName, MethodName)>`.
  - Multi-key registry избегает single-key last-wins проблемы:
    несколько типов с методом `next` (Counter, Doubler, RangeIter и т.д.)
    больше не вытесняют друг друга.
- **Bootstrap-ограничения:**
  - Element type из `Option[T]` не infer'ится — payload эмитится как
    `nova_int` (соответствует Result hardcoded на nova_int).
  - Tuple destructuring `for (k, v) in m.entries()` ещё не работает
    (Ф.2 plan'а 06).
  - Implicit `.iter()` для коллекций (`for x in deque` без
    `.iter()`) ещё не работает (Ф.3).
- **Тесты:** `nova_tests/syntax/for_iter.nv` — 4 теста: custom Counter
  (basic, empty, single), stateful Doubler (бесконечный с
  None-return). 68/68 nova_tests PASS.
- **Не сделано в этом коммите:** Ф.2 (tuple-destructuring), Ф.3
  (implicit `.iter()`), Ф.5 (sweep std/collections тестов).
- **План:** docs/plans/06-iter-protocol-codegen.md.
---

### [ЗАКР 2026-05-08] Plan 08 Ф.3: D77 4-way auto-derive synthesis

- **Что:** программист пишет ОДНУ форму конверсии, codegen синтезирует
  обратную:
  - `T.try_from(v V)` → `v.@try_into() -> Result[T, E]` (новое в Ф.3)
  - `T.from(v V)` → `v.@into() -> T` (уже работало; добавлен infer)
- **Где:**
  - `compiler-codegen/src/codegen/emit_c.rs`: новые registries
    `try_from_targets: HashMap<TargetT, Vec<SourceV>>` и
    `try_into_targets: HashMap<SourceV, TargetT>`. Заполняются при
    AST-walk fn-items с receiver'ом.
  - Member-call dispatch для `v.@try_into()`: lookup в
    `try_from_targets` → emit `Nova_T_static_try_from(v)`.
  - Type-inference для `v.@try_into()` → `Nova_Result*`,
    `v.@into()` → `Nova_<Target>*`.
- **Bonus fix:** `nova_type_name_from_c` helper — конвертирует C-type
  обратно в Nova-имя (`nova_int → int`, `Nova_Wrapper* → Wrapper`).
  Без него from_targets/try_from_targets lookup'ы не находили primitive
  receiver'ы (registry хранит Nova-имена, runtime даёт C-имена).
- **Тесты:** `nova_tests/runtime/auto_derive.nv` — 7 тестов:
  Celsius.try_from(int) валидное/невалидное, int.@try_into() через
  synthesis, Wrapper.from(int) + 100.into() через synthesis.
  69/69 nova_tests PASS.
- **Bootstrap-ограничения:**
  - Compile-time check «synthesis target существует?» не реализован
    (если нет ни from, ни into для пары — silent fall-through).
    Делается в Ф.6 (generic-bound enforcement).
  - Транзитивный auto-derive (`int.from(i32)` + `f64.from(int)` ⇒
    `f64.from(i32)`) НЕ делается (consciously, чтобы не выдавать
    surprising paths).
- **План:** docs/plans/08-from-into-conversions.md (Ф.3 закрыт).
---

### [ЗАКР 2026-05-08] Cleanup: keyword-алиасы or/and/not удалены

- **Что:** в D49 фиксировались `or`/`and`/`not` как keyword-aliases для
  `||`/`&&`/`!`. Реальных употреблений в `.nv` корпусе ноль; алиасы
  нарушают D9 «один очевидный путь» / D40 «один способ».
- **Где:**
  - `compiler-codegen/src/lexer/{mod.rs,token.rs}` — удалены KwAnd/
    KwOr/KwNot из identifier-маппинга, enum, display-имён.
  - `compiler-codegen/src/parser/mod.rs` — упрощены `parse_or`,
    `parse_and`, `parse_unary` (`Bang | KwNot` → `Bang`).
  - `compiler-bootstrap/*` — те же 6 правок симметрично.
  - `spec/decisions/03-syntax.md` D49 — переписан «||/&&/or/and» → «||/&&».
- **Verification:**
  - `grep KwAnd|KwOr|KwNot` — ноль матчей в обоих компиляторах.
  - `cargo check` — оба прошли чисто (без regression-warnings).
  - `nova_tests/` + `std/` — ноль реальных употреблений (single
    match — английское слово в test-name string-literal).
- **Урок:** keyword-алиасы можно безопасно удалять если:
  (a) ноль реальных употреблений в корпусе кода,
  (b) есть символьный эквивалент,
  (c) удаление освобождает identifier для пользовательского кода.
  Все три выполнены — `or`/`and`/`not` теперь обычные identifier'ы.
---

### [ЗАКР 2026-05-08] Plan 04 зафиксирован: Buffer split на 3 типа + external keyword

- **Что:** текущий `Buffer` (Q-buffer ✅) смешивает text-domain и
  binary-domain. Split на три типа со специализированной семантикой:
  - **StringBuilder** (UTF-8 string accumulator, `@into() -> str`
    infallible).
  - **WriteBuffer** (binary serialization, `@write_*_le/be`).
  - **ReadBuffer** (cursor-style binary reader, `@read_*` Fail-form +
    `@try_read_*` Result-form auto-derive на C-runtime уровне).
- **Plus новый keyword `external`** для stdlib runtime-implemented
  функций. `external` только для функций; типы Builder/Buffer'ов
  built-in opaque как примитивы (не объявляются отдельно).
- **char ↔ str через D73:** `str.from(c char)` external + auto-derived
  `char.@into() -> str`.
- **D30 расширение:** «полные слова, не сокращения», `len`/`iter`
  mainstream exceptions. `@position` не `@pos`, `@capacity` не `@cap`.
- **Где:** план зафиксирован в `docs/plans/04-buffer-split-and-external.md`.
  Реализация после Plan 08 (D73+D77 4-way auto-derive infrastructure).
- **Эволюция дизайна:** длинная итерация по naming (10+ поворотов:
  `add_*`/`append_*`/`write_*`/`put_*`) показала что **не naming
  главное, а split типов**. Когда зафиксировали split — naming
  решился сам:
  - `StringBuilder.@append` (Java/Go convention),
  - `WriteBuffer.@write_*` (Go bytes.Buffer-style),
  - `ReadBuffer.@read_*` (Go/Rust convention).
- **Урок:** когда обсуждение naming идёт в десятках поворотов — это
  signal что **не naming проблема**. Что-то более фундаментальное
  (часто — структура типов / разделение domain'ов) не решено. После
  правильного split'а имена находятся естественно.
---

### [ЗАКР 2026-05-08] Build pipeline scripts: build_c.ps1 + build_c.sh

- **Что:** документация `.nv → .exe` pipeline была частично сделана
  и **с ошибками** в `compiler-codegen/README.md`:
  - Упоминание `build_c.bat` в неправильном контексте (он принимает
    `.c`, не `.nv`).
  - GCC command с `-Inova_rt` (неправильный path; нужно `-I.` потому
    что codegen эмитит `#include "nova_rt/nova_rt.h"`).
  - MSVC pipeline отсутствовал — был только в `run_tests.ps1`.
  - Top-level README'и build pipeline не упоминали.
- **Где:**
  - **Создан** `compiler-codegen/build_c.ps1` — Windows wrapper
    (`.nv → .c → .exe` one-shot, опции `-Run`, `-Output`, `-KeepC`,
    `-VCVarsPath`).
  - **Создан** `compiler-codegen/build_c.sh` — Linux/Mac wrapper
    (`--run`, `-o`, `--keep-c`, `--cc gcc|clang`).
  - Существующий `build_c.bat` оставлен — другая роль (advanced,
    с `gc=malloc|rc|boehm` для GC backend).
  - `compiler-codegen/README.md` переписан: walkthrough'и, разделение
    ролей трёх wrapper'ов, CLI-флаги, ограничения, batch через
    `run_tests.ps1`.
  - Top-level `README.md` + `README.ru.md` — секции «Building from
    source» / «Сборка из исходников» с ссылками.
- **Verification:** `build_c.ps1` тестирован end-to-end на hello.nv
  (Hello, Nova! работает) + error-path (broken.nv → понятная
  диагностика unresolved symbol).
- **Урок:** документация build-pipeline критична для onboarding'а.
  Ошибки в README жили ~год без detection — нужны end-to-end
  walkthrough-тесты (запуск примеров **из README** в CI), не только
  cargo test самих компиляторов.
---

### [ЗАКР 2026-05-08] Editor support: sublime/vim/emacs plugin'ы + sync VSCode подсветки

- **Что:** до этой задачи в `editors/` был только VSCode plugin.
  Добавлены plugin'ы для остальных популярных IDE.
- **Где:**
  - `editors/sublime/` — переиспользует TextMate-grammar от VSCode
    напрямую через symlink в `Packages/Nova/`. Sublime parser
    Oniguruma-compatible с VSCode.
  - `editors/vim/{ftdetect,ftplugin,syntax}/nova.vim` — handcrafted
    Vim plugin (~150 строк), filetype detection + comment/indent
    settings + syntax keyword'ы.
  - `editors/emacs/nova-mode.el` — single-file major-mode с font-lock-
    keywords, syntax-table, auto-mode-alist + optional rainbow-
    delimiters integration.
  - `editors/README.md` — общий index по всем IDE с table support'а
    + roadmap (LSP > tree-sitter > JetBrains).
  - `editors/vscode/syntaxes/nova.tmLanguage.json` — sync со spec'ом:
    удалены `resume`/`String`/`Mutex`/`RwLock`/`Atomic`/`money`/etc,
    добавлены `protocol`/`external`/`RuntimeError`/`CancelToken`/
    `TryFrom`/`TryInto`/`StringBuilder`/`WriteBuffer`/`ReadBuffer`/
    `Detach`/`Blocking`/`Mem`.
  - VSCode README исправлен (неправильный path `\.vscode\nova-extension`
    → корректный `editors\vscode`).
  - Bracket pair colorization recommendations во всех 4 IDE README'ях
    (VSCode settings.json, Vim rainbow.vim, Emacs rainbow-delimiters,
    Sublime BracketHighlighter).
  - Top-level README'и расширены таблицей всех 4 plugin'ов.
- **Cover:** VSCode + Cursor + VSCodium + Sublime + TextMate + Vim +
  Neovim + Emacs (8 IDE через 4 plugin'а).
- **Не сделано:** JetBrains plugin (требует Java/Kotlin), tree-sitter
  grammar (Zed/Helix/Neovim 0.5+, отдельный ~10-20ч проект),
  GitHub Linguist (требует PR в чужой repo + 200+ файлов).
- **Source-of-truth для keyword'ов:** `compiler-codegen/src/lexer/mod.rs`
  функция `lex_ident_or_keyword`. Все 4 plugin'а синхронизируются
  против этого файла — задокументировано в editors/README.md.
- **Урок:** TextMate-grammar (Oniguruma) переиспользуется в VSCode
  семействе (Cursor/VSCodium/Sublime/TextMate) без изменений. Для
  Vim/Emacs нужен handcrafted формат. tree-sitter — современный
  стандарт (Zed/Helix/Neovim 0.5+/GitHub web), но единый grammar
  для 4+ редакторов requires separate проекта ~10-20ч. MVP покрывает
  достаточно через TextMate + handcrafted без tree-sitter.
---

### [ЗАКР 2026-05-08] Plan 08 Ф.4: strict `if cond: bool` в codegen

- **Что:** D54 требует cond обязан быть `bool`, не truthy-int (Rust/
  Swift/Kotlin прецедент). Раньше bootstrap'е `if int_value { ... }`
  silently компилировался — C принимает int как truthy. Закрывает
  silent-bug class.
- **Где:**
  - `compiler-codegen/src/codegen/emit_c.rs::emit_if_expr` и
    `ExprKind::While` arm: проверка `check_bool_condition(cond_ty)`
    перед emit'ом. Если cond_ty в списке non-bool primitives
    (nova_int, nova_f64, nova_str, ...) — compile error.
  - **Conservative**: type-neutral (void*, unknown user types) —
    пропускаем, чтобы не ломать существующий код.
- **Bonus prerequisite fixes** (необходимы для strict-check):
  - `infer_expr_c_type` для `ExprKind::Unary`:
    `!x` → nova_bool, `-x` → тип operand'а. Раньше fall-through на
    nova_int (например `if !cancelled` падал даже когда `cancelled: bool`).
  - `infer_expr_c_type` для `ExprKind::Block(b)`: возвращает тип
    trailing-expression. Раньше fall-through на nova_int (например
    `let cond = { ...; n > 0 }; if cond` падал).
  - `infer_expr_c_type` для closure-call (Ident-call к binding в
    fn_param_sigs): возвращает ret_ty из sig. Раньше fall-through на
    nova_int (`pred(x)` где `pred: fn(int) -> bool` инфер'ился как int).
  - `let pred = (n int) -> bool => ...` — registration в fn_param_sigs
    теперь использует Lambda's return_type-аннотацию.
- **Тесты:** `nova_tests/syntax/strict_if_bool.nv` — 9 positive-тестов:
  bool literal, comparisons, unary !, &&/||, block-expr cond, while
  с bool, fn-call возвращающий bool, closure-call возвращающий bool.
  70/70 nova_tests PASS, без регрессий.
- **Bootstrap-ограничения:**
  - Negative cases (`if int_value` → compile error) проверяются
    вручную через `nova-codegen check`, не в test-runner'е.
  - Compile-error suggestions ("use `n != 0`...") — TBD в Ф.7.
- **План:** docs/plans/08-from-into-conversions.md (Ф.4 закрыт).
---

### [ЗАКР 2026-05-08] Plan 08 Ф.5: as-cast restrictions для char/byte/bool

- **Что:** D54 явно запрещает некоторые `as`-cast'ы из-за неочевидной
  или небезопасной семантики. Bootstrap раньше silently разрешал всё —
  теперь даёт compile error с suggestion'ом использовать `try_from`
  или explicit comparison.
- **Запрещённые пары** (compile error):
  - `int as char`, `i32/i64/u32/u64 as char` → use `char.try_from(n)?`
  - `char as byte` → use `byte.try_from(c)?`
  - `int/byte/f64/etc as bool` → use `n != 0`
  - `str as int/i32/f64/bool` → use `T.try_from(s)?`
  - `int/f64/bool/char as str` → use `str.from(v)`
- **Исключение для CharLit:** `'A' as byte`, `'A' as int`, `'A' as u8`
  разрешены — программист видит codepoint буквально, range-check не
  нужен (existing stdlib usage в str_search.nv использует этот паттерн).
- **Где:**
  - `compiler-codegen/src/codegen/emit_c.rs::ExprKind::As`: добавлен
    `check_as_cast_allowed(src_nova, tgt_nova, inner_kind)` перед emit'ом.
    Detection через `target_nova` из `TypeRef::Named`, не C-имя
    (char и int имеют одинаковый C-тип nova_int).
  - Helper `nova_type_name_from_c` (Plan 08 Ф.3) reused для извлечения
    Nova-имени src.
- **Тесты:** `nova_tests/syntax/as_cast_restrictions.nv` — 6 positive-
  тестов: char-literal cast разрешён, byte→int widening, int→byte
  wraparound, f64→int saturation, bool→int, newtype-alias identity.
  71/71 nova_tests PASS, без регрессий.
- **Bootstrap-ограничения:**
  - Negative cases — manual check через `nova-codegen check`.
  - Compile-error не имеет file:line:col — использует error из
    emit_expr fallthrough. Полная diagnostic — Ф.7.
- **План:** docs/plans/08-from-into-conversions.md (Ф.5 закрыт).
---

### [ЗАКР 2026-05-10] Plan 16: D63 forbid + D64 realtime capability enforcement

- **Что:** `forbid X { body }` теперь действительно блокирует вызовы
  fn'ов с эффектом X внутри body. `realtime { body }` блокирует
  suspend-effects (Net/Fs/Db/Time/Blocking). `realtime nogc { body }`
  дополнительно блокирует alloc-fn'ы (`[]T.new`, `HashMap.new`,
  `StringBuilder.new`, `str.from`, etc.). `with X = ...` внутри
  `forbid X` — compile error (D63 «forbid непреодолим»).
- **Где:** `compiler-codegen/src/types/mod.rs::CapabilityCtx` (~492
  строки). AST + parser: новый `RealtimeAttr` enum + `@realtime`
  attribute parsing (~50 строк). Test infra: `// EXPECT_COMPILE_ERROR`
  маркер в `run_tests.ps1` (~46 строк).
- **Use-site эффект:**
    ```nova
    type Net effect { fetch(url str) -> str }
    fn http_get(url str) Net -> str => Net.fetch(url)

    fn run_user_script() Fail -> () =>
        forbid Net, Fs {
            http_get("/api")  // ← compile error: requires effect Net,
                              //   forbidden by enclosing `forbid Net`.
        }

    @realtime
    fn checksum(data []byte) -> int {
        mut sum = 0
        for b in data { sum += b as int }
        sum
    }

    realtime nogc {
        ro xs = []int.new()  // ← compile error: cannot allocate
                              //   inside `realtime nogc`.
    }
    ```
- **Не покрывается:**
    + Транзитивные effect-tracking (callee → callee → effect) — пока
      pure name-based, без effect-row inference.
    + Closure-capture handler'ов через `with` — не отслеживается.
    + User-defined record-конструкторы как alloc-fn'ы (требует
      heap-alloc inference).
    + Runtime sentinel-frame для transitive effects (D63 mentions it
      как production runtime mechanism, отдельная задача).
- **План:** docs/plans/16-capability-enforcement.md ✅ ЗАКРЫТ
  (Ф.1-Ф.9). nova_tests **97/97 PASS** (92 baseline + 5 negative).
---

### [ЗАКР 2026-05-09] Plan 15 Ф.5: D53 strict-mode (split Protocol vs Effect)

- **Что:** AST теперь различает `protocol` и `effect`-keyword'ы через
  отдельные `TypeDeclKind` variants (раньше оба попадали в `Effect`).
  BoundCtx (D72) регистрирует только Protocol-kind; попытка использовать
  effect как bound — R5.3-style compile-error с hint'ом «`X` is an
  effect, declare as `protocol`». Codegen пропускает vtable-emission для
  Protocol → попутно фиксит pre-existing Self-bug.
- **Где:** `compiler-codegen/src/{ast/mod.rs, parser/mod.rs,
  codegen/emit_c.rs, types/mod.rs, lints.rs}` — ~70 строк.
- **Use-site эффект:**
    ```nova
    type Db effect { query(q str) -> []str }
    fn bad[T Db](x T) -> T => x
    // ← compile-error:
    //   type `Db` is an effect, not a protocol — generic bounds
    //     require protocol-types (D72/D53).
    //   Hint: declare `Db` as `type Db protocol { ... }`.

    type Hash protocol { hash() -> u64; eq(other Self) -> bool }
    // ← Self в protocol-методе теперь работает (Self-bug fix bonus).
    ```
- **Не покрывается:** D53 §628 анонимные protocol-литералы в позиции
  типа (`fn close(c protocol { close() -> () })`) — требует нового
  `TypeRef::Protocol(...)` variant'а, отдельная задача.
- **План:** docs/plans/15-...md Ф.5 ✅.
---

### [ЗАКР 2026-05-09] Plan 15 Ф.1+Ф.2+Ф.3: D72 generic bounds enforcement

- **Что:** `[T Hash]` синтаксис теперь парсится и type-checker
  валидирует на use-site, что concrete-тип удовлетворяет protocol-
  bound'у. На mismatch — структурированный R5.3-style diagnostic с
  required/missing методами.
- **Где:** `compiler-codegen/src/{ast/mod.rs, parser/mod.rs,
  types/mod.rs}` — ~600 строк (AST 30 + parser 60 + type-checker 500).
- **Use-site эффект:**
    ```nova
    type Hash protocol { hash() -> u64 }
    type User { id u64, name str }
    export fn User @hash() -> u64 => @id

    fn dedup[T Hash](xs []T) -> []T => xs

    ro users = [User { id: 1 as u64, name: "a" }]
    ro _ = dedup(users)   // ← type-check OK, User satisfies Hash

    type NoHash { name str }
    ro xs = [NoHash { name: "x" }]
    ro _ = dedup(xs)
    // ← compile-error:
    //   type `NoHash` does not satisfy `Hash` bound
    //     `Hash` requires: hash() -> u64
    //     `NoHash` is missing: hash() -> u64
    //     fix: добавить недостающие методы...
    ```
- **Не покрывается:**
    + D53 разделение Protocol vs Effect в AST — все `protocol`/`effect`
      попадают в `TypeDeclKind::Effect`. BoundCtx permissively принимает
      любой Effect-kind как potential bound. Strict D53 compliance —
      отдельная задача.
    + Self в protocol-методах падает в codegen (vtable-emit issue) —
      pre-existing bug, тесты обходят протоколами без Self.
    + Method calls с bounds (`obj.method[T Hash]()`) — пропускаются.
    + Bound на ассоциированном типе — open question.
- **План:** docs/plans/15-generic-bounds-enforcement.md Ф.1/Ф.2/Ф.3 ✅;
  Ф.4 partial (positive tests); Ф.5 spec — pending.
---

### [ЗАКР 2026-05-09 codegen + 2026-05-10 interp Ф.6-bis] Plan 14 Ф.6: D69 variadic + spread

- **Что:** declaration `fn f(...items []T)` + call-site spread
  `f(...arr)` + mixed `f(a, ...arr, b)` работают **в обоих pipeline'ах**:
  C-codegen (production) и interp-mode (`nova-codegen test/run`).
  Покрывает D69 спецификацию полностью.
- **Где:**
    + `compiler-codegen/src/{ast/mod.rs, parser/mod.rs, codegen/emit_c.rs}`
      — ~500 строк (CallArg enum + variadic-routing в codegen).
    + `compiler-codegen/src/interp/{mod.rs, value.rs}` — ~170 строк
      (Closure.variadic_last + spread unfolding в eval_call +
      try_member_call_values).
- **Use-site эффект:**
    ```nova
    fn join(...parts []str) -> str { ... }
    join("a", "b", "c")          // → []str = ["a","b","c"]
    join(...arr)                  // → []str = arr
    join(...prefix, "tail")       // mixed
    ```
- **Verification history:** изначально (2026-05-09) считалось ✅
  закрытым по `run_tests.ps1` (codegen pipeline). 2026-05-10
  обнаружен gap: interp-mode даёт 7/7 FAIL для variadic.nv. Я
  проверял только один pipeline. Ф.6-bis закрыл interp.
- **Не покрывается:**
    + print/println — продолжают быть special-case (миграция на
      variadic — отдельная задача).
    + Multiple variadic-overloads — отвергаются (single-overload only).
- **Std refactor:** `std/path/path.nv` `Path.join(parts []str)` →
  `Path.join(...parts []str)` — теперь принимает variadic.
- **План:** docs/plans/14-stdlib-codegen-gaps.md Ф.6 ✅ + Ф.6-bis ✅.
- **Verification practice TODO:** добавить в CI / pre-commit hook'е
  параллельный прогон `nova-codegen test` для всех `nova_tests/**.nv`
  (interp baseline 43/92 — pre-existing bootstrap limits в
  concurrency/effects/runtime).
---

### [ЗАКР 2026-05-09] Plan 14 Ф.1: Option[T] full refactor (generalize Iter[T])

- **Что:** `Option[T]` в codegen теперь правильно типизирован для любого
  T (primitive / str / tuple / record-pointer / nested Option), вместо
  legacy `NovaOpt_nova_int` int-stomp'а. Изначальная узкая задача
  (Iter[T] generalization) расширилась до полного refactor'а Option-
  эмиссии — work-around на cast'ах не покрывал struct-typed payload
  (str/tuple).
- **Где:** `compiler-codegen/src/codegen/emit_c.rs` — ~250 строк по
  7 codegen-paths:
    + `type_ref_to_c(Option[T])` → `NovaOpt_<sanitized T>`;
    + lazy NovaOpt_<T> typedef'ы через marker+splice;
    + Some(v) / None через compound literal с реальным T;
    + ?-оператор typed early-return None;
    + pattern-match через temp `var_types` registrations;
    + emit_for использует typed `MethodSig.return_c_type`;
    + infer_expr_c_type Some/None → typed NovaOpt_<T>.
- **Use-site эффект:**
    ```nova
    // Iter[bool] — strict bool-check теперь работает на binding'е:
    fn BoolToggler mut @next() -> Option[bool] => Some(true)
    for b in toggler {
        if b { ... } else { ... }   // OK, b: bool, не nova_int
    }

    // Nested Option — typed pattern match:
    ro x = Some(Some(Some(42)))
    ro v = match x {
        Some(Some(Some(n))) => n      // n: int, не cast'ится
        _ => -1
    }
    ```
- **Не покрывается:**
    + Tuple типизация (`(int, str)` сейчас all-nova_int в
      `_NovaTupleN`) — отдельная задача.
    + Channel.recv остаётся NovaOpt_nova_int (runtime-erased generic).
    + Result[T, E] — аналогичный refactor отложен.
- **План:** docs/plans/14-stdlib-codegen-gaps.md Ф.1 ✅ (Option[T]
  full refactor).
---

### [ЗАКР 2026-05-09] Plan 14 Ф.7: int-литерал → char без try_from

- **Что:** `0x41 as char`, `65 as char` теперь работают на use-site без
  обёртки `char.try_from(n)?` (которая требует `Fail` в сигнатуре).
  Применимо к **compile-time-known IntLit** в валидном Unicode-
  диапазоне `U+0..=U+10FFFF` исключая surrogate range
  `U+D800..=U+DFFF`. Range-check выполняется статически в checker'е.
- **Где:** `compiler-codegen/src/codegen/emit_c.rs::check_as_cast_allowed`
  (~25 строк). Spec — `spec/decisions/03-syntax.md` D54 (абзац-
  исключение в существующем разделе, без нового D-номера).
- **Use-site эффект:**
    ```nova
    // Было (spec-strict, требует Fail-handling):
    fn ascii_a() -> char ! Fail { char.try_from(0x41)? }

    // Стало (Ф.7):
    fn ascii_a() -> char => 0x41 as char
    ```
- **Не покрывается:** `n as char` где `n` — переменная или арифметика
  (`('0' as int + n) as char`). Нужен либо Ф.7-bis (binary-pattern
  recognition), либо рефактор под `try_from(...)?`. Std/ файлы
  (uuid/ulid/base64/hex) сейчас используют именно arithmetic-pattern
  и Ф.7 их не разблокирует.
- **План:** docs/plans/14-stdlib-codegen-gaps.md Ф.7 ✅ (spec-only).
---

### [ЗАКР 2026-05-08] Plan 08 Ф.7: spec D54 расширение + conversions.md

- **Что:** spec D54 в `03-syntax.md` дополнен таблицей запрещённых
  `as`-cast'ов для char/byte/bool с suggestion'ами и прецедентами
  (Rust/Swift/Kotlin/Java сравнение). Создана сводная страница
  `spec/conversions.md` (~280 строк) — single source of truth для
  всех правил конверсии.
- **Где:**
  - `spec/decisions/03-syntax.md` D54 — раздел «Запрещённые `as`-cast'ы
    для char/byte/bool» + раздел «Strict `if cond: bool` / `while
    cond: bool`».
  - `spec/conversions.md` — новый файл. Структура: 3 механизма (as /
    from / try_from), полная таблица всех типов конверсий
    (numeric ↔ numeric, numeric ↔ str, char/byte/[]byte/str, bool,
    newtype, sum-discriminant), запрещённые конверсии, auto-derive
    4-way, прецеденты по 6 языкам, bootstrap status.
- **Bootstrap-status таблица:** Plan 05/07/08 Ф.1-Ф.5 ✅, Ф.6 ❌
  (отложено — требует полного type-checker'а), транзитивный
  auto-derive consciously не делается.
- **Не сделано:**
  - Ф.6 (generic-bound `[T Into[X]]` enforcement) — требует
    полноценного type-checker'а; отложен до фазы рефакторинга.
  - Compile-error suggestions с file:line:col — TBD при добавлении
    diagnostic'ов.
- **План:** docs/plans/08-from-into-conversions.md (Ф.7 закрыт; план
  на 5 из 6 фаз готов, Ф.6 отложен).
---

### [ЗАКР 2026-05-08] Plan 06 Ф.2: tuple-destructuring в for-in

- **Что:** `for (k, v) in iter { ... }` для итераторов возвращающих
  tuple-pairs. Раньше `pattern_binding` падал на Pattern::Tuple
  ("complex pattern in let binding not yet supported").
- **Где:**
  - `compiler-codegen/src/codegen/emit_c.rs::pattern_binding`:
    Pattern::Tuple теперь возвращает fresh_tmp; caller (emit_for)
    делает destructure отдельно.
  - Новый helper `pattern_destructure_tuple(pat, scr_tmp, scr_is_pointer)`:
    эмитит локальные биндинги через `tmp.f0`/`tmp.f1`/etc для каждого
    field tuple'а.
  - `emit_for` Case 3 (Iter[T]): для tuple-pattern эмитит
    `_NovaTupleN tmp = *(_NovaTupleN*)(intptr_t)opt.value`, затем
    destructure.
- **Bootstrap-ограничения:**
  - Все tuple-fields эмитятся как `nova_int` (bootstrap-convention
    для tuple-payload через nova_int slot). Для пар `(str, int)` или
    `(int, Custom)` нужен полный element-type infer (отложено).
  - Wildcard `_` поддержан — emit `(void)(field);` без binding.
  - Nested patterns (`for ((a, b), c) in ...`) пока не поддержаны.
- **Тесты:** `nova_tests/syntax/for_iter_tuple.nv` — 2 теста:
  basic destructure `(i, v)` через EnumeratedCounter, wildcard
  destructure `(_, v)`. 72/72 nova_tests PASS, без регрессий.
- **План:** docs/plans/06-iter-protocol-codegen.md (Ф.2 закрыт; Ф.3
  implicit `.iter()` остаётся).
---

### [ЗАКР 2026-05-08] Plan 06 Ф.3: implicit `.iter()` для коллекций

- **Что:** `for x in coll` где `coll` имеет `mut @iter() -> IterT` —
  codegen автоматически вставляет `.iter()` и итерируется по
  результату. По D58: «`for x in collection` вызывает
  `collection.iter().next()` в цикле».
- **Где:**
  - `compiler-codegen/src/codegen/emit_c.rs::emit_for` Case 4: после
    Range/Array/Iter[T] fallback'ов проверяем
    `all_methods.contains((iter_struct, "iter"))`. Если есть —
    synthesize'им `iter.iter()` Member-call и рекурсивно дёргаем
    emit_for. Recursion безопасна: ret-type `coll.iter()` имеет
    `next` через iter_returns registry → Case 3 cрабатывает.
  - Новый registry `iter_returns: HashMap<TypeName, IterTypeName>` —
    заполняется при AST-walk fn-items для `mut @iter() -> IterT`.
  - `infer_expr_c_type` для Member-call `coll.iter()`: lookup в
    `iter_returns` → возвращает `Nova_<IterT>*`.
- **Тесты:** `nova_tests/syntax/for_iter_implicit.nv` — 2 теста:
  basic `for x in coll` (implicit .iter()), legacy форма
  `for x in coll.iter()` (explicit). 73/73 nova_tests PASS,
  без регрессий.
- **План:** docs/plans/06-iter-protocol-codegen.md (Ф.3 закрыт;
  все 3 фазы плана 06 закрыты).
---

### [ЗАКР 2026-05-08] Plan 04 Этап 1: spec changes (D82 external, D30 full-words, D26 prelude split)

- **Что:** Спека для Plan 04 (split Buffer на StringBuilder /
  WriteBuffer / ReadBuffer + новый keyword `external`):
  - **D82** (новый блок в `spec/decisions/08-runtime.md`) — `external fn`
    keyword: модификатор для функций с runtime-implementation в `nova_rt/*.h`.
    Применяется только к функциям, не к типам. Body должен отсутствовать.
    Порядок modifiers: `export external fn`. Whitelisted namespace —
    `std.runtime.*` (программистский Nova-код не пишет).
  - **D30** расширен (`spec/decisions/03-syntax.md`) — раздел «Полные
    слова, не сокращения»: правило, mainstream-исключения (`len`/`iter`/`idx`),
    запрет ad-hoc сокращений (`pos`/`cap`/`dest`/`buf`/`val`/`cnt`/`tmp`/...).
  - **D26** расширен (`spec/decisions/08-runtime.md`) — добавлены
    StringBuilder/WriteBuffer/ReadBuffer как built-in opaque-типы
    (рядом с примитивами); ReadBufferError sum-тип; таблица verb/finalize.
- **Open-questions:**
  - **Q-buffer** помечена `⚠️ REPLACED (2026-05-08)` — split на три
    Q-блока. История сохранена.
  - **Q-string-builder** (новый, ✅ closed) — UTF-8 string accumulator,
    `@into() -> str` infallible, append-only.
  - **Q-write-buffer** (новый, ✅ closed) — binary serialization, 18
    числовых × LE/BE, `@into() -> []byte`.
  - **Q-read-buffer** (новый, ✅ closed) — cursor-style reader, pair
    `@read_*`/`@try_read_*` с auto-derive на C-runtime уровне.
- **Stub:** `std/runtime/builtins.nv` — documentation-stub с
  external-декларациями всех методов трёх типов + `str.from(c char)`.
  Сейчас это **только документация**: bootstrap codegen ещё не парсит
  `external` keyword (Plan 04 Этап 2).
- **Что осталось:** Этапы 2 (codegen), 3 (runtime), 4 (тесты), 5
  (финализация). Итого ~7-9 часов работы.
- **План:** docs/plans/04-buffer-split-and-external.md (Этап 1
  закрыт).
---

### [ЗАКР 2026-05-08] Plan 04 Этап 2-5: codegen + runtime + tests

- **Что:** Полная реализация Plan 04 (split Buffer на StringBuilder/
  WriteBuffer/ReadBuffer + новый keyword `external`):
  - **Lexer/Parser:** новый `KwExternal` token; `external` modifier
    парсится между `export` и `fn`. Body для external fn должен
    отсутствовать (compile error «cannot have a body» если есть).
  - **AST:** `FnDecl.is_external: bool` flag; `FnBody::External`
    вариант (для функций без тела).
  - **Codegen:** external fn skip'аются в `emit_fn` и
    `emit_fn_forward_decl` — никакого Nova body не эмитится.
    Dispatch для built-in opaque-типов (StringBuilder/WriteBuffer/
    ReadBuffer) — special-case в emit_call (по аналогии с Buffer/
    Channel pattern).
  - **Overload по типу аргумента:** `StringBuilder.from(s)` vs
    `StringBuilder.from(c)` — разные C-funcs (`Nova_StringBuilder_
    static_from_str` / `Nova_StringBuilder_static_from_char`).
    `types::mod` разрешает duplicate top-level names для
    external-fn (single-key registry — last-wins, dispatch
    делается в codegen).
  - **type_ref_to_c:** `StringBuilder`/`WriteBuffer`/`ReadBuffer` —
    fallback на `Nova_<Name>*` (как обычные record-types).
  - **infer_expr_c_type:** возвращает правильные types для всех
    методов трёх типов.
- **Runtime:** Три новых header'а в `nova_rt/`:
  - **`string_builder.h`** — UTF-8 string accumulator. Метод
    `_nova_utf8_encode` для char→bytes (1-4 байта). `Nova_str_
    static_from_char(cp)` для D73 char.into() → str.
  - **`write_buffer.h`** — binary serialization. 18 numeric × LE/BE
    через макросы `NOVA_WB_WRITE_LE/BE_16/32/64`. f32/f64 через
    IEEE 754 bit-cast.
  - **`read_buffer.h`** — cursor-style reader. **Auto-derive
    pattern** (Plan 04 ключевая фича): одна `_nova_rb_read_uN_LE/BE_raw`
    функция → две Nova-обёртки (`@read_*` Fail-form через
    `_nova_read_buffer_throw_unexpected_end`; `@try_read_*`
    Result-form через `_nova_rb_make_err`). Минимизирует C-код в 2x.
- **Тесты (41 новых):**
  - `nova_tests/runtime/string_builder.nv` — 15 тестов (создание,
    append str/char, UTF-8 multi-byte, capacity grow, hot-loop 100 raz,
    clone, into).
  - `nova_tests/runtime/write_buffer.nv` — 14 тестов (создание,
    write_byte/u32_le/be/u64_le/be/u16/i32 с проверкой byte order,
    auto-grow, clone, into).
  - `nova_tests/runtime/read_buffer.nv` — 12 тестов (cursor
    metadata, read_byte advances position, write/read round-trip
    LE/BE, try_read Ok/Err, multi-value sequence, read_bytes,
    remaining_bytes).
- **Bootstrap-ограничения:**
  - **`ReadBufferError` через nova_str.** Bootstrap-codegen Result
    зашит на `(nova_int Ok, nova_str Err)` (D26). Поэтому Err-payload
    — strings вида `"ReadBuffer.UnexpectedEnd: wanted N, available M"`.
    Когда fail-frame mechanism будет расширен на `void*` payload
    (по аналогии с RuntimeError plan), wrappers обновятся для
    структурированного `Nova_ReadBufferError*`.
  - **f32/f64 в Result через bit-cast.** `try_read_f64_le()` упаковывает
    `nova_f64` как `int64` через `_nova_f64_to_bits` (memcpy double→
    uint64). Вызывающий должен распаковать обратно через bit-cast
    (TBD: добавить helper `f64.from_bits(int)` в codegen).
- **Регрессии:** все существующие тесты проходят (buffer.nv 15/15,
  channels 10/10, auto_derive 6/6, from_into_basic 26/26, etc).
- **`std/runtime/builtins.nv`:** теперь parses & codegens'ится
  (пустой output т.к. все external). Live!
- **План:** docs/plans/04-buffer-split-and-external.md — все этапы
  закрыты. Ключевая learn: **dispatch table pattern** через
  receiver-type check + name-match достаточен для opaque built-in
  типов; полный overload-by-arg-type (Q-overloading) пока только
  whitelisted для external — этого хватает для StringBuilder/
  WriteBuffer/ReadBuffer.
---

### [ЗАКР 2026-05-08] Plan 04 follow-ups: whitelist enforcement + f64.from_bits + macro UB fix

Три follow-up задачи которые остались open после Plan 04 закрытия:

1. **Whitelist `std.runtime.*` enforcement** (D82). `types::check_module`
   проверяет module.name начинается с `["std", "runtime"]`; если нет
   и встретил `external fn` — error с понятным сообщением: «`external fn`
   is only allowed in `std.runtime.*` modules; for FFI use future
   `extern("C")` (Q-ffi)». Ручной negative-test подтвердил: compile
   error даётся; в `std.runtime.*` всё работает.

2. **`f64.from_bits(int)` / `int.to_bits(f64)` helper pair** для
   распаковки `try_read_f64_*` Result-payload. Codegen dispatch для
   обоих (Path-form и Member-form), infer возвращает правильные types.
   C-helpers — `nova_f64_from_bits` / `nova_int_from_f64_bits` в
   `nova_rt/cast.h` через memcpy bit-cast. 3 новых теста добавлены в
   read_buffer.nv (теперь 15/15 PASS).

3. **Bugfix: UB shadowing в WriteBuffer macros.** Macros
   `NOVA_WB_WRITE_LE/BE_16/32/64` объявляли локальную `uint16_t u =
   (uint16_t)(v)`. Когда вызывались из `write_f32_le/f64_le` — outer
   scope тоже имел `uint32_t u`/`uint64_t u`. Declarator в C вводит
   имя в scope **до** инициализатора, поэтому `(uint16_t)(u)` в macro
   читал неинициализированную shadow'ed variable (UB). MSVC давал
   мусор для f32/f64. **Fix:** переименовали macro-internal `u → _nova_u`.
   Round-trip f64 заработал.

   Урок: macros **обязаны** использовать имена с префиксом (`_nova_*`)
   для всех internal variables — иначе риск shadowing с outer scope.

**Тесты:** read_buffer.nv добавлены 3 теста (f64 round-trip Fail-form,
f64.from_bits + try_read_f64_be Result-form, int.to_bits round-trip
pair). 15/15 PASS. Все остальные buffer-тесты регрессий не имеют
(write_buffer 14/14, string_builder 15/15, buffer 15/15).
---

### [ЗАКР 2026-05-08] Plan 11 Ф.4.5 + Ф.1-Ф.3 + Ф.6: Self in expr + ad-hoc overload + spec

Закрыты три фазы Plan'а 11 (method values + overload). Не закрыты:
**Ф.4** (method values как first-class), **Ф.5** (disambiguation через
`as fn(...)`) — отложены на следующую сессию.

#### Что сделано

1. **Ф.4.5 — D66 Self в expression position** (~50 строк codegen):
   - `Self.method(args)` в теле метода: rebind `Path[Self, ...]` →
     `Path[<current_receiver>, ...]` в начале эмиссии.
   - `Self { fields }` literal — уже работало через
     `current_receiver_type` resolution (D66 type-position).
   - `Self.method(args)` через Member-form (`obj=Ident("Self")`) —
     rebind на Ident(<current>).
   - infer тоже резолвит Self → current.
   - 4 теста в `nova_tests/syntax/self_in_expr.nv`: default →
     parameterized constructor, Self literal, Builder chain, args.

2. **Ф.1 — Multi-overload registry** (~100 строк codegen):
   - Новый `MethodSig` struct: `param_c_types`, `return_c_type`,
     `is_instance`, `is_external`, `c_name`.
   - `method_overloads: HashMap<(type, name), Vec<MethodSig>>`
     рядом с старым single-key `method_receivers`.
   - Регистрация в AST-walk: для каждого fn-item с receiver'ом
     добавляется sig в Vec по ключу (type, name).
   - Backward compat: первая overload использует короткое C-имя
     (`Nova_T_method_m`); ≥2 — с param-types suffix.

3. **Ф.2 — Overload resolution на call-site**:
   - emit_call: для Member-form (`obj.method(args)`) и Path-form
     (`T.method(args)`) — strict matching по типам args.
   - infer_expr_c_type: ranne находит overload через ту же multi-key
     лютбук → return_c_type правильный (раньше был last-wins).
   - 0 matches при ≥2 candidates → fallback на legacy single-key
     путь.

4. **Ф.3 — C-name mangling**:
   - `Nova_T_method_m` (1st overload) → `Nova_T_method_m__nova_str`
     (2nd, str-version) → `Nova_T_method_m__nova_int_p` (3rd, int*).
   - Pointer `*` → `_p`, `[` → `_arr_`, `]` → ``. Sanitized для
     C-identifier.

5. **Ф.6 — Spec update**:
   - D35 расширен разделом «Перегрузка методов» — strict matching,
     mangling, bootstrap-status, Self в expr position.
   - Q-overloading помечен ⚠️ PARTIALLY CLOSED — variant 1 (ad-hoc)
     закрыт для методов; free-functions overload остаётся запрещён;
     variant 4 (protocol-based) — Plan 12.
   - `types::check_module` разрешает duplicate top-level name для
     методов с receiver'ом (overload), но не для free functions.

#### Тесты

- `nova_tests/syntax/self_in_expr.nv` — 4 теста (Self в expr).
- `nova_tests/syntax/overload.nv` — 3 теста: static `from(int|str)`,
  instance `@add(int|str)`, одноимённые `make()` на разных типах.
- **169/169 PASS** на full regression (15 файлов: buffer/auto_derive/
  from_into_basic/result_methods/unwrap_or/error_runtime_error/
  channels/read_buffer/write_buffer/string_builder + 5 syntax).

#### Что отложено (Plan 11 Ф.4)

**Method values как first-class** — `let f = acc.balance` сохраняет
bound method (pointer + self). Требует:
- Runtime struct `BoundMethod_T_m { fn_ptr, self }` с GC integration
  (self должен outlive bound value).
- Codegen для unbound (`Account.@balance`) и static (`Account.new`)
  как plain function pointers.
- Адаптер для передачи в higher-order функции (`nums.map(int.@to_str)`).

Не делается в этой сессии — отдельный план (~150 строк codegen +
runtime). Plan 11 Ф.4.

#### Урок

**Multi-overload registry рядом с single-key** — миграционный паттерн.
Вместо ломки старого single-key `method_receivers` мы добавили
`method_overloads` и сделали путь fallback'ом. Backward compat: все
существующие 169 тестов проходят без изменений; новый функционал
работает через новый путь. Это **safe-rollout pattern**: новая
инфраструктура поверх старой, миграция отдельная задача.
---

### [ЗАКР 2026-05-08] Plan 11 Ф.9: D39 anonymous embed `use _ Type`

Реализация D39 в bootstrap-codegen с anonymous embed (`use _ Type`)
+ override-precedence (Own > Delegated).

- **Ф.9.1 Parser:** `use name Type` (named) и `use _ Type` (anon).
  Anonymous имя поля — синтезированное `__embed_<TypeName>`.
- **Ф.9.2 AST + MethodSig.is_delegated:** RecordField.is_embed +
  embed_anonymous, MethodSig.is_delegated, embed_fields registry.
- **Ф.9 Auto-proxy generation:** pre-pass регистрирует Delegated
  MethodSig для каждого Own-метода embedded-типа; emit_embed_proxies
  эмитит C-функцию-делегатор `Nova_Wrapper_method_X(self) →
  Nova_Embedded_method_X(self->field)`.
- **Ф.9.3 Override-precedence Own > Delegated:** в emit_call и infer
  paths, после strict-match — фильтр pool на Own (Delegated wins
  только если Own нет).
- **Ф.9.4 Multi-anonymous detection:** declaration-time error если
  ≥2 anonymous embeds одного типа.
- **Ф.9.5 Lint warning:** stderr-warning при detect Own-override на
  Delegated в anonymous embed (невозможен `@<base>.method()` call).

Тесты: anonymous_embed.nv (3 теста) — named auto-proxy, anonymous
auto-proxy, override Own wins. **175/175 PASS** на full regression
(17 файлов).

Spec D39 обновлён: добавлена Bootstrap-status секция.

**Урок:** **auto-proxy через отдельный emit-pass + override-precedence
в shared dispatch path**. Delegated регистрируются в общем
`method_overloads`, эмиттинг C-кода — отдельный pass после Own
fn-emit'ов. Resolution унифицирован для Own/Delegated через
priority-фильтр. Тот же pattern что для overload в Ф.1-Ф.3 —
**common path с priority**.
---

### [ЗАКР 2026-05-08] Plan 11 Ф.7: расширение тестов

Plan 11 Ф.7 фиксировал тестовые наборы для overload, self_in_expr,
anonymous_embed. Изначально были минимальные (3+4+3 = 10 тестов).
Расширены до полноценного покрытия:

- **overload.nv: 3 → 9 тестов.** Добавлены: arity overload
  (`@log(msg)` vs `@log(level, msg)`), 3+ overloads на одном методе
  (int/str/bool), mixed static+instance одного имени, разные
  return-types по arg-type, multi-arg overload (по first arg type),
  no-arg vs N-arg arity overload.
- **self_in_expr.nv: 4 → 7 тестов.** Добавлены: Self.method из
  instance-метода (`@si_double` вызывает `Self.si_make`), Self в
  return + Self literal в body одновременно, nested Self.method
  calls (`Self.nst_one().depth + 1`).
- **anonymous_embed.nv: 3 → 9 тестов.** Добавлены: explicit base-call
  через `@<alias>.method()` в named embed, несколько auto-proxy
  методов от одного embed, auto-proxy с args, два named embed разных
  типов, anonymous embed coexists с extra fields, override через
  anonymous embed (Own wins, lint warning).

**Negative test verification:** multi-anonymous detection (`use _
Inner / use _ Inner` в одном record'е) даёт ожидаемый compile error
«multiple anonymous embeds of `Inner`». Проверено напрямую через
nova-codegen (без cl.exe — это compile-time check).

**Регрессия 17 файлов: 190/190 PASS** (было 175 после Ф.9, +15
новых тестов).

Урок: **расширение тестов раскрывает граничные случаи**. При
написании arity-overload (`@log(msg)` vs `@log(lvl, msg)`) проверил
что bootstrap правильно различает по `param_c_types.len()` — да.
При написании Self в instance-методе — проверил что Self резолвится
не только в Path-form но и в Member-form (`obj=Ident("Self")`) —
работало благодаря раннему rebind на 4276.
---

### [ЗАКР 2026-05-08] Plan 04 Этап 6: Buffer удалён из языка

Plan 04 закрыт полностью. Buffer удалён без backward compat
(Nova не в production, неудачное решение).

#### Что сделано

1. **Plan 11 multi-overload generic-boxing fix.** Регрессия
   stack_queue: новый Plan 11 multi-overload путь не делал
   void*-boxing для generic types (`Stack[T]`). Fix: добавлен
   `is_generic_recv = self.generic_types.contains(&rt)` check;
   args боксируются как nova_str* / void* / void* via intptr.
   `nova_tests/modules/stack_queue` снова PASS.

2. **WriteBuffer @write_char + @write_str** (Plan 04 Этап 6.1).
   `nova_rt/write_buffer.h`: использует `_nova_utf8_encode` из
   string_builder.h (1-4 byte). Codegen registry method_receivers
   обновлён. Smoke-tests в write_buffer.nv (4 новых теста).

3. **str.try_from([]byte)** (для финализации mixed text+binary).
   `Nova_str_static_try_from_bytes(arr)` в `nova_rt/string_builder.h`:
   валидирует UTF-8 через `_nova_validate_utf8`, на success
   `Result.Ok(boxed_str)`, иначе `Result.Err("invalid UTF-8...")`.
   Codegen Path-form dispatch для `str.try_from(bs)`.

4. **Buffer удалён из codegen** (Этап 6.3). 31 reference удалена:
   - record_schemas.insert("Buffer"...) и method_receivers
     (init блок).
   - obj_ty == "Nova_Buffer*" instance dispatch (5 методов).
   - Path-form `Buffer.method` (Member-form + Path-form).
   - infer paths для Nova_Buffer* и effect-schema Buffer.

5. **nova_rt/buffer.h удалён** (Этап 6.4). nova_rt.h `#include`
   убран.

6. **nova_tests/runtime/buffer.nv удалён** (Этап 6.5).
   `nova_tests/types/char_literals.nv` и `nova_tests/types/str_search.nv`
   мигрированы на StringBuilder и WriteBuffer соответственно.

7. **Q-buffer ❌ REMOVED** (Этап 6.6). Помечен как удалённый,
   с замечанием что компилятор Buffer не знает; используйте
   StringBuilder/WriteBuffer/ReadBuffer/WriteBuffer+str.try_from.

#### Регрессии

- nova_tests: **78/78 PASS** (было 79; -1 buffer.nv тоже удалён).
- stdlib: pre-existing failures в std/ (parser limitations,
  multi-line types, codegen ограничения) **не от моих изменений** —
  существующие issues от других sweeps.

#### Pre-existing url.nv issue

url.nv не компилируется на HEAD из-за tuple-destructure infer
ограничения (Plan 06 Ф.2: `let (sch, after) = ...` поля типизируются
как nova_int → `if after.starts_with(...)` падает на strict-bool
check). Это **не Plan 04 issue**. Decode_query реализован правильно
(WriteBuffer + str.try_from), но весь файл idle до tuple-destructure
infer fix.

#### Урок

**Multi-overload путь должен учитывать все аспекты dispatch'а** —
не только resolution, но и generic-boxing. Когда я делал Plan 11,
Stack[T] был bypass'ом legacy single-key path где boxing был.
Теперь Plan 11 покрывает все случаи. Уровень покрытия codegen-
dispatch'а можно проверить через regression-suite — stack_queue
поймал регрессию который иначе попал бы в production.
---

### [ЗАКР 2026-05-08] Plan 12: builtins.nv-driven external dispatch

`std/runtime/builtins.nv` теперь single source of truth для
StringBuilder/WriteBuffer/ReadBuffer. Codegen читает AST через
`ExternalRegistry` и применяет mangling автоматически. Hard-coded
match'и на ~150 строк удалены.

#### Что сделано

1. **Ф.1 ExternalRegistry** (~200 строк нового кода в
   `compiler-codegen/src/codegen/external_registry.rs`):
   - `include_str!("../../../std/runtime/builtins.nv")` — embedded
     в binary; парсится при `CEmitter::new()`.
   - Двухпроходный `from_module`: подсчёт overload'ов per ключ →
     генерация ExternalDecl с правильным mangling'ом.
   - Mangling: для overload'ов суффикс по Nova-type первого param
     (`_str`/`_char`/`_bytes`/...) — compatible с runtime naming.
   - `lookup(recv_ty, method)` → `Option<&[ExternalDecl]>`.

2. **Ф.2 record_schemas + method_receivers из registry**: hard-coded
   таблицы для StringBuilder/WriteBuffer/ReadBuffer удалены из
   init блока. Replace через iteration по
   `external_registry.receiver_types`. method_receivers использует
   `entry().or_insert()` чтобы НЕ перетирать prelude entries
   (Error.new etc.).

3. **Ф.3 emit_call dispatch через registry**: добавлены
   registry-driven path'и (Member-form instance, Member-form
   static, Path-form static) **до** hard-coded блоков. Strict
   match по arg-types + override Plan 11 multi-overload pattern.

4. **Ф.4 str.from(char) — skip-list**: `str.from` имеет hard-coded
   special-case путь для `int/bool/f64 → str` через
   `nova_int_to_str`/etc helpers (НЕ external fn). Registry
   skip'ает `str.from` чтобы старый hard-coded path работал.

5. **Ф.5 удалить hard-coded dispatch**: 3 блока × ~50 строк удалены:
   - StringBuilder/WriteBuffer/ReadBuffer Member-form instance
     (`obj_ty == "Nova_StringBuilder*"` etc.).
   - StringBuilder/WriteBuffer/ReadBuffer Member-form static
     (`name == "StringBuilder"` etc.).
   - StringBuilder/WriteBuffer/ReadBuffer Path-form static
     (`parts[0] == "StringBuilder"` etc.).
   - Runtime renames: `Nova_WriteBuffer_static_from_bytes` →
     `Nova_WriteBuffer_static_from`, `Nova_ReadBuffer_static_from_bytes`
     → `Nova_ReadBuffer_static_from` (consistent с registry naming
     для single-overload methods).

6. **Ф.7 Acceptance test**: добавлено `WriteBuffer @write_zero(n int)`:
   - `builtins.nv`: `export external fn WriteBuffer mut @write_zero(n int) -> ()`.
   - `nova_rt/write_buffer.h`: `Nova_WriteBuffer_method_write_zero` impl.
   - test в `nova_tests/runtime/write_buffer.nv`.
   **Без правки Rust-codegen'а** — registry парсит builtins.nv,
   mangling даёт правильное имя, dispatch находит. PASS.

7. **Ф.6 — отложен**. Type-checker gate для unknown methods на opaque
   types. Сейчас unknown даёт linker error (late stage); ideal —
   early-stage type error. Отдельный refactor `types/mod.rs`.

#### Регрессии

- 78/78 PASS на nova_tests.
- Регрессия в процессе: prelude.Error.new перетёрся registry-init →
  fix через `entry().or_insert()` чтобы не trample existing entries.

#### Урок

**`include_str!` для embedded source** — правильный паттерн для
"compile-time validated config". Альтернативы:
- Хардкод путя через CARGO_MANIFEST_DIR — fragile, зависит от FS.
- Build script — overengineering для одного файла.
- include_str! — atomic, валидируется на compile time, single binary.

**Двухпроходный mangling** — необходимо для overload'ов с suffix.
Single-pass не знает «всего» количества overload'ов на момент
обработки первой; нужен pre-pass count. Этот pattern переиспользуется
в любом mangling'е где decoration зависит от глобального состояния.

**Single source of truth pattern** масштабируется: добавить новый
opaque type → declare в builtins.nv + impl runtime → готово. Ни
codegen, ни method_receivers init не правятся. Это значит **ниже
порог входа** для расширения stdlib runtime.
---

### [MVP-CLOSED 2026-05-08] Plan 13: Runtime stdlib projection (str/math)

`std/runtime/*.nv` расширен с `builtins.nv` (StringBuilder/WriteBuffer/
ReadBuffer) на str/math API через **auto-generation** из
`runtime_registry.rs`. MVP: Ф.1-Ф.3, Ф.5-Ф.7 готовы. Ф.4 (полная
migration special-case dispatch'ей в emit_call → registry-driven)
отложен — риск регрессий в 78 тестах требует careful refactor.

#### Что сделано

1. **Ф.1 runtime_registry.rs** (~280 строк):
   - Struct `RuntimeFn`: module/receiver/params/return_ty/c_name/doc.
   - 17 str API entries (char_len, byte_len, find, slice, trim, ...).
   - 27 f64 math entries (sin/cos/sqrt/atan2/pow/hypot/is_nan/...).
   - `all()`/`group_by_module()`/`render_nv()` helpers.
   - Stable order (by module → by receiver → by name) для детерминизма.

2. **Ф.2 nova_rt/string.h + math.h umbrella headers**:
   - String функции уже в nova_rt.h; string.h re-includes для stable
     include-point.
   - Math wrappers ↦ libc <math.h>; math.h re-includes для stable point.
   - Future migration: фактические декларации могут переехать сюда без
     ломки user-кода.

3. **Ф.3 emit-runtime-stubs subcommand**:
   - `nova-codegen emit-runtime-stubs [--root <path>] [--check]`
   - Без `--check`: пишет `std/runtime/string.nv` + `math.nv` (44 funcs).
   - С `--check`: сравнивает existing с registry, fail если diff.
     **Используется в CI/pre-commit для предотвращения manual edits.**
   - Bonus: `nova-codegen dump-runtime` — sanity-print реестра.

4. **Ф.5 D26 + D74 spec update**:
   - D26 → раздел "Runtime stdlib проекция (Plan 13)" — explains что
     методы str/f64/f32 живут в std/runtime/*.nv (auto-gen).
   - D74 → cross-link на std/runtime/math.nv.
   - D82 Bootstrap status → расширен Plan 13 projection описанием.

5. **Ф.6 CI guard**:
   - `--check` режим в emit-runtime-stubs.
   - README.md compiler-codegen — раздел "Регенерация
     std/runtime/*.nv" с workflow.
   - Pre-commit hook integration — TBD (можно добавить как opt-in
     git hook позже).

6. **Ф.7 docs**:
   - README.md compiler-codegen обновлён.
   - docs/promts/regen-runtime.md уже существует от user'а.

#### Ф.4 deferred — почему

Полная migration `f64_method_to_c` / `str_method_to_rt` special-case'ов
в emit_call на registry-driven dispatch требует:
- Замена 2 больших match-таблиц (~50 строк каждая).
- Изменение dispatch path'ов для str/math инстанс-методов.
- Обработка edge cases: `str.from(int/bool/f64)` через nova_*_to_str
  (НЕ external fn), оставить hard-coded; runtime registry's `str.find/etc`
  через registry.
- Тщательный regression test 78 nova_tests на каждом шаге.

Попытка Ф.4 в этой сессии (через `merge_runtime_registry`) trigger'ила
регрессию в self_universal — `Nova_str_static_from` (single overload
без suffix) не существует в runtime (`Nova_str_static_from_char`).
Откатил merge, оставил Ф.1-Ф.3 infrastructure.

Следующая итерация Ф.4: **отдельная сессия** с careful step-by-step
+ runtime renames для consistency.

#### Тесты

- 78/78 PASS на nova_tests после Plan 13.
- Detrminism `emit-runtime-stubs --check` после регена → OK.
- Round-trip: dump-runtime print'ает 44 fn; nova-codegen check
  std/runtime/string.nv + math.nv → both PASS.

#### Урок

**Auto-gen separation of concerns**: registry (Rust) — driver,
.nv-файлы — projection. CI guard через `--check` — **lightweight
typesafety**: drift поймается при review даже если разработчик
случайно отредактировал .nv. Прецеденты: Cargo.lock vs Cargo.toml,
go generate, protoc-generated .pb.go — все используют этот pattern.

**Migration risk-management**: full Ф.4 был соблазнительным «всё
одним коммитом», но conservative split (Ф.1-Ф.3 + Ф.5-Ф.7
infrastructure → Ф.4 dispatch отдельно) даёт **safe-rollout**:
каждая фаза независимо проверяема, регрессии не накладываются.
---

### [ЗАКР 2026-05-08] Plan 13 Ф.8: декомпозиция builtins.nv + f32 math

После Ф.8 **в `std/runtime/` нет ни одного handwritten файла** — всё
auto-generated. Single source of truth pattern окончательно завершён
для opaque types и numeric/str API.

#### Что сделано

1. **Ф.8.1 string registry audit**: убран `is_empty` (нет в runtime),
   добавлен `eq` (есть в runtime, использовался через operator).
   Все 17 special-case'ов в emit_c.rs соответствуют registry.

2. **Ф.8.2 f32 math** (~25 entries):
   - C-имена через `f`-suffix (sqrtf, sinf, cosf, ...).
   - Predicates (isnan/isfinite/isinf) — type-generic C99 macros,
     те же имена.
   - Auto-generated в `std/runtime/math.nv` параллельно f64 секции.

3. **Ф.8.3 декомпозиция builtins.nv** (~70 entries):
   - `string_builder.nv`: StringBuilder API (new/with_capacity/
     from(s|c)/append(s|c)/len/capacity/clone/into).
   - `write_buffer.nv`: WriteBuffer API (write_byte/write_bytes/
     write_zero/write_char/write_str + 18 numeric × LE/BE +
     finalize).
   - `read_buffer.nv`: ReadBuffer API (cursor metadata + 20 read_*
     × Fail-form/try-form pairs = 40 entries).
   - `char.nv`: `str.from(c char)` UTF-8 encode.
   - Box::leak'ом для `'static str` runtime-вычисленных имён.

4. **Ф.8.4 regen + delete**:
   - 6 файлов сгенерированы.
   - `std/runtime/builtins.nv` удалён.
   - 78/78 PASS regression.

5. **Ф.8.5 Multi-file ExternalRegistry**:
   - `include_str!` для 4 файлов (string_builder/write_buffer/
     read_buffer/char) — все embedded в binary.
   - `merge_from_module` aggregator: каждый файл парсится → merge
     entries в общий registry.
   - string.nv/math.nv пока не loaded в codegen (Plan 13 Ф.4
     deferred — special-case dispatch остаётся для str/math).

6. **Spec D26/D82** — описания заменены на per-type файлы.
   Plan 13 раздел в D82 расширен — таблица 6 файлов + объяснение
   ExternalRegistry multi-file load.

7. **regen-runtime.md** prompt обновлён.

#### Total numbers

- Registry entries: **157** (было 44 — +113 от opaque types + f32).
- Auto-generated .nv файлов: **6** (было 2 — string + math; +4
  opaque + char).
- Handwritten .nv в `std/runtime/`: **0** (было 1 — builtins.nv).

#### Тесты

- 78/78 PASS на nova_tests.
- `nova-codegen check std/runtime/*.nv` — все 6 файлов parse'ятся.
- Detrминизм `regen_runtime.bat --check` → OK.

#### Урок

**Декомпозиция handwritten exception**: один handwritten файл рядом
с auto-generated — это «исключение из правила». Plan 13 Ф.8
устраняет его, делая единообразный single source of truth pattern.

**Multi-file include_str!** — паттерн для embedded sources где их
несколько. `include_str!` принимает literal path (не runtime),
поэтому каждый файл — отдельная константа. Загрузка через цикл
`for src in [SRC_A, SRC_B, ...]` aggregator'ом. Это даёт
extensibility без runtime FS dependency.

**Box::leak для 'static str из runtime-computed строк**: registry
содержит ~50 entries которые формируются программно (`format!`).
Для `&'static str` lifetime — leak'аем, один-time alloc. Альтернатива
(static lookup table) — тысячи строк boilerplate'а.
---

### [ЗАКР 2026-05-08] Plan 13 Ф.9.6: StringBuilder.@len bag-fix (codepoints)

- **Где:** `compiler-codegen/nova_rt/string_builder.h` +
  `compiler-codegen/src/codegen/runtime_registry.rs` +
  `compiler-codegen/src/codegen/emit_c.rs` (type-inference) +
  тесты в `nova_tests/runtime/string_builder.nv` + `types/char_literals.nv`.
- **Bag:** `Nova_StringBuilder_method_len` возвращал `b->len` —
  размер буфера в **байтах** (UTF-8). Но D26 школа B диктует:
  `@len` для текстовых типов = **codepoint count**. `nova_str.@len`
  через `nova_str_char_len` — codepoints, а StringBuilder — байты.
  Ассиметричность ловила пользователей: `StringBuilder.from('Я').len()`
  возвращало 2 (байт), хотя `"Я".len == 1` (codepoint).
- **Фикс:**
  1. `Nova_StringBuilder_method_len` — UTF-8 lead-byte walk (O(n)),
     совпадает с `nova_str_char_len`.
  2. Добавлен `Nova_StringBuilder_method_byte_len` (O(1) — `b->len`)
     для FFI / capacity-планирования.
  3. Registry doc обновлён.
  4. 14 тестов в `string_builder.nv` переписаны с двойным покрытием
     (для каждого теста проверяется `len()` и `byte_len()`).
- **Урок:** имя поля в struct (`b->len` — байты) и имя публичного
  метода (`@len` — codepoints) не должны быть 1:1 если spec
  диктует разную семантику. Field representation — internal,
  method API — public contract. Аудит таких mismatches —
  обязательная часть API review.
---

### [ЗАКР 2026-05-08] Plan 13 Ф.9.2: оператор `+` через `@plus` Nova-метод (D46)

- **Где:** `compiler-codegen/src/codegen/runtime_registry.rs` (RuntimeFn
  расширен полем `nova_body: Option<&str>` + renderer) +
  `compiler-codegen/src/codegen/emit_c.rs` (BinOp::Add routing) +
  std/runtime/{string,string_builder}.nv (regen) + новый
  `nova_tests/runtime/plus_operator.nv` (9 тестов).
- **Что было:** Bootstrap имел invisible-intrinsic для `str + str`
  (hardcoded `nova_str_concat` в emit_c.rs:3621). Программист не
  видел декларации `@plus` в registry/.nv → IDE / AI помощники
  не знали о существовании оператора.
- **Что стало:**
  1. `RuntimeFn.nova_body: Option<&'static str>` — `Some("@append(s)")`
     для записей с body, `None` для external. `c_name` игнорируется
     для записей с body.
  2. Renderer: `nova_body.is_some()` → `export fn ... -> T => {body}`
     (без `external`).
  3. Registry-записи:
     - `StringBuilder.@plus(s str) -> Self => @append(s)`
     - `StringBuilder.@plus(c char) -> Self => @append(c)`
     - `str.@plus(other str) -> str => @concat(other)`
     После regen `std/runtime/string_builder.nv` + `string.nv`
     содержат явные Nova-fn декларации `@plus` — программисту виден
     contract.
  4. Codegen `BinOp::Add` routing:
     - `Nova_StringBuilder*` + `nova_str` → `Nova_StringBuilder_method_append_str`.
     - `Nova_StringBuilder*` + `nova_int` (char) → `Nova_StringBuilder_method_append_char`.
     - `nova_str` + `nova_str` → `nova_str_concat` (теперь это C-имя
       метода `@concat` объявленного в registry — связь явная).
- **Bootstrap-ограничение:** routing для `BinOp::Add` сейчас hardcoded
  для встроенных типов (str, StringBuilder). User-defined `@plus`
  через `+` ещё не работает — нужен method_overloads lookup в codegen
  для BinOp::Add. Future task (отдельный план или Ф.9.7).
- **Тесты:** `nova_tests/runtime/plus_operator.nv` (9 тестов) —
  str+str (empty, ASCII, Unicode codepoint count + byte count),
  sb+str (sequential append, UTF-8 mixed), sb+char (single ASCII,
  multiple, 4-byte codepoint), смешанно sb+str/sb+char.
- **Урок:**
  - Nova-метод с body в registry — естественное расширение
    single-source-of-truth. `=> @append(s)` это не magic, обычный
    Nova syntax; программист видит делегацию в .nv-файле.
  - Auto-derive паттерны различимы по симметрии: D73 From↔Into
    (симметричное) остаётся, Plan 12 Ф.4.5 try_read auto-derive
    (асимметричное) отменён в Ф.9.5. Plan 13 Ф.9.2 — третий путь:
    body-as-data вместо synth-rule.
  - C-имя метода = invisible intrinsic не хуже visible declaration в
    registry. Inline emit того же C-вызова сохранён для performance,
    но связь через registry делает API discoverable.

---

## Секция 2 — Хроники и диагнозы (исторически записаны сюда, упрощениями не являлись)

Диагнозы багов, хроники внедрений/фиксов, отчёты закрытий, war-story — не
описывали действующее осознанное упрощение, а фиксировали расследование или
факт проделанной работы. Открытые хвосты из этих записей (если были)
продублированы маркером в [`docs/plans/backlog-followups.md`](../plans/backlog-followups.md).

[2026-07-16 [M-embed-dir] — ЗАКРЫТ (Plan 210 реализован: embed_dir("dir") компайл-тайм интринсик), ✅ ЗАКРЫТО, ветка p210-embed-dir] Владелец: реализуй Plan 210 целиком (owner-go 2026-07-16 на Ф.0). `embed_dir("dir")` — компайл-тайм интринсик: вшивает ВСЮ папку (рекурсивно) в бинарь → иммутабельный `EmbeddedDir` (`get`/`paths`/`len`/`has`/`entries`), Go `//go:embed`-эталон. **Ф.1 std-тип:** `std/src/prelude/embed.nv` — `EmbeddedEntry{path,data}` + `EmbeddedDir{priv entries}` + `EmbeddedDir.new(entries)` (O(N) verify-sorted-unique, panic на нарушении, защитная копия входа) + `@len`/`@paths`/`@has`/`@entries()->ro []EmbeddedEntry`/`@get`(бинарный поиск); re-export в `std/prelude.nv`, `PRELUDE_VERSION` 17→18; `embed_test.nv` (7 тестов, включая 2× `panics "sorted"`). **Ф.0 спека:** D412-амендмент дописан в конец `spec/decisions/03-syntax.md` (форма/контракт/детерминизм/коды/предупреждения/CRLF-заметка/Option E future). **Ф.2 резолвер:** `compiler-codegen/src/embed_resolve.rs` — `try_replace_embed_dir` (зеркало `try_replace_embed`) синтезирует `Call{EmbeddedDir.new([RecordLit{EmbeddedEntry,path,data}, …])}` из рекурсивного обхода папки (dot-skip кроме явно названного корня, symlink-skip+warn, non-ASCII+warn, POSIX-байтовая сортировка) — **НОЛЬ правок emit_c.rs/types/number_exprs**, синтезированные узлы (`HexBlobLit` для `data`) идут через существующий zero-copy-конвейер D412 как есть (подтверждено спот-грепом `.c`: `nova_blob_view(nova_blob_<h>, N)`, без memcpy). Попутно найдено и пофикшено: `resolve_embeds` возвращал ГОЛЫЙ `Vec<PathBuf>` без канала для warning на success-пути — `W_EMBED_DIR_*` были бы физически недоносимы; сигнатура сменена на `(Vec<PathBuf>, Vec<LintWarning>)`, пофикшено во всех 4 call-сайтах (nova-cli check/build, compiler-codegen check/compile, test_runner). Добавлены `E_EMBED_IS_A_DIR` (симметрия `embed("папка")`) и `E_EMBED_PATH_BACKSLASH` (оба интринсика). **Ф.4 фикстуры:** pos (spec_tests.conformance CU, рекурсия+dot-skip+sorted+round-trip) + 6 neg (not_found/not_a_dir/escape/not_literal/embed_on_dir/backslash) + 2 standalone (W_EMBED_DIR_EMPTY на папке только с `.gitkeep`; W_EMBED_DIR_NON_ASCII_PATH на `café.txt`, `get()` находит по точному байтовому ключу) — все PASS. **Найден и пофикшен баг в собственной фикстуре:** пояснительный комментарий начинался ровно с текста `EXPECT_COMPILE_WARNING` (проза) → раннер (first-wins per marker-type) принял прозу за директиву раньше настоящей строкой ниже → `NEG-WRONG-WARN`; пофикшено перефразированием. **Верификация:** `nova check std` δ-нейтрально ОКОНЧАТЕЛЬНО подтверждено прямым сравнением полного прогона main (нетронутый, свой бинарь) vs nova-210: FAIL 21==21 байт-в-байт идентичный список (все — `neg/`-фикстуры, которые `nova check` не умеет трактовать как ожидаемо-падающие, плюс пре-existing `E_STR_NO_LEN` в `date.nv`, подтверждено отдельно), PASS 118→120 (+2 новых файла), WARN 151→153 (+2 тот же системный "unused import Vec"). Детерминизм: два прогона резолвера → embed_dir-related контент (blob-байты/entries-порядок/interned-строки) байт-в-байт идентичен; единственный diff — pre-existing generic-typedef-order nondeterminism (`[M-codegen-emission-nondeterminism]`, подтверждено на НЕТРОНУТОМ контрольном фикстуре). **Ф.3 (флагман, опционально) — ПРОПУЩЕНО:** `examples/flagship/aggregator` embed'ит один самодостаточный `index.html` — замена на `embed_dir` была бы недраматичной (папка из 1 файла) демонстрацией через живой продакшен-пример с деликатной concurrency-историей; риск/выгода несоразмерны для явно опциональной фазы при «хост нагружен». **`embed_dir(".")`/self-embed корня** (§9.2 ревью-3 owner-open-вопрос, НЕ в закрытой таблице кодов §4.3) — НЕ реализовано (не в объёме). В main НЕ мёржил (ветка `p210-embed-dir`, worktree `nova-210`) — язык-меняющее, гейт+merge делает оркестратор (мега-CU conformance + флагман-examples). Модель: sonnet.
---

[2026-07-15 [M-tls-xpkg-tlsversion-value-ptr-dispatch] — ЗАКРЫТ (cross-package sum-type `??`-локал: нераскрытый `_p`-маркер), ✅ ЗАКРЫТО, ветка fix-tlsversion-dispatch] Владелец: почини value/pointer ABI-mismatch cross-package sum-type-метода (`TlsVersion.@to_str()` из nova-tls, вызван из `examples/tls/echo_client.nv`). **Репро (из корня worktree, чтобы читать ЧИСТЫЙ std, а не dirty integ-206 в main-репе):** `nova build examples/tls/echo_client.nv` → C-compile error `unknown type name 'Nova_TlsVersion_p'` + `passing 'Nova_TlsVersion' (value) to parameter 'Nova_TlsVersion_p*'` — метод-def эмитился как `Nova_Nova_TlsVersion_p_method_to_str(Nova_TlsVersion_p* nova_self)`, `??`-локал как `Nova_TlsVersion_p version = …`. **Root cause (НЕ receiver-ABI-модель sum-type — одна точка type-инференса):** legacy-ветка `ExprKind::Coalesce` в `infer_expr_c_type` (`compiler-codegen/src/codegen/emit_c.rs` ~54063) стрипила `NovaOpt_`-префикс и возвращала payload-идентификатор `Nova_TlsVersion_p` ВЕРБАТИМ, не разворачивая sanitized-pointer-маркер `_p`→`Nova_TlsVersion*` (Coalesce-ЭМИССИЯ ~30988 УЖЕ звала `desanitize_c_from_ident` — рассинхрон инференс/эмиссия). Битый C-тип `??`-локала отравлял ВНИЗ receiver-мэнглинг метод-диспатча → on-demand эмиссия метода с несуществующим `Nova_TlsVersion_p*`. Локальные однотипные sum-type (`type Ver enum V12|V13` + `@name` + `Option[Ver] ?? Ver.V13`) НЕ задеты — резолвятся через Channel-2 (чекерский resolved-type → чистый `Nova_Ver*`); cross-package падал в legacy-ветку (канал промахивался на cross-package payload). **Fix (минимальный, 1 строка семантики):** `Self::desanitize_c_from_ident(sani)` вместо `.to_string()` — идемпотентно для value-payload (`nova_int`/`nova_str`/`NovaValue_…`/`NovaTuple_…` без `_p`-суффикса → byte-identical), разворачивает только heap-pointer payload. **Верификация (точечная, мега-CU за оркестратором):** `echo_client.nv` — был C-error, стал **linked**; сген. C чист (`Nova_TlsVersion_method_to_str(Nova_TlsVersion* nova_self)`, `Nova_TlsVersion* version = (… ? _tmp.value : …)`, `_p` только в легитимном мэнгле `NovaOpt_Nova_TlsVersion_p`); `echo_server.nv` не регрессировал (linked); локальный `Option[enum] ?? default`+метод собирается. **Замечание (вне периметра, не воспроизведено):** соседняя `Try/Bang`-ветка (~54152) несёт тот же нераскрытый `_p` для cross-package `Option[Sum]?`/`!!` — другой символ/путь, оставлено наблюдением (echo_client использует `??`, не `?`). Runtime/codegen-фикс, НЕ язык-меняющий → D-амендмент не нужен. В main НЕ мёржил (гейт+merge — оркестратор; emit_c.rs конкурентен с 206/209/D39). Модель: opus.
---

[2026-07-15 [M-187-supervised-nested-fiber-slot-race] — ЗАКРЫТ (yielded-FIFO black-hole в nested-supervised pump), ✅ ЗАКРЫТО, ветка p83-4-5-12-slot-race, Plan 83.4.5.12] Владелец: закрыть P1-блокер непрерывной работы флагман-сервера. **Репро (флагман aggregator, worktree nova-83race):** `nova build examples/flagship/aggregator/src/main.nv` + серия `curl` по эндпоинтам (`/api/events`/`/api/run` глубже всего — `aggregate()` parallel-for + `fetch_guarded`'s `supervised(deadline:)`). Стабильно виснет НАВСЕГДА на ~14-м последовательном запросе (иногда 2-7-м; «успешные» перед зависанием тянулись 4-8с вместо budget≈1.2с — late-unblock по дедлайну), после чего сервер полностью wedged (все последующие curl = timeout). Гонка вероятностная, но 100% детерминированно доходит до deadlock в пределах серии. **State-dump (по шаблону case-study — watchdog временно включён на worker-тредах + добавлен dump yielded_count):** `[w.0.fiber.s1] mco_status=3 (SUSPENDED) parked=0 pstate=0 hdl=0 stop_cb=0 ⚠ STUCK_ALIVE_NOT_PARKED` + `[w.0.yielded] count=1` — застрявший фибр приостановлен, НЕ запаркован, НЕ в runq/runnext/global, а сидит в yielded-FIFO worker 0. **Root cause:** `nova_runtime_worker_pump_scope` (`compiler-codegen/nova_rt/runtime.c`, вызывается из `nova_supervised_run_impl` когда вложенный `supervised` крутится на worker-треде и ждёт `pending_remote==0`) дренировал runnext + runq + global-overflow, но НЕ `_worker_yielded_pop`. При этом сам pump на шаге (4a) резюмит фибр инлайн через `_worker_run_one_fiber`, а тот при кооперативном вытеснении (Plan 44.7 sysmon-preemption / `runtime.yield`) пушит фибр в yielded-FIFO (`runtime.c:1980`). Пока worker застрял в nested-pump-цикле, он НЕ возвращается в `_worker_main` (`runtime.c:890`), где yielded-FIFO дренируется штатно → вытесненный дочерний фибр ТОГО ЖЕ pump'ящегося scope black-hole'ится → его `pending_remote` не декрементируется → цепочка supervised виснет навсегда. Тот же класс, что задокументированный global-overflow «black-hole» (`_worker_main` комментарий «MUST run as a consumer here, else overflow fibers strand forever → pending_remote never reaches 0 → deterministic supervised hang») — yielded-FIFO в pump-пути был пропущен. НЕ STALE-slot из case-study (тот fix present и корректен); отдельный дефект того же семейства «фибр жив, но застрял в очереди, которую текущий drain не обслуживает». **Fix (минимальный, `runtime.c`, +13 строк, только nova_runtime_worker_pump_scope):** добавлен `if (!co) co = _worker_yielded_pop(w);` между runq и global-overflow — порядок дренирования теперь зеркалит `_worker_main` ровно: runnext → runq → yielded → global. Non-matching (шаг 4b) yielded-фибр перекладывается в runq (тоже нормально: runq дренируется). Инварианты не тронуты: TLS-race (83.10.4), per-slot child_error (173.0), slot-lock/CAS-guard, atomic fiber-state — всё без изменений (правка чисто в выборе источника фибра). **Верификация (гонка вероятностная → несколько серий):** сервер выдержал 350 последовательных запросов через 5 серий (60+60+60+70+100, events-heavy) — 0 зависаний; макс латентность упала до ~1.7-4.3с (late-unblock-артефакт исчез — фибры завершаются сразу). До фикса: стабильный hang на #14. M:N spec_tests: `std/concurrency` — `supervisor_test` (вложенный supervised+spawn+policy), `supervised_deadline_test` (тот самый `supervised(deadline:)`), `rate_limiter_test` — все PASS; `retry_test` CC-FAIL — pre-existing несвязанный generic-mono type-error (`nova_str` vs `Nova_T*`) в сгенерированном `.c`, физически невозможен от 13-строчной runtime-правки (падает на C-компиляции до запуска рантайма). Полный conformance ОДНИМ CU — гейт оркестратора. Runtime-фикс (не язык-меняющий) → D-амендмент не нужен. Модель: opus.
---

[2026-07-15 [M-174.1-to-str-name-collision-codegen-bug] — ЗАКРЫТ (Plan 196.7, канал+receiver-тип, БЕЗ name-guard), ✅ ЗАКРЫТО, ветка p196-dispatch] Владелец: почини codegen-дефект name-collision method-dispatch ПРАВИЛЬНО — через канал `resolved_callees`, не ещё одним гардом; оформи подпланом 196.7 + сними обход `decode_utf8`. **Баг:** `[]u8 @to_str() -> Result[str, Utf8Error]` (фасад) в CU, где есть чужой same-name `to_str` — bare-T бланкет `fn[T] T @to_str() -> str` (D410-скаляр→строка) и/или пользовательский `T @to_str() -> str` (Display: NetError/Path/Url/SerError/IoError/SocketAddr) — мис-диспатчился: codegen эмитил `Nova_Nova_Vec____nova_byte_method_to_str(bytes)` (бланкет, возвращает `nova_str`) на месте `Nova_NovaArray_nova_byte_method_to_str` (Result) → `->tag` на `nova_str` → CC-FAIL. Тот же класс, что D164 (примитивный ресивер), но D164 чинил ГАРДОМ — здесь через канал. **Две первопричины:** (1) **checker** `check_instance_overload` (types/mod.rs) НЕ писал `resolved_callees`: array/slice-ресивер нормализуется в `"Vec"` (D239), но фасад лежит под element-написанием `method_table["[]u8"]` (`receiver.type_name`), невидимым для `method_table["Vec"]` → `methods.get("to_str")=None` → канал пуст → codegen перевыводит по имени; (2) **codegen** method-call резолвил по имени через protocol-aware blanket dispatch (Plan 164 Ф.3, emit_c.rs ~37374) / single-key `method_receivers` last-wins — оба игнорировали фасад под array-C-ident-ключом. **Fix (одно окно, receiver-truth, не name-last-wins, ~120 строк, БЕЗ правки frozen `infer_call_ret_c` 46293-48883):** *checker* — для array/slice/`Vec[E]`-ресивера, когда метод отсутствует в `method_table["Vec"]`, резолвим по element-написанию `[]E` (`render_type_ref`), single/unique-compatible (c1/c2) → пишем `resolved_callees[call_id]=span([]E-метода)`; GATED к методам, которых нет у "Vec" → байт-идентично для Vec-методов (D84 concrete-beats-generic). *codegen dispatch* (перед blanket ~37374) — конкретный фасад-callee двумя receiver-источниками: (A) канал `resolved_callees→FnDecl.span`, gated `fn_ret_by_span` (только конкретные callee; бланкет-span отсутствует ⇒ настоящий бланкет не трогается); (B) конкретный C-тип ресивера `Nova_Vec____<E>*`/`NovaArray_<E>*` + `[]E @<method>` под array-ключом — покрывает ресивер-формы, которых не достаёт static `infer_arg_ty` чекера (`@field`/expr/pattern-binding); GATED к наличию unconstrained bare-T бланкета (единственный сценарий перехвата) → байт-идентично иначе. *codegen return-type* (`infer_expr_c_type` «Channel 1b», ВНЕ frozen) — зеркало (B) для возврат-типа фасад-вызова на channel-less ресивере: `sig.return_c_type` (Result) вместо бланкет-`nova_str`. **Урок:** канал заполняется чекером ТОЛЬКО из `f1_check_call` (тело fn); тело `test { }` идёт через другой визитор → канал пуст → репро строится в РЕГУЛЯРНОЙ fn. Static `infer_arg_ty` чекера не достаёт field/pattern-ресивер → codegen-fallback (B) по C-типу ресивера как страховка. **Снят обход:** удалён `export fn []u8 @decode_utf8()` (runtime/string/core.nv) + экспорт из prelude.nv; мигрировано на `.to_str()` (11 call-сайтов: encoding/serde/json, encoding/url, fs/fs, fs/path×2, io/core×2, net/addr×2, net/error, net/tcp, runtime/string_builder); устаревшие комментарии-обходы вычищены. nova-tls (src/stream.nv×4) — отдельная репа, отдельный коммит. **Тесты:** `nova_tests/repro_to_str_collision/` (fresh type `@to_str` + `bytes.to_str()` в fn — до: CC-FAIL, после: PASS) + `spec_tests/conformance/to_str_facade_collision.nv` (позитив: []u8-фасад + user `FacadeErr.to_str` + bare-T `7.to_str()` в одном CU; local+pattern ресиверы; мега-CU-гейт за оркестратором). **Гейты (точечные — мега-CU за оркестратором):** repro PASS; `std/src/net` 1/0, `std/src/io` 1/0, `std/src/fs` 1/0, `std/src/encoding` 9/0+7skip, `std/src/runtime` 3/0+13skip — без НОВЫХ фейлов; байт-паритет: фиксы GATED к blanket-collision+Vec/array-facade → на прочем не срабатывают (подтверждено зелёными модулями с массой не-коллизионного кода). В main НЕ мёржил (оркестратор вливает после 206/209Ф.3/D39 — emit_c.rs конкурентен). Модель: opus.
---

[2026-07-13 [M-parfor-record-result-miscompile] — окончательно ЗАКРЫТ (loop-var non-scalar by-ref capture root cause), ✅ ЗАКРЫТО, ветка parfor-173-1] Владелец: срочный баг-фикс — «parallel for, собирающий record-результаты, мискомпилится». Расследование: Plan 173.1 Ф.2 (2026-07-09, `parallel-collect-173-1`) уже закрыл маркер ДЛЯ матрицы `nova_tests/err173_1/parfor_elem_matrix.nv` (примитив/heap-record/value-record/tuple/sum/nested-[]T — каждый элемент строился ВНУТРИ тела из int-loop-var через `Range`). Первые repro-попытки этой волны (fn-return-trailing, nested-supervised, call-arg-position — «aggregate()»-идиома из Plan 187) ПРОШЛИ чисто → казалось, маркер уже мёртв. Настоящий репро нашёлся только когда loop-var сам НЕ-скалярного типа (str) итерировался из МАССИВА (не Range) и передавался в тело НАПРЯМУЮ: `parallel for s in ["a","b","c"] { Report{source: s, ...} }` — ВСЕ собранные Report показывали ОДИН И ТОТ ЖЕ (последний) `source`; `parallel for s in ["a","b","c"] { s }` давал дубликаты/пропуски вместо `{a,b,c}`. 100% детерминированно (не флака). **Root cause:** `emit_spawn`/`emit_detach`/`emit_blocking` (`compiler-codegen/src/codegen/emit_c.rs`, 3 идентичных сайта capture-анализа) гейтили by-value capture условием `is_scalar && !is_mut`, где `is_scalar` — узкий whitelist `{nova_int,nova_bool,nova_f64,nova_f32,nova_byte}`. Loop-переменная for-loop'а переиспользует ОДИН и тот же C-stack-слот на каждой итерации; при `is_scalar=false` (str/heap-record-pointer/value-record/tuple/sum) capture шёл BY-POINTER (`ctx->cap = &s`), т.е. каждый spawned fiber получал адрес ОБЩЕГО слота — к моменту реального исполнения (fiber'ы шедулятся асинхронно, обычно ПОСЛЕ того как родительский `for` продвинулся дальше) все видели ОДНО и то же (последнее) значение. Комментарий кода уже ДОКУМЕНТИРОВАЛ этот риск («loop-variables... capturing by value snapshots them; by pointer would let all queued fibers see the last iteration's value») — но whitelist был неполным, реализация не покрывала собственное намерение. **Fix (компилятор, `emit_c.rs`, 3 сайта, ~40 строк):** `by_value = !is_mut` (убран `is_scalar`) — ЛЮБОЙ immutable capture (не только скаляр) идёт by-value. Обоснование безопасности: для non-loop immutable capture (объявлен один раз, не переприсваивается) by-value и by-pointer поведенчески ИДЕНТИЧНЫ (значение не меняется до конца scope в обоих случаях) — расширение чисто чинит loop-var-aliasing кейс, не меняя поведение где-либо ещё. Верифицировано против БАЗОВОГО (pre-fix) бинаря — идентичное поведение вне loop-var-repro кейсов. **Побочная находка (НЕ чинилась, отдельный маркер на будущее):** на масштабе ВСЕГО `spec_tests/conformance` (~950 тестов, один CU) анонимный-tuple под-тест (`(i, i*i)` из Range) детерминированно (100%, воспроизведено и на baseline-бинаре) даёт неверную сумму ПРИ ВЫБОРЕ ENTRY-файла с одним конкретным соседним .nv (`c_keyword_ident_mangling.nv`/директория), но ПРОХОДИТ при выборе другого entry-файла того же папки-модуля (folder=один модуль, ожидался byte-identical результат независимо от entry — не подтвердилось: разный `t-<hash>` build-id). Не связано с этим фиксом (та же картина на baseline); anon-tuple parallel-for УЖЕ покрыт `nova_tests/err173_1/parfor_elem_matrix.nv` на меньшем масштабе (проходит стабильно) — исключён из нового conformance-файла, чтобы не блокировать гейт неродственной проблемой. `[M-parfor-tuple-corpus-scale-order-sensitive]` — floating-маркер, требует отдельной state-dump-style инвестигации (кандидат tuple-mono-instance naming/counter collision при большом числе зарегистрированных generic-инстансов). **Тест:** `spec_tests/conformance/d414_parfor_record_collect.nv` — 7 pos-тестов: fn-return-trailing (без `ro`-биндинга), nested `supervised{}`, call-arg-position (все три — «aggregate()»-идиома Plan 187), + value-record/named-tuple/sum-type/nested-`[]T` матрица в канонической gate-локации (её раньше не было — `nova_tests/err173_1` не входит в `spec_tests/conformance` single-CU гейт). **Гейты:** cargo build (nova-cli, release) чисто; `spec_tests/conformance --positive --compile-error --jobs 4` 97/97, 3× стабильно; `nova_tests/err173,err173_1,err173_2,err173_3` все зелёные; `nova_tests/plan83_6,plan83_7,plan83_10_3` зелёные (`plan83_10_4` блокирован pre-existing несвязанным `[P67-LEGACY]` ICE — задокументирован в Plan 183/178 как известный до-172-rework гэп, не трогал); `std/concurrency` зелёный кроме `retry_test.nv` (pre-existing несвязанный generic-mono CC-FAIL — файл вообще не использует spawn/detach/blocking, подтверждено чтением исходника). Хэш: `770ab3367` (branch `parfor-173-1`, worktree `nova-p173`, база `77239c014`). Модель: sonnet.
---

[2026-07-10 [M-108-empty-frompairs-nonhashmap-kv-infer-gap] — ЗАКРЫТ (checker-фикс, дёшево), ✅ ЗАКРЫТО, ветка recordlit-callarg-fix] Владелец: «чини дёшево». Побочная находка волны d55-hashmap-fix: `extract_hashmap_kv` (compiler-codegen/src/types/mod.rs) был захардкожен на literal-имя типа `"HashMap"` — для generic user-типа с `#from_pairs`, но НЕ named `HashMap` (напр. `type Bag[K,V] #from_pairs`), K/V не выводились из expected в EMPTY/all-spread `[]`-map-lit ветке → `inferred_key`/`inferred_value` оставались `None` → desugar (`build_map_block`) шёл через fallback-callee `Bag.new` (`Path`, без K/V-мономорфизации) вместо `Bag[K,V].new` (TurboFish) → RUN-FAIL (runtime-краш собранного бинаря; НЕ compile-error). Непустой литерал `[1: "a"]` работал (unify берёт типы из literal-элементов, минуя `extract_hashmap_kv`). **Root cause:** дизайн-хардкод имени вместо декларативного атрибута — при том что рядом уже есть `expected_is_from_pairs`/`expected_is_from_fields` (проверяют `from_pairs_types`/`from_fields_types`-множества по `#`-атрибуту, не по строке). **Fix (дёшево, checker-слой, ~15 строк):** `extract_hashmap_kv(expected, is_kv_type: bool)` — kv-извлечение из `Named[K,V]` теперь гейтится флагом `is_kv_type`, который вызывающий вычисляет через `expected_is_from_pairs` (ветка MapLit) / `true` (ветка `#from_fields`, где мы УЖЕ внутри `expected_is_from_fields`-гарда). Literal-имя `"HashMap"` оставлено вторым дизъюнктом-fallback (HashMap несёт оба атрибута — байт-идентично для canonical-пути). `inferred_target_type` уже определялся по атрибуту (`expected_is_from_pairs`, не по имени) — не трогал. **Тест:** `spec_tests/conformance/d108_from_pairs_user_type.nv` расширен — empty-литерал `[]` для user `#from_pairs`-типа `D108Bag[K,V]` в трёх контекстах (let-с-типом / return-позиция / call-arg позиция), все три ранее давали RUN-FAIL, теперь PASS (`.len() == 0`). **Гейты:** cargo build (оба крейта) чисто; conformance `--positive --compile-error` 91/0 (тот же single-CU, новые empty-тесты внутри — PASS подтверждает компиляцию И рантайм); err173-корпус δ0. Модель: sonnet.
---

[2026-07-10 [M-d55-anon-recordlit-codegen-gap] — plain-record call-arg подкласс ЗАКРЫТ (codegen-фикс), ✅ ЗАКРЫТО, ветка recordlit-callarg-fix] Владелец: «чини сейчас» (был отложен предыдущей волной как «нужен отдельный codegen-трек»). Репро (подтверждено заново): `type Point { x int, y int }` + `fn takes(p Point) -> int => p.x + p.y` + `takes({x:1,y:2})` → `codegen error: anonymous record literal without spread not supported in codegen`. **Root cause:** `compiler-codegen/src/codegen/emit_c.rs::emit_record_lit` знает ДВА источника expected-типа для голого (`type_name: None`) anon-RecordLit — `current_fn_return_ty` (return-position) и разовые save/set/restore-скоупы `expected_record_type` (let-с-типом / вложенное поле record-литерала / Some-Ok-Err payload / sum-payload, каждый точечно у своего вызывающего места) — ни один НЕ покрывает call-arg позицию вообще; чекер-канал (`resolved_types_buf`) эмиттер тоже не читает (диагноз 172.13 подтверждён дословно). **Почему НЕ checker-канал (в отличие от HashMap-подкласса выше):** для HashMap годился `inferred_map_v`-паттерн, потому что desugar ПЕРЕПИСЫВАЕТ узел в другой AST ДО codegen. Для plain-record эквивалентный «правильный» codegen-путь (`type_name: Some(...)`) уже существует и полнофункционален (generic-mono/sum-variant/sret/value-record ветки — не переизобретать); не хватало только ЗАПОЛНИТЬ `type_name` для голого литерала в call-arg позиции. Пробовать ещё один разовый `expected_record_type`-скоуп было бы ХУЖЕ: он не покрывает ни generic-mono, ни sum-variant-lookup, ни sret-путь (см. чуть более бедную ветку на `expected_record_type` в `emit_record_lit`) — пришлось бы дублировать логику. **Fix (компилятор, `emit_c.rs`, ~90 строк, чисто codegen call-site):** `emit_call` (Р10/172.14 методология — уже дважды использует паттерн «синтезируй переписанный `args`-список ДО дальнейшего диспатча», см. `synthesize_inout_refargs`/`synthesize_method_byref_at_callsite`) получил третий wrap — `synthesize_record_lit_typed_call_args`: резолвит callee по `user_fn_sigs` (ЕДИНСТВЕННЫЙ источник, где параметр-тип известен ДО мономорфизации по call-site — намеренно ТОЛЬКО non-generic free fn, см. регистрацию `f.receiver.is_none() && f.generics.is_empty()` несколькими сотнями строк выше; методы/generic fn — вне периметра этого фикса, их сигнатура известна только пост-мономорфизации), и для каждого позиционного (`CallArg::Item`) аргумента — голого anon-RecordLit (`type_name: None`, БЕЗ spread-полей, `inferred_map_v: None` — HashMap/`#from_pairs`-манифестация уже десугарена в другой узел ДО этой точки) с параметром на этой позиции, резолвящимся (`debt_struct_name_from_c_type`) в известный `record_schemas`-тип — переписывает узел так, будто пользователь явно написал `Point { x: 1, y: 2 }`. Идемпотентно с двумя существующими wrap'ами; не трогает typed/spread/HashMap-литералы, named/spread call-args, методы, generic fn — их fallback-ошибка остаётся safety-net'ом. **Тест:** `spec_tests/conformance/d55_recordlit_callarg.nv` — 4 pos-теста (call-arg 1-я позиция, call-arg 2-я позиция среди двух параметров, регресс: уже-типизированный литерал в call-arg, регресс: spread-литерал в call-arg). **Гейты:** cargo build (compiler-codegen + nova-cli) чисто; conformance `--positive --compile-error` 91/0 (baseline тот же single-CU агрегация, новый файл внутри неё — PASS подтверждает компиляцию И рантайм, `assert`); err173-корпус 5/5 δ0 (индивидуально, известная параллельная флака не проявилась). Модель: sonnet.
---

[2026-07-10 [M-d55-anon-recordlit-codegen-gap] — HashMap #from_fields/#from_pairs подкласс ЗАКРЫТ (checker-фикс, дёшево); plain-record call-arg подкласс остаётся ОТКРЫТ (codegen-эмиссия, отдельный трек); +conformance-покрытие +новый floating-маркер] Задача владельца: почини маркер дёшево + проверь покрытие `#from_fields`/`#from_pairs` тестами. Репро (ветка `d55-hashmap-fix`): 4 контекста анонимного record-литерала `{field: v}` в позиции `HashMap[str,V]` (`#from_fields`, std/collections/hashmap.nv:89) реально падали/мисэмитились: (1) `Stmt::Assign` — переприсваивание существующей `mut`-переменной (`m = {a: 1}`); (2) вложенное поле именованного record-литерала (`Outer { field: {...} }`) — эмитилось НЕВЕРНО (кодоген пытался построить `Nova_HashMap` как обычный record → `no member named 'a'`); (3) элемент `ArrayLit` (`[{...}, {...}]` при `-> []HashMap[str,V]`); (4) аргумент built-in конструкторов `Some`/`Ok`/`Err` (`Some({x: 10})` при `-> Option[HashMap[str,V]]`). Корень — НЕ codegen-эмиссия (`emit_c.rs`), а checker-annotator (`MapLitAnnotator`/`MapLitCtx`, `compiler-codegen/src/types/mod.rs`): все 4 места спуска AST вели `expected`-тип безусловным `None`, поэтому `inferred_map_v`/`inferred_key`/`inferred_value` не заполнялись и D55 map-coercion не срабатывала. **Фикс (компилятор, `compiler-codegen/src/types/mod.rs`, ~140 строк):** добавлено поле `record_field_types: HashMap<String, HashMap<String, TypeRef>>` в `MapLitCtx` (собирается в `build()` из `module.items` + `peer_files`, любой `TypeDeclKind::Record`), плюс проброс `expected` в `Stmt::Assign` (через `var_types`-lookup цели), в рекурсии по полям `RecordLit` (через `record_field_types`), в элементах `ArrayLit` (через `TypeRef::Array`/`FixedArray` unwrap), и в arg0 built-in `Some`/`Ok`/`Err` (новая ветка в `ExprKind::Call`, читает outer `expected` как `Option[T]`/`Result[T,E]`). `emit_c.rs` НЕ тронут — фоллбек-ошибка «anonymous record literal without spread not supported» остаётся safety-net'ом для действительно нераспознанных контекстов. **Разграничение (важно):** ОТДЕЛЬНЫЙ, более глубокий манифест ТОГО ЖЕ маркера — анонимный record-литерал БЕЗ `#from_fields` (обычный record-тип, напр. `takes(p Point)` + `takes({x: 1, y: 2})`) в call-arg позиции ПО-ПРЕЖНЕМУ падает идентичной ошибкой; репродуцировано заново (совпадает с репро из секции «anon-RecordLit D55» этого файла/172.13-constraint-inference.md) — это ГЕНУИННЫЙ codegen-эмиссионный гэп (`emit_c.rs` финальная ветка anon-RecordLit читает ТОЛЬКО `current_fn_return_ty`, вообще не видит call-arg expected-тип) и НЕ дёшево чинится (нужен отдельный канал call-arg-expected-типа до codegen, кандидат для 172.12/172.13 codegen-трека) — НЕ трогал, маркер остаётся ОТКРЫТЫМ для этого подкласса. Итог: `[M-d55-anon-recordlit-codegen-gap]` — ЗАКРЫТ для HashMap/`#from_fields`+`#from_pairs`-манифестации, ОТКРЫТ для plain-record call-arg манифестации. **Побочная находка (новый floating-маркер, НЕ чинился — вне периметра задачи):** `[M-108-empty-frompairs-nonhashmap-kv-infer-gap]` — `extract_hashmap_kv` (types/mod.rs) хардкожен на literal-имя `"HashMap"`; для generic user-типа с `#from_pairs`, но НЕ named `HashMap` (напр. `type Bag[K,V] #from_pairs`), K/V не выводятся в EMPTY/`[]`-литерал ветке (`ro b Bag[int,str] = []`) → RUN-FAIL (runtime-краш собранного бинаря, НЕ compile-error). Непустой литерал (`[1: "a"]`) работает нормально — unify берёт типы из literal-элементов, не из `extract_hashmap_kv`. **Покрытие тестами (явный запрос владельца):** `d55_literal_coercion.nv`/`d55_sized_literal_contexts.nv` НЕ тестировали `#from_fields` вообще (только coercion анонимного литерала в обычный user-record, не в HashMap) и `#from_pairs` для user-типов (только `d108_map_literal.nv`, canonical HashMap-desugar). Добавлены `spec_tests/conformance/d55_hashmap_from_fields.nv` (7 pos-тестов — все 4 ранее-падавших контекста + 3 уже-рабочих для регресс-покрытия: let-с-типом/call-arg/return) и `spec_tests/conformance/d108_from_pairs_user_type.nv` (user-тип `D108Bag[K,V]` с `#from_pairs`-протоколом: `new`/`cap`/`insert_new`; empty-литерал-кейс задокументирован но НЕ включён — триггерит находку выше). **Гейты:** cargo build чисто; conformance `--positive --compile-error` 91/0 (baseline тот же, новые тесты — внутри той же single-CU агрегации `spec_tests/conformance`, верифицированы отдельным `test-build` PASS); HashMap-инициализация `{k: v}` компилируется И работает на рантайме (проверено 7 assert-тестами со значениями); `#from_fields`+`#from_pairs` pos-тесты зелёные; err173-корпус 5/5 δ0 (1 TIMEOUT под параллельной нагрузкой — подтверждённая флака, соло PASS за 10с); `nova check std` — 123 PASS / 25 FAIL / 169 WARN, FAIL-set байт-идентичен baseline (сравнение через temp `git worktree` на родительском коммите `d034eaf62`, тот же env). Модель: sonnet.
---

[2026-07-10 [M-toml-repeated-fail-call-run-fail] закрыт (мисдиагноз исправлен) + промоушен toml, ✅ ЗАКРЫТО] Предыдущая волна (Plan 186) завела маркер с гипотезой «runtime-баг в повторных вызовах Fail-эффектной функции внутри одного with-скоупа» (4/6 toml-тестов RUN-FAIL). Расследование (state-dump, минимальный репро без toml) показало гипотезу ОШИБОЧНОЙ: Fail-frame/`with Fail[E]`/D65-диспатч не сломаны — репро с повторными Fail-вызовами внутри with-скоупа и внутри while-цикла прошли чисто. Прямой запуск собранного .exe в обход `nova test-build`'овской обёртки (которая усекает detail-вывод до подмножества FAIL-строк) показал: на неисправленном toml.nv падают ВСЕ 6/6 тестов, а не 4/6 — исходное наблюдение само было артефактом отчётности. Настоящий корень — ДВА независимых, чисто локальных бага в toml.nv: **(1)** `is_bare_key_char`'s многострочная `||`-цепочка использовала ВЕДУЩИЙ `||` на continuation-строках; `||` одновременно синтаксис zero-arg closure-литерала (`|| body`), и парсер (`parse_or`, compiler-codegen/src/parser/mod.rs) сознательно не продолжает OR через ведущий `||` (во избежание мисparse настоящего closure-statement'а). Каждая ведущая-`||` строка молча становилась discarded zero-arg closure-литерал-statement'ом; итоговое значение функции — указатель на ПОСЛЕДНИЙ closure, coerced в `nova_bool` (всегда truthy). Ни checker, ни codegen не диагностируют — новый floating-маркер `[M-closure-trailing-scalar-coercion-no-typecheck]` (НЕ чинился — несоразмерно точечной задаче). Фикс: `||` перенесён в конец каждой строки (trailing-оператор — легитимная continuation, без closure-неоднозначности). **(2)** `@parse_number` вызывал ретрактированный `f64.try_from`/`i64.try_from(str)` ([M-f64-try-parse-to-parse-f64], Plan 174.1, известно-сломан: `f64.try_from("3.14")` молча даёт `3.0`) — фикс на канон `str @to_f64()`/`str @to_i64()` (§1а conversion-on-source). Оба репродуцированы в изоляции (fn без toml/Fail/consume; прямой вызов f64.try_from), подтверждая отсутствие связи с Fail-повторами. Промоушен: `std/_experimental/encoding/toml.nv` → `std/encoding/toml.nv`; inline-тесты вынесены в peer `toml_test.nv` (конвенция w2/batch3); +3 новых pos-регресс-теста, закрепляющих корректность повторных Fail-вызовов в одном with-скоупе (раз это было исходной гипотезой) — 9/9 PASS. std/nova.toml + std/_experimental/STATUS.md обновлены (encoding/-домен полностью PROMOTED). **Гейты:** cargo build чисто; conformance --positive --compile-error 90/0; err173-корпус 11/0 δ0; toml peer-тесты test --full PASS; `nova check` std — 118 PASS/30 FAIL, тот же FAIL-set что std-hygiene-baseline (δ0). Побочная находка: `nova check <dir>` с POSIX-относительным путём на Windows даёт ложное «no .nv files to check» — абсолютный `D:/`-путь работает; не расследовано глубже. Модель: sonnet.
---

[2026-07-10 std-hygiene — priv-field same-name-method bypass закрыт + слайс-миграция, ✅ ЗАКРЫТО] Две задачи по находкам владельца (ветка std-hygiene, модель sonnet). **(1) Дыра приватности полей:** владелец нашёл, что `ro n = raw.len` (std/http/server/wire.nv:20) компилируется из чужого модуля — читает priv/module-priv поле `Vec[T].len` напрямую вместо канон-метода `.len()`. Причина: `f3_check_member_ctx` (compiler-codegen/src/types/mod.rs) для типа с полем И одноимённым методом (Vec: поле `len`+метод `len()`, Plan 124/D220+Plan 160/D281) имеет fast-path `has_same_name_method` для CALL-формы (`s.len()`→метод, не priv-field-read), но для НЕ-call бэйр-формы (`raw.len`, без скобок) тот же fast-path материализовал тип поля и возвращался ДО priv-гейта — приватность не проверялась ВООБЩЕ на этом пути. **Fix:** зеркалирую priv_field_access_allowed/module_priv_access_allowed гейт в non-call ветку — closes [M-173-priv-field-samename-bypass]. Миграция std: ~30 бэйр-сайтов `.len`→`.len()` вне collections.vec (внутримодульные легальны, D281). Тесты: nova_tests/std_hygiene/{neg_vec_priv_field_bare_access,pos_vec_len_call_ok}.nv. Коммит 43d117122. **(2) Слайс-миграция**, класс [M-lint-findings-manual-slice-copy] (~30 сайтов): `push(x[i])` в счётном цикле → срез-вид `b[a..b]` (D262) / `.append(view)` — §18а nv-coding-style. deflate/inflate (append вместо push-цикла; filtered-gather — не contiguous-range, false-positive снят локальной переменной), crypto (digest `[N]u8`→`[]u8` — экспериментально подтверждено: range-index фикс-массива ВСЕГДА копирует, `.clone()` не нужен), http wire.nv×2 (снесена дублированная `fn slice()`-обёртка), servernet.nv (снесён `head_slice()`), fs/io read-loop (`append(chunk[0..n])`), fs/path.nv (scan-boundary+slice, component-сплит переписан), fs/mock.nv+server.nv percent_decode (conditional/filtered copy — false-positive снят той же техникой). **Попутно:** комментарии «`.new().cap(n)` мисроутит на WriteBuffer.cap» — МЁРТВЫЙ баг (репро чисто прошёл); реальный (иной) баг [M-vec-spelling-consume-chain-cap-collision] (string/transform.nv:195) — про `consume`-bound цепочки, не про `.cap`/WriteBuffer; комментарии были ошибочной атрибуцией, снесены, сайты — цепочкой. `nova lint --rule W_MANUAL_SLICE_COPY std/` = 0 (было ~30). Коммит f2f7f65e2. **Гейты (обе, на слитом с main efd66ea64):** cargo build чисто; conformance 90/0; nova check std — FAIL-set идентичен baseline (114/30, δ0 diff'ом множеств); err173+err173_1..3 — 25/0; targeted PASS (crypto/fs-path/io-buffered/compress/std_hygiene). **Известный pre-existing блокер (не регрессия, есть на main без диффа):** http decompress-путь тесты (mock_roundtrip/decompress/body_test) — CODEGEN-FAIL на compress/error.nv:121 E_UNKNOWN_TYPE Checksum, вне скоупа.
---

[2026-07-10 Plan 116 Ф.3 — runtime-дедлок TLS-handshake ЗАКРЫТ; +rt/codegen фиксы той же волной, ✅] Дедлок `[M-116-handshake-socket-deadlock]` (~1/300, обе фибры навечно в Net.read-park) = «остаточный класс» `[M-183-net2-loop-affinity-cross-thread-op]` (теперь CLOSED): work-steal уводит фибру на другой worker между park'ами, следующий uv-оп на хендле старого worker'а мутирует чужой не-thread-safe uv_loop → completion теряется. **Фикс A (rt, 5ca0ace10):** `nova_loop_defer_call` (generic-обобщение `nova_loop_defer_close`) — cross-thread issue-сторона (tcp read/write/accept/shutdown, udp send/recv) маршалится на owning-loop-поток; same-thread путь байт-в-байт прежний (урок: unconditional latch+wake на same-thread = reentrant self-wake ДО собственной парковки → гонка gopark/goready, ~50% зависаний accept-пути — задокументировано в net.c). **Фикс B (codegen, a89597277, вскрыт снятием дедлока):** heap-promoted примитивный локал (Plan 118 Ф.1, эскейп `&x` в вызов) читался БЕЗ deref — `c_fn(&err); use(err)` передавал heap-АДРЕС вместо значения (TlsError.from_shim получал мусор → NEG bad-PEM классифицировался Internal вместо InvalidPem; тест-тела эскейп-анализ не проходят (только Item::Fn) — прямые FFI-пробы в тестах «работали», маскируя). **Регресс:** `std/net/pingpong_test.nv` (alternating write→read на одном сокете — паттерн TLS-pump, stress_test не покрывал). **Ложный след (не дефект):** «split_test виснет ~50% на baseline» = slow-DNS (NXDOMAIN-тест до ~17с) при 8с-таймауте стресс-обвязки; с 60с — стабильно 35/35. Гейты: handshake_test 19/19 (было TIMEOUT); smoke-стресс 0 hang / 720+ прогонов (8×90); std/net CU (37 тестов) ×3 PASS; conformance 90/0 (слит main d1b9b2bc8); err173-корпус 5/5 δ0.
---

[2026-07-10 волна handler-annot — «один канал» для тел эффект-хендлеров, ✅ ЗАКРЫТО; + новый floating-маркер [M-closurefull-let-empty-ty]] Дефект (владелец 2026-07-10): оп-тела handler-литерала эмитятся в отдельные C-функции (`emit_c.rs::emit_handler_lit`), но НЕ переключали типовой контекст — `expected_record_type`/`current_fn_return_ty`/`contracts_post_label` оставались от внешней функции. Следствие: анонимный record-литерал (D55) в оп-теле падал «anonymous record literal without spread not supported in codegen» — блокер (b) из D316-amend Ф.2 (typed-wire Time). **Fix (02d5da526):** per-op save/restore того же ЕДИНОГО канала, что у emit_fn_body/лямбд/протокол-методов (ret из effect-схемы = та же разметка, что видел чекер); инференция не дублируется. Матрица `nova_tests/plan175_handler_annot/repro_matrix.nv`: было сломано → починено (анонимный heap/value record, =>/block-формы, захват+anon-record); работало и работает (именованные литералы, tuple, sum-вариант, Type.new); вложенные хендлеры с захватами и cross-effect вызовом — PASS (capture/#define механизм цел). Spec: D316-amend UPD (04-effects.md) + docs/time.md Ф.2 — ограничение (b) снято, option C для Time остаётся по причине (a) opacity Monotonic (решение владельца), провод Time не менялся. **Гейты (на слитом с main afc31820f):** conformance 90/0; матрица PASS; std/testing 2/0; err173*/1/2/3 PASS (err173_0 PASS с --timeout 300 — стресс 40×5 медленный, дефолт 60с мало); δ0-выборка (basics/effects/plan61/plan97/contracts/plan110/concurrency/std/time) на базовом и фикс-бинарях — БАЙТ-ИДЕНТИЧНЫЕ множества (9 PASS + 4 одинаковых pre-existing падения). **Попутная находка (НЕ чинилась, вне класса):** let-bound ClosureFull `ro f = fn(a int) -> int => a+1` — CC-FAIL «undeclared identifier f» В ЛЮБОМ контексте вкл. обычные тест-тела (nova_tests/basics/functions.nv падает на чистой базе 6582887e1): `infer_expr_c_type(ClosureFull)` даёт пустой `ty_c` (чекер аннотирует `resolved_types` только для zero-param ClosureLight — types/mod.rs:8424; ClosureFull-ветка :8445 не пишет в канал). Новый floating-маркер `[M-closurefull-let-empty-ty]` (backlog), трек 172.12.
---

[2026-07-10 Plan 172.5 хвосты — `[M-172.5-chain-gating-ro-at]` закрыт, `[M-172.5-generic-mut-ref-codegen]` ретрагирован как moot, ✅ ЗАКРЫТО] Возобновление 172.5-хвостов вскрыло: Plan 184 (закрыт 2026-07-08) ПОЛНОСТЬЮ суперседед исходный param-mode дизайн 172.5 (`ParamRefMode`/`mut ref`/call-site-маркер удалены; `ref T` теперь ограниченный тип, in-out = `mut x T`, Р1-Р14). `[M-172.5-generic-mut-ref-codegen]` описывал codegen для формы `mut ref x T`, которой больше не существует — ретрагирован как moot (современный аналог — `[M-184-value-mut-mode-overload-abi]`). `[M-172.5-chain-gating-ro-at]` — маркер тоже описывал мёртвую машинерию, НО названная им дыра (`c.peek().bump()` не гейтится) оказалась РЕАЛЬНОЙ и обострилась: до Плана 184 `-> @` non-mut метода был копией (безобидная мутация temp'а), после Р7 — настоящая `ref Self` (эмпирически подтверждено: `c.x` становился `1`, не `0`). **Fix:** `types/mod.rs::consume_walk_expr` — новая ветка рядом с `lvalue_root_ident` (который не видит Call-shaped chain-receiver), гейтит mut-метод на confirmed-ro `-> @`-звене → `E_RECEIVER_BINDING_NOT_MUT`. Потребовались ДВА доп. guard'а против ложных срабатываний (обнаружены прогоном по всему `std/`): (1) arity-aware `mut_methods_arity`/`ro_methods_arity` (иначе коллизия `@cap()`-геттер vs `mut @cap(n)`-сеттер, D117-идиома `.new().cap(n)` повсюду в std), (2) гейт по `recv_returning` (D132 self-return) — иначе `filter()/map()/chars()/post()` (строят НОВОЕ значение, не `-> @`) ложно попадали под запрет. Spec: D33 амендмент §«Fluent `-> @` chain-receiver mutability gate» (`02-types.md`). Тесты: `spec_tests/conformance/d326_chain_gating_ro_at.nv` (+neg). **Гейты:** cargo build чист; conformance 90/0 (было 89/0 + 1 новый neg); std/ check — идентичный FAIL-set с чистым baseline (114/30, оба ребилда); nova_tests fluent/chain-выборка (plan77/128/128_2/135/cgfix_fluent_tail_if/plan123_chain_elem) — идентичны (16/3, те же 3 pre-existing).
---

[2026-07-08 Plan 173.0 Ф.2+Ф.3 — рантайм-субстрат supervised: per-slot child_error[] retention + serialized decision-loop + ctx-pinning, ✅ ЗАКРЫТО] Гейт для 173.1/173.2 (без него их инварианты рантайм не давал). Ф.1 (drain/grow-vs-wake race) была уже закрыта в рантайме до этой волны (chunked stable-address SchedState, 2026-06-11) — deliverable-часть (доки/spec-amend/guard-фикстура) тоже уже закрыта предыдущей волной, проверено по коду. Эта волна = Ф.2+Ф.3. **Решение 1 (Ф.2):** новый `NovaChildError[]`/`child_ctx[]` массив на `NovaFiberQueue` (`nova_rt/fibers.h`) — ОТДЕЛЬНОЕ индекс-пространство от локальных `fibers[]/fiber_error[]/count` (Ф.1, заморожено, не тронуто); каждый remote (M:N) ребёнок получает свой слот при спавне (`nova_scope_alloc_child_slot`, вызывается из `nova_runtime_spawn_into`), индекс хранится в новом `NovaSpawnCtxBase._nova_parent_slot` (зеркалирован в обеих codegen-раскладках `emit_spawn`/`emit_detach`, `emit_c.rs`). При throw ребёнок пишет ТОЛЬКО свой слот (`nova_fiber_report_child_kinded` — без CAS, индексы не пересекаются); старый `first_error_atomic` путь не тронут (дешёвый cancel-сигнал остаётся, обратная совместимость). Read-API: `nova_scope_collect_child_errors`. **R2-инвариант** (torn-base на grow под конкурентной записью — жало §7.7): `_drain_started`-tripwire латчится в начале `nova_supervised_run_impl`, assert в `nova_scope_grow_children` доказывает grow-во-время-drain структурно недостижим (все `spawn` исполняются на вызывающем потоке ДО цикла дрейна; у спавненного тела нет ссылки на родительский stack-local scope; armed/local ветки никогда не смешиваются в одном scope-объекте — auto-arm происходит ДО первого пользовательского кода). **Решения 2-3 (Ф.3):** retention SpawnCtx при смерти ребёнка — `nova_scope_retain_or_release_child` вызывается из 3 точек `nova_spawn_pool_release` в `runtime.c` (**R1-guard**: подавляет pool-recycle для упавших детей, чтобы retained-указатель не aliasnul на следующий spawn). Serialized decision-loop в `nova_supervised_run_impl` (строго после `nova_sched_drop_state`, до free `ctx_pins`) — по разу на retained падение, ctx жив на входе; `nova_supervised_decide` — hook-точка для 173.2 (`on_child_fail`), default-политика 173.0 = pure no-op observer (внешнее поведение escalate-all-or-throw через `first_error`/`first_error_atomic` НЕ меняется — byte-parity с текущим re-throw, G-NEG). Ретеншн-ctx освобождается в pool ПОСЛЕ decision-loop. **Найденный попутно баг (не мой, но блокировал гейты, починен той же волной):** `nova_tests/err173_0/supervised_drain_mn_guard.nv` использовал устаревший `Time.now()` (переименован в `Time.now_unix_ms()` коммитом D316/Plan 175 ДО момента пиннинга этой ветки) — internal ICE `P67-LEGACY method=now` на любом прогоне; единственный оставшийся файл в репо со старым именем, исправлен. **Тесты:** `nova_tests/err173_0/child_error_retention_test.nv` (2 теста: N=40 одновременных детей с разными ошибками, форсирует grow child_error[] за NOVA_SCOPE_INITIAL_CAP=16; N=12×8 повторов lifecycle-профиль) — проверяют, что перевыброшенная ошибка ВСЕГДА well-formed тег из множества прогона (R1/R2 corruption исказила бы содержимое или уронила процесс). **Гейты:** cargo build чистый (compiler-codegen + nova-cli); conformance `--positive --compile-error` **70/0** (ровно ожидаемое число); err173_0 ×20 отдельных process-запусков под armed M:N — все зелёные; std/concurrency + std/net/stress_test + std/fs/concurrent_stat_test — PASS; regression-δ на `nova_tests/concurrency` (109 файлов, non-slow) против temp-checkout базиса 9551f4c99 — **byte-identical PASS/FAIL sets** (`diff` exit=0; 2 pass/107 fail оба до и после — 107 fail это ДРУГОЙ pre-existing баг, тот же D316-дрейф в масштабе всей папки `nova_tests/concurrency`, вне периметра 173.0, задокументирован честно, НЕ чинился этой волной — 100+ файлов, чужой план). Spec: D-блок «runtime-субстрат supervised» + `06-concurrency.md` amend (D14/D50/D75) — уже были закрыты предыдущей волной, дополнена только `docs/debugging-races.md` строкой-мостом к R1/R2/R3.
---

[2026-07-06 `[M-time-default-handler-not-wallclock]` — боевой default-обработчик Time.now_unix_ms() чинён на wall-clock, ✅ CLOSED] Default (без `with Time = handler {...}`) обработчик `Time.now_unix_ms()` (`_nova_time_default_now()`, `nova_rt/fibers.h`) возвращал `_nova_monotonic_ms()` (`uv_hrtime()`-uptime процесса) вместо настоящего unix-эпох — `Timestamp.now()` в боевом режиме лгал про календарную дату. Fix: `_nova_wall_unix_ms()` (новая, рядом с `_nova_monotonic_ms`) через `uv_gettimeofday(uv_timeval64_t*)`; `_nova_time_default_now()` переключён на неё (автоматически чинит все три default-делегата `now_unix_ms`/`now_ms`/`now_ns`). Monotonic (`now_monotonic_ns`/`_nova_monotonic_ns`) и mock-обработчики (`fixed_ms`/`mut_clock`) не затронуты (свой vtable-слот). Тест-детектор — `std/time/units_test.nv` (`Timestamp.now() без with > 1_700_000_000_000` мс). Spec: [D316 amend](../spec/decisions/04-effects.md#d316). **Verify:** cargo build clean; conformance 54/0 (не тронут); targeted `std/time/units_test.nv`+`std/concurrency/supervised_deadline_test.nv` PASS; дельта vs base-бинарь a3a4da52 (temp git-worktree, `target/libuv-cache` скопирован) на `nova_tests/time`+`nova_tests/concurrency` — 0 непредвиденных регрессий (только path-prefix diff; идентичные pre-existing `_repro_p110` CODEGEN-FAIL / `plan175_f1_timer_metrics_split` CC-FAIL до/после, не связаны с этим фиксом).
---

[2026-07-06 Plan 175 owner side-task — единицы времени в именах Time-опов + `Duration.@sleep()`, 🟢 ПРИЗЕМЛЕНО] D316 amend (вне формальной Ф-нумерации плана 175 — не путать с Ф.4, отдельным TODO про sleep-семантику/tolerance). `Time`-эффект (`std/prelude/effects.nv`): `now()`→`now_unix_ms()`, `now_monotonic()`→`now_monotonic_ns()` (`sleep(ms int)` не тронут). Факт-единицы подтверждены из рантайма, не предположены: `now_unix_ms` = unix-epoch мс (`Timestamp.from_unix_millis`); `now_monotonic_ns` = наносекунды (`_nova_monotonic_ns()`/`nova_rt/fibers.h` = `uv_hrtime()` без деления). Обновлены все вызовы в `std/` (schema-decl, `std/testing/handlers.nv` mock-handlers, `std/time/duration.nv`, `std/concurrency/{timer,supervised_deadline_test}.nv`, `std/_experimental/concurrency/rate_limiter.nv`). **Найден hardcode-дрейф вне `.nv`:** C-side `NovaVtable_Time` (`nova_rt/effects.h`) — hand-written struct с полем `now`, и wrapper-функции `Nova_Time_now`/`Nova_Time_now_monotonic` (`nova_rt/fibers.h`) — codegen designated-init'ит vtable по имени опы из `.nv`-схемы (НЕ хардкод в `src/`, подтверждено grep'ом), но сами struct/wrapper-имена были захардкожены под старые опы → CC-FAIL `no member named 'now_unix_ms' in 'NovaVtable_Time'` на первом же ре-компиле conformance; переименованы синхронно (`now`→`now_unix_ms` в vtable + wrapper, `Nova_Time_now_monotonic`→`Nova_Time_now_monotonic_ns`). Новый сахар `Duration.@sleep()` (`std/time/duration.nv`) = `Time.sleep(@to_millis_ceil())` — округляет ВВЕРХ до целых мс (никогда не спит меньше запрошенного; приватный helper `@to_millis_ceil` рядом). Тест `std/time/units_test.nv` (5 блоков). **Verify:** conformance 54/0; grep-инвариант `Time\.now()` в `std/`+`spec_tests/` = 0; дельта vs main-бинарь (temp git-worktree, `target/libuv-cache` скопирован для быстрой сборки) на `nova_tests/time`+`nova_tests/concurrency` — concurrency: идентичный pre-existing CC-FAIL (`_repro_p110`, str.len()/ro-binding, НЕ связан с Time) до/после; time: 1 ОЖИДАЕМЫЙ новый CC-FAIL (`plan175_f1_timer_metrics_split.nv` зовёт старое `Time.now()` — `nova_tests` сознательно НЕ мигрирован этим заходом, per рецепт «уходит в санацию»). Spec: [D316 amend](../spec/decisions/04-effects.md#d316).
---

### Plan 137 — Protocol rename: drop -able suffix (2026-06-09)
Hash→Hash, Equal→Equal, Compare→Compare, Clone→Clone, Display→Display, Debug→Debug.
Method renames: @equal→@equal, @display→@display, @debug→@debug.
E_PROTOCOL_RENAMED diagnostic with hint for old names. 4/4 plan137 tests PASS.
---

### Plan 133 — Remove usize/isize (2026-06-09)
int = intptr_t (address-sized signed integer) everywhere. Replaced usize/isize throughout.
~44 nova_tests sites + std/raw_mem.nv (7 params) + std/vec_owned.nv (2 casts) migrated.
nova_int: int64_t → intptr_t. nova_uint: uint64_t → uintptr_t. i64/u64 now separate from int/uint.
---

### Plan 134 — Remove ptr builtin, use *() (2026-06-09)
*() (pointer-to-unit) replaces ptr builtin. *() = void* in C codegen.
Removes nova_ptr typedef, ptr special-case. Also fixed 4 codegen bugs discovered during migration:
void* in tuple monomorphization, sqlite_mini_ffi.h nova_ptr usage, .0 field on void* newtype.
20 files migrated (plan115×10, sync.nv, examples/ffi, plan118/plan91/plan127).

---

### Plan 59 Phase 7 — production polish (M-priority items закрыты, 2026-05-17)

После production-grade audit'а Plan 59 (изолированно в worktree
plan-59-audit) добавлены 3 M-priority улучшения:

**Ф.7.1 ✅ (commit 12ac69b9700):** tuple arity mismatch diagnostics —
Nova-level clear codegen error до C-emit'а. Pre-check в 3 sites
(emit_tuple_destructure, pattern_destructure_tuple, pattern_bind_typed).
Test f24_arity_mismatch_diagnostic.

**Ф.7.2 ✅ (commit 4a6532ccea5):** HashMap.@clone() idiomatic
`for (k, v) in @iter()` (после Plan 63 Fix E). Audit подтвердил
LRU/Set/Deque не имеют workaround-loops — LRU index needed для
skip-last; Set уже idiomatic. plan56 6/6 PASS.

**Ф.7.3 ✅ (commit a27e1968040):** sizeof warning для больших mono'd
tuples (>5 elements OR >128 bytes estimated). Helper
`estimate_c_type_size_bytes` + RefCell<Vec<String>> warnings field
+ test_runner combines codegen_warnings + lint_warnings для
EXPECT_COMPILE_WARNING. Test f25_large_tuple_warning.

**Ф.7.4-7.6 deferred (commit 3b542940507):** L-priority — named tuple
fields (~200 LOC + design decisions), full mono'd Result (~300-400),
tuple subtyping (~200+ variance). Defer до dedicated plans (Plan 64+)
с design pre-discussion. Rationale: production-grade = не делать
наполовину; защита от half-baked feature.
---

### [examples/stdlib/] — 11 demo-файлов не компилируются в bootstrap'е (2026-05-06)
- **Где:** `examples/stdlib/*.nv`
- **Что:** complex, duration, hashmap, json, linkedlist, queue, range,
  semver, set, sql, vec — все 11 spec-faithful демо-файлов падают на
  codegen-stage. Подробный список причин см. `examples/stdlib/STATUS.md`.
  Группы блокеров: char-литералы, `&` operator, multi-line handler/if-else,
  `effect` keyword as type, anonymous record literal, `throw` в expression-
  position, generic-syntax парсера.
- **Почему:** Эти файлы — aspirational. Они написаны как «как Nova код
  должен выглядеть в зрелой версии», но bootstrap-codegen фокусировался на
  языковом ядре (concurrency, эффекты, типы) и не покрыл полный stdlib API.
- **Как запустить:** `.\run_tests.ps1 -IncludeStdlib` запускает обычный
  suite + 11 stdlib (опционально). По умолчанию — только nova_tests/.
- **Roadmap:** spec-clarifications (A: char-литералы; B: убрать `&` —
  Nova managed heap; G: throw expr position) → парсер (C, D, F) →
  codegen (E). Финальная цель: 11/11 stdlib PASS.
- **Приоритет:** M (важно для AI-кодинга — без stdlib в зелёном CI
  трудно генерировать пользовательский код, основывающийся на этих типах).
---

### [2026-05-07] nova_tests/ — иерархическая реорганизация
- **Где:** все 57 файлов мигрированы из плоского `01_X.nv` в
  `<group>/X.nv` (commit a33b245).
- **Группы:** basics/ types/ syntax/ effects/ concurrency/ runtime/
  modules/ — соответствуют тематическим областям spec/decisions/.
- **Module decls:** `module spec.X` → `module nova_tests.<group>.X`
  (D29-compliant: package name из nova.toml + filesystem path).
- **Keyword collisions:** `cancel_scope_test`, `detach_test`,
  `effects/basic.nv` (избегаем conflict'ов с keyword/runtime files).
- **run_tests.ps1:** recursive search + per-test obj_dir + relative
  display name + case-insensitive path comparison.
- Spec D29 дополнен примером — раздел «Иерархическая структура
  test-suite (D29 в действии)» в 07-modules.md.
---

### Pattern: handler-обёртка для cleanup ресурсов (D10 demo)
- **Где:** `nova_tests/effects/handler_wrappers.nv` (4 теста).
- **Идея:** Nova не имеет defer/RAII (Q20 open). Cleanup через
  функцию-обёртку с body-lambda и внутренним `with Fail = handler`.
  На throw — handler ловит, выполняет cleanup, re-throw'ит наружу.
- **Bootstrap-ограничения, выявленные при написании:**
  - `mut` в свободных fn-параметрах не парсится → record-Tracker.
  - `fn T @method(...) Fail[E] -> R` парсер не любит → throw-методы
    как свободные fn (receiver в первом аргументе).
  - Trailing-block с non-int closure-параметром падает в codegen
    (нет type-erasure для closures) → body принимает int (id), не
    сам Resource.
- **Закрывает:** ничего конкретно (Q20 defer всё ещё open), но
  демонстрирует канонический D10-pattern для cleanup'а.
---

### Plan 13 Ф.9 — API polish, читаемость auto-gen, Self everywhere (2026-05-08)

После завершения Ф.8 ревью сгенерированных `std/runtime/*.nv` файлов
выявило 6 unfortunate API-decisions, которые лучше зафиксировать
до того как пользовательский код устаканится. Ф.9 — точечные правки:

#### Ф.9.0 — пустые строки между методами в auto-gen

Renderer добавляет `\n\n` после каждой `// doc + external fn` пары
(было `\n`). Файлы стали читаться как нормальные spec-документы.

Проблема старого формата: 24+ `external fn` подряд без визуальных
групп — глаз теряется. Diff-review был мучительным.

#### Ф.9.1 — Self-return everywhere для chaining

Mutating-методы (`@append`, `@write_*`) и creation-static (`new`,
`from`, `with_capacity`) — теперь все возвращают `Self`. Единый
паттерн на opaque types вместо «здесь Self, тут явный тип».

```nova
// Было: read_buffer.nv
export external fn StringBuilder.new() -> StringBuilder
export external fn StringBuilder mut @append(s str) -> ()

// Стало:
export external fn StringBuilder.new() -> Self
export external fn StringBuilder mut @append(s str) -> Self
```

Chaining работает: `sb.append("hello ").append(name).append("!")`.
C-side: `Nova_<T>_method_*` возвращают `Nova_<T>*` self-pointer
(тот же receiver, без аллокации). `void`-returning функции стали
identity функции с return value — backward-compat для statement-style
вызовов сохраняется.

#### Ф.9.3 — str.@char_len → str.@len, []char первоклассный

D26 spec говорит «s.len — длина в codepoint'ах». Имя `@char_len`
отражало реализацию (codepoint = char), но противоречило spec.
Переименовано в `@len` (C-name `nova_str_char_len` сохранён).

`str.@chars() -> []int` → `-> []char` — char стал first-class type
в API, eager allocation как минимум (lazy `Iter[char]` — future).

#### Ф.9.4 — read_char/read_str для парсинга текста

ReadBuffer покрывал только числовые типы. Для HTTP headers, CSV,
text-протоколов нужны codepoint-методы:

```nova
fn ReadBuffer mut @read_char()      Fail[ReadBufferError] -> char
fn ReadBuffer mut @read_str(n int)  Fail[ReadBufferError] -> str
```

Plus Result-формы `try_read_char` / `try_read_str(n)`.

`ReadBufferError` расширен вариантом `InvalidUtf8 { position }` —
distinct ошибка от `UnexpectedEnd` (мусорный байт vs неполная sequence).

C-runtime получил helper `_nova_rb_decode_utf8_one(p, avail, *cp,
*consumed)` — общий UTF-8 декодер. Используется в read_char,
read_str, try_read_char, try_read_str — DRY.

#### Ф.9.5 — отмена auto-derive try_read_* из Plan 12 Ф.4.5

Plan 12 Ф.4.5 предлагал: компилятор синтезирует `@try_read_X()` из
`@read_X() Fail[E]`. Отменено в Ф.9.5 по 3 причинам:

1. **Hidden magic.** В registry/.nv видна только Fail-форма, но IDE
   автокомплит показывает try_read_X неоткуда. AI-генерируемому коду
   ещё сложнее.
2. **Edge cases.** UTF-8 ошибки (Ф.9.4) делают universal правило
   хрупким — synth должен мапить и UnexpectedEnd, и InvalidUtf8.
3. **D82 single source of truth.** Auto-derive противоречит принципу
   «всё что компилятор знает — видно в registry».

В runtime_registry все 17 пар read_*/try_read_* (16 numeric + char +
str) явно перечислены. C-функции тоже две (Fail + Result).

D73 From↔Into auto-derive **остаётся** — симметричное правило в D73,
не зависит от Plan 12 Ф.4.5.

Plan 12 Ф.4.5 помечен ❌ ОТМЕНЕНО, spec D82 обновлён.

#### Бонус: str.from(int) regression fix

После Ф.8 в registry появился `str.from(c char)`, что засветило `from`
в `method_receivers` и сломало dispatch для `str.from(int_val)` —
codegen эмитил `Nova_str_static_from(v)` без mangling-suffix'а, а
реальная C-функция называется `Nova_str_static_from_char`.

Fix: routing через method_overloads с поиском подходящего overload'а
по C-типу аргумента. Если match — sig.c_name. Иначе fallback на legacy
`nova_int_to_str(v)`. Применено в обеих точках emit_c.rs.

#### Бонус: hashmap.nv `&T` borrow → plain field

`std/collections/hashmap.nv` использовал `map_ref &HashMap[K, V]` —
`&T` borrow запрещён в Nova (D43, см. spec/decisions/05-memory.md:63).
Поле переименовано в `map HashMap[K, V]` (короче, без borrow). GC
держит мапу живой через field-reference.

#### Что отложено

**Ф.9.2 — оператор `+` как alias** (`StringBuilder + str` → `@append`,
`str + str` → `@concat`). Требует careful routing через
method_overloads и parameter-type mangling. Риск регрессий в 78
тестах. Перенесено в следующую сессию.

#### Total numbers (после Ф.9)

- Registry entries: **161** (было 157 — +4 от Ф.9.4 read_char/str
  пар).
- Auto-generated .nv файлов: **6** (без изменений).
- Handwritten в `std/runtime/`: **0**.
- Self-return mutating/creation методов: **30+** (вместо `()`/тип).

#### Урок

**Plan rev iterations работают.** Ф.9 — это «после ревью
сгенерированного» этап. Без regen → review → fix цикла файлы
выглядели бы хуже. AI-friendly auto-gen означает что один этап
не финализирует API — нужен полный round-trip с ревью.

**Self vs explicit type — единый паттерн лучше микрооптимизации.**
Изначально creation-static возвращали `WriteBuffer` (явный тип),
а instance-mut → `Self`. Ревью показало: непоследовательно.
Унификация на Self (везде в opaque-context) убрала когнитивную
нагрузку.

**Auto-derive symmetry rule (D73 ↔ Plan 12 Ф.4.5).** D73 From↔Into —
симметричное правило: synthesized метод имеет ту же семантику. Plan
12 Ф.4.5 try_read auto-derive — асимметричное (Fail vs Result, разная
семантика для caller'а). Симметричные правила выживают, асимметричные
становятся source of bugs.
---

### Plan 11 Ф.4+Ф.5 — Method values как first-class (2026-05-08, вечер)

Plan 11 Ф.1-Ф.3 (overload по типу аргумента) был закрыт в первой
половине дня. Ф.4 (method values) и Ф.5 (`as fn(...)` disambig)
оставались deferred до этой сессии.

#### Ф.4 — три формы method values

**Bound** — `obj.@method`. Closure struct {fn_ptr, captured_self}.
При вызове `f(args)` codegen unpacks struct, вызывает fn с env+args,
fn-wrapper извлекает self из env и вызывает реальный
`Nova_<T>_method_<m>(self, args)`.

**Unbound** — `Type.@method`. Closure struct {fn_ptr, dummy_env}.
fn-wrapper принимает self как первый параметр явно, не хранит его в
env.

**Static** — `Type.method` (без `@`). Уже работало через
`nova_fn_<name>` поинтер.

#### NovaClosBase — generic closure layout

До Ф.4 nova_rt.h имел только 5 hardcoded closure structs (NovaClos_vi,
ii, ib, iii, vii) — для конкретных сигнатур lambda. Method values
имеют **произвольные** сигнатуры (Counter*, int) → int, etc.

Решение: добавлен `NovaClosBase = { void* fn; void* env }` —
**generic** layout. Bit-уровень: same as NovaClos_*. На call-site
codegen cast'ит `fn`-поле к нужной сигнатуре:

```c
((ret(*)(void*, args...))((NovaClosBase*)f)->fn)(((NovaClosBase*)f)->env, args...)
```

Это работает для **любой** сигнатуры без per-sig macros. Per-sig
macros остаются для optimization (когда сигнатура hardcoded — компилятор
видит typed call) и backward-compat для NovaClos_* lambda emission.

#### Ф.5 — `as fn(P...) -> R` disambiguation

Когда у метода несколько overload'ов по типу аргумента:

```nova
fn Buf mut @push(n int) -> int => ...
fn Buf mut @push(b bool) -> int => ...

ro f = buf.@push                       // ambiguous → берётся first
ro g = buf.@push as fn(int) -> int     // выбор первого overload'а
ro h = buf.@push as fn(bool) -> int    // выбор второго overload'а
```

В codegen emit_expr для `As(Member, TypeRef::Func)`:
1. Извлекаем target_signature из Func type.
2. Вызываем `emit_method_value_typed(obj, method, Some(sig))`.
3. `emit_method_value_typed` фильтрует overloads по param-types match.
4. Match'ed overload даёт правильный mangled c_name (Plan 11 Ф.3
   уже эмитил `Nova_Buf_method_push__nova_bool` для второго overload'а).

Для unbound `Type.@method as fn(Recv, P...) -> R` skip первый param
(receiver) при сравнении.

#### Ф.7 — тесты

- `nova_tests/syntax/method_values.nv` — 7 тестов: bound (no/one/two
  args), unbound, разные obj несут свои self, as-fn annotation.
- `nova_tests/syntax/overload_method_values.nv` — 3 теста: bound int
  overload, bound bool overload, unbound int overload — все через
  `as fn(...)`.

После Ф.4+Ф.5: **80/80 nova_tests PASS** (было 78 — +2 новых тестовых
файла, оба passes).

#### Bootstrap-ограничение

**External methods (str, int runtime) не доступны как method values.**
`s.@byte_len` сейчас bails: codegen ищет в `method_overloads` registry,
а built-in str API живёт в `ExternalRegistry` (`std/runtime/string.nv`
external decls). Routing через ExternalRegistry — future work.

Workaround: для current bootstrap'а — оборачивать в lambda:
`let f = (s) => s.byte_len()`. Future: emit_method_value_typed
fallback'ом ищет в external registry.

#### Урок

**Generic `NovaClosBase` lifts the «hardcoded sig matrix» limitation.**
Closures с произвольными сигнатурами были невозможны без per-sig macros.
NovaClosBase + cast-at-call-site решает это в ~10 строк runtime'а +
~15 строк codegen-fallback.

**Desugar to lambda был bardziej elegant, но не нужен.** Думал сначала
synthesize Lambda AST для bound case → reuse emit_lambda. Но direct
emission (генерация wrapper-fn + env-struct + closure-alloc inline)
оказался проще: меньше indirections, прозрачнее в emitted C.

**Type annotation как hint для codegen — стандартный приём.**
`as fn(...)` не меняет run-time поведение (остаётся `(void*)expr`
cast), но **меняет codegen** на let-binding и emit_method_value
levels — выбор overload'а. Прецедент: TypeScript type assertions
влияют на overload resolution.
