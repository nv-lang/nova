# Отчёт 133: триаж 48 отказов E_NOVAC_SUBSET корпуса 118

## РЕПРО

Не применимо к прогону: замер по УЖЕ СОХРАНЁННЫМ уликам задачи 128
(`doc/tasks/128-proverit-reliznyy-build-carina-0-3-118-p/evidence/`:
`table-118.tsv` 118 строк + `logs-118/` 118 дословных логов; main `d6d026947`,
Windows, clang, pinned Carina B1). Ничего не чинилось и не запускалось повторно;
все «прогоны» ниже — чтение сохранённых файлов и разбор их JSON.

## ТОЧКА (метод)

Строки `table-118.tsv`, содержащие `E_NOVAC_SUBSET`, дают 48 входов. Для каждого
входа из его лога `L<NNN>.txt` (NNN = номер строки таблицы; у таблицы НЕТ строки
заголовка — проверено сверкой `argv`/`staged under` лога с файлом строки) взят
первый JSON-диагностик с `"code":"E_NOVAC_SUBSET"` и из него дословный
`primary.message`. Класс = точный текст сообщения. Сумма сошлась двумя путями:
по таблице 48 строк и по логам 48 сообщений.

Контроль полноты корпуса: 118 = 62 PASS + 56 FAIL; 56 FAIL = 48 E_NOVAC_SUBSET
(строки 8, 48, 62–108, 114, 115, 117) + 3 E_NOVAC_LANG (№1929: строки 68, 69, 71)
+ 1 E_NOVAC_ICE (№1924-подпись: строка 113 `modules_tour.nv`) + 4 «Carina-generated
C compile/link failed» (строки 72, 109, 110, 118). Остатка нет.

## Ранжированный список классов (точные подписи, по убыванию)

