# Backlog Followups

This file tracks deferred items, known limitations, and future improvement tickets
referenced from plan docs and simplifications.md.

---

## Single-letter type names — E_TYPE_NAME_TOO_SHORT

- **[M-single-letter-type-ban]** CLOSED Plan 167. Запретить `type X { ... }` где имя типа длиной 1 символ.
  Мотивация: однобуквенные имена конфликтуют с generic-параметрами (`fn[S Iter[T]]` vs `type S`),
  вызывая E_PREFIX_SHADOWS_NAMED_TYPE. Haskell решает регистром (type vars строчные), Nova
  решает запретом однобуквенных типов — generic-параметры остаются однобуквенными по конвенции.
  Реализация: новый error E_TYPE_NAME_TOO_SHORT в checker (name.len() == 1 для TypeDecl).
  Sweep: grep `^type [A-Z] ` по nova_tests/ и std/ — исправить (~10 в nova_tests/plan118_1_addr_chains/).
  Priority: M.

---

## Name shadowing diagnostics

- **[M-prelude-name-shadow-hint]** Улучшить диагностику когда пользовательский тип называется так же как prelude-протокол.
  Сейчас: `type Iter { ... }` в модуле + использование в generic bound → `E_BOUND_NOT_PROTOCOL` (технически верно, но неясно почему).
  Хотим: hint «type name `Iter` shadows prelude protocol `Iter` — rename your type or use a qualified path».
  Реализация: в check_bound_ref, если bound-name резолвится в user TypeDecl (не Protocol) И в prelude есть Protocol с тем же именем — добавить hint к E_BOUND_NOT_PROTOCOL.
  Priority: M.

- **[M-vec-shadow-leak-e7310]** User-shadow обобщённого типа протекает во внутренние type-refs
  импортированного модуля. `type Vec { x int, y int }` (не-generic) в пользовательском модуле,
  затеняющий прелюд/импортированный `Vec[T]` (D29 «user wins»), приводит к тому, что СОБСТВЕННЫЙ
  код `std/collections/vec.nv` / `hashmap.nv` (`Vec[T]`, `Vec[Slot[K,V]]`) резолвится на
  пользовательский НЕ-generic `Vec` (0 type-параметров) → `[E7310] type Vec is not generic —
  takes no type arguments, but 1 was provided`. Затенение должно быть scope'нуто к модулю
  пользователя, не протекать в чужие модули. Комментарий fixture'а (plan138_2/t14) утверждает,
  что это когда-то чинилось → вероятно регресс (или дрейф от Vec-prelude-flip). Вскрыто
  консолидацией 169.1.2; обходной путь применён — shadow-fixtures plan138_2 (t14/t15/t16)
  переименованы в `UserRecNN` (shadow-покрытие снято, см. 169.2). Priority: M.

---

## Plan 118.6 — Safe &x model

- **[M-118.6-tuple-field-escape]** `&tuple.N` (tuple field by index) escape analysis chain-root tracking.
  Current: only named struct field chains are tracked. Tuple index access `&t.0` may not
  correctly promote the parent tuple. Verify and extend escape_analyze.rs if needed.
  Priority: M.


---

## D215 amend — Named tuple field defaults

- **[M-D215-defaults-handler-lambda-type]** `infer_handler_interrupt_ty` не может вывести тип
  lambda-параметра `e` в паттерне `with Fail[E] = |e| interrupt Some(e) { ... None }`.
  Корень: `infer_expr_c_type(Lambda(...))` не знает тип `e` без binding annotation или
  type-propagation от `Fail[E]` окружающего контекста. Следствие: `Some(e)` → `NovaOpt_nova_int`
  вместо `NovaOpt_ParseComplexError` → match на `Option[ParseComplexError]` падает.
  Тест в `std/_experimental/math/complex.nv` закомментирован.
  Fix: propagate Fail-binding type через context при выводе типа handler-lambda параметров.
  Priority: M (нужен для любого non-trivial Fail-bound error handler).

---

## Plan 147 — Three-axis mutability (D246)

