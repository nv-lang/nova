# PROGRESS — окно p-w2 (№254, протокольный резолв Next[T]/Iter[I])

worktree: `d:/Sources/nv-lang/nova-w2`, ветка `pw2`, база `main@5adb8b331`.

## Дизайн-записка (точки врезки + правила специфичности)

Дизайн владельца (2026-08-02, дословно в №254): (1) bound-чек generic-кандидатов —
непрошедшие ОТПАДАЮТ; (2) специфичность — собственный `Next[T]`-носитель
выигрывает у `Iter[I]`-делегата; (3) остаток → `E_AMBIGUOUS`; (4) `Iter[I]`-
перегрузки `collect`/`reduce`/`min`/`max`/`collect_set` в std (путь Kotlin).

**Три причины из диагноза окна p-recv-pack, подтверждены репро:**
1. Vec/Range не реализуют `Next[T]` (только `.iter()`) — чекер лениво пропускал
   `entries.collect()` без проверки protocol-bound.
2. T-binding gap для конкретных (не-generic) `Next[T]`-имплементоров без
   `#impl(Next[..])` (RangeIter/StepRangeIter/ReverseRangeIter — были ЧИСТО
   структурными).
3. Last-wins — следствие (1)/(2), не отдельный корень.

### Точки врезки

- **Bound-чек (причина 1)** — `compiler-codegen/src/types/mod.rs`,
  `resolve_prefix_generic_method_return` (бare-typevar-receiver ветка): перед
  принятием кандидата `I := peeled` против `I`'s bound, СТРУКТУРНАЯ проверка —
  «протокол `Next`/`Iter`, lowercase имя протокола = обязательный метод» (конвенция
  задокументирована в `prelude/collections.nv`), ищем метод в
  `sig.method_table.get(concrete_name)`. Область СОЗНАТЕЛЬНО сужена до `Next`/
  `Iter` — расширение на произвольные протоколы = отдельный класс (backlog
  №260/№262), вне окна.
- **Специфичность (причина/дизайн 2)** — ДВЕ независимые точки (чекер сам по себе
  НЕ управляет тем, что реально выполнится в C, это отдельный канал):
  - чекер: та же функция, `ordered.sort_by_key` перед перебором кандидатов —
    кандидат, чей bound НЕ `Iter`, пробуется первым (стабильная сортировка,
    не хэш-порядок HashMap).
  - codegen: `compiler-codegen/src/codegen/emit_c.rs`, `blanket_keys.sort_by_key`
    (Plan 164 Ф.3 dispatch, было `blanket_keys.sort()` — алфавитный, "C" раньше
    "I", ВСЕГДА выбирал делегат первым) — тот же приоритет: не-`Iter` раньше
    `Iter`. **Оба места обязательны** — без codegen-правки чекер один не
    предотвращает разворачивание в C (эмпирически подтверждено: тип с ОБОИМИ
    `#impl(Next[T])` и тривиальным `#impl(Iter[Self])` уходил в
    fiber-stack-overflow — делегат зовёт `@iter()` (== self), тот же вызов
    резолвится снова в делегат, бесконечная рекурсия — до правки ОБЕИХ точек).
- **E_AMBIGUOUS (дизайн 3)** — **НЕ реализован отдельным диагностическим кодом.**
  С текущей специфичностью (Next всегда раньше Iter) настоящая ничья не
  возникает ни для одного из 5 методов (по одному blanket на бáунд-класс).
  Гэп: тип, ФАКТИЧЕСКИ не удовлетворяющий НИ Next, НИ Iter, при вызове
  `.collect()` сейчас падает ГРОМКИМ compiler-internal panic'ом
  (`[P67-LEGACY] method call \`.collect\` return type unknown`) — это НЕ
  silent misresolve, НЕ «метод не найден» (сообщение точно называет вызов),
  но и не полированный `E_PROTOCOL_BOUND_NOT_SATISFIED`. Написан
  `bound_violation_message`/расширен `prefix_generic_method_exists`
  (types/mod.rs) — код рабочий и подключён к ОДНОМУ из E7320-сайтов, но НЕ
  накрывает форму репро (`ro v = n.collect()` на value-record без бáунда) —
  другой checker-путь до него не дошёл за отведённый бюджет. Честно оставлено
  ОТКРЫТЫМ — см. «Не сделано» ниже. NEG-фикстура НЕ заведена (ICE уронил бы
  весь прогон раннера, не только один тест — хуже, чем отсутствие фикстуры).
