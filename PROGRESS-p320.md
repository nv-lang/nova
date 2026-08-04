# p320-indexmap — прогресс (модель: sonnet)

## Фаза 1 — компиляторный фикс №320 [M-generic-sumlift-mono-missing-variant-wrap] — ЗАКРЫТА

Место фикса: `compiler-codegen/src/types/mod.rs`, функция `single_wrap_candidates`
(Sum-плечо — гейт `!generics.is_empty()` снят, Newtype-плечо не тронуто) +
`try_wrap_leaf` (материализация в BARE `Leaf(w)` для generic-цели вместо
квалифицированного `Node.Leaf(w)`).

Фикстуры: `spec_tests/conformance/standalone/p320_sumlift_concrete_pos.nv`,
`p320_sumlift_generic_pos.nv`, `p320_sumlift_generic_multi_pos.nv` — все
зелёные (см. отчёт для дословных строк прогона).

Гейты: cargo build чистый; `nova test std/src` — per-directory сравнение с
pristine main БАЙТ-В-БАЙТ идентично (PASS/FAIL/SKIP counts совпадают везде,
включая пред-существующие CC-FAIL/ICE в concurrency/retry_test,
encoding/serde/decode_errors_test, net/addr, time/cron_test,
time/civil/civil_arith_test, identifiers/*, crypto/* — все подтверждены
ИДЕНТИЧНЫМИ на pristine main, НЕ регрессии этого окна); arch-ratchet ok
(lines=64532<=64532, infer=348<=348).

Доп. находка (НЕ зачинена, вне скоупа №320): спекин собственный пример
`type Wrapper[T] enum W(T) | Empty` / `ro w Wrapper[int] = 42` (D55
"Генерики:") был СЛОМАН уже до этого окна (не регрессия) и остаётся
сломанным — payload = голый generic-параметр самого sum'а требует
generic-aware WrapKind-подстановки, которой фикс №320 не делает (фикс
закрывает только payload = ИМЕНОВАННЫЙ generic-зависимый тип, `Wrap[K,V]`-
паттерн). Задокументировано в спеке (D55 амендмент, реестр 221.1 №320).

Доп. находки при построении второй (multi-variant) фикстуры — ДВА отдельных
пред-существующих codegen-гэпа, НЕ регрессии, НЕ зачинены:
1. generic-сумма, смешивающая type-param-ЗАВИСИМЫЙ unary-вариант с
   type-param-НЕЗАВИСИМЫМ (`Tag(str)`/`Num(i64)`) в ОДНОЙ декларации —
   erased ctor неправильно боксит независимый payload в `void*`.
2. unary-вариант, чей payload зависит только от ПОДМНОЖЕСТВА типовых
   параметров суммы (`Boxed(Box[K])` на `Node2[K, V]`) — та же ошибка
   боксинга.
Оба воспроизведены и на pristine main через hand-written (не-lifted) вызовы
конструктора — подтверждённо НЕ вызваны фиксом №320.

Реестр: №320 → ✅ (формула закрытия — выше). Спека — D55 амендмент внесён тем
же слиянием.

## Фаза 2 — новый generic-тип `IndexMap[K, V]` в std — ЗАКРЫТА

Дом: `std/src/collections/index_map/core.nv` (folder-module, по прецеденту
`std/src/collections/vec/`) + пир `index_map_test.nv` (16 test-блоков).
Реализация: dense `[](K, V)` (insertion order) + `HashMap[K, int]` (key →
index, O(1) lookup) — честная граница объёма (значения не дублируются, ключи
— ещё дублируются внутри `HashMap[K,int]`; `[M-hashmap-index-style-backing]`
остаётся ОТКРЫТ отдельно) зафиксирована в доккомменте типа.

API: `.new()`, `@len()`, `@is_empty()`, `@get()`, `@contains()`, `mut
@insert()`, `mut @remove()` (swap-remove, O(1), задокументировано как
НЕ-order-preserving для остатка — расширение сверх минимума брифа), `mut
@clear()`, `@keys() -> []K`, `@equal(other …) -> bool`, `@iter() ->
IndexMapIter[K, V]`.

Находка при реализации: `@equal(other Self)` (буквально `Self`, как в
брифе) триггерит компиляторный баг эрейзд-stub синтеза —
`Nova_IndexMap_method_equal` получает ВТОРОЙ, конфликтующий C-forward-decl
с `other`, типизированным как НЕСВЯЗАННЫЙ `Nova_EmbeddedDir*` (auto-derived
`@equal` из `std/prelude/embed.nv`, транзитивно затянут в CU) —
`CC-FAIL: conflicting types for 'Nova_IndexMap_method_equal'`. Обход —
`@equal(other IndexMap[K, V])` (явный тип вместо `Self`) — ТА ЖЕ форма, что
уже использует `HashMap[K, V] @equal(other HashMap[K, V])` (не хак, а
следование существующему прецеденту в этом же файле-соседе). Не
эскалировано как новый компиляторный фикс (Фаза 1 — единственная
компиляторная часть окна; баг узкий и обходится идиоматично).

Гейты: `nova test std/src/collections/index_map` — PASS (все 16 test-блоков
зелёные, подтверждено дословно, включая обязательные регресс-тесты
«re-insert не двигает позицию» и «итерация = порядок вставки», плюс
sanity-проверка что раннер реально детектирует упавший assert — было
временно вставлено, дало RUN-FAIL с точным именем теста, затем убрано);
`nova test std/src/collections` (весь каталог, регресс) — PASS: 14 FAIL: 0;
`nova lint std/src/collections/index_map` — 0 находок (после фикса
`Vec[(K,V)]` → `[](K,V)`-спеллинга, W_VEC_SPELLING); arch-ratchet
не тронут (std не входит в счёт).

## Фаза 3 — ретайп `JsonObject`/`Object`-варианта на `IndexMap[str, JsonValue]` — ЗАКРЫТА

`std/src/encoding/json.nv`: `JsonValue enum ... | Object(JsonObject)` →
`| Object(IndexMap[str, JsonValue])`. `JsonObject` — `type JsonObject alias
IndexMap[str, JsonValue]` (D52 alias, zero-cost).

**Проба ПЕРВЫМ делом (по инструкции брифа) показала: alias НЕ даёт
метод-резолв прозрачно** — `x.insert(...)` на голом alias-типе без
собственных методов мис-резолвится в НЕСВЯЗАННЫЙ одноимённый метод другого
типа в том же CU (не ошибка, не forward — тихий мисроут). Поэтому
`JsonObject.new/@len/@is_empty/@get/@contains/@insert/@keys/@equal/@iter` +
`JsonObjectIter mut @next()` оставлены ТОНКИМИ делегатами (не копии) —
каждый ретайпит `@`/аргументы в `IndexMap[str, JsonValue]` через локальный
`ro`/`mut`-биндинг (бесплатно, тот же repr) и зовёт настоящую реализацию;
для `mut`-методов — `@ = m` после вызова, чтобы мутация пробросилась назад.

`JsonValue.object(fields HashMap[str, JsonValue])` →
`JsonValue.object(fields IndexMap[str, JsonValue]) -> Self => Object(fields)`
— тело стало ОДНОЙ строкой (не циклом с сортировкой — `IndexMap` уже хранит
порядок). **ПОВЕДЕНЧЕСКОЕ ИЗМЕНЕНИЕ**, задокументировано doc-comment'ом на
месте (JSON-модуль не в основной языковой спеке — по инструкции брифа
doc-comment достаточен): вывод `.object()` был SORTED, стал insertion-order.
Плюс это breaking-изменение СИГНАТУРЫ (`HashMap`→`IndexMap`) — любой вызывающий
код с `HashMap`-аргументом больше не компилируется (осознанно, не тихая
порча поведения).

Потребители (грепнуто по всей репе nova + nova-http + nova-polaris):
- `std/src/crypto/jwt.nv` (`make_header_hs256` строит `JsonObject` через
  `.insert()`, НЕ через `.object()`) — продолжает работать БЕЗ изменений
  через alias + делегаты. `nova check std/src/crypto/jwt.nv` → ok.
  `nova test std/src/crypto` — БЛОКИРОВАН пред-существующим ICE
  (`Path call return type unknown for method=now`, emit_c.rs:59655) —
  подтверждено идентичным на pristine main И до, И после этого окна
  (Фаза 1 investigation) — НЕ регрессия Фазы 3, вне скоупа фикса.
- `std/src/encoding/serde/json.nv` (`SerFrame.obj JsonObject`,
  `fr.obj.insert(...)`, `JsonValue.Object(fr.obj)`) — `nova check
  std/src/encoding/serde` → ok (3 pre-existing unused-import warnings,
  несвязанные). Полный `nova test std/src/encoding/serde` заблокирован
  ДРУГИМ пред-существующим CC-FAIL (`decode_errors_test`, Vec[str]/Vec[int]
  Option-type mismatch, НЕ про JSON/IndexMap) — идентично на main, до этого
  окна тоже НЕ проходил батчем (folder-module = один CU на всю папку,
  один упавший файл блокирует весь батч) — не новая находка, не регрессия.
- `nova-http` — grep не нашёл `.object(`/прямой `JsonObject`-конструкции,
  только `JsonValue` как opaque тип (`RequestBuilder @json`, `@into_json`)
  — не задет.
- `nova-polaris/examples/05-auth/src/main.nv:38-39` — использует
  `HashMap[str, JsonValue]` + `JsonValue.object(fields)` — **СЛОМАЕТСЯ**
  при пересборке против нового std (по конструкции, задокументировано
  брифом заранее). Миграция — ОТДЕЛЬНЫЙ коммит в pkg-репе, НЕ сделана
  здесь (координация с владельцем/pkg-сессией).
- `spec_tests/conformance/{json_roundtrip_object_flat,
  json_roundtrip_object_empty, json_roundtrip_nested}.nv` — использовали
  `HashMap[str, JsonValue]` + `JsonValue.object(m)` — МИГРИРОВАНЫ на
  `IndexMap[str, JsonValue]` (ни один тест не проверял КОНКРЕТНЫЙ порядок
  ключей в выводе — только `len()`/`contains()`/shape — миграция чистая,
  не ослабляет проверки). `spec_tests/conformance/{d114_map_literal_sum_lift,
  neg/d114_insert_generic_arg_incompatible_neg,
  neg/d114_map_literal_incompatible_value_neg}.nv` используют
  `HashMap[str, JsonValue]` тоже, но НЕ вызывают `.object()` — не задеты,
  перепроверены зелёными без изменений.

Гейты: `nova test std/src/encoding/json_test.nv` — PASS; `nova test
std/src/encoding` (весь каталог) — PASS: 7 FAIL: 1 (тот самый пред-
существующий `decode_errors_test`, δ0 против baseline); `nova test`
на 3 мигрированных conformance-фикстур вместе — PASS: 3 FAIL: 0; d114-тройка
(sum-lift + оба neg) — PASS: 3 FAIL: 0 (regression-check, не задеты).

## Фаза 4 — попытка упразднить `.object()` через sum-lift — ЗАКРЫТА (СРАБОТАЛО ЧАСТИЧНО)

Проба: голое значение `IndexMap[str, JsonValue]` в позиции, ожидающей
`JsonValue` — коэрсится ли само в `Object(...)` без `.object()`?

**RETURN-позиция — ДА, работает.** `fn f(m IndexMap[str,JsonValue]) -> ro
JsonValue => m` авто-оборачивается в `Object(m)`. Задокументировано
doc-comment'ом на `JsonValue`-enum'е (json.nv) + фикстура
`spec_tests/conformance/standalone/p320_phase4_jsonvalue_sumlift_pos.nv`.
`.object()` НЕ упразднена (осталась `#stable`, явное читаемое имя — по
инструкции брифа).

**CALL-ARG-позиция — ДА, но с оговоркой.** Работает, ЕСЛИ тип источника
статически известен чекеру (аннотированный `let`, record-литерал). НЕ
работает (МОЛЧА — неверный runtime-результат, не compile-error!) когда
источник — НЕаннотированный `let`, построенный через turbofish-статик-
конструктор (`mut m = IndexMap[str, JsonValue].new()`). Корень: `simple_
expr_type` (types/mod.rs) — сидирует `var_types` для НЕаннотированных
биндингов — распознаёт голый `Type.new()` (2-сегментный `Path`) и record-
литерал, но НЕ turbofish-квалифицированный `Type[Args].new()` — `m` не
попадает в `var_types` → `try_wrap_leaf`'s `Ident`-плечо молча бейлится.
Подтверждено: явная аннотация типа на `let` чинит проблему. Узкий, точно
диагностированный гэп — НЕ зачинен (вне скоупа №320-фикса, отдельный
маркер/номер — за интегратором).

**LET-INIT-позиция — НЕТ, сломана, и это НЕ про sum-lift.** `ro n
Node[K,V] = w` не работает — но ТА ЖЕ ошибка (`CC-FAIL: initializing 'void
*' with an expression of incompatible type ...`) воспроизводится и на
ПОЛНОСТЬЮ ручном, полностью аннотированном коде (`ro n Node[str,int] =
Leaf(w)`, БЕЗ какого-либо участия sum-lift) — то есть это кодоген-гэп
эмиссии bare generic-sum-ctor-вызова в let-биндинг, ортогональный фиксу
Фазы 1 (детекция кандидата тут ни при чём — RHS уже ЯВНЫЙ `Leaf(w)`).
НЕ зачинено (глубже, чем «добавить условие в чекер» — кодоген-архитектура
mono-инстанс-очереди для let-присваивания). Задокументировано в doc-
comment'е `JsonValue` + фикстуре (НЕТ let-init позитив-теста — не стал
утверждать то, что не работает).

Обе находки — НЕ регрессии этого окна (обнаружены ВПЕРВЫЕ при построении
Фазы 4, не существовали раньше как «работавшее и сломанное» — sum-lift для
generic-типов вообще не работал НИГДЕ до Фазы 1 этого окна). Изолированы
через hand-written (без sum-lift) репро — корень НЕ в моём фиксе.

Гейты: `nova test` на финальной фикстуре (3 test-блока: return / call-arg
annotated / explicit `.object()`) — PASS: 1 FAIL: 0 (все 3 блока внутри
зелёные, дословно). `nova test std/src/encoding/json_test.nv` — PASS
(регресс после doc-comment правки на `JsonValue`). `nova lint` — 0 находок
по ВСЕМ файлам окна разом (7 файлов).
