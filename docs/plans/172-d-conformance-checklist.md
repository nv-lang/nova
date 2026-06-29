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
## Триаж gaps (2026-06-28, isolated) — driver-роль suite
Падающие D-тесты классифицированы (test-bug vs компилятор-gap по конвенции владельца):
- **d326** value-record fluent `mut @ -> @` → **КОМПИЛЯТОР-GAP = 172.4 Ф.3** (carrier-model refactor; D-тест ПОДТВЕРЖДАЕТ Ф.3-блокер → драйвит F-фазу).
- **d54** `-1.0 as u16 == 0` (float→uint saturation, D54/D130 neg→0) → **вероятно компилятор-GAP** (saturation для u16 не работает; `300 as u8 == 44` wrapping работает) → база (реализовать saturation). Verify по D54/Plan07.
- **d55** `ro a D55Wrap = 25` (sum-coercion литерала, D55) → E7301. D55Wrap = 2 unary-ctor (D55S/D55I) → уточнить: D55 требует type-directed coercion (int→D55I) = gap, ИЛИ single-ctor-only = test-bug.
- **d52** `Red as int == 0` (sum auto-disc) → assert-fail. Уточнить: специфицирует ли D52 `variant as int` = discriminant (вероятно test-bug — niche cast).
- **d123** refutable variant-pattern в `ro` → **TEST-BUG** (нужен `match`/`if let`). Фикс теста → merge.
- **d156** consume-var `t2` не consumed → **TEST-BUG** (тест должен consume). Фикс теста → merge.

**Итог driver-роли:** V-трек нашёл 1 подтверждённый компилятор-gap (d326→Ф.3) + 2 кандидата (d54/d55) +
2 test-bug (d123/d156) + 1 уточнить (d52). Компилятор-gaps → база (172.4 Ф.3 + saturation/coercion); test-bugs → фикс+merge.

## Уточнённая классификация gaps (2026-06-28) — компилятор-gaps = base-задачи
V-трек выявил, что несколько D НЕ полностью реализованы (compiler-gaps, не test-bugs) → база:
- **d54 → КОМПИЛЯТОР-GAP:** `-1.0 as u16` не сатурирует в 0 (D130 neg→0 / Plan07 float→int saturation;
  `300 as u8 == 44` wrapping работает). Реализовать saturation float→narrow-uint. Verify по D54/Plan07.
- **d52 → КОМПИЛЯТОР-GAP:** `Red as int` не даёт discriminant 0 (D52 sum→int cast, 02-types:331-339).
  Реализовать sum-variant→int cast = discriminant.
- **d55 → КОМПИЛЯТОР-GAP (вероятно):** sum-coercion литерала `ro a D55Wrap = 25`→D55I(25) (D55) не работает
  (E7301). Уточнить: D55 type-directed при 2 unary-ctor. Реализовать literal sum-coercion.
- **d326 → 172.4 Ф.3** (value-record fluent, carrier-refactor) — F-фаза.
- **d156:** generic [T consume] obligation через consume-method (D156) — gap или test-bug, careful analysis.
- **d123:** ✅ FIXED (test-bug, tuple-annotation→inference) + merged.

**Driver-итог:** V-трек = backlog base-фич (D54 saturation, D52 sum-cast, D55 coercion, D156 consume,
D326/Ф.3). Каждая реализуется careful по §0/§5/§7 ВМЕСТЕ с merge своего D-теста в conformance.

## Точная диагностика As-cast компилятор-gaps (2026-06-28) — ready base-задачи
- **d52 sum→int cast:** `Red as int` эмитит `((nova_int)(c))` (emit_c.rs As-cast :21714) — кастит УКАЗАТЕЛЬ
  Nova_Color*, НЕ discriminant. Spec (02-types:331-334) требует disc (0/1/2). ФИКС: в As-cast детектить
  sum-source → emit discriminant-extraction (tag) per sum-ABI, не generic pointer-cast. d52-CORE
  (newtype+sum-resolve) смержен зелёным; sum→int — отдельный gap-тест ВМЕСТЕ с фиксом.
- **d54 float→narrow-uint saturation:** Plan07 saturation ЕСТЬ (As-cast :21745, helper nova_<src>_to_<dst>),
  но для f64→u16 helper отсутствует → падает в C-wraparound (`-1.0 as u16`=65535≠0). ФИКС: добавить
  saturation-helper'ы для narrow-uint (u8/u16/u32) в nova_rt/cast.h + детект в As-cast.
