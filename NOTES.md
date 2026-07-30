# NOTES — план 196: `gs` несёт бАУНДЫ (носитель GenericScope)

Модель: sonnet. Ветка `p196-gs-migration`, worktree `D:/Sources/nv-lang/nova-gs`.

## Что сделано

### Шаг 1 (коммит `25889de89`) — носитель + механическая миграция
- Введён `GenericScope = HashMap<String, GenericParam>` (types/mod.rs) — по образцу
  уже работающего прецедента `current_fn_generics` (RefCell<Vec<GenericParam>>).
- Trait `GenericNameSet { has_generic_name }` — общий для старого `HashSet<String>`
  (method-level generic-множества: `method_names`/`self_only`/`recv_generic_names` —
  ДРУГАЯ сущность, НЕ мигрируется) и нового `GenericScope`, чтобы
  `typeref_mentions_any`/`mark_type_params`/`unify_type`(const_fn_trampoline.rs)/
  `closure_arg_return_peek` не раздваивали код обхода.
- Все 37 сигнатур `gs`/`expr_gs`/`exp_gs: &HashSet<String>` → `&GenericScope`
  (types/mod.rs) — механическая замена (sed) + читающие сайты (`gs.contains` →
  `gs.contains_key`, `~16` сайтов).
- `fn_generic_scope` + 6 inline-мест популяции (f1_check_fn ×2 (одно — дубль
  вручную, отдельно от вызова fn_generic_scope), walk_type_decl,
  check_direct_value_cycle, callee_gs в overload_applicability, 4 пустых test/const
  сайта) строят `GenericScope` вместо `HashSet`.

### Шаг 2 (текущий) — польза: `resolve_generic_bound_receiver_method`
Новая функция (types/mod.rs, рядом с `resolve_prefix_generic_method_return`)
консультирует `gs[T].bounds` при резолве instance-method-call, чей RECEIVER —
голый generic-параметр-в-скоупе (`v: T`), когда ни `method_overloads`, ни
`self.types.get(type_name)`, ни `resolve_prefix_generic_method_return` не нашли
метод. Обобщает УЗКИЙ прецедент `resolve_generic_bound_method_return`
(match-scrutinee-only, через `current_fn_generics`) на ЛЮБОЙ instance-call.

Threading: `resolve_instance_method_return_arity` получил параметр
`gs: &GenericScope`; `infer_method_call_channel_type` — тоже (пробрасывается из
`f1_check_call`, единственного места с реальным `gs` в этой цепочке, плюс
собственный рекурсивный вызов).

**ВАЖНАЯ НАХОДКА (regression, зафиксирован и закрыт в этом же окне):** первая
версия функции резолвила ЛЮБОЙ protocol-bound, включая ПАРАМЕТРИЧЕСКИЙ
(`D355Source[T]`, Plan 161/D355 blanket dispatch,
`spec_tests/conformance/d355_blanket_protocol.nv`). Это дало CC-FAIL:
`pos_option_debug.c:164219/164240` — `NovaOpt_nova_int` инициализирован
`NovaOpt_nova_str` (кросс-моно-инстанс мешанина), т.к. D355's `T` — БАУНДА
собственный inferred type-arg, не ключ ВНЕШНЕГО `gs` → `mark_type_params` не
распознаёт residual `Named("T")` как TypeParam → канал пишет ложный "конкретный"
тип. **Исправлено**: функция теперь консультирует ТОЛЬКО непараметрические
protocol-бАунды (`td.generics.is_empty()` — `Debug`/`Display` и подобные,
0 собственных generics), где return-тип метода не может упомянуть ничего, кроме
имён, УЖЕ бывших в `gs`. Параметрические бАунды (D355-класс) — вне окна, честно
не покрыты (декларируется в отчёте).

## RED/GREEN — эмпирическая проверка

Фикстура `spec_tests/conformance/pos_option_debug.nv` (Some(int)/Some(str)/nested
`${x:?}` — вызывает `Option[T Debug]@debug`'s внутренний `v.debug(f)` на голом
generic-ресивере `v: T`).

- **ДО** (коммит `25889de89`, носитель мигрирован, но `resolve_generic_bound_receiver_method`
  ЕЩЁ НЕ существует): `NOVA_CALL_TRACE=debug` → `[AP-MISS] ... obj=i:v(true)` при
  каждом вызове `v.debug(f)`/`e.debug(f)` внутри Option/Result `@debug` — канал
  МИМО (Channel 2 miss), см. `docs/plans/196-one-truth-closeout.md` B11q root-cause.
