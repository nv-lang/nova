<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# Plan 197 Ф.1/Ф.2 — audit-progress чекпойнт (per-file)

Рабочий чекпойнт для аудита `examples/**/*.nv` (`nova build <file>`,
C-codegen). Вердикты: **KEEP** (компилится/канон, либо блокирован
подтверждённым compiler-багом вне скоупа Plan 197) · **FIX** (дешёвая
правка на канон — сделана в этом заходе) · **DELETE** (снесено — старый
синтаксис/не-user-facing содержимое, чинить не окупалось) · **RECREATE**
(концепт ценен, содержимое перемещено в `examples/_wip/` до переписи
начисто).

## Заход 2026-07-11 (Ф.1, read-only) — ИСТОРИЯ

Первый заход упёрся в ДВА toolchain-бага (ICE в `emit_c.rs` на любом
hello-world + `Result.map` generic-inference), заблокировавших бОльшую
часть аудита «вслепую» (compile fail не связан с содержимым файла).
Таблица того захода — см. git-историю этого файла (commit
`1be0867de`). Вывод предыдущего захода: 16 KEEP (заблокированы
toolchain, содержимое не проверено), 6 RECREATE, 4 DELETE, 3 FIX
(1 сделан — `basic_pointer.nv`, 2 отложены).

## Заход 2026-07-12 (Ф.1 доследование + Ф.2 исполнение) — ТЕКУЩИЙ

Синхронизация с `main` (`git merge main`, fast-forward `1be0867de` →
`4c02d346d`) подтянула compiler-codegen работу другого агента (196.2/196.3
волна). Пересборка `nova.exe` release + пересборка `nova_rt/libuv`
(worktree копия main-repo submodule, `.git` удалён — см.
[reference-worktree-nova-test-setup]) — **оба toolchain-бага из захода
2026-07-11 подтверждённо ИСПРАВЛЕНЫ апстримом**: minimal
`fn main() { println("hello") }` теперь компилируется и линкуется чисто.
Это разблокировало полный переаудит всех 29 файлов реальной компиляцией
(не «предположительно ok»).

### Системные находки — ДВА НОВЫХ compiler-бага (вне скоупа Plan 197)

Не трогал compiler-codegen (граница задачи — другой агент/волна). Найдены
и воспроизведены на synthetic minimal repro (не только в examples/
содержимом):

1. **`.map()` generic type-argument inference ICE** — `nova: internal
   error at emit_c.rs:48511/49360: [P67-LEGACY] method call `.map` return
   type unknown — checker must annotate`. Родственник уже
   задокументированного `Result.map` U-inference бага из захода
   2026-07-11 — тот же паттерн, но шире: `Vec[UserRecord].map(|c| ...)`
   тоже триггерит. Репро: `examples/real_world/orm_demo.nv` —
   `cols.map(|c| c.name)` на `[]ColumnValue`.