- **[M-147-ro-binding-index-freeze]** `ro a []int` → `a[i] = x` должен давать ошибку по P7
  («голый `ro r` = freeze, весь owned-граф»), но сейчас **разрешается**.
  Корень: `check_target_readonly` ветка `ExprKind::Index` проверяет только `tr.is_readonly()`,
  но не `ro_binding_names`. Для `ExprKind::Member` `is_through_ro_binding` есть — для Index нет.
  ВАЖНО: `a[i]=x` для `[]T` codegen-inlined (`Stmt::Assign + ExprKind::Index`), НЕ диспатчится
  через `mut @index` метод (vec/access.nv:53-54) — поэтому `mut_methods` реестр не помогает.
  Баг актуален сейчас для `[]T` + после Plan 121 для `[N]T`.
  Fix: добавить `is_through_ro_binding(obj)` в Index-ветку `check_target_readonly` + oracle-тест.
  Priority: M.

- **[M-147-ro-ro-redundant-binding]** Следующие формы должны давать `E_REDUNDANT_TYPE_MODIFIER`
  (D246 «Канон синтаксиса»), но сейчас принимаются без ошибки:
  - `ro a ro T` — явный `ro` на binding + явный `ro T` на типе
  - `func(a ro T)` — параметр ro по умолчанию (D176) + явный `ro T` на типе
  - `mut a mut T` — `mut` binding + явный `mut T` (тип без модификатора уже mutable)
  - `func(mut a mut T)` — то же для параметра
  Fix: в checker при let/param — если (binding ro явно или по умолчанию) И тип явно `ro T` →
  `E_REDUNDANT_TYPE_MODIFIER`; если binding mut И тип явно `mut T` → то же.
  Priority: M.

- **[M-147-param-index-freeze]** `func(a []int)` → параметр ro-binding по умолчанию (D176);
  `a[i] = x` внутри fn должен давать ошибку — codegen-inlined путь, не через `mut @index`.
  Связан с [M-147-ro-binding-index-freeze] — один и тот же фикс в Index-ветке `check_target_readonly`.
  Priority: M.

---

## Plan 138 — `[]T` sugar / Vec codegen

- **[M-138-vec-pointer-element-mono]** `Vec[*T]`/`Vec[*mut T]`: codegen монорфизация для pointer-element-type сломана — `Vec.new()` вызывает generic-заглушку `Nova_Vec_static_new()` → NULL вместо специализированного конструктора → SEGFAULT при push/index. Структура `Nova_Vec____int64_t_p` и методы push/index генерируются правильно; ломается только static constructor. `Option[*mut T]: Some(p)→*p=v` работает (другой путь). Воспроизводится: `mut v Vec[*mut i64] = Vec.new(); v.push(&a); unsafe{*v[0]=100}`. Priority: P2.

---

## Plan 168 — Vec generic fwd-decl (D300)

- **[M-168-resize-with-free-fn-shadow]** `plan153_1/resize_with_free_fn_shadow` — pre-existing CODEGEN-FAIL: `undefined identifier f` when a module-level free fn `f` clashes with closure param `f` inside Vec.resize_with/fill_with. Not caused by Plan 168. Requires fix in name resolution (closure param scope should shadow outer free fn). Priority: M.

- **[M-168-other-generic-fwd-decl]** Other generic types (HashMap[K,V], Set[T], etc.) may have similar body-only instantiation gaps if they're used in fn bodies but not in signatures/fields. The Plan 168 tuple-elem fwd-decl fix covers them too (via MONO_TUPLE_TYPEDEFS), but the pre-pass body-scan only scans Vec TurboFish. If HashMap[str, u32] appears body-only it may also fail. Monitor for CC-FAIL patterns and extend scan if needed. Priority: L.

---

## Plan 91.8b — operator-dispatch cleanup

- **[M-91.8b-precompiled-c-rebuild]** ✅ CLOSED (Plan 91.15, 2026-06-17) — plan91_8b 6/6 PASS.
- **[M-91.15-hashmap-precompiled-eq]** `std/collections/hashmap.c` (precompiled) still uses `k.eq(key)` struct-member syntax instead of `Nova_str_method_equal`. CC-FAIL on map_literals tests with str keys. Fix: regenerate hashmap.c via `nova build-std` after Plan 91.8b @eq→@equal rename. Priority: M.

