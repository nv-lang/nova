# p325-barrier — Ш.0 заслон (№325): линейный тип нельзя положить в std-коллекцию

Модель: sonnet. Worktree: `d:/Sources/nv-lang/nova-p325`, ветка `p325-barrier`.
Задача — ТОЛЬКО Ш.0 (заслон), без Ш.1 (манглинг по режиму, №309/№317) и без
Ш.2 (полный энфорс, план 246). Ветка не вливалась и не пушилась.

## Шаг A — разведка (эмпирическая), ДО кода

### Воспроизведение обеих проб из записи №325

`type Res consume { x int }` — обе пробы из бага дают зелёный `nova check`
ДО фикса (проверено на билде worktree ДО правки):

- проба 1 (два живых владельца): `mut v = Vec[Res].new(); consume r = mk(1);
  v.push(r); assert(r.finish() == 1)` — `ok`.
- проба 2 (обязательство исчезает с контейнером): `mut v = Vec[Res].new();
  v.push(mk(1))` (элемент никогда не потреблён) — `ok`.

### Какие std-контейнеры реально затронуты

Пробным инстанцированием (`Container[Res].method(...)`, `Res` — минимальный
`consume`-тип с одним `consume`-методом) проверены ВСЕ generic-контейнеры
`std/src/collections/`, у которых есть слот элемента `T`/`K,V`:

| Контейнер | Файл декларации | Пробa зелёная ДО фикса? |
|---|---|---|
| `Vec[T]` | `std/src/collections/vec/core.nv:67` | да |
| `HashMap[K,V]` | `std/src/collections/hashmap/core.nv:86` | да |
| `Set[T]` | `std/src/collections/set/core.nv:42` | да |
| `Deque[T]` | `std/src/collections/deque.nv:38` | да |
| `Queue[T]` | `std/src/collections/queue.nv:41` | да |
| `LinkedList[T]` | `std/src/collections/linkedlist.nv:43` | да |
| `PriorityQueue[T Ord]` | `std/src/collections/priority_queue.nv:29` | да |
| `Lru[K Hash,V]` | `std/src/collections/lru.nv:33` | да |

`BloomFilter` — не generic (только `int`-хеши), не участвует.
`vec_iter`/`vec_lazy`/`vec_seq` (`MapIter`, `FilterIter`, `BoxIter`, …) —
не контейнеры: single-slot lazy-обёртки над источником `I` (как
`Option`/`Result`), не хранят множество T одновременно — НЕ включены в
заслон (та же причина, что для `Option`/`Result`).

Итог: заслон целится в фиксированный список из 8 имён: `Vec`, `HashMap`,
`Set`, `Deque`, `Queue`, `LinkedList`, `PriorityQueue`, `Lru`.

### Грепп корпуса — опирается ли что-то на дырявое поведение

Список consume-типов в std: `File` (`std/src/fs/fs.nv:191`), `BufWriter[W]`
(`std/src/io/buffered.nv:33`), `TcpListener`/`TcpStream`/`TcpReadHalf`/
`TcpWriteHalf` (`std/src/net/tcp.nv`), `UdpSocket` (`std/src/net/udp.nv:27`),
`StringBuilder` (`std/src/runtime/string_builder.nv:32`), `MutexGuard`/
`ReadGuard`/`WriteGuard`/`Permit`/`OnceGuard` (`std/src/runtime/sync.nv`).
Плюс все `consume`-типы, объявленные в `spec_tests/`, `bench/`, `examples/`,
`nova_tests/`, и в пакетных репах (`nova-http`: `Body`, `Request`,
`Response`; `nova-polaris`: `WebSocket`; `nova-tls`: `TlsStream`).

Комбинированный грепп (`Vec[Name]`, `[]Name`, `HashMap[_, Name]`, `Set[Name]`,
`Deque[Name]`, `Queue[Name]`, `LinkedList[Name]`, `PriorityQueue[Name]`,
`Lru[_, Name]`) по каждому имени — по всем шести репам на диске (`nova`,
`nova-http`, `nova-polaris`, `nova-tls`, `nova-bignum`, `www`): **ноль
реальных совпадений**. Два ложных срабатывания отсеяны вручную
(`HashMap[str, TokenBucket]` — `TokenBucket` НЕ consume-тип, совпадение по
подстроке `Token`; `HashMap[str, SessionEntry]` — `SessionEntry` объявлен
`value`, не `consume`, совпадение по подстроке `Session`).

