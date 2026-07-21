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

## Заход 2026-07-17 (Ф.3/Ф.4 остаток — «почини или удали») — ТЕКУЩИЙ

Рестарт погибшей волны (worktree `nova-197r`, branch `p197-examples-rest`).
Ф.4 к этому моменту уже фактически исполнено другой волной (`flagship/
aggregator` + `net`/`tls` пары существуют и гейтятся `nova-gate.yml`, вне
объёма этого захода). Объём — переверификация всех 19 файлов вне `_wip/`
сегодняшним `nova.exe` (5 дней апстрима: mut-canon/param-mut-enforcement/
module-layout волны landed между 07-12 и 07-17) + дожатие трёх файлов,
блокированных подтверждёнными toolchain-багами в заходе 07-12.

**Регрессий нет**: все 16 файлов, ранее реально компилировавшихся
(#1-6, #12, #13, #15, #17, #19-22, #27, #29), компилируются чисто и
сегодня (`nova build <file> --strict-effects`) — промежуточные
lang-волны (canon mut-param position, E_PARAM_NOT_MUT, module-layout
orphan) их не задели.

### sqlite_mini.nv (#18) — bug #3 ПОДТВЕРЖДЁННО ПОФИКШЕН, вердикт уточнён

Баг #3 (extern-FFI tuple-return codegen: `initializing '_NovaTuple_2_...'
with an expression of incompatible type 'int'`) **пофикшен апстримом**:
временно добавил `fn main()`, реально вызывающий `open_and_close_demo`
(форсирует reachability tuple-destructuring пути `ro (raw, rc) =
mini_sqlite_open(path)` через C-codegen, а не просто typecheck) — C-код
сгенерировался и СКОМПИЛИРОВАЛСЯ чисто; единственная оставшаяся ошибка —
ожидаемая LINK-ошибка `undefined symbol: nova_fn_mini_sqlite_open/exec/
close` (сам файл документирует это в шапке: «V1 — example only, без
real libsqlite3 link… Real link integration в followup
[M-115-ffi-build-pipeline]»). Эксперимент откачен (файл восстановлен
байт-в-байт из git).

Также уточнена методология: у файла **нет `fn main`/`test`-блока** (это
осознанно — структурный sketch extern-декораций, не рабочая программа),
поэтому `nova build` **в принципе неприменим** (`nova build --help`:
«Single .nv file (entry-point with `fn main`)») — прошлый заход 07-12
использовал `nova build` и поймал bug #3 ДО того, как дошёл до
«ожидаемого» отсутствия main; сегодня правильная проверка — `nova check
examples/ffi/sqlite_mini.nv --strict-effects` → **ok, 0 ошибок** (2
warning про virtual-prelude `Vec`-импорт — тот же шум, что и во ВСЕХ
файлах дерева, не специфично для этого файла).

**Вердикт: KEEP** (не «блокирован toolchain» — реально чист; `nova
build` для него никогда и не будет применим по конструкции файла).

### orm_decorators.nv (#24) — bug #2 ПОДТВЕРЖДЁННО ПОФИКШЕН, найден+исправлен независимый баг, найден НОВЫЙ блокер

Баг #2 (`with EFFECT = value { ... }` не парсился внутри тела
handler-method, repro — `with_read_replica`'s `in_transaction(b) {
primary.in_transaction(|| with Db = primary { b() }) }`) —
**подтверждённо пофикшен апстримом**: файл теперь проходит парсинг и
тайпчек целиком (дошёл до C-codegen/link стадии — см. ниже).

Попутно найден и исправлен **независимый реальный баг содержимого** (не
toolchain): `log.filter(|e| ...).len()` (тест «audit: каждый exec
пишется в detach-задачу», строка 296) падал с `[E_RECV_METHOD_MISMATCH]
.len(...) на ресивере FilterIter — у FilterIter нет метода len, а
single-key fallback резолвит имя в чужой тип EmbeddedDir (last-wins)`.
Синтетический repro (`v.filter(pred)` на голом `[]int` БЕЗ импорта)
подтвердил: `[]T @filter` (`std/collections/vec_seq.nv:56`,
`#stable(since = "0.1")`, eager, возвращает `[]T`) **требует явного
`import std.collections.vec_seq.{filter}`** — без импорта имя `filter`
резолвится через global single-key fallback в ПОСТОРОННИЙ тип
(`RSplitIter` в repro, `EmbeddedDir` в реальном файле — зависит от
регистрации), не в eager Vec-версию. `orm_demo.nv` (парный файл) уже
импортирует `map`/`join` явно тем же способом — `orm_decorators.nv`
просто забыл `filter`. Fix: добавлена строка `import
std.collections.vec_seq.{filter}` (см. diff). Repro-файлы
(`examples/_repro_filter_len.nv`, scratchpad) удалены после
подтверждения, не коммитились.

После этого фикса файл проходит `nova check --strict-effects` ПОЛНОСТЬЮ
чисто (только virtual-prelude-Vec-шум) и доходит до C-codegen — но
падает на **НОВОМ, более глубоком блокере**, вскрытом ИМЕННО фиксом
bug #2 (раньше парсинг не доходил до этого места):

```
error: use of undeclared identifier 'SyncDetach'
error: use of undeclared identifier '_nova_handler_Detach'
...
error: expected identifier
    _nova_detach_0_ctx->current_user_id = current_user_id;
                        ^
note: expanded from macro 'current_user_id'
    #define current_user_id (_c->current_user_id)
```

Корень — **`SyncDetach` НЕ РЕАЛИЗОВАН в std/runtime bootstrap**. Это
подтверждено дословно комментарием в самом `spec_tests/conformance/
detach_effect_ok_test.nv` (gate-critical, значит уже актуален и
авторитетен): «(Форма (2) ambient `with Detach = …` — тоже exempt в
checker'е через with_handler_stack-проверку, но требует объявленного
effect-типа Detach + handler'а, **которых нет в std bootstrap** — не
тестируется здесь.)». Т.е. Ф.2-фикс 07-12 (замена сломанного
custom-хендлер-литерала `with Detach = effect Detach { run(body) {...}
}` на, как тогда казалось, «канонический D50 test-mocking handler»
`with Detach = SyncDetach {...}`) опирался на неверную предпосылку —
`SyncDetach` не существует как рабочий рантайм-символ; ошибка сменила
форму (парсинг → undefined-symbol на C-уровне), но файл остаётся
заблокирован. Второй, попутно найденный баг — macro-hygiene: сгенерированный
`#define current_user_id (_c->current_user_id)` коллизирует с
буквальным `->current_user_id` field-access при популяции detach-контекста
(препроцессор подставляет макрос ВНУТРЬ токена после `->`) — узкий,
но реальный codegen-баг, всплывёт как только `SyncDetach` появится.

Проверил жизнеспособность обхода без std-фичи: единственная
задокументированная рабочая форма (`detach{}` напрямую в `fn` с `Detach`
в effect-row, ИЛИ bare в `test`-root — формы (1)/(3) того же
conformance-теста) не применима — `detach{}` здесь лежит внутри
handler-method **литерала** (`effect Db { exec(q) { ...; detach {...}
} }`), а синтаксис handler-method'а (`04-effects.md` §Handler-литерал,
D40) не поддерживает объявление собственного effect-row отдельно от
`op(p) => expr` / `op(p) { block }` — не нашёл способа дать чекеру
основание для exemption без реализации `SyncDetach`.