Оба — careful As-cast правки (regression-риск в шаренном cast) → база с fresh-focus, НЕ хвост-сессии.

## d55.1 sum-coercion — ПОДТВЕРЖДЁН КОМПИЛЯТОР-GAP (2026-06-28)
`ro a SingleCtorSum = <value>` (D55.1: значение S → единственный unary-ctor C(S)) НЕ реализован —
E7301 даже для single-ctor sum (`type Box | Val(int); ro a Box = 25` → ошибка). d55 merged частично
(D55.2 record-coercion + D55.4 numeric работают). ФИКС D55.1: чекер должен в let/param-позиции с явным
sum-типом T с РОВНО ОДНИМ unary-ctor — coerce литерал/значение в C(value). База.

## d156 generic-consume — careful analysis (uncertain)
`consume t2 = d156_id(t); t2.done()` → D133-not-consumed на t2 (тип T). Чекер видит t2 как generic T,
consume-метод done() на конкретном D156Tx не трекается как consuming. Gap (generic-consume tracking
через id-return) ИЛИ test-bug — нужен careful consume-checker анализ. Документирован, не классифицирован.

## d54 — TEST-BUG (прецедентность), НЕ компилятор-gap (2026-06-28, ИСПРАВЛЕНО)
Все D54-касты КОНФОРМНЫ (saturation neg→0/positive→max, wraparound, int→float). Helper cast.h корректен.
Провал был ПРЕЦЕДЕНТНОСТЬ: `-1.0 as u16` парсится как `-(1.0 as u16)` = `-(1)` u16 = 65535 (`as` биндит
ТУЖЕ унарного минуса). d54 merged со скобками `(-1.0) as u16` → suite green (4 каста).
⚠️ FINDING ДЛЯ ВЛАДЕЛЬЦА (spec/design, не блокирует): прецедентность `as` > унарный `-` ПРОТИВОПОЛОЖНА
Rust (там унарный минус туже `as`). `-1.0 as u16` читается неестественно. Решить: (a) сделать унарный
минус туже `as` (parser, как Rust) — behavior-change; (b) оставить + задокументировать в D-спеке. НЕ менял.

## d52 sum→int — ФИКС РЕАЛИЗОВАН (2026-06-28), в дереве, ждёт combined-regress
emit_c.rs As-cast (после saturation-блока): pointer-sum (`Nova_X*`, sum_schemas) `as int-family` →
`((target)((v)->tag))` (discriminant), не generic `(nova_int)(pointer)`. Verified: `Red as int==0,
Green==1, Blue==2` PASS (target_alt). Combined full-regress (с ARM2) → commit → merge d52 sum→int тест.

## d55.1 sum-coercion + d156 generic-consume — involved/uncertain base-задачи (2026-06-28)
**d55.1 (компилятор-gap, involved, 2 сайта):**
- Checker `assignable` (types/mod.rs:9424): после Any-check добавить pre-check — если `expected` =
  single-unary-ctor sum + `assignable(expr, ctor_param_ty)` Ok → return Ok. НО checker имеет только
  `sum_variant_names` (имена, BoundCtx:11329), нужен `single_unary_ctor_of(T)→(ctor,param_ty)` из
  sig/type-decls (sum-структура).
- Codegen decl-value: обернуть значение в `nova_make_<sum>_<ctor>(value)` когда target = single-ctor
  sum (прецедент record-coercion emit_c.rs:5328). Full-regress. (d55.2/.4 уже merged.)

**d156 (GAP — classified 2026-06-29: consume-checker generic-return-subst):** `consume_methods` keyed по
КОНКРЕТНОМУ типу (mod.rs:16423); для `consume t2=generic_call()` checker держит t2 как generic T, return-type
subst (T→концрет via `resolve_instance_method_return`) НЕ применён к типу consume-var → consume-method `done()`
не в `consume_methods["T"]` → не трекается → D133. FIX: в consume-flow применить return-subst к типу consume-var
ПЕРЕД linearity-проверкой. Involved (consume-checker sensitive → §7-verify). Исходный симптом: `consume t2 = d156_id(t); t2.done()` → D133 not-consumed на t2.
Consume-checker видит t2 как generic T; consume-метод done() на конкретном D156Tx не трекается как
consuming через id-return. Нужен careful анализ consume-checker (где трекаются obligations + распознаётся
ли method-call-consume на generic-typed binding). НЕ классифицирован.