| № | Число | Дословная подпись (`primary.message`) | Входы (строка таблицы = лог; файл; спан) |
|---|---|---|---|
| 1 | **12** | `outside the subset: no such method in the declarations handed to novac` | L078–L083 `flagship/aggregator/src/app/{aggregate,aggregate_test,emit,live,live_test,scenarios}.nv` — ВСЕ ШЕСТЬ указывают в одно место `aggregate.nv 2810-2817` = `map_err`; L085 `aggregator/src/main.nv 2917-2925` = `flat_map`; L094 `net/echo_client.nv 744-751` = `connect`; L095 `net/echo_server.nv 1155-1165` = `local_port`; L101 `tls/echo_client.nv 1453-1465` = `verification`; L102 `tls/echo_server.nv 1880-1890`; L103 `tour/collections_tour.nv 462-468` = `filter` |
| 2 | **8** | `outside the subset: a record constructor is compiled only as a binding initializer, a call argument, the body of a match arm, the tail of a value body or the right side of = -- other positions are not compiled yet (E2, the records/sums horizon)` | L086–L091 `flagship/http_proxy_chain/src/{config,http,main,relay,upstream,upstream_socks5}.nv` — все шесть в `http.nv 2072-2086` = `terminator_len`; L098 `plan110/ffi_sqlite_consumable.nv 3092-3093`; L105 `tour/consume_tour.nv 935-936` |
| 3 | **5** | `novac did not parse this -- and it cannot tell whether the form is one it does not read yet or the text is malformed; this is the parser's fallback, narrowed form by form` | L048 `basics/string_literals.nv 1352-1353`; L064 `effects/serde_fs_build.nv 1384`; L074 `aggregator/regressions/spawn_capture_value_struct/…nv 1016-1017`; L093 `mini_aggregator.nv 2053-2054`; L104 `tour/concurrency.nv 965-966` |
| 4 | **3** | `outside the subset: effect `Counter` has no runtime pieces in the shell novac links into -- its operation table, thread slot and dispatchers are the shell's, carried for the std effects its probe uses; an effect a file declares is not emitted yet (E5, effect declarations)` | L062 `effects/effects.nv 102-108`; L063 `effects/effects_d61.nv 524-530`; L106 `tour/effects_tour.nv 346-352` |
| 5 | **3** | `outside the subset: this type form is not compiled yet (slices, tuples, fn types and qualifiers arrive with generics, E2-b)` | L066 `ffi/ptr_basics.nv 343-345`; L067 `ffi/sqlite_mini.nv 614-616`; L108 `tour/ffi_tour.nv 429-431` |
| 6 | **3** | `outside the subset: an effect attribute is read but not compiled yet` | L076, L077 `aggregator/src/api/{report_json,report_json_test}.nv` — оба в `report_json.nv 3599-3600` = `#`; L084 `aggregator/src/domain/domain.nv 1353-1354` = `#` |
| 7 | **2** | `fn declares a return type but its body ends without a value` | L096 `os/agent_runner.nv 1943`; L097 `os/pty_session.nv 1897` |
| 8 | 1 | `outside the subset: a spread `...xs` inside an array literal is not compiled yet (274.5 P5)` | L008 `basics/demo.nv 578-579` |
| 9 | 1 | `unknown name: nothing with this name is bound at this point` | L065 `effects/spawn_demo.nv 2264-2272` |
| 10 | 1 | `outside the subset: an interpolation slot of a non-primitive type without its own `@display(mut f Fmt)` (`Option[T]`, a record that relies on `#impl(Display)`, a generic instance) is not compiled yet (274.7 B1b) -- every primitive family and every plain type with `@display` is` | L070 `aggregator/regressions/interp_to_str_fallback_valuerecord_recv/…nv 844-845` |
| 11 | 1 | `outside the E1 subset: generic parameters are not compiled yet (planned for E2-b2)` | L073 `aggregator/regressions/serde_encode_pointer_op/…nv 2467-2468` |
| 12 | 1 | `outside the subset: an array literal is compiled only as an initializer, a call argument, a constructor field, the body of a match arm, the right side of = or the tail of an if-value branch -- other positions are not compiled yet (E2, the basics horizon)` | L075 `aggregator/regressions/spawn_throw_multifield_payload/…nv 729-730` |
| 13 | 1 | `outside the subset: effect `Audit` has no runtime pieces in the shell novac links into -- … (E5, effect declarations)` (та же подпись, что №4, имя эффекта `Audit`) | L092 `getting_started.nv 2758-2764` |
| 14 | 1 | `outside the subset: a handler literal is compiled only as a with binding's handler or a local binding's initializer (`ro h = effect E { ... }`) that captures no `mut` binding -- … (E4, boundary E.5 of plan 274.11)` | L099 `real_world/orm_decorators.nv 3402-3408` |
| 15 | 1 | `outside the subset: `#impl` of this protocol is derived only for `Display`, `Equal` and `Serialize` -- the derived methods of the others (`Deserialize`, D341; `Hash`, `Clone`, `Compare`, `Debug`) are not synthesized yet (E2-b3)` | L100 `real_world/orm_demo.nv 3192-3193` |
| 16 | 1 | `outside the subset: the shell novac links into carries no `Result` instance for `NovaRes_nova_int_Nova_ParseErr_p` -- the interop surface is the probe program (novac/probe/shell_probe.nv, E2-b); novac emits no `Result` instance of its own yet (E2-b3)` | L107 `tour/errors.nv 204-207` |
| 17 | 1 | `a bare `None` here has no type to take: it stands for an Option only where a return type says which one (`-> Option[int]`, as a tail or a `return` operand) -- a typed binding or a later assignment is not compiled yet (E2-b3)` | L114 `tour/patterns.nv 763-767` |
| 18 | 1 | `outside the subset: a format specifier in string interpolation (`${expr:?}`, D229) is not compiled yet (274.7 B1b, the Display family) -- write the value plain and format it by hand` | L115 `tour/strings_tour.nv 509-518` |
| 19 | 1 | `outside the subset: an `extern` declaration is read, but novac emits no FFI call yet` | L117 `typed_pointers/basic_pointer.nv 946-952` |

**Сумма: 12+8+5+3+3+3+2+12×1 = 48.** Сходится.

## Инженерные классы (склеенные по корню)

| Класс | Число | Доля |
|---|---|---|
| A. Методы, которых нет в декларациях у novac (std-поверхность: `map_err`, `flat_map`, `connect`, `local_port`, `filter`, …) | **12** | 25% |
| B. Позиции конструктора записи вне разрешённых (E2, records/sums horizon) | **8** | 17% |
| C. Парсер-fallback: форма не читана, отличить «не читает» от «сломан текст» сид не может | **5** | 10% |
| D. Объявления эффектов файла не эмитятся (E5: `Counter`×3 + `Audit`×1) | **4** | 8% |
| E. Чтение effect-атрибута `#` | **3** | 6% |
| F. Несобранные типовые формы E2-b (slices/tuples/fn types/qualifiers) | **3** | 6% |
| G. Тело fn без значения при объявленном возврате | **2** | 4% |
| H. Прочие точечные горизонты (по 1: spread, unknown name, интерполяция без `@display`, генерики, позиции массива-литерала, handler-литерал, `#impl`-деривация, `Result`-инстанс, голый `None`, `${x:?}`, `extern`/FFI) | **11** | 23% |