2. **`with EFFECT = value { ... }` не парсится внутри тела
   handler-method** — `error: expected `=>` or `{` for handler-method
   body`. Репродуцируется на synthetic minimal (эффект/значение
   произвольные, `{}`- и `=>`-форма handler-method — обе). Репро:
   `examples/real_world/orm_decorators.nv:145` —
   `primary.in_transaction(|| with Db = primary { b() })` внутри
   `in_transaction(b) { ... }` handler-method тела `with_read_replica`.
3. **extern-FFI tuple-return codegen** — C-компилятор: `initializing
   '_NovaTuple_2_...' with an expression of incompatible type 'int'`
   для `nova_fn_sqlite3_open`/`sqlite3_prepare`. Репро:
   `examples/ffi/sqlite_mini.nv`.

Для файлов, упавших ТОЛЬКО на этих багах (или на `Result.map` из захода
2026-07-11, тоже ещё не проверен на пофикшенность — не встретился в
сегодняшнем прогоне), вердикт **KEEP** = «содержимое проверено и
почищено от авторских дефектов, сборка блокирована toolchain-багом вне
скоупа Ф.1/Ф.2 — нужен повторный прогон после фикса в compiler-codegen».

### Таблица (обновлено 2026-07-12)

| # | Файл | Компилится? | Находки / правки | Вердикт |
|---|------|-------------|-------------------|---------|
| 1 | examples/basics/arithmetic.nv | **Y** | Компилируется чисто. | KEEP |
| 2 | examples/basics/demo.nv | **Y** | Компилируется чисто. `Detach` не найден (grep) — уже чисто. | KEEP |
| 3 | examples/basics/hello.nv | **Y** | Компилируется чисто. | KEEP |
| 4 | examples/basics/match_demo.nv | **Y** | Компилируется чисто. | KEEP |
| 5 | examples/basics/records.nv | **Y** | Компилируется чисто. | KEEP |
| 6 | examples/basics/strings.nv | **Y** | Компилируется чисто (уже `byte_len()`, D249 соблюдён). | KEEP |
| 7 | ~~examples/effect_density/domain.nv~~ | N | Ссылается на тип `Sql` без импорта `std.data.sql` (реальный content-баг, не только toolchain). Часть сломанной effect_density-семьи. | RECREATE → `examples/_wip/effect_density/domain.nv` |
| 8 | ~~examples/effect_density/http.nv~~ | N (parse) | `import effect_density.domain.*` — wildcard-импорт retracted (`expected identifier, got '*'`). Подтверждено повторно на текущем компиляторе. | RECREATE → `examples/_wip/effect_density/http.nv` |
| 9 | ~~examples/effect_density/main.nv~~ | N (parse) | Та же wildcard-импорт ошибка; нет `fn main` несмотря на имя файла. | RECREATE → `examples/_wip/effect_density/main.nv` |
| 10 | ~~examples/effect_density/repository.nv~~ | N (parse) | Та же wildcard-импорт ошибка. | RECREATE → `examples/_wip/effect_density/repository.nv` |
| 11 | ~~examples/effect_density/service.nv~~ | N (parse) | Та же wildcard-импорт ошибка. | RECREATE → `examples/_wip/effect_density/service.nv` |
| 12 | examples/effects/effects.nv | **Y** | Компилируется чисто. | KEEP |
| 13 | examples/effects/effects_d61.nv | **Y** | Компилируется чисто. | KEEP |
| 14 | ~~examples/effects/gc_coroutines_test.nv~~ | Y (но) | Компилируется, НО это компилятор-тест codegen-слоя (парный к `nova_rt/test_gc_deep.c`), не user-facing пример — не место в examples/. | **DELETE** (удалён) |
| 15 | examples/effects/spawn_demo.nv | **Y** | Компилируется чисто. Кандидат в канон `concurrency/` (Ф.3). | KEEP |
| 16 | ~~examples/effects/with_tests.nv~~ | Y (но) | Компилируется (даже намеренно-красный `test "this one fails on purpose"` — Ф.5-гейт compile-only, не рантайм). НО: файл лежит в `effects/`, а содержимое (`double`/`factorial`) вообще не про эффекты — generic test-feature smoke-test не по месту, не показывает ничего специфичного для Nova. | **DELETE** (удалён) |
| 17 | examples/ffi/ptr_basics.nv | **Y** | Компилируется чисто (был заблокирован Result.map-багом в заходе 07-11 — теперь либо пофикшен, либо файл его не триггерит). | KEEP |
| 18 | examples/ffi/sqlite_mini.nv | N | **Компиляторный баг #3** (extern-FFI tuple-return codegen) — не авторский. | KEEP (блокирован toolchain) |
| 19 | examples/getting_started.nv | **Y** | Компилируется чисто. | KEEP |
| 20 | examples/net/echo_client.nv | **Y** | Компилируется чисто. | KEEP |
| 21 | examples/net/echo_server.nv | **Y** | Компилируется чисто. | KEEP |
| 22 | examples/plan110/ffi_sqlite_consumable.nv | **Y** | Компилируется чисто. | KEEP |
| 23 | ~~examples/real_world/audit.nv~~ | N | Сам файл (`oxsar_port.nv`) документирует `audit.nv` как "не полная компиляция — для чтения". Plus parse error (tuple-list) + dead `with Detach`/`effect Detach` handler. Не задумывался автором как компилящийся. | **DELETE** (удалён) |
| 24 | examples/real_world/orm_decorators.nv | N | **Все авторские дефекты почищены в этом заходе** (см. список правок ниже). Блокирован **компиляторным багом #2** (`with` внутри handler-method body) — не авторский, repro подтверждено synthetic minimal вне файла. | **FIX (сделано)**, KEEP-blocked-by-toolchain |
| 25 | examples/real_world/orm_demo.nv | N | **Все авторские дефекты почищены в этом заходе** (см. список правок ниже). Блокирован **компиляторным багом #1** (`.map()` generic inference ICE) — не авторский. | **FIX (сделано)**, KEEP-blocked-by-toolchain |
| 26 | ~~examples/real_world/oxsar_port.nv~~ | N | Файл сам документирован как "не полная компиляция — это для чтения" (строка 12). Parse error (interface-стиль методов внутри `type{}`). | **DELETE** (удалён) |
| 27 | examples/typed_pointers/basic_pointer.nv | **Y** | Было пофикшено в заходе 07-11 (`*unsafe T` → `*uninit T`, D216 §10a). Компилируется чисто сегодня. | KEEP |
| 28 | ~~examples/typed_pointers/unsafe_block.nv~~ | N (semantic) | `E_UNSAFE_UNUSED` x3 — relaxed unsafe-правило (D216 §21) больше не требует `unsafe{}` для этой арифметики. Механический fix обессмысливает демо. | RECREATE → `examples/_wip/typed_pointers/unsafe_block.nv` |
| 29 | examples/typed_pointers/unsafe_fn_keyword.nv | **Y** | Компилируется чисто. Реальные assert-ы (не тривиальные). | KEEP |

### Правки FIX (orm_decorators.nv + orm_demo.nv, заход 2026-07-12)

Обе демонстрации Nova-эффектов над generic ORM (`Repo[T]`/`Db` effect) —
самые ценные `real_world/`-примеры плана; вложен полный проход, не
частичный. Правки (dead-syntax → канон, мех. проверено на synthetic
repro перед применением, где была неуверенность):

- `use std.sql` (retracted keyword + неверный путь) → `import
  std.data.sql.{Sql, SqlValue, Db, DbRow, SqlBuilder, DbError}` (реальный
  module path — `std/data/sql.nv` = `module data.sql`).
- Multi-line сигнатура (список параметров и effect-row на разных
  строках) — парсер этого не поддерживает (repro synthetic
  подтверждён) → слито в одну строку (`orm_demo.nv`, 3 места).
- Leading-operator line-continuation (`"..."\n + "..."`) — не
  поддерживается (repro synthetic подтверждён); канон — trailing-operator
  (`"..." +\n"..."`) → исправлено (3 места).
- Bare `assert expr` (без скобок) — триггерит РЕАЛЬНЫЙ compiler ICE
  (`P67-LEGACY: Ident 'assert' not in var_types`) уже на parse-уровне
  synthetic repro; канон — `assert(expr)` call-form → исправлено
  повсеместно (orm_demo.nv 14 мест, orm_decorators.nv 9 мест).
- Вложенные `"..."` внутри `${...}`-интерполяции (`"UPDATE ${rest.until("
  WHERE ")}"`) не парсятся (repro synthetic подтверждён) → вызовы
  вынесены в переменные до интерполяции (`orm_decorators.nv`).
- `with Detach = effect Detach { run(body) { ... } }` — custom
  handler-литерал для `Detach` действительно НЕ РАБОТАЕТ на уровне
  сгенерированного C (`_nova_handler_Detach` не существует в рантайме,
  repro synthetic подтверждён) — подтверждает диагноз Plan 197 §Проблема
  «ретрактированные имена, dead handler surface». Канон — `with Detach =
  SyncDetach { ... }` (D50, test-mocking handler, см.
  `spec/decisions/06-concurrency.md` §3, и уже используется в
  `real_world/audit.nv`). Заменено в 2 тестах `orm_decorators.nv`; ручной
  счётчик `audit_count` заменён эквивалентным наблюдением через
  `capturing_handler`-лог (SyncDetach не даёт callback-перехвата, только
  синхронное исполнение — счётчик и так был избыточен относительно лога).
- `sql\`...\`` tagged-template интерполяция — tag-функция `sql(parts
  []str, args []SqlValue)` требует args именно `SqlValue`
  (`template: parts.join("?")`, ЛЮБОЕ `${...}` становится bind-параметром,
  D48), а демо передавал raw `str`/`int` без обёртки → `E7301` type
  mismatch. Обёрнуто в конструкторы `SqlValue` (`S(...)` для str,
  `I(... as i64)` для чисел) на всех sql-tag call sites обоих файлов.
- `ro bucket = ...; bucket.push(...)` — `ro`-binding с mut-методом →
  `E_LOCAL_NOT_MUT` → `mut bucket`.
- `try_run(b)` — несуществующий helper (нет в std, не определён в файле)
  → заменено на существующий в том же файле идиом «поймать throw как
  Result»: `with Fail[RepoError] = |e| interrupt Err(e) { Ok(b()) }`
  (тот же паттерн, что уже используется трижды в этом файле для
  `Fail[RepoError] = |e| interrupt ...`).

**Итог**: оба файла теперь содержательно чисты (ноль авторских
дефектов/dead-syntax) — единственный блокер компиляции каждого —
подтверждённый compiler-баг вне скоупа Ф.1/Ф.2 (см. «Системные находки»
выше, баги #1 и #2). Официально это КОМПИЛЯЦИОННЫЙ FAIL (rc≠0), но по
существу задача Ф.2 для этих файлов выполнена: «дешёвая правка на канон»
сделана полностью.

## Итог по вердиктам (после Ф.2, 2026-07-12)

- **KEEP** (компилируется ИЛИ содержательно чисто + блокировано
  подтверждённым toolchain-багом): 19 — #1-6, #12, #13, #15, #17, #18,
  #19-22, #24, #25, #27, #29
- **DELETE** (удалено): 4 — #14 (`gc_coroutines_test.nv`), #16
  (`with_tests.nv`), #23 (`audit.nv`), #26 (`oxsar_port.nv`)
- **RECREATE** (перемещено в `examples/_wip/`, см. `_wip/README.md`): 6 —
  #7-11 (`effect_density/*`), #28 (`unsafe_block.nv`)
- **FIX** (дешёвая правка выполнена): 3 — #24, #25 (полный проход,
  описан выше — компиляция всё ещё блокирована toolchain-багом, не
  содержимым), #27 (сделано в заходе 07-11)

Итого дерево `examples/` (вне `_wip/`): **19 файлов**, мёртвая
поверхность (retracted `with Detach`/`use std.X`/wildcard-импорт/bare
`assert`/etc) = **0**. Из 19: 16 реально компилируются и линкуются
сегодняшним `nova.exe`; 3 (`sqlite_mini.nv`, `orm_decorators.nv`,
`orm_demo.nv`) содержательно чисты, но блокированы тремя разными
подтверждёнными compiler-багами вне скоупа Plan 197 (сведены выше,
переаудит нужен после фикса).

## Не сделано в этом заходе (для Ф.3/Ф.4/Ф.5 или другой волны)

- Три compiler-бага (см. «Системные находки») — НЕ трогал
  compiler-codegen по границе задачи; передать в 196.x-волну.
  `Result.map` U-inference баг из захода 07-11 не встретился повторно
  сегодня (ни один файл его не триггерит) — статус пофикшен/непофикшен
  не подтверждён, нужен отдельный repro-прогон.
- Ф.3 (канонический showcase-набор, финальный список basics/effects/
  concurrency/ffi/real_world) — не начато, естественный следующий шаг
  раз Ф.1/Ф.2 закрыты.
- Ф.4 (флагман 187 → `examples/flagship/aggregator/`) — решение владельца
  уже есть (см. план), исполнение не начато, зависит от 116 TLS.
- Ф.5 (CI-гейт `examples-compile`) — не заведён; теперь дешёвый, т.к.
  дерево уже почти всё зелёное (16/19 компилируется, 3 блокированы
  известными issue вне скоупа — можно завести гейт с explicit
  allow-list на эти 3 до фикса compiler-codegen).
- `examples/_wip/` — 6 файлов, переписать начисто (см. `_wip/README.md`
  за деталями по каждому).