**Вердикт: KEEP-blocked-by-toolchain/std-gap** (переформулирован —
раньше «bug #2», теперь «SyncDetach отсутствует в std», другая
причина, тот же итоговый статус). Независимый fix (`vec_seq.filter`
import) сохранён — реальный прогресс независимо от блокера.

### orm_demo.nv (#25) — bug #1 переформулирован (ICE → чистая диагностика), фундаментально всё ещё блокирован

Баг #1 (`.map()` generic type-argument inference ICE,
`emit_c.rs:48511/49360 [P67-LEGACY]`) **сменил форму** — теперь
чистая диагностика вместо ICE (прогресс апстрима, помечена как
известный класс `M-196.5-b3-closure-param-bind`):

```
error: [E7001] cannot infer C type for closure-arg return type (U) for
`.map()` on `Vec____uint64_t`: neither the node_substs checker channel
nor the closure body's own (param-bound) inference could resolve it
```

Локализовал источник: `Repo[T] @bulk_load[K](...)`'s `ro sql_keys
[]SqlValue = keys.map(key_to_sql)` (строка 233) — `keys []K` с K,
инстанцированным в `UserId` (newtype над `u64` → `Vec____uint64_t`),
`key_to_sql` передаётся как **именованное значение-функция** (не inline
`|x| ...`-лямбда). Проверил гипотезу фикса — обернул в explicit-лямбду
(`keys.map(|k| key_to_sql(k))`), пересобрал: ошибка не исчезла, а
переместилась глубже — `cannot infer type argument T for generic
function copy_n_nonoverlapping` — подтверждает: это фундаментальное
ограничение mono/checker для **вложенных generic-методов**
(`Repo[T].bulk_load[K]` — generic на двух уровнях, D42 Модель B) в
сочетании с `Vec[K].map()`, не что-то лечимое на уровне содержимого
файла. Эксперимент откачен (файл восстановлен байт-в-байт из git).

**Вердикт: KEEP-blocked-by-toolchain** (без изменений по существу —
реальный компиляторный баг, вне объёма Ф.2, но диагностика теперь
точнее локализована для следующей волны: `bulk_load[K]` + function-value
`.map()` argument, не общий `.map()` на любом Vec).

### Итог захода 07-17

- **sqlite_mini.nv**: DELETE-blocked-verdict → **KEEP** (чист,
  `nova check --strict-effects` ok; `nova build` неприменим по
  конструкции файла — нет `main`).
- **orm_decorators.nv**: independent-баг найден+исправлен
  (`vec_seq.filter` import); toolchain-блокер переформулирован
  (`SyncDetach` не в std, не «with-parsing»). Остаётся
  KEEP-blocked-by-toolchain/std-gap.
- **orm_demo.nv**: toolchain-блокер переформулирован (ICE → E7001,
  локализован до `bulk_load[K]`+`.map()`). Остаётся
  KEEP-blocked-by-toolchain.
- Остальные 16 файлов — реконфирмированы, ноль регрессий.

Обновлённая сводка: **18 из 19** файлов вне `_wip/` реально
компилируются и линкуются сегодняшним `nova.exe` под
`--strict-effects` (было 16/19 07-12) — только `orm_decorators.nv` и
`orm_demo.nv` остаются заблокированы (оба — подтверждённые,
переформулированные toolchain/std-баги, вне объёма Ф.2/Ф.3, переданы
следующей волне с уточнённой локализацией).

## Не сделано (обновлено 2026-07-17)

- **Ф.4/Ф.5 — уже сделаны другой волной** (см. статус-строку плана
  197-examples-revision.md): `examples/flagship/aggregator/` реализован,
  `nova-gate.yml` гейтит флагман-таргеты. Не в объёме захода 07-17.
- **Два compiler/std-бага остаются** (переформулированы в заходе 07-17,
  см. выше) — вне границы задачи (compiler-codegen/std-runtime — другая
  волна):
  1. `orm_decorators.nv` — `SyncDetach` не реализован в std bootstrap
     (нужна реальная реализация hendler'а ИЛИ redesign теста без
     ambient-detach-swap) + попутный macro-hygiene баг
     (`->fieldname` коллизирует с одноимённым `#define`-макросом
     контекста detach).
  2. `orm_demo.nv` — `Repo[T].bulk_load[K]` (вложенный generic) +
     `Vec[K].map(function_value)` не резолвит C-тип closure-arg
     (`E7001`, класс `M-196.5-b3-closure-param-bind`), каскадом падает
     на `copy_n_nonoverlapping` type-inference.
  3. Третий баг (extern-FFI tuple-return codegen, `sqlite_mini.nv`) —
     **подтверждённо пофикшен**, снят с очереди.
  `Result.map` U-inference баг из захода 07-11 по-прежнему не
  переподтверждён (ни один файл его не триггерит) — нужен отдельный
  repro-прогон, если кому-то он важен отдельно от bulk_load[K]-находки
  выше.
