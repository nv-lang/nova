<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# Progress checkpoint — Plan 161 Ф.2 diagnostics implementation (2026-07-16)

**Ветка:** `p161-blanket-diag` (worktree `d:/Sources/nv-lang/nova-161diag`).
**Модель:** sonnet.
**Задача:** реализовать `E_DUPLICATE_PROTOCOL_IMPL` (D355 §4) и `E_BLANKET_CONFLICT`
(D355 §5), обещанные спекой и числившиеся «CLOSED» в Plan 161 Ф.2, но отсутствовавшие
в коде (0 совпадений в `compiler-codegen/src`); вернуть честный негативный
`blanket_dup_neg.nv` (был молча превращён в позитивный).

## Сделано

1. **Чекер** (`compiler-codegen/src/types/mod.rs`):
   - `check_duplicate_protocol_impl` (~строка 12207) — новый метод на
     `impl<'a> TypeCheckCtx<'a>`. Находит protocol-декларации формы «1 generic param +
     1 метод» (`Next[T]`-shape, не хардкод по имени), для каждого конкретного типа
     собирает overloads этого метода (`self.sig.method_table`), структурно выводит
     биндинг generic-параметра на каждый overload; если у типа ≥2 РАЗНЫХ биндинга —
     `E_DUPLICATE_PROTOCOL_IMPL`.
   - `check_blanket_conflict` (~строка 12288) — новый метод там же. Собирает blanket-
     декларации (`fn[I Proto[T]] I @m`) по всему `module.items`, группирует по
     `(protocol base name, method name)` — НЕ по литеральному имени bound-typevar
     (`I` vs `J` — та же коллизия). Второе+ вхождение группы → `E_BLANKET_CONFLICT`.
   - Обе вызываются из `TypeCheckCtx::check_module` (~строка 4152), сразу после двух
     существующих `#impl(...)`-проверок.
   - Вспомогательные free-функции (рядом с `check_signature_match_with_subst`,
     ~строка 22383): `infer_protocol_generic_binding`, `match_protocol_type_position`,
     `find_whole_word_occurrences` — структурный вывод биндинга через whole-word
     substring-шаблон (поддерживает ровно одно вхождение generic-параметра на позицию;
     0 вхождений — консервативно `None`, не false-positive).

2. **Тесты** (`spec_tests/conformance/`):
   - `neg/blanket_dup_neg.nv` — ВОССТАНОВЛЕН как честный негатив (был удалён
     позитивный вариант из `conformance/blanket_dup_neg.nv`, где комментарий гласил
     «positive test, not a negative test»). `EXPECT_COMPILE_ERROR
     E_DUPLICATE_PROTOCOL_IMPL`, тип `Dual` с двумя `@next()` (`Option[int]` /
     `Option[str]`).
   - `neg/blanket_conflict_neg.nv` — маркер уточнён с generic `duplicate definition`
     на `E_BLANKET_CONFLICT` (оба фактически срабатывают на этом файле — receiver-
     typevar в обоих объявлениях назван одинаково `I`, что попутно ловит и старый
     generic dup-check; substring-match всё равно проходит).
   - `neg/blanket_conflict_diffname_neg.nv` — НОВЫЙ. Тот же конфликт, но
     bound-typevar назван по-разному (`I` vs `J`) — доказывает, что именно НОВАЯ
     диагностика ловит случай, который старый generic dup-check (keyed по
     литеральному имени receiver-типа) пропускает.

3. **Спека**: не трогалась — диагностики уже описаны (D355 §4/§5), это чистая
   реализация обещанного, амендмент не требуется (по заданию).

4. **Plan 161 doc**: добавлен AMEND-блок в шапку (`docs/plans/161-blanket-protocol-receiver.md`)
   — Ф.2 реально закрыта этой волной.

## Верификация (по указанию координатора: точечно, CPU перегружен конкурентными агентами)