- **ПОСЛЕ** (текущий HEAD): проверить заново — команда:
  ```
  NOVA_CALL_TRACE=debug ./nova-cli/target/release/nova.exe test \
    spec_tests/conformance/pos_option_debug.nv
  ```
  (env `NOVA_GC_LIB_DIR`/`NOVA_GC_INCLUDE_DIR` → main-repо vcpkg_installed,
  worktree их не содержит).

### Шаг 3 — НАЙДЕН И ЗАКРЫТ regression (флагман сломался, потом почищен)

Первая версия payoff-функции резолвила ЛЮБОЙ non-parametric protocol bound, включая
методы, возвращающие `Self` внутри COMPOUND carrier (`Deserializer.enter_field(key) ->
Result[Self, DeError]`, std/src/encoding/serde/serde.nv). Флагман (nova-polaris,
`.deser_int`-семья через `enter_field(..)?`-цепочку) СЛОМАЛСЯ:
`[E_RECV_METHOD_MISMATCH]` — receiver мискатегоризован (сначала как `[]T`, после
попытки Self-substitution — как `DeError`). Корень: substituting `Self` ВНУТРИ
compound carrier взаимодействует с `?`-operator carrier-unwrap машинерией непонятным
для этого окна образом (глубже одного фикса).

**Решение (сузил охват, а не залатал):** payoff-функция substitute'ит `Self` ТОЛЬКО
когда он — ГОЛЫЙ прямой возврат (`-> Self`, зеркалит соседнюю ветку в том же файле);
`Self` ВНУТРИ compound carrier (`Result[Self,E]`/`Option[Self]`/…) → `continue`
(следующий bound / None) — тот же путь, каким эти вызовы шли ДО появления этой
функции (легаси, без регрессии). Зафиксировано через `typeref_mentions_any` +
singleton-`GenericScope` пробник `Self::self_only_gs()`.

**Подтверждено ПОСЛЕ сужения:**
- Флагман: `nova build examples/flagship/aggregator/src/main.nv --strict-effects` →
  `built: D:\Sources\nv-lang\nova-gs\main.exe` (0 ошибок, было
  `[E_RECV_METHOD_MISMATCH]` до фикса).
- `nova check std/src` → `PASS: 147 FAIL: 26` (целевое число, без изменений).
- Целевая фикстура (`pos_option_debug.nv`+`d355_blanket_protocol.nv`, один CU):
  `NOVA_CALL_TRACE=debug` — AP-MISS для `obj=i:v(true)`/`i:e(true)` (внутренний
  `v.debug(f)`/`e.debug(f)` вызов) БОЛЬШЕ НЕ ФИГУРИРУЕТ в трейсе (было ДО фикса) —
  канал резолвит его теперь. d355 (parametric-bound, вне охвата) остаётся зелёным
  без изменений (не задет).
- `arch-ratchet.sh` / `check-marker-registry-sync.sh` — зелёные без правки baseline.

**Урок для отчёта:** изначальная реализация была ПОСПЕШНОЙ (не учла compound-Self
carrier) — обнаружено ТОЛЬКО благодаря прогону реального флагмана (не просто
изолированной фикстуры). Подтверждает конвенцию: изолированный repro НЕ
гарантирует отсутствие регрессии на живом corpus (`feedback-isolate-conformance-
before-push`).

## Хвосты (честно, что осталось)
- Легаси-ветки B11q/B11r/B10m: снос НЕ делался в этом окне (не было целью данного
  окна, см. задание п.4). Резолв ПОКРЫВАЕТ non-parametric protocol bound с ГОЛЫМ
  или НЕ-Self возвратом на ЛЮБОМ instance-call-receiver (шире прежнего match-
  scrutinee-only прецедента `resolve_generic_bound_method_return`), но НЕ покрывает:
  parametric-bound (D355-класс, по конструкции декларировано вне охвата),
  method-level generics на bound-методе, compound-Self-carrier возвраты
  (Deserializer.enter_field-класс). Так что B11q/B11r НЕ доказаны мёртвыми — это
  расширение канала, а не снос легаси-ветки.
- Полный официальный мега-CU (`spec_tests/conformance` целиком, 1068 файлов,
  authoritative gate) НЕ прогонялся целиком (по прямому указанию задания —
  «мега-CU целиком НЕ гонять»); прогнаны точечные фикстуры + `nova check std/src`
  + флагман, все зелёные.