**Вывод: корпус НЕ опирается на дырявое поведение — честного стопа не
требуется, заслон можно ставить без риска сломать существующий код.**

## Шаг B — реализация (чекер-канал, `types/mod.rs`)

Место: `compiler-codegen/src/types/mod.rs`, НЕ `emit_c.rs` (канал 196
соблюдён — `arch-ratchet.sh` подтверждает: `lines=64532` /
`infer=348` — БЕЗ сдвига от baseline).

Добавлено (перед `fn check_consume`):

- `const CONSUME_UNSAFE_STD_COLLECTIONS` — фиксированный список из 8 имён
  (см. таблицу выше). `Option`/`Result`/пользовательские sum-типы
  сознательно НЕ включены.
- `check_typeref_consume_collection_barrier` — рекурсивный обход
  `TypeRef` (generics, `[]T`≡`Vec[T]` по D239, tuple/func/ro/mut/pointer-
  обёртки), флагует любое вхождение имени из списка с must-consume
  generic-аргументом.
- `turbofish_base_name` + `walk_block_for_consume_collection_barrier` +
  `walk_expr_for_consume_collection_barrier` — обход тела функции/теста
  (по образцу существующего `walk_expr_for_defers`/
  `walk_block_for_defers`) для сайтов вида `Vec[Res].new()`
  (`ExprKind::TurboFish`) и типизированных `let`-аннотаций.
- `check_consume_in_std_collections` — module-wide проход: параметры/
  возврат `fn`, поля `record`/`sum`-варианта/named-tuple/alias, top-level
  `let`/`const`, тела `fn`/`test`. Строит собственный `LinearityRegistry`
  (не трогает регистр `check_consume`, отдельная функция — минимальный
  риск конфликта с параллельным окном p320).
- Вызов добавлен в драйвер сразу после `check_consume(module, &mut
  errors);` (строка ~1674).

Код диагностики: `[E_CONSUME_IN_STD_COLLECTION]` (по конвенции соседних
`E_BOUND_UNKNOWN`/`E_TYPE_UNKNOWN`/`E_MULTIPLE_TYPE_SETS` — не `D133-*`,
т.к. это НЕ часть нормативного D133, а временный барьер до Ш.3-амендмента).
Текст — по-английски, объясняет причину (push/insert не забирают
владение), ссылается на план 221 п.9 / дефект №325 / план 246 (Ш.1/Ш.2),
явно отмечает временность и что `Option[T]`/`Result[T,E]` не затронуты.

## Шаг C — фикстуры

- `spec_tests/conformance/neg/consume_collection_vec_push_two_owners_neg.nv`
  — `EXPECT_COMPILE_ERROR E_CONSUME_IN_STD_COLLECTION`, проба 1 из записи
  №325 (два живых владельца).
- `spec_tests/conformance/neg/consume_collection_vec_push_forgotten_neg.nv`
  — `EXPECT_COMPILE_ERROR E_CONSUME_IN_STD_COLLECTION`, проба 2 (элемент
  никогда не потреблён).
- `spec_tests/conformance/consume_collection_p325_barrier_control_ok.nv` —
  три pos-контроля в одном файле: `Option[consume T]` (образец рабочей
  формы взят из `spec_tests/conformance/consume_through_match_result_ok.nv`
  — `Ok(RecordLit)`/`Some(RecordLit)`, обходит `D133-consume-rvalue-in-
  view`), `Result[consume T, E]`, `Vec[int]` (не consume-элемент).

Уникальные имена типов (`P325TwoOwnersRes`, `P325ForgottenRes`,
`P325CtrlRes`) — во избежание коллизий: весь `spec_tests/conformance/*.nv`
(плоский уровень, 1121 файл) делит один `module spec_tests.conformance`
(модель «папка = модуль»), `neg/*.nv` — по одному `module neg.<уникальное
имя>` на файл (каждый — отдельный compile-unit, подтверждено эмпирически:
`nova check` на одном neg-файле — 1.4с, не тянет соседей).