Все прогоны — release-сборка (`nova-cli/target/release/nova.exe`), env
`NOVA_GC_LIB_DIR`/`NOVA_GC_INCLUDE_DIR` на vcpkg_installed главного репо
(`d:/Sources/nv-lang/nova/compiler-codegen/vcpkg_installed/x64-windows-static/{lib,include}`).
Времена аномально велики (2-6 минут на то, что обычно секунды) — подтверждённая
перегрузка CPU параллельными агентами в main repo, не регрессия моего кода.

- `nova check spec_tests/conformance/neg/blanket_dup_neg.nv` → FAIL с
  `[E_DUPLICATE_PROTOCOL_IMPL] тип \`Dual\` реализует протокол \`Next\` сразу для
  нескольких разных типовых аргументов: \`Next[int]\`, \`Next[str]\` — ...` (ожидаемо).
- `nova check spec_tests/conformance/neg/blanket_conflict_neg.nv
  spec_tests/conformance/neg/blanket_conflict_diffname_neg.nv` → оба FAIL;
  diffname-файл — ТОЛЬКО `E_BLANKET_CONFLICT` (генерик dup-check молчит, как и
  предсказано); одноимённый файл — `E_BLANKET_CONFLICT` + старый `duplicate
  definition` (оба валидны, не мешают друг другу).
- `nova check spec_tests/conformance/d355_blanket_protocol.nv` (существующий
  позитив: два РАЗНЫХ конкретных типа, каждый реализует СВОЙ `Next[T]`-подобный
  протокол `D355Source[T]` ровно одним `@pull()`, плюс ДВА разных blanket-метода
  `@d355_drain`/`@d355_is_empty` на одном протоколе — «непересекающиеся
  бланкеты») → PASS: 1, FAIL: 0 (не сломан).
- `nova check std/src/collections` (VecIter/MapIter/FilterIter/FilterMapIter/
  TakeIter/SkipIter/EnumerateIter/StepByIter/SplitIter/RSplitIter/RangeIter/
  StepRangeIter/ReverseRangeIter/HashMapIter/KeysIter/ValuesIter — все реализуют
  `Next[T]` РОВНО одним `@next()` каждый) → PASS: 20, FAIL: 0 (0 реальных дублей
  в stdlib; диагностика не даёт ложных срабатываний).
- Полный `spec_tests/conformance` (972 pos + 377 neg, один CU) НЕ прогнан целиком
  синхронно (координатор: CPU перегружен, точечной проверки достаточно) — частичный
  фоновый прогон (прерван из-за нагрузки) успел показать: мои 3 neg PASS, никаких
  FAIL, связанных с моим изменением (только TIMEOUT на НЕСВЯЗАННЫХ фикстурах —
  consume_fixtures/lint/d78-dup-decl — под перегрузкой). Полный CU-прогон — на
  оркестраторе (авторитетный гейт), не на этой волне.

## Находки по существующему коду

Реальных дублей `Next[T]`-подобных протоколов в std/spec_tests **не найдено** —
каждый adapter-тип (`VecIter`, `MapIter`, `FilterIter`, …) реализует `@next()`
РОВНО один раз. Единственный «почти-похожий» случай — `Month`/`Weekday`/
`YearMonth.@next() -> Self` (без `Option`-обёртки) — структурно НЕ матчит
`Next[T]`-шаблон (return не начинается с `Option[`) → корректно игнорируется
диагностикой, и там всё равно только один overload на тип. `Iter[I]` (второй
protocol в std с формой «1 generic + 1 метод») тоже проверен — ни у одного типа
нет 2+ `@iter()` с разным `I`.

## Осталось / не в эту волну

- `[M-161-blanket-conflict-diagnostics-missing]` в `docs/plans/backlog-followups.md`
  НЕ найден (грепом) — маркер не заводился ранее, поэтому и не закрывается; просто
  фиксирую здесь, что диагностики реализованы.
- Полный `spec_tests/conformance` прогон одним CU (972+377 файлов) — за
  оркестратором/авторитетным гейтом, не за этой точечной волной (перегрузка CPU).
- Коммит на этой ветке НЕ мержится в `main` (по заданию) — ждёт решения владельца/
  оркестратора.