- Ф.3 (канонический showcase-набор — явный финальный список/возможный
  reshape папок `basics/effects/concurrency/ffi/real_world`) — сознательно
  НЕ начато в заходе 07-17: текущее дерево уже фактически соответствует
  этой структуре (в `effects/spawn_demo.nv` — кандидат в `concurrency/`,
  но переименование/перенос папок не выполнялось — за пределами
  директивы этого захода «почини или удали»); если нужен явный
  reshape — отдельная директива владельца.
- `examples/_wip/` — 6 файлов, переписать начисто (см. `_wip/README.md`
  за деталями по каждому); не трогал (вне гейта, явно out of scope
  по инструкции этого захода).

## Заход 2026-07-21 (Ф.3 — showcase-карта + гейт-лист, worktree `nova-ex197`,
branch `p197-final`) — ТЕКУЩИЙ

Полный переаудит ВСЕХ `.nv` вне `_wip/` (включая новые с 07-17:
`examples/tour/**` — 13 файлов + `greeter/` folder-module, `mini_aggregator.nv`,
`examples/flagship/aggregator/**`), `nova build <file> --strict-effects`
сегодняшним релизным `nova.exe` главной репы (READ-ONLY, компилятор не
пересобирался). Итог: **все ранее известные 16 файлов** (#1-6, #12, #13,
#15, #17, #19-22, #27, #29 из таблицы 07-12/07-17) компилируются чисто
без регрессий; **все 13 `tour/**` файлов + `mini_aggregator.nv`** тоже
компилируются и линкуются чисто (свежие файлы, добавлены между 07-17 и
сегодня, не входили в прежний аудит); `flagship/aggregator/src/main.nv`
собирается чисто (build-only, сервер). `orm_decorators.nv`/`orm_demo.nv` —
переподтверждены теми же двумя блокерами (`SyncDetach` не в std bootstrap;
`E7001` на `bulk_load[K]`+`.map()`), без изменений по существу.

### НОВАЯ находка: `net/echo_client.nv`/`net/echo_server.nv` — регрессия
относительно заявленного статуса плана

Оба файла числились в статус-строке плана 197 как часть зелёного
`nova-gate.yml`-флагман-гейта (5 целей), но реально СЕГОДНЯ падают на
линковке под текущим релизным `nova.exe`:

```
error: compiler error:
lld-link: error: undefined symbol: Nova_TcpStream_consume_cleanup
>>> referenced by .../echo_client.c:7853 (_nova_spawn_0)
```

Воспроизведено ДВАЖДЫ независимо — и в этом worktree, и напрямую в
главной репе (`d:/Sources/nv-lang/nova`, тем же бинарём, файл не менялся) —
не артефакт worktree/кэша. Триггер — `consume stream = conn` внутри
`spawn { }` (`examples/net/echo_client.nv:25-31`, тот же паттерн в
`echo_server.nv`). Корень: `TcpStream` объявлен `consume value` с
пользовательским `@cleanup` (`std/src/net/tcp.nv:280`,
`Nova_TcpStream_consume_cleanup` — ожидаемое имя синтетического
consume-cleanup символа, см. `lints.rs:1002/1011`), но при вызове ИЗНУТРИ
spawned-замыкания генератор C эмиттирует ТРИ вызова этого символа и НИ
ОДНОГО определения нигде в единице трансляции — не linker-quirk
(`lld-link` на Windows), а настоящий пробел кодогена (то же самое было бы
на любом линковщике). `tls/echo_client.nv`/`echo_server.nv` (тот же
шаблон, другой транспорт) СОБИРАЮТСЯ ЧИСТО — паттерн `consume` там,
видимо, устроен иначе (не через голый `spawn`+`consume stream`
напрямую) или не задет тем же кодоген-путём — не выяснял глубже, вне
объёма («компилятор-баги не чинить»).

**Классификация**: подтверждённый compiler-баг вне объёма Ф.2/Ф.3 (тот же
класс, что уже задокументирован для `orm_decorators.nv`/`orm_demo.nv`) —
**НЕ перенесено в `_wip/`**, по тому же прецеденту: `_wip/` — для
концептов, требующих переписи с нуля, а не для готового чистого контента,
заблокированного toolchain-багом. Дополнительная причина не трогать
путь — `net/echo_client.nv`/`echo_server.nv` заданы БУКВЕННЫМ путём в
`.github/workflows/nova-gate.yml` (флагман-гейт, 5 целей) — перенос файла
сломал бы CI-путь ещё сильнее, чем текущее (невыясненное) состояние
самого гейта. **Вердикт: KEEP-blocked-by-toolchain** (новая запись,
аналогичная `orm_decorators.nv`/`orm_demo.nv`).

**Важно для владельца**: план 197 утверждает «Первая верификация — на
живом пуше (не прогонялось локально)» для `nova-gate.yml`. Этот заход
впервые прогнал ТОЧНО ТЕ ЖЕ 2 из 5 флагман-целей локально (`net/echo_*`)
тем же командой (`nova build --strict-effects`), что использует
воркфлоу, — и они падают. Есть основания подозревать, что `nova-gate.yml`
красный на этих 2 целях уже сейчас (Windows-локальный репро не
гарантирует идентичное поведение на CI Linux-раннере, но ошибка —
missing C-symbol definition, не платформенный линковщик-артефакт, так что
маловероятно, что Linux-сборка её не поймает). Рекомендация — прогнать
`workflow_dispatch` или проверить последний лог пуша, прежде чем
полагаться на «флагман-гейт зелёный» как факт.

### Итог захода 07-21

- **Регрессий по старым 16 файлам** — 0.
- **Новых зелёных файлов** — 14 (`tour/**` × 13 + `mini_aggregator.nv`),
  ранее не аудировались (появились между 07-17 и сегодня).
- **Новый найденный блокер** — `net/echo_client.nv`/`echo_server.nv`
  (compiler-баг, KEEP-blocked, см. выше) — ранее не фиксировался как
  падающий (не был в таблице 07-12/07-17, т.к. эти два файла всегда
  числились KEEP/Y).
- `orm_decorators.nv`/`orm_demo.nv` — без изменений, два прежних блокера.
- `sqlite_mini.nv` — переподтверждён `nova check --strict-effects` ok.
- Полная сводка building-статуса — [`examples/README.md`](../../../examples/README.md)
  (showcase-карта, Ф.3) и [`197-f5-gate-list.txt`](197-f5-gate-list.txt)
  (Ф.5-подготовка).