---

## Plan 91.15 — std API tuning

- **[M-91.10-remove-needs-caps-field]** ✅ CLOSED (Plan 91.15 Ф.5, 2026-06-17) — FnDecl.needs_caps removed from AST.
- **[M-91.14-option-result-debug]** ✅ CLOSED (Plan 91.15 Ф.2, 2026-06-17) — Option/Result @debug work via DeclaredBody interp dispatch.
- **[M-91.14-derive-debug]** ✅ CLOSED (Plan 91.15 Ф.3, 2026-06-17) — `#impl(Debug)` auto-derive works for record types. known-limit: checker does not validate field Debug bounds at synthesis time.

---

## Plan 147 Ф.7 — D246 checker enforcement gaps

- **[M-147-ro-binding-index-freeze]** ✅ CLOSED (Plan 147 Ф.7, 2026-06-17) — `ro a = [...]; a[0] = x` now gives `E_READONLY_CONTENT`. `is_through_ro_binding` check added to `check_target_readonly` Index arm in `compiler-codegen/src/types/mod.rs`; entry-code guard avoids false positives in prelude/std imports.
- **[M-147-param-index-freeze]** ✅ CLOSED (Plan 147 Ф.7, 2026-06-17) — non-`mut` params are now registered in `ro_binding_names` at fn entry (snapshot/restore), so `v[i] = x` on a plain `v []int` param gives `E_READONLY_CONTENT`.
- **[M-147-ro-ro-redundant-binding]** ✅ CLOSED (Plan 147 Ф.7, 2026-06-17) — `ro a ro []int = [...]` gives `E_REDUNDANT_TYPE_MODIFIER`; handled at parser level (`parser/mod.rs` lines 5198–5205, already present); oracle test `f7_neg3` confirms.
- **[M-147-readonly-content-lsp-quickfix]** nova-lsp `E_READONLY_CONTENT` quick-fix (Plan 147 Ф.7, 2026-06-17) — базовый `fix_readonly_content` добавлен в `nova-lsp/src/code_actions.rs`: ищет `ro <name>` binding вверх по файлу и предлагает `ro → mut`, или добавляет `mut ` перед параметром. Priority: P2 (улучшить heuristic при необходимости).

- **[M-118.7-safe-addr-outside-fn-scope]** Plan 118.6/118.7 known limitation: `&ident` без `unsafe {}` как trailing expr в fn body даёт `undefined identifier` (checker ищет ident в другом контексте). Workaround: `unsafe { &ident }` — поведение идентично после 118.7. Priority: P3 (правильная fix requires full type-inference in escape sink).

---

## Plan 91.18 — str + unicode API audit & cleanup (followups)