**Доминируют A+B+C: 25 из 48 (52%).** Рычаг внутри A: шесть входов aggregator-свиты
(L078–L083) — ОДИН вызов `map_err` в `aggregate.nv 2810-2817`; шесть входов
http_proxy_chain (класс B) — одно место `http.nv 2072-2086` (`terminator_len`).
Один точечный фикс в каждом закрывает по 6 входов корпуса.

## Пересечения

**№1924 (многомодульные, package-qualified ICE).** Дословная подпись №1924
(`internal compiler error at novac/src/pipeline/program.nv:157 … belongs to module
…, which the entry's imports do not reach`) в корпусе есть ровно один раз:
строка 113 `examples/tour/modules_tour.nv`, код **E_NOVAC_ICE** — она ВНЕ 48
(код не SUBSET). Среди 48 многомодульных (стейджинг `target/carina-build`,
признак `staged under` в логе) — 15 входов: L076–L083, L085 (flagship/aggregator,
9) и L086–L091 (http_proxy_chain, 6). Их отказы — НЕ класс №1924: стейджинг
сработал, сид судил реальные растейдженные исходники и отверг конкретные
подмножествные формы (классы A/B/E выше). То есть №1924 мешает корпусу
(`modules_tour.nv`), но в 48 отказов НЕ входит.

**№1929 (три регрессии E_NOVAC_LANG, D54/D406).** Носители №1929 — строки 68
(`const_receiver_generic_ext_ice`), 69 (`errorkind_variant_arity_collision`), 71
(`monotonic_now_bare_binding`); первый код у них E_NOVAC_LANG, в 48 они НЕ входят.
Оговорка: в логах строк 69 и 71 после LANG-диагностики есть и E_NOVAC_SUBSET —
даже при решении №1924/№1929 в пользу корпуса эти три входа не соберутся, пока
не закрыты их подмножественные формы. Прямого пересечения с 48 нет.

## Критерии приёмки (дословно из задания)

- «каждый класс: число примеров, дословная сигнатура диагностики, перечень
  входов» — таблица выше: 19 точных подписей, у каждой число, дословный текст,
  входы с номерами строк/логов и спанами; ✓ (перечень входов и сумма — машиной;
  отнесение к классу — приёмка глазами, потому что класс = точный текст
  сообщения, группировка по корню — суждение исполнителя)
- «сумма сходится на 48» — 12+8+5+3+3+3+2+12×1 = 48, посчитано по таблице и по
  логам независимо; ✓
- «ничего не исправлено» — правок продукта/реестра/спеки нет, единственные
  изменения — этот отчёт и строка в log.md; ✓
- «отчёт сохранён в задаче» — этот файл, `doc/tasks/133-…/report.md`; ✓

## Прогулы по шагам приёмки

- **fixture:** не применимо: правок кода нет, замер по сохранённым уликам.
- **sabotage:** не применимо (нечего ломать). Вместо него три перекрёстные
  проверки: (а) соответствие «строка таблицы ↔ лог» спаном и `argv` — нашёл и
  устранил сдвиг нумерации на 1 (у `table-118.tsv` нет заголовка); (б) сумма 48
  двумя независимыми путями; (в) расклад 56 FAIL = 48+3+1+4 без остатка.
- **class:** не применимо: анализ, не фикс; класс-кандидат следующей задачи — A (12).
- **registry:** новых дефектов не найдено; 48 подтверждает уже записанные
  горизонты 274.16 (№1929 прямо называет «остальные 48 emit-отказов —
  E_NOVAC_SUBSET, известные горизонты 274.16»). Запись не заводится.
- **spec:** не применимо: язык не меняется.
- **std-check:** не применимо: компилятор и std не тронуты.
- **invariant:** не применимо: новых инвариантов нет.
- **gate/ci:** не применимо к локальному гейту: правок продукта нет, ветка
  содержит только отчёт; integrate-кандидат и CI — на усмотрение приёмщика.
- **commits:** один коммит, английский, DCO `-s`, `git add` по имени, `commit
  --only -- <файлы>`.

## ЧТО НЕ СДЕЛАНО

- Не судил правоту сида против спеки (граница задачи; приоритет источников —
  вопрос владельца, как и в №1929).
- Спаны 7–14 байт декодированы по байтовым смещениям (совпадение с
  идентификаторами `map_err`/`flat_map`/`connect`/`filter` — чистое); для
  спана №2 (14 байт, `terminator_len`) интерпретация та же.
- Прогоны не повторялись; всё — чтение улик 128 из main.