- **T-binding (причина 2)** — `#impl(Next[int])` добавлен на `RangeIter`/
  `StepRangeIter`/`ReverseRangeIter` (`std/src/collections/range/core.nv`) —
  единственная причина, по которой T не биндился (существующий
  `[M-next-collect-value-record]`-мост в обеих точках, checker и codegen, уже
  читал `#impl`-спеку правильно — просто спеки не было).
- **Iter-делегат (дизайн 4)** — новый blanket-блок в
  `std/src/collections/vec_iter/core.nv`: `fn[C Iter[I], I Next[T]] C @method()`
  (4 метода: `collect`/`reduce`/`min`/`max`), тело `mut it = @iter(); it.method()`
  — ПЕРЕИСПОЛЬЗУЕТ уже рабочую generic-body-calls-generic-method машинерию (тот
  же класс, что чинил №170). **`@collect_set()` (5-й метод по дизайну) ПОПРОБОВАН
  И СНЯТ** — подтверждено standalone (НЕ мега-CU-артефакт): идентичный delegate-
  shape для `Set[T]`-возврата даёт `T` НЕ связанным в C (`Nova_Set____Nova_T_p` —
  буквальное "T" в имени типа, CC-FAIL «unknown type name»), тогда как ТОТ ЖЕ
  shape для `Vec[T]`/`Option[T]` (`collect`/`reduce`/`min`/`max`) биндит `T`
  корректно. Корень не найден за бюджет окна (гипотеза: codegen-elem-lookup для
  bound'а имеет явные `Nova_Vec____`/`NovaArray_`-ветки и ничего для `Set` —
  `compiler-codegen/src/codegen/emit_c.rs`). Ни один из 16 сайтов не зовёт
  `.collect_set()` на контейнере напрямую — не блокирует приёмку, оставлено
  явной заметкой в коде (vec_iter/core.nv) + backlog №295.
  `#impl(Iter[VecIter[T]])`/`#impl(Iter[RangeIter])`/
  `#impl(Iter[HashMapIter[K,V]])` на `@iter()` контейнеров (Vec/Range/HashMap) —
  БЕЗ этого делегат не регистрируется как кандидат (codegen `type_impl_protocols`
  требует явный `#impl`, структурного «есть метод iter()» недостаточно).
  Побочный фикс: `#impl(Next[K])`/`#impl(Next[V])`/`#impl(Next[(K,V)])` на
  `KeysIter`/`ValuesIter`/`HashMapIter` (были структурными без `#impl` —
  root-cause-2-класс, нужны для 3 из 16 сайтов, использующих `.keys().collect()`).

### Побочный фикс парсера (не в исходном плане, но обязателен для дизайна 4)

