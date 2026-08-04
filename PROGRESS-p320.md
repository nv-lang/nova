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

## Фаза 3-4 — НЕ начаты (зависят от Фазы 2, следующий шаг этого же окна)
