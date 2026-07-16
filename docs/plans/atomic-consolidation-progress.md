# Консолидация atomic-семейства (решение владельца 2026-07-16)

## Статус: выполнено (сборка + точечная верификация; полный conformance — оркестратор)

Ветка: `p207-cmpxchg-rename`, worktree `d:/Sources/nv-lang/nova-cmpxchg`.
Продолжение этапа A (haiku, коммит `5b87737d8`: `__cas_raw`→`cmpxchg` +
схлопывание перегрузок compare_exchange в default SeqCst).

### Шаг 0 — верификация этапа A

- Пересобран компилятор (`compiler-codegen` + `nova-cli`) с нуля тем же
  бинарём worktree — `cargo build --release` в обоих крейтах, чисто (те же
  51/3 pre-existing warnings, ноль новых).
- Обнаружено и исправлено: libuv submodule не был инициализирован в
  worktree (`git submodule update --init` недоступен — нет `.git` в
  подмодуле) → скопирован контент из `d:/Sources/nv-lang/nova/compiler-codegen/nova_rt/libuv`
  + удалён `.git` (см. `project-worktree-nova-test-setup.md`).
- `nova test std/src/runtime` → **PASS: 3, FAIL: 0** (`fmt_buf`,
  `string_builder_test`, `sync_test`).
- Фикстура `2-арг`/`3-арг (позиционный success)`/`4-арг` `compare_exchange`
  на `AtomicI64` — все три формы компилируются и рантайм корректен
  (проверено отдельным temp-модулем `scratch_cas/cas_fixture.nv`, удалён
  после проверки).
- Линковка подтверждена фактически: бинарь скомпилирован из
  `compiler-codegen/nova_rt` ЭТОГО worktree (переименованный
  `sync_primitives.h`), не из главного репо — подтверждено grep'ом
  символов `Nova_AtomicInt_method_cmpxchg` в собранном хедере worktree.

### Шаг 1 — консолидация имён

1. **AtomicIsize → AtomicInt.** Legacy `AtomicInt` (int32-precision, узкий
   API без `MemOrdering`) снят целиком — его 4 живых потребителя
   (`std/src/net/tcp.nv` `rc`-поле, `std/src/concurrency/supervisor_test.nv`,
   `std/src/testing/handlers.nv`, `std/src/runtime/sync_test.nv`) используют
   только `new/load/store/fetch_add/fetch_sub/compare_exchange` без
   ordering — 100% покрыто дефолт-параметрами переименованного типа,
   миграция звонков НЕ ПОТРЕБОВАЛАСЬ (то же имя `AtomicInt`, тот же call
   shape). GC-roots потребителей НЕ БЫЛО (AtomicPtr, не AtomicInt — см. п.3).
2. **AtomicUsize → AtomicUint.** Прямое переименование, конфликта имён не
   было. 2 живых потребителя (`std/src/runtime/sync_test.nv`,
   `spec_tests/conformance/plan103_2_atomic_isize_usize.nv`) — мигрированы.
3. **AtomicPtr — снят.** Потребители — ТОЛЬКО тесты (3 spec_tests файла,
   0 в std/ production, 0 в examples/). ⚠️ Ключевая находка: собственный
   `.nv` doc-comment типа заявлял «GC tracking: AtomicPtr registers its
   value with GC roots on store» — но фактическая C-реализация в
   `sync_primitives.h` НИКОГДА не регистрировала GC roots (голый
   `__atomic_*` над `nova_int`, без единого вызова root-API,
   grep `root` по файлу — ноль совпадений). Живого потребителя,
   хранящего GC-managed адрес через `AtomicPtr` и полагающегося на
   root-регистрацию, НЕ НАЙДЕНО — доклад оркестратору не требуется (условие
   «если найдётся — стоп» не сработало). Тип снят; 2 test-файла удалены
   вместе с типом (`plan103_2_atomic_ptr_basic.nv`,
   `neg/plan103_2_atomic_ptr_no_such_op_neg.nv` — негативный тест проверял
   несуществующий метод НА AtomicPtr, бессмыслен после снятия типа); 1 файл
   (`d168_sized_atomics.nv`) только упоминал в комментарии — поправлен.