**V-трек триаж ЗАВЕРШЁН (6 gaps):** d52✅FIXED(codegen) · d54✅FIXED(test-bug прецедентность) ·
d123✅FIXED · d55.1(involved base) · d156(uncertain) · d326(=172.4 Ф.3).

## d156 — ПОДТВЕРЖДЁН КОМПИЛЯТОР-GAP (2026-06-28, control-test)
Контроль: `id_concrete(consume x D156Tx) -> D156Tx` (НЕ generic) + `consume t2 = id_concrete(t); t2.done()`
→ **PASS**. Generic `d156_id[T consume](consume x T) -> T` + тот же паттерн → fail (D133 not-consumed на t2).
**Механизм:** consume-checker НЕ подставляет generic-return `T`→concrete (D156Tx) при `consume t2 =
<generic-call>` → t2 записан как тип `T` → `t2.done()` не резолвится (done() в `consume_methods["D156Tx"]`,
не `["T"]`) → obligation не satisfied. ФИКС: consume-checker должен инферить concrete-return generic-вызова
(подставить inferred type-args в return-тип) перед записью типа consume-binding. Careful base (checker).

**V-трек триаж ОКОНЧАТЕЛЬНО (6/6 definitive):** d52✅FIXED(codegen sum→int) · d54✅FIXED(test-bug
прецедентность) · d123✅FIXED(test-bug) · d55.1 КОМПИЛЯТОР-GAP(checker+codegen coercion, involved) ·
d156 КОМПИЛЯТОР-GAP(consume-checker generic-return subst, confirmed) · d326=172.4 Ф.3.
V-трек как DRIVER: нашёл 4 компилятор-gap (1 fixed: d52) + 2 test-bug (fixed) → база подхватывает d55.1/d156/d326.

## Уроки V-трек workflow-автоматизации (2026-06-28) — критично для будущих прогонов
1. **Workflow-агенты ПИШУТ файлы на диск**, даже когда промпт просит «верни контент». Batch-workflow
   (51 D) написал 43 НЕВЕРИФИЦИРОВАННЫХ draft-файла прямо в spec_tests/conformance/ → folder-module
   (один CU) сломался (duplicate-top-level + битый синтаксис). ВПРЕДЬ: явно «НЕ создавай файлы, только
   верни nv_content» В ПРОМПТЕ, ИЛИ ожидать запись + чистить untracked после.
2. **Проверять РЕАЛЬНЫЙ dir (tracked+untracked) перед author/merge.** COVERED-список нельзя
   предполагать — derive из `git ls-files` + `git status`. Мой неполный список (19 vs реально больше)
   → workflow дублировал D32/D15 → клэш. D32-инцидент: смержил без full-suite verify → revert.
3. **НЕ запускать конкурентные nova.exe-heavy задачи** (полный регресс + salvage/workflow). Контеншн →
   transient CODEGEN-timeout'ы = ФЛАКА (d52-регресс показал 3 ложные «регрессии» plan170/plan172_*,
   все PASS в изоляции на обоих бинарях). «флака≠регрессия» — верифицировать аномалии в изоляции.
4. **Salvage-протокол:** staged drafts → инкрементально add в conformance → full folder-module compile →
   keep зелёный (FAIL:0, без error/CODEGEN/CC/RUN-FAIL) / reject битый. Clash-aware (реальный peer-модуль).

## d55.1 sum-coercion — codegen-сайт скоуплен (2026-06-28, прецедент record-coercion)
Record-coercion (D55.2) реализован через `self.expected_record_type = struct_name_from_c_type(ty_c)`
ПЕРЕД `emit_expr(value)` (emit_c.rs:5331-5334) — анонимный record-литерал подхватывает тип из target.
**Аналог для D55.1 sum-coercion:** на decl-value emission, если declared-target = single-unary-ctor sum
И значение типа ctor-param → обернуть emitted-value в `nova_make_<sum>_<ctor>(value)` (или
expected_sum_coercion-хинт по образцу expected_record_type). ПЛЮС checker `assignable`:9424 pre-check
(после Any-check) + helper `single_unary_ctor_of(T)→(ctor,param_ty)` из sum-структуры (checker имеет
только sum_variant_names:11329 — нужен fuller decl-доступ). Оба сайта скоуплены → careful base-фикс.