`E_UNUSED_PREFIX_TYPEVAR` (types/mod.rs) считал `I` в
`fn[C Iter[I], I Next[T]] C @collect()` неиспользуемым (`I` НЕ встречается
буквально в receiver/params/return — только внутри bound'а `C`). Расширен: скан
bound'ов ВСЕХ prefix-generic'ов тоже фидит "referenced"-множество (зеркало уже
существующей effects-clause оговорки в том же коде).

## Обходы: 16 из 16 развёрнуты

Все сайты сверены байт-в-байт с `git show 0075b8c72`/`a81e6c5ba`/`73ba9e731`
(канонические формы ДО откатов): `checksums/{adler32,crc32}_test.nv`,
`math/statistics.nv`, `prelude/embed.nv`, `collections/range/core_test.nv`,
`collections/vec/core_test.nv`, `collections/vec_iter/core_test.nv`,
`encoding/json.nv`, `encoding/serde/{json,serde}.nv` (10 «Vec/KeysIter.collect()»
сайтов) + `collections/vec_iter/core.nv` и `collections/vec_lazy/core.nv`
(reduce/min/max ×2 файла = 6 `@next()?` сайтов). Грep по маркеру —
`grep -rn "M-try-on-next-generic-receiver-misresolve" std/src` → 0 находок.

**Побочно найдено и НЕ было в списке 16**: `std/src/time/cron.nv` (уже
канонический `.collect()` на Range/SplitIter — НИКОГДА не был реально
собран/протестирован до этого окна — латентный root-cause-1 баг). Работает
после фикса (не требовал правки исходника — только компилятор).

## Гейты (вердикты дословно)

- `cargo build --release` (nova-cli, ×N пересборок): чисто, только pre-existing
  warnings (dead-code/unused, не задеты этим окном).
- `nova check std/src`: **PASS: 147 FAIL: 26 WARN: 60** — байт-в-байт канон.
- ratchet (`scripts/guards/arch-ratchet.sh`): **lines=64505 <= 64505**,
  **infer=348 <= 348** — emit_c.rs НЕ вырос (обе правки codegen — замена ОДНОЙ
  строки на ОДНУ строку, δ0 строк).
- `nova lint` на 17 правленых .nv: **0 находок**.
- polaris `./nova.sh test src --strict-effects`: см. отчёт (запускалось отдельно).
- Мега-CU `spec_tests/conformance` (НЕ гейт исполнителя по брифу — «интегратор
  при приёмке» — но прогнано доп. диагностикой): канон **634/0/68**, ветка pw2
  (финальное состояние, после снятия `collect_set`-делегата) даёт **633/1/68**
  (baseline main чисто **635/0/68**). Заведён **№295**
  (`[M-blanket-overload-same-name-mono-registry-collision]`,
  backlog-followups.md + 221.1-bug-sweep.md) — НЕ pre-existing флак, реальная
  регрессия ОТ дизайна 4 (первый в std случай ДВУХ generic-blanket-перегрузок
  одного имени под разными typevar-ключами в одном CU). До снятия `collect_set`
  счёт был 632/2/68 — второй, нондетерминированный victim был изолирован к Plan
  123.2 LICM и полностью ушёл вместе с `collect_set`. Остающаяся ЕДИНСТВЕННАЯ,
  детерминированная причина (`a_q3_println_debug_record` CC-FAIL) НЕ доведена
  до корня за бюджет окна. Изолированные/targeted-folder repro — ВСЕ чистые
  (только полный мега-CU 636-файловый merge триггерит).
- **№296** (`[M-iter-delegate-collect-set-t-unbound]`): `@collect_set()`
  делегат СНЯТ из std — standalone-подтверждённый (не мега-CU) T-binding баг,
  идентичный delegate-shape для `Vec[T]`/`Option[T]` работает, для `Set[T]` —
  нет (`Nova_Set____Nova_T_p`, буквальное "T"). Не блокирует №254 (ни один из
  16 сайтов не зовёт `.collect_set()` на контейнере напрямую).

## Что НЕ сделано (честно)

1. **E_AMBIGUOUS** как отдельный диагностический код — не реализован (нет
   естественного neg-сценария при текущей детерминированной специфичности;
   `bound_violation_message` написан, но не накрывает repro-форму value-record
   без бáунда за отведённый бюджет — падает ICE вместо диагностики).
2. **№295** (мега-CU регрессия) — не зафикшено, задокументировано и заведено
   отдельным номером для интегратора/следующей волны.
3. Специфичность и bound-чек СОЗНАТЕЛЬНО сужены до протоколов `Next`/`Iter`
   (конвенция «имя протокола lowercase = метод» подтверждена ТОЛЬКО для этих
   двух в prelude/collections.nv) — не обобщены на произвольные протокольные
   бáунды (backlog №260/№262, отдельный, более широкий класс).

## Файлы (абсолютные пути)

- `compiler-codegen/src/types/mod.rs` — bound-чек + специфичность (чекер-канал),
  E_UNUSED_PREFIX_TYPEVAR фикс, bound_violation_message/prefix_generic_method_exists.
- `compiler-codegen/src/codegen/emit_c.rs` — специфичность (codegen blanket-dispatch
  ranking), 1 строка → 1 строка, δ0.
- `std/src/collections/vec_iter/core.nv` — Iter-делегаты (5 методов) + `@next()?`
  разворот (reduce/min/max, 3 сайта).
- `std/src/collections/vec_lazy/core.nv` — `@next()?` разворот (reduce/min/max, 3 сайта).
- `std/src/collections/range/core.nv` — `#impl(Next[int])` ×3 + `#impl(Iter[RangeIter])`.
- `std/src/collections/vec/iter.nv` — `#impl(Iter[VecIter[T]])`.
- `std/src/collections/hashmap/core.nv` — `#impl(Next[K])`/`#impl(Next[V])`/
  `#impl(Next[(K,V)])`/`#impl(Iter[HashMapIter[K,V]])`.
- 10 сайтов-разворотов: `std/src/checksums/{adler32,crc32}_test.nv`,
  `std/src/math/statistics.nv`, `std/src/prelude/embed.nv`,
  `std/src/collections/range/core_test.nv`, `std/src/collections/vec/core_test.nv`,
  `std/src/collections/vec_iter/core_test.nv`, `std/src/encoding/json.nv`,
  `std/src/encoding/serde/{json,serde}.nv`.
- `spec_tests/conformance/m254_iter_protocol_bound_dispatch_pos.nv` — новая
  pos-фикстура (T-binding + Iter-делегат + специфичность).
- `docs/plans/backlog-followups.md` + `docs/plans/221.1-bug-sweep.md` — №295
  заведён, №254 обновлён.