4. `RUNTIME_DEFINED_TYPES` (emit_c.rs:27-75) — обновлён. Дополнительно
   найдены и обновлены ЕЩЁ 3 независимых name-list'а с тем же семейством
   (историческая accretion, не консолидированы в единый источник — вне
   scope): `debt_is_runtime_backed_newtype`'s `RUNTIME_BACKED_NEWTYPES`
   (~3508), `BUILTIN_RUNTIME_TYPES` (~5474), `RUNTIME_NATIVE_CONCRETE_TYPES`
   (~49512, в `debt_is_generic_stub_c`, задокументированно как
   намеренный дубль). Плюс комментарий в `lexer/mod.rs` (пример
   `AtomicPtr.null()`).
5. Тесты/спеки мигрированы: `sync_test.nv`, `plan103_2_atomic_isize_usize.nv`
   (переименован контент, файл оставлен на месте — историческое имя плана),
   `neg/plan103_2_atomic_isize_no_such_method_neg.nv`, `d168_sized_atomics.nv`
   (12→11 типов, комментарий). Ptr-тесты удалены (см. п.3).
6. **D426-амендмент** в `spec/decisions/06-concurrency.md` (тем же
   коммитом) — amends D168 (таблица типов, §1 матрица, §4 AtomicPtr,
   Эволюция/backward-compat note) и D425 (список CAS-методов 13→11,
   CasRaw witness-width). Плюс попутные фиксы: TOC-таблица, D370
   AI-guidance пример (`AtomicPtr.compare_exchange` → note о снятии).

### Шаг 2 — док-фиксы (int = intptr_t, не i64/int64_t)

- `sync.nv` ~1044 (AtomicInt header) и ~1173 (AtomicUint header, было
  «uint = u64 (Plan 70.5)») — исправлены на «nova_int = intptr_t» /
  «nova_uint = uintptr_t» (address-sized, Plan 133; на x64 совпадает по
  ширине с int64_t/uint64_t — явно как совпадение, не тождество).
- `sync_primitives.h` ~603 (было «AtomicIsize (int = nova_int = int64_t)»)
  и ~656 (было «AtomicUsize (uint = uint64_t)») — аналогично исправлены.
- Финальный grep «int64_t»/«i64» в комментариях рядом с atomic-семейством
  в обоих файлах — остались только МОИ correct-формулировки («coincides
  in width with int64_t», не «=»).

### Верификация

- `nova test std/src/runtime` → PASS 3/3 (после ВСЕХ правок, включая
  консолидацию).
- `nova check std/src/net/tcp_share_test.nv` → ok (production-код,
  `rc AtomicInt` поле type-checks против консолидированного типа).
  Полный `nova test` с runtime network loop НЕ запускался — по указанию:
  машина перегружена параллельными агентами, только точечные
  синхронные проверки.
- CC-FAIL в `std/src/concurrency/retry_test.nv` — **НЕ СВЯЗАН** с этой
  задачей (grep "Atomic" по `retry_test.nv`/`retry.nv` — ноль совпадений;
  ошибка про `nova_str`/`Nova_T*` generic mismatch, pre-existing).
- Финальные грепы-нули: `AtomicIsize|AtomicUsize|AtomicPtr|__cas_raw` по
  `std/src`, `compiler-codegen/src`, `compiler-codegen/nova_rt` (кроме
  libuv-вендора), `spec_tests`, `examples` — только объяснительные
  комментарии этой же волны + 2 несвязанных `std::sync::atomic::AtomicUsize`
  (Rust-стандарт в `test_runner.rs`/`nova-lsp/debouncer.rs`, НЕ Nova-тип).

### Полный список изменённых файлов

1. `std/src/runtime/sync.nv`
2. `std/src/runtime/sync_test.nv`
3. `compiler-codegen/nova_rt/sync_primitives.h`
4. `compiler-codegen/src/codegen/emit_c.rs`
5. `compiler-codegen/src/lexer/mod.rs`
6. `spec_tests/conformance/plan103_2_atomic_isize_usize.nv`
7. `spec_tests/conformance/neg/plan103_2_atomic_isize_no_such_method_neg.nv`
8. `spec_tests/conformance/d168_sized_atomics.nv`
9. `spec_tests/conformance/plan103_2_atomic_ptr_basic.nv` — удалён
10. `spec_tests/conformance/neg/plan103_2_atomic_ptr_no_such_op_neg.nv` — удалён
11. `spec/decisions/06-concurrency.md` (D426-амендмент)
12. `docs/plans/atomic-consolidation-progress.md` (этот файл)

### Следующий шаг

Полный `spec_tests/conformance` (single CU) + flagship examples под
`--strict-effects` — оркестратор (авторитетный гейт для behavior-changing
слияния, per test-conventions.md).