## V-трек batch итог (2026-06-28): 56 D-блоков covered, 8 rejected триажированы
**Salvaged+committed (e92a3a1a): 36 новых D** → suite 20→56. **8 rejected (isolated-триаж):**
- DRAFT-BUGS (CC-FAIL в изоляции, invented/invalid синтаксис — discard/re-author): d42, d53, d222, d284.
- ПОТЕНЦИАЛЬНЫЕ КОМПИЛЯТОР-GAPS (валидный parse, codegen/runtime не тянет — investigate per-draft):
  • d232 vec_growable_array (CODEGEN-FAIL) • d282 blanket_protocol_receiver (CODEGEN-FAIL)
  • d299 as_slice_append (CODEGEN-FAIL) • d281 module_priv_field (RUN-FAIL).
  Next: прочитать draft + D-норму → correct-per-D? → фикс компилятора (driver) / draft-bug.
**Остаток uncovered (75−56 в 02-types): ~D17/D110/D126/D133/D156(gap)/D163/D164/D181-186(refs/172.5)/
D277/D326(Ф.3)** — refs-семейство = 172.5 (не реализовано); D156/D326 = известные gaps; остальные — next batch.

## V-трек batch2 (2026-06-28) — 17 uncovered D → 10 usable → verify: 8 KEEP + 2 компилятор-gap
**Workflow wvxibfvv6** (Write-запрещён → 0 pollution, .of-конвенция). Verify isolated на main:
- ✅ **8 KEEP (готовы к merge в conformance после ARM 3+4 + folder-module clash-check):**
  d17 (type-decls/record/punning/partial-match), d42 (structural protocol satisfaction),
  d222 (priv record-protocol boundary), d232 (vec growable array), d281 (module priv field),
  d282 (blanket protocol receiver), d284 (enumerate index invariant), d299 (as_slice append).
- ❌ **2 КОМПИЛЯТОР-GAP (V-трек driver-находки → base-фиксы):**
  • **D53** anon-protocol-тип в позиции параметра (`fn f(x protocol { @sig })`) — CC-FAIL в 2
    независимых авторингах (batch1+batch2) → компилятор не поддерживает anon-protocol-param.
    Spec §674-695 (симметрично []T/(A,B)/fn()->T). Фикс: codegen anon-protocol-param-типа.
  • **D277** by-value mono generic value-records (Plan 153.2) — void*-return CC-FAIL (type-param
    erasure, связан с bail-gap §0-долгом). Фикс: generic value-record mono codegen.

**База compiler-fix backlog (V-трек-driven, двусторонняя конвергенция):** d55.1 (sum-coercion) ·
d156 (consume generic-return) · d326 (=172.4 Ф.3) · **D53** (anon-protocol-param) · **D277** (generic
value-record mono). Все — починить компилятор по конвенции (correct-per-D + CC-FAIL = gap, не draft-bug).

## V-трек NEG-batch (2026-06-29) — 10 D → 9 vetted NEG-фикстур + новый компилятор-gap
**Workflow wxbi7g3ya** (Write-запрещён; агенты ЭМПИРИЧЕСКИ тестили nova-codegen check/test-build →
высокая надёжность). Staged (vneg/): D54 (int as bool ✗), D55 (lit out-of-range), D227 (i32 overflow),
D32 (param-mut→method), D36, D175 (ro-freeze), D52, D128 (char/int), D246. Verify (--compile-error) + merge
в neg/ folder-module после ARM 3+4.
🔬 **NEG driver-находка — компилятор-gap D55.4-field-range:** range-check (E_LIT_OUT_OF_RANGE) НЕ
распространён на record-field позиции. `ro a u8 = 300` ловится (прямой IntLit, assignable:9272), но
`{ n: 300 }` в u8-поле — НЕТ (RecordLit→`_=>{}`:9390, field-walk с expected=None не сверяет поле против
типа поля). §0/§1-долг (silent-accept неверного значения). Spec 02-types:1011 подтверждает field-coercion
range-check gated. → base-фикс: распространить range-check на record-field/element позиции.

**База compiler-fix backlog (V-трек-driven, расширен):** d55.1 · d156 · d326 · D53 (anon-protocol-param) ·
D277 (generic-value-record mono) · **D55.4-field-range** (NEG-находка).