## Гейты — вердикты дословно

```
$ ./nova-cli/target/release/nova.exe test --compile-error --filter consume_collection --jobs 2 spec_tests/conformance/neg
PASS           spec_tests/conformance/neg/consume_collection_vec_push_forgotten_neg  # (negative)
PASS           spec_tests/conformance/neg/consume_collection_vec_push_two_owners_neg  # (negative)

===== SUMMARY =====
PASS: 2  FAIL: 0
```

```
$ ./nova-cli/target/release/nova.exe test --positive --filter consume_collection_p325 --jobs 2 --timeout 120 spec_tests/conformance/consume_collection_p325_barrier_control_ok.nv
PASS           spec_tests/conformance/consume_collection_p325_barrier_control_ok

===== SUMMARY =====
PASS: 1  FAIL: 0
```

(Позитивный прогон реально слился с ВСЕМИ 1121 плоскими файлами
`spec_tests/conformance/*.nv` — общий `module spec_tests.conformance` —
первый прогон занял 3м39с из-за холодной сборки `libnova_rt`/toolchain;
повторный прогон после rename — быстрый, кэш тёплый. Ноль ошибок на всём
плоском позитивном корпусе — сильное подтверждение отсутствия
false-positive.)

```
$ ./nova-cli/target/release/nova.exe check std/src
===== SUMMARY =====
PASS: 147  FAIL: 26  WARN: 60
```
Канон 147/26/60 — БЕЗ сдвига.

```
$ bash scripts/guards/arch-ratchet.sh
arch-ratchet ok: lines=64532 <= 64532
arch-ratchet ok: infer=348 <= 348
```
Без сдвига (emit_c не тронут).

```
$ cargo build --release   # compiler-codegen
Finished `release` profile [unoptimized] target(s) in 0.44s   # инкрементально, 0 error
$ cargo build --release   # nova-cli
Finished `release` profile [optimized] target(s) in 2m 43s   # 0 error, 3 pre-existing warning
```

```
$ ./nova-cli/target/release/nova.exe lint spec_tests/conformance/neg/consume_collection_vec_push_two_owners_neg.nv spec_tests/conformance/neg/consume_collection_vec_push_forgotten_neg.nv spec_tests/conformance/consume_collection_p325_barrier_control_ok.nv
lint: 3 file(s), 0 finding(s)
```
(Первый прогон нашёл `W_CONSUME_NAKED_NAME` на `@finish()` — переименовано
в `@into_finish()` по nv-coding-style §1a, повторный прогон — 0 findings.)

## Известные ограничения (сознательно вне Ш.0)

- Заслон не различает key/value в `HashMap`/`Lru` — флагует консюм-тип в
  ЛЮБОЙ generic-позиции контейнера (и ключ, и значение). Это строже
  минимально необходимого, но безопаснее и проще для temporary-барьера.
- Обход AST для `let`/`TurboFish` — не 100% исчерпывающий по всем формам
  выражений (`Select`, `Supervised`, closures и т.д. покрыты по образцу
  `walk_expr_for_defers`, но экзотические вложенные позиции могут
  теоретически проскочить). Для Ш.0 (заслон, не полный энфорс) это
  приемлемо — Ш.2 сделает полный энфорс на уровне сигнатур `push`/`insert`.
- `Vec[Vec[Res]]`, `HashMap[str, Vec[Res]]` и т.п. — рекурсивно тоже
  ловятся (проверено обходом nested generics), не отдельно протестировано
  фикстурой (вне обязательного скоупа Ш.0, задел для Ш.2).

## Пересечение с p320

`p320` (`d:/Sources/nv-lang/nova-p320`) правит sum-lift (D55) в том же
файле `types/mod.rs`. Мои правки — НОВЫЕ функции, вставленные перед
`fn check_consume` (район строки ~37170) + одна строка вызова в драйвере
(~1674-1680). Не трогал существующие функции/строки. Конфликт при
слиянии маловероятен, но возможен на уровне diff-контекста — разруливает
интегратор.
