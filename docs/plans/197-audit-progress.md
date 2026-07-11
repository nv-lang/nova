<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# Plan 197 Ф.1 — audit-progress чекпойнт (per-file)

Рабочий чекпойнт для аудита `examples/**/*.nv` (`nova build <file>`, C-codegen).
Коммитится после КАЖДОЙ строки — если сессия оборвётся, следующий заход
продолжает с последнего коммита (файлы с вердиктом ниже — пропускаются).

Вердикты: **KEEP** (компилится/предположительно канон) · **FIX** (дешёвая
правка: `str.len`→`byte_len`, `with Detach`→актуальная модель,
retracted-имена) · **DELETE** (старый синтаксис/фичи или не user-facing
контент — чинить не окупается) · **RECREATE** (концепт нужен, переписать
начисто).

## ⚠️ Системные находки (блокируют бОльшую часть аудита)

Прогон `nova build` по всем 29 файлам (env NOVA_GC_* на main-repo vcpkg,
nova.exe = `nova-cli/target/release/nova.exe`) вскрыл **ДВА
toolchain-бага**, не связанных с содержимым примеров — компилятор ломается
даже на synthetic minimal `fn main() { println("hello") }` вне examples/:

1. **ICE P67-LEGACY** — `nova: internal error at emit_c.rs:48630: method call
   \`.offset\` return type unknown ... obj=Member(<expr>.ptr)`. `<expr>` —
   компилятор-синтезированное выражение (не пользовательский код); span
   идентичен во всех срабатываниях (2424..2438, file_id 24) → похоже на
   баг в prelude/std codegen-пути (вероятно строковый литерал → println),
   а не в конкретном файле. Репродуцируется на пустом hello-world.
2. **Result.map U-inference** — `codegen error: cannot infer method-level
   type argument \`U\` for \`Result.map\`` — в файлах, где нет НИ ОДНОГО
   текстового `.map(` на Result (проверено grep) → тоже компилятор-side,
   не авторский баг примера.

**НЕ трогал compiler-codegen** (по границе задачи — там работает другой
агент; вероятно это то, что он чинит). Для файлов, упавших ТОЛЬКО на этих
двух багах, вердикт **KEEP** ниже означает «содержимое предположительно ок,
сборка блокирована toolchain-регрессией — нужен повторный прогон после
фикса emit_c.rs/Result.map, не в рамках Ф.1». `examples/STATUS.md` уже
предупреждает пользователей, что примеры сейчас не гарантированно собираются.

## Таблица

