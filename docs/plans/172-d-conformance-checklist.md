# Plan 172 — D-conformance closure-checklist (V-трек = U.7 расширенный)

> spec_tests/conformance тесты на 172.1-172.5-релевантные D-блоки. Проходящий = D конформен (часть 172 закрыта);
> падающий/отсутствующий = gap. Против ТЕКУЩЕЙ spec-семантики (амендменты inline; forward-compat D — carrier-level).
> **Авторинг ТРЕБУЕТ качества** (агентская пачка 2026-06-28 дала невалидный Nova — refutable-pattern/D133-misuse/E7301;
> discarded). Каждый тест — вручную/careful, валидный синтаксис (examples/ + spec/), D-prefixed уникальные типы.

| D | covered | kind | поведение / test-идея |
|---|---|---|---|
| D119 | ⬜ gap | pos | Method-level type-параметры в generic methods: метод имеет собственные type-params независ … → type D119Box[T] { inner T }; fn D119Box[T] @map[U](f fn(T) -> U) -> D119Box[U] => D119Box[ |
| D123 | ⬜ gap | both | Concrete tuple `(T1..TN)` мономорфизируется в struct с REAL field types (не int-slot erasu … → `ro p (str, int) = ("a", 1)`; assert(p.0 == "a"); assert(p.1 == 1). Nested: `ro n ((int, i |
| D125 | ⬜ gap | pos | `byte` удалён; единственное каноническое имя 8-битного unsigned — `u8`; срез байт — `[]u8` … → `ro b u8 = 255`; assert(b == 255); `ro z u8 = 0`; assert(b > z). Опц. метод-ресивер на u8  |
| D128 | ⬜ gap | pos | `char` distinct от `int`: nova_char = uint32_t (codepoint, U-суффикс). Generic mono различ … → `ro c char = 'A'`; assert(c as int == 65); `ro n char = 0x10FFFF as char`; assert(n as int |
| D129 | ⬜ gap | pos | `int` = alias `i64` (64-bit signed); оба → nova_int (намеренный alias, НЕ collapse-баг). З … → `ro a int = -5`; `ro b int = 3`; assert(a < b); assert(a + b == -2); `ro c i64 = -5`; asse |
| D130 | ✅ | pos | `uint` — unsigned 64-bit (alias u64), маппится в nova_uint во ВСЕХ позициях включая method … → ПОКРЫТО: d130_uint_method_compare.nv (метод-ресивер unsigned compare, K1) + d130_uint_lite |
| D156 | ⬜ gap | both | Generic `[T consume]` bound — opt-in strict mode: внутри generic-body silent-forget T → co … → POS: type D156Tx consume { id int } + fn D156Tx consume @done() -> (); fn[T consume] d156_ |
| D215 | ⬜ gap | both | Named tuple — stack-allocated VALUE type с именованным доступом (`.x`,`.y`); конструируетс … → type D215Vec3(x f64, y f64, z f64); `ro v = D215Vec3(x: 1.0, y: 2.0, z: 3.0)`; assert(v.x  |
| D216 | ⬜ gap | pos | Generic anonymous tuple `(T,U)` мономорфизируется per instantiation с REAL element types ( … → fn[T] d216_dup(v T) -> (T, T) => (v, v); `ro (a, b) = d216_dup[int](42)`; assert(a == 42 & |
| D226 | ⬜ gap | pos | Signed indexing convention: все API len/capacity/index — signed `int` (= i64), не uint. Ра … → `mut v []int = [10, 20, 30]`; assert(v.len() == 3); assert(v.len() - 1 == 2); `mut e []int |
| D239 | ⬜ gap | pos | `[]T` — синтаксический псевдоним `Vec[T]`; компилятор разворачивает `[]T`→`Vec[T]` на type … → `mut a []int = [1, 2, 3]`; assert(a[0] == 1); assert(a[2] == 3); fn d239_first(xs []int) - |
| D310 | ⬜ gap | both | Type-set bounds: `type Name set M1 / M2 / ...` — именованное множество конкретных типов (п … → type D310Ints set i32 / i64 / int; fn[T D310Ints] d310_twice(x T) -> T => x + x; assert(d3 |
| D315 | ⬜ gap | pos | ResolvedType — ЕДИНЫЙ канонический носитель типа: несёт полную семантическую личность (res … → Наблюдаемое следствие lossless width/sign: `mut a []u32 = [1, 2, 3]`; assert(a[0] == 1); ` |
| D326 | ⬜ gap | both | `ref` — режим передачи параметра (borrow), НЕ тип. `mut ref` — in-out (callee пишет в call … → value-record mut @ fluent (172.4/172.5 acceptance): type D326Counter value { n int }; fn D |
| D327 | ⬜ gap | pos | Unicode codepoint (0..0x10FFFF) — `u32`, НЕ `int` (категория значение-идентификатор, отлич … → fn d327_is_ascii_upper(cp u32) -> bool => cp >= 0x41 && cp <= 0x5A; assert(d327_is_ascii_u |
| D328 | ✅ | pos | Value-record `==` — СТРУКТУРНОЕ (field-by-field, нет heap-identity), через единый emit_fie … → ПОКРЫТО: d328_value_record_eq.nv (== структурное на Point, != негация). Тип Point из types |
| D52 | ⬜ gap | pos | Единый keyword `type` для всех data-форм; форма различается ПЕРВЫМ токеном после имени: ne … → type D52Meters int (newtype), type D52Color / Red / Green / Blue (sum, auto-disc 0/1/2), t |
| D54 | ⬜ gap | both | `as` — compile-time конвертация (numeric cast, newtype↔underlying, sum→int) с DEFINED beha … → assert((300 as u8) == 44) — iN→uM wraparound (300 mod 256); assert((-1.0 as u16) == 0) — f |
| D55 | ⬜ gap | both | Literal coercion в позиции с явным целевым типом T: (1) sum-coercion (значение S оборачива … → type D55Wrap / D55S(str) / D55I(int); `ro a D55Wrap = 25` → coerced в D55I(25); match a {  |
| D72 | ⬜ gap | pos | Generic bounds через `[T Protocol]` — protocol-тип как bound (universal/static mono) либо  … → type D72Show protocol { show() -> str }; type D72Item { } + fn D72Item @show() -> str => " |

**Покрыто (committed, passing):** D130 (d130_uint_method_compare + d130_uint_literal_width — K1/K4), D328 (d328_value_record_eq — Ф.2).
**Gaps (careful authoring TODO):** все ⬜ выше. Идеи энумерированы (workflow wl7ffiqz3); реализация — вручную по одному, с проверкой компиляции.
**Критерий приёмки V:** все 172-D ✅ + suite зелёный + legacy удалён (D/P67) → движок конформен спеке/D = зонт 172 закрыт.