- **[M-91.18-to-words-array]** `str @to_words() -> []str` — eager materialization of word segments (mirrors `to_chars`). Priority: P2.
- **[M-91.18-eq-u8-slice]** `Equal` for `ro []u8` — would simplify `string_builder.nv @starts_with/@ends_with` (`.compare(b)==0` → `==b`). Priority: P2.
- **[M-91.18-from-bytes-lossy-slice]** `str.from_bytes_lossy` valid-sequence push optimization: `out.append(bytes[i..i+seq])` instead of per-byte push. Priority: P2.
- **[M-91.18-validate-utf8-dedup]** Shared `utf8_seq_len()` helper to de-duplicate utf8 sequence-length calculation between `from_bytes_lossy` and `chars.nv` decode. Priority: P3.
- **[M-91.18-stringbuilder-len-naming]** Consider `@len` → `@byte_len`, `@capacity` → `@cap` on StringBuilder (aligns with str convention; WriteBuffer family naming context). Priority: P3.
- **[M-91.18-unicode-cat-enum]** `GCB_*` / `WB_*` / `GC_*` / `SB_*` constants as real enums (requires codegen enum-from-int support). Priority: P3.
- **[M-91.18-import-gated-str-methods]** `str @to_upper()` / `str @to_lower()` extension methods currently resolve without `import std.unicode` (str ext-methods bypass import gating). Fix would require per-module method visibility tracking in the resolver. Priority: P2.
- ~~**[M-152.5-collation-conformance-u32-overflow]**~~ ✅ **FIXED 2026-06-19.** `nova_tests/plan152_5/collation_conformance.nv` RUN-FAIL `array: index 12884901890 out of bounds for length 4` (= 3·2³²+2). Root cause: in `collate.nv` `s21_match`, the consumed-index list (`Vec[int]`) was pushed through `cp_seq_push(src Vec[u32], x u32)` — the `(hi<<32)|lo` garbage came from reinterpreting 64-bit ints as 32-bit u32 words. Triggered only on the DUCET **S2.1 discontiguous** contraction path (Tibetan U+0FB2+U+0F71+U+0F80). Fix: added `idx_seq_push(src Vec[int], x int)` and routed both `cur_consumed` pushes through it. Regression-guard added to `collation.nv`.
- ~~**[M-vec-elem-type-mismatch-silent]**~~ ✅ **FIXED 2026-06-19** (generalized to **[M-generic-arg-type-mismatch-silent]**, commit `a9726e91`). The checker accepted passing a whole generic value with a different concrete-primitive type-argument (`Vec[int]`→`Vec[u32]`, user `Stack[int]`→`Stack[u32]`, `Option[f32]`→`Option[f64]`, …) — a pointer reinterpretation that surfaced only as a runtime OOB or a late C-stage CC-FAIL. Root cause: `cat_of`/`TyCat` folds all int widths into one `TyCat::Int` AND drops a named type's generic arguments. Fix (general, NOT Vec-specific): `f1_check_call` compares each type-argument of matching generic types at raw-TypeRef granularity and emits `[E_ARG_ELEM_TYPE_MISMATCH]`. (Scalar `int`→`u32` coercion outside a generic is NOT touched by this check — but per spec it should require explicit `as`; the current lenient behavior is a SEPARATE gap, see `[M-scalar-nonliteral-narrowing-not-enforced]`.) Supporting: `cat_of` lowers named `Vec[T]`→`Array` (D239 `[]T ≡ Vec[T]`); `infer_expr_type` resolves `Type[T].{new,with_capacity,from,default,filled}(…)` to carry element types into scope. Tests: `nova_tests/vec_elem_type/` + `plan70_4/neg/`.
- ~~**[M-scalar-nonliteral-narrowing-not-enforced]**~~ 🟡 **MOSTLY DONE 2026-06-19** (commit `f96016e6`). Per spec D54+D227 a non-literal wider int narrowing into a narrower / value-range-unsafe int position now requires explicit `as` → `[E_IMPLICIT_NARROWING]`. Enforced at: **bindings** (`ro a u8 = int_var`), **free-fn / static-method arguments** (`take_u8(int_var)`), and **reassignment** (`a = int_var`). Rule: value-range-preserving widening stays implicit (signed→wider-signed, unsigned→wider-unsigned, unsigned→strictly-wider-signed, `int`≡`i64`, `uint`≡`u64`); narrowing + signed→unsigned + value-unsafe cross (u32→i32, u64→int) need `as`. Literals keep their D227 range-check; `as`-casts auto-exempt. **Blast radius was ZERO** (no std migration needed) — see the remaining gap below. Tests: `nova_tests/narrowing/`. Spec amend pending (D54/conversions.md — gated on the other session's in-flight spec edits to `03-syntax.md`).
- **[M-instance-method-arg-scalar-narrowing]** Precise scope (corrected 2026-06-19 after empirical mapping): argument types of method calls ARE validated, just by other layers — overloaded fns/methods resolve by static arg types in the **codegen overload resolver** (emit_c.rs:23026; a no-match → CODEGEN-FAIL `no matching overload for g(nova_bool)`), and a category mismatch (struct↔scalar, e.g. `Vec[int].push(str)`) is caught by the **C compiler** (CC-FAIL `passing nova_str to incompatible type nova_int`). The Nova type-checker itself does not type-check method args, but the ONLY thing that slips through ALL layers is **scalar→scalar implicit narrowing** through a single-overload method arg (`vec_u32.push(int_var)`: arity matches the one `push(u32)`, and int→u32 is a C-legal truncation). So the gap is narrow (NOT "methods are untyped"). Fix is point-sized: the codegen resolver already knows each param's C-type (`param_c_types`), so add an int-narrowing check there comparing arg C-type vs param C-type. This WILL flag the ~375 std `push(int)` sites → migrate them with explicit `as`. Priority: P1 (soundness).
- ~~**[M-generic-arg-mismatch-records-followup]**~~ ✅ **DONE 2026-06-19** (commit `4e5533ff`). The generic-argument mismatch check now flags concrete **record/sum/newtype** type-args too (`Box[Dog]`→`Box[Cat]`) and **nested** generics (`Vec[Vec[int]]`→`Vec[Vec[u32]]`) via a recursive `generic_arg_mismatch()`. Alias-safe (resolved via `cat_of`, so `Box[Meters alias int]`→`Box[int]` does not false-flag); permissive on generic type-params / protocols / unknowns. Zero false positives across the corpus.
- **[M-172.1-U1-cli-stdpath]** Plan 172.1 U.1.1: std-path is configurable via env `NOVA_STD_PATH` + `nova.toml [workspace]/[package].std` (resolver `manifest::resolve_std_path`, default `repo/std` byte-identical). The CLI `--std-path` flag (a third config surface above env) is not yet wired — env+manifest already satisfy the §2 «WHERE is config» requirement. Priority: P3 (UX convenience). Add a `--std-path` arg threaded (via a process-global set at startup) into `resolve_std_path`.
- **[M-172-nova-int-fallback-audit]** Plan 172 / conventions §1 «никаких авто-выводимых неверных типов». `emit_c.rs` имеет **~78** сайтов молчаливого fallback-типа (`_ => "nova_int"`, `unwrap_or(... nova_int)`) в путях вывода C-типа: при неизвестном типе codegen подставляет `nova_int` вместо резолва → **soundness-дыра** (маскирует ошибку типа: `if` на «int» вместо `bool`, мис-диспатч; всплыла на `self.try_start_won()` → `nova_int` при инлайне sync, см. [M-172.1-U1-lib-import-needs-U4]). Это симптом фрагментированного inference; **корректный фикс — U.4** (типизированный IR: чекер резолвит тип, codegen читает; genuinely-unresolvable → `[E_*]`-диагностика). НЕ патчить точечно в codegen (§0/§1). Audit: `grep -nE '_ => "nova_int"|unwrap_or.*nova_int' emit_c.rs`; каждый сайт при переносе на единый inference либо получает реальный тип, либо становится диагностикой. Priority: P1 (soundness). Gate: U.2→U.3→U.4.
- **[M-172.1-U2.3-synth-overlay]** Plan 172.1 U.2.3 (variant A; commits `930f3eda`/`12e492f6`/`0b225980`). Три контекста чекера (BoundCtx/CapabilityCtx/TypeCheckCtx) больше НЕ строят собственные `fn_decls`/`method_table` — читают ОДИН base-реестр `SigRegistry::build_base` (§0; три дублирующихся build-цикла устранены). Осознанный компромисс (F2): общий реестр = **base-only**; синтезированные auto-derive методы (Plan 126) остаются TypeCheckCtx-PRIVATE overlay `synth_methods` (НЕ в общем реестре — Bound/Cap не должны их видеть: их резолв base-only, byte-identical к до-рефактора). Поле `method_table` убрано из всех трёх; TypeCheckCtx сохраняет `synth_methods` (genuinely-unique забота auto-derive, НЕ дубль build-цикла). Полная унификация (вариант B: synth внутрь общего реестра + корпус-пруф, что Bound/Cap не затронуты) — возможный follow-up; A выбран как min-risk byte-identical. Priority: P3 (чистота §0; функционально завершено + byte-identical-верифицировано на 43 фикстурах зон риска вкл. plan126).
- **[M-172.1-U2.4-mangling-fragmented]** Plan 172.1 U.2.4 (разведка 2026-06-20). Исходная форма U.2.4 («standalone `SigRegistry` → populate `method_overloads` из неё») byte-identical НЕВЫПОЛНИМА: `method_overloads` строится из 5+ источников (ExternalRegistry builtins :2374 / free-fn D84 :3189 / receiver methods :3311 / embed-proxy D39 :3568 / mono :5650,:9560) с РАЗНЫМИ mangling-схемами — codegen использует `receiver_type_c_ident` + суффикс по ВСЕМ param-C-types (sanitized) + `__mut`/`__ro` tiebreak (Plan 135) + `erased_type_ref_c` (generic-recv) + `free_fn_c_name` (modular/file-priv/literal); SigRegistry (`mangle_method_c_name`+`last_param_suffix`) — упрощённая (last-param-Nova suffix, raw type, без mut/erasure/modular), совпадает лишь для single-overload concrete-recv (кейс parity-теста U.2.2). Плюс `ExternalRegistry::type_ref_to_c` (standalone) ≠ `CEmitter::type_ref_to_c` (state-aware). Корень: codegen mangling/type-map зависят от `CEmitter`-состояния (generic_types/mono/receiver-context/fn_module_map), которого нет у независимого реестра. Развилка: (1) строить SigRegistry ВНУТРИ CEmitter + единый mangler (целевая §0) / (2) вынести ОДИН shared mangler, источник не менять / (3) отложить U.2.4-impl за U.4/U.5 (typed IR) + U.6 (collapse `type_ref_to_c`×3); сейчас закрыть U.2.5 (fold MethodSig + del `resolve_overload`). Priority: P1 (§0 ядро). Gate: решение владельца + (для целевой) U.4/U.5/U.6.
- **[M-172-codegen-typedef-order-nondeterminism]** Pre-existing (обнаружено при U.2.3 byte-identical гейте, 2026-06-20). Codegen эмитит forward-typedef-блок (`typedef struct Nova_X Nova_X;`) в порядке HashMap-итерации → **порядок строк варьируется между запусками ОДНОГО бинаря** (подтверждено: 2 прогона одного `nova.exe` на одном входе дают разный порядок `Nova_U`/`Nova_F`/`Nova_K`). Семантически безвредно (forward-typedef порядок-независимы), но: (1) нарушает §2-детерминизм сборки; (2) ломает наивный byte-diff `.c` как verification-гейт (приходится сравнивать line-multiset, `diff <(sort a) <(sort b)`); (3) снижает эффективность `.c`-кэша (байт-идентичный вход → разный `.c`). Фикс: эмитить forward-typedefs в детерминированном порядке (BTreeMap / сортировка по имени / declaration-order items). Priority: P2 (детерминизм сборки + byte-identical-верифицируемость будущих рефакторов).

- **[M-169.2-vec-fn-empty-literal-nova-int]** `mut arr []fn() -> int = []` — пустой
  array-литерал для `[]fn` выводит element-type как **`nova_int`** (fallback), а не
  fn/void_p: codegen создаёт `Nova_Vec____nova_int_static_new()`, но `arr` типизирован
  `NovaArray_void_p*` и в него пушатся closure-указатели → type-confused контейнер.
  Малый N работает (совпадение layout), на масштабе (≥~512, realloc) расходится →
  элемент читается как null → `NOVA_CLOS_CALL_vi(null)` → детерминированный SEGV (READ@0,
  frame[1]=`nova_fn_main_impl`). **НЕ GC** (`GC_DONT_GC=1` не чинит). Это конкретный
  инстанс класса **[M-172-nova-int-fallback-audit]** (silent nova_int fallback на unknown
  element-type) → **гейтован на Plan 172 U.4** (removal of fallback). Репро: plan55
  `f1_closure_array_gc_stress` (RUN-FAIL 3/3); диагностика по docs/debugging-races.md §2.1.1.
  Priority: M (гейт 172 U.4).

## Plan 110.5.7 / D189 — errdefer/okdefer retraction cleanup

- **[M-172-errdefer-okdefer-dead-surface]** `errdefer`/`okdefer` ретракнуты (D189, Plan 110.5.7,
  hard cutover): парсер реджектит их диагностикой (`parser/mod.rs:9835-9850`). Остаточный мусор в
  трёх слоях. **(1) USER-FACING БАГ (P1):** диагностика `D133-not-consumed` строит
  machine-applicable suggestion с `errdefer` (`types/mod.rs:15306-15318`:
  `"errdefer {{ {name}.{cl}() }}\n{name}.{primary}()"`) — применение quick-fix даёт код, который
  парсер реджектит. Заменить на `defer`. **(2) Мёртвый AST+codegen (P3):** узлы `ErrDefer`/`OkDefer`
  (`ast/mod.rs:1842-1853`) недостижимы (парсер реджектит до их конструирования); keyword'ы
  `KwErrDefer`/`KwOkDefer` (`token.rs:143-149`) нужны ТОЛЬКО как tombstone для дружелюбной ошибки —
  оставить; но большой dead-codegen в `emit_c.rs` ~17518-18093 (DeferScope.is_error,
  error-path/success-path dispatch, okdefer-skip, hoist-for-errdefer) + ветки в
  may_gc/escape_analyze/interp — выпилить. **(3) Внутренние error-строки (P3):** `emit_c.rs:16462`
  + `:19092` ("defer/errdefer[/okdefer] outside defer scope") → убрать errdefer/okdefer из текста.
  Test-rot (stale-комменты про errdefer/okdefer в тестах) уже подметён осью 169.2.

## D13 — panic catchability (soundness)

- **[M-172-with-fail-swallows-panic]** `with Fail[E]`-handler **ловит `panic`** как
  recoverable-ошибку → **нарушение D13** (panic перехватывается ТОЛЬКО runtime'ом на
  границе fiber'а; «программист НЕ ловит panic в обычном коде», нет `try_panic`/`catch` —
  spec/decisions/08-runtime.md §«Три уровня катастрофы»). **Эмпирически подтверждено
  2026-06-20** (C-codegen): `panic("BOOM")` внутри
  `with Fail[E1] = effect Fail { fail(_e) { interrupt () } } { risky_panic() }` → with-блок
  отдаёт значение, выполнение продолжается. Сырой stdout = `PROBE\nREACHED_AFTER_HANDLER`,
  процесс жив (exit 0), `panic: BOOM` НЕ всплыл. Ожидалось: паника проходит сквозь
  Fail-handler до границы fiber'а — в синхронной CLI = смерть процесса с `panic: BOOM`.
  **Root cause:** re-dispatch ветка Fail-handler'а (`emit_c.rs:6648-6675`) ре-throw'ит
  ТОЛЬКО `NOVA_THROW_CANCEL`; `NOVA_THROW_PANIC` проваливается в «USER path: handler already
  ran» → паника проглатывается (а CANCEL — единственный структурный throw, который
  корректно пробрасывается). **Фикс:** добавить симметричную ветку `if (ff.error_kind ==
  NOVA_THROW_PANIC) { nova_fail_pop(); nova_interrupt_pop(); restore handlers; nv_panic(ff.error_msg); }`
  ПЕРЕД USER-path (NB: `supervised{}` ДОЛЖЕН продолжать ловить panic для restart — это
  ОТДЕЛЬНАЯ граница, не трогать). Priority: **P1** (soundness — panic recoverable вопреки D13).
  Репро (scratch, удалён — пересоздать при фиксе как `EXPECT_RUNTIME_PANIC BOOM`):
  ```nova
  module nova_tests.<stem>
  type E1 { msg str }
  fn risky_panic() Fail[E1] -> () { panic("BOOM") }
  fn main() -> () {
      println("PROBE")
      with Fail[E1] = effect Fail { fail(_e) { interrupt () } } { risky_panic() }
      println("REACHED_AFTER_HANDLER")   // НЕ должно печататься после фикса
  }
  ```