| # | Файл | Компилится? | Находки | Вердикт |
|---|------|-------------|---------|---------|
| 1 | examples/basics/arithmetic.nv | N (ICE-1) | Блокирован ICE-1, репро на minimal hello. Контент не проверен иначе. | KEEP |
| 2 | examples/basics/demo.nv | N (ICE-1) | Блокирован ICE-1. `Detach` НЕ найден в файле (grep) — расходится с формулировкой Plan 197 §Проблема (возможно уже почищено ранее / текст плана устарел). | KEEP |
| 3 | examples/basics/hello.nv | N (ICE-1) | Минимальный hello-world сам триггерит ICE-1 (подтверждено на synthetic копии вне examples/). | KEEP |
| 4 | examples/basics/match_demo.nv | N (ICE-1) | Блокирован ICE-1. | KEEP |
| 5 | examples/basics/records.nv | N (ICE-1) | Блокирован ICE-1. | KEEP |
| 6 | examples/basics/strings.nv | N (ICE-1) | Блокирован ICE-1. Проверено: уже использует `byte_len()` (НЕ `.len()`) — D249 не требуется. | KEEP |
| 7 | examples/effect_density/domain.nv | N (Result.map) | Блокирован Result.map-багом. Часть сломанной effect_density-семьи (см. #8-11) — нет отдельного смысла без остальных файлов. | RECREATE |
| 8 | examples/effect_density/http.nv | N (parse) | `import effect_density.domain.*` — wildcard-импорт retracted (`expected identifier, got '*'`). Нет `fn main`. | RECREATE |
| 9 | examples/effect_density/main.nv | N (parse) | Та же wildcard-импорт ошибка. НЕТ `fn main` несмотря на имя файла — только stub-тела `=> ...`. Не собирается как отдельная единица даже после фикса импорта. | RECREATE |
| 10 | examples/effect_density/repository.nv | N (parse) | Та же wildcard-импорт ошибка. | RECREATE |
| 11 | examples/effect_density/service.nv | N (parse) | Та же wildcard-импорт ошибка. | RECREATE |
| 12 | examples/effects/effects.nv | N (ICE-1) | Блокирован ICE-1. | KEEP |
| 13 | examples/effects/effects_d61.nv | N (ICE-1) | Блокирован ICE-1. Контент отличается от effects.nv (D61 handler-substitution), не дубликат. | KEEP |
| 14 | examples/effects/gc_coroutines_test.nv | N (Result.map) | Заголовок файла: "Nova-level tests for GC and coroutines... проверяют codegen-слой" — это компилятор-тест (парный к nova_rt/test_gc_deep.c), НЕ user-facing пример. Не должен жить в examples/. | DELETE |
| 15 | examples/effects/spawn_demo.nv | N (ICE-1) | Блокирован ICE-1. Кандидат в канон `concurrency/` (Ф.3). | KEEP |
| 16 | examples/effects/with_tests.nv | N (Result.map) | Файл = "тесты прямо в файле" (заголовок), включая тест "this one fails on purpose" (намеренно красный) — конфликтует с целью Ф.5 (CI-гейт зелёный). Демонстрация `test`-фичи, не реальный пример. | DELETE |
| 17 | examples/ffi/ptr_basics.nv | N (Result.map) | Блокирован Result.map-багом (в файле нет текстового `Result`/`.map` — баг компилятора, не автора). | KEEP |
| 18 | examples/ffi/sqlite_mini.nv | N (Result.map) | Блокирован Result.map-багом. | KEEP |
| 19 | examples/getting_started.nv | N (ICE-1) | Блокирован ICE-1. | KEEP |
| 20 | examples/net/echo_client.nv | N (ICE-1) | Блокирован ICE-1 (file_id 37, тот же паттерн). | KEEP |
| 21 | examples/net/echo_server.nv | N (ICE-1) | Блокирован ICE-1 (file_id 37). | KEEP |
| 22 | examples/plan110/ffi_sqlite_consumable.nv | N (Result.map) | Блокирован Result.map-багом. | KEEP |
| 23 | examples/real_world/audit.nv | N (parse + Detach) | Заголовок-комментарий у `oxsar_port.nv` явно называет audit.nv образцом "не полная компиляция — для чтения". Плюс parse error (`[(str, str)]` tuple-list синтаксис) И dead `with Detach`/`effect Detach` handler-поверхность (см. Plan 197 §Проблема). Самим автором не задумывался собираться. | DELETE |
| 24 | examples/real_world/orm_decorators.nv | N (parse + Detach) | Parse error: unterminated `${...}` interpolation с вложенным `.until(...)` — вероятно грамматика не разрешает вложенный вызов внутри интерполяции (чинится вынесением в переменную, но НЕ входит в одобренный FIX-CHEAP список в этом заходе). Плюс dead `with Detach = effect Detach` поверхность (реальная модель-правка, не тривиальна). | FIX |
| 25 | examples/real_world/orm_demo.nv | N (parse) | Parse error x3 (строки 201, 324, 471): `EffectRow -> RetType {` на ОТДЕЛЬНОЙ строке от `)` параметров — `expected \`=>\` or \`{\` for function body, got identifier`. Похоже на old multi-line signature style, не входит в одобренный FIX-CHEAP список (риск неверной правки без понимания текущего правила переноса строк). | FIX |
| 26 | examples/real_world/oxsar_port.nv | N (parse) | Файл явно документирован как "Не полная компиляция — это для чтения" (строка 12) — конфликтует с целью Ф.5 (CI-гейт компиляции). Parse error (`type Router { method() -> str ... }` — interface-стиль методов внутри `type{}`, видимо retracted паттерн). | DELETE |
| 27 | examples/typed_pointers/basic_pointer.nv | Y (после FIX) | Было: retracted `*unsafe T` (possibly-uninit pointer) вместо `*uninit T` (D216 §10a rename, Plan 174.5). **Починено в этом заходе** (4 места: 1 код + 3 комментария) → парсится, доходит до того же системного Result.map-бага (не связан с этим файлом). Демо-тесты внутри тривиальны (`assert(1==1)`) — не показывают реальное поведение. | FIX (сделано) |
| 28 | examples/typed_pointers/unsafe_block.nv | N (semantic) | `E_UNSAFE_UNUSED` x3 — тривиальная арифметика внутри `unsafe {}` больше не требует unsafe-контекста. Формальный fix ("убрать unsafe{}") обессмысливает демо (тест называется "unsafe block — basic usage", но перестаёт трогать что-либо unsafe) → нужен реальный unsafe-триггер (deref/pointer op) внутри, не механическая правка. | RECREATE |
| 29 | examples/typed_pointers/unsafe_fn_keyword.nv | N (Result.map) | Блокирован Result.map-багом. Контент содержит реальные assert-ы (не тривиальные `1==1`) — качественнее demo, чем #27/#28. | KEEP |

## Итог по вердиктам

- **KEEP**: 16 — #1,2,3,4,5,6,12,13,15,17,18,19,20,21,22,29 (bОльшая часть заблокирована ДВУМЯ toolchain-багами выше, не собственным содержимым)
- **RECREATE**: 6 — #7,8,9,10,11 (вся effect_density-семья: retracted wildcard-импорт + main.nv без `fn main` + stub-тела), #28 (unsafe_block.nv — демо-концепт инвалидирован relaxed unsafe-правилом)
- **DELETE**: 4 — #14, #16 (компилятор-тесты не на месте в examples/), #23, #26 (реал-ворлд файлы, явно НЕ предназначенные для компиляции автором + parse errors)
- **FIX**: 3 — #24, #25 (найдены, не почищены в этом заходе — не входят в одобренный FIX-CHEAP список, риск неверной правки), #27 (retracted-имя `*unsafe T`→`*uninit T` — **починено и закоммичено** в этом заходе)

## Не сделано в этом заходе (для Ф.2 / следующего захода)

- #24 orm_decorators.nv, #25 orm_demo.nv — найдены конкретные строки/причины (см. таблицу), но правка требует понимания текущих грамматических правил (перенос строк в сигнатуре / вложенные вызовы в интерполяции) — не мех. "поиск-замена", поэтому не тронуто в рамках FIX-CHEAP.
- Все 16 KEEP-файлов нужно ПЕРЕАУДИТИТЬ после фикса ICE-1 / Result.map-бага в compiler-codegen (другим агентом) — текущий "KEEP" не равен "проверено компилируется".
