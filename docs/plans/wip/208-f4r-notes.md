# Plan 208 Ф.4R — прогресс (worktree nova-f4r, branch p208-f4r)

Карта: docs/plans/208-unified-formatter.md §10R «Ф.4R». Модель: sonnet.

## Ш0 — DONE (2 коммита: 845509410, 649e7bdf1)

Эталон-фикстуры сняты прогоном probe.nv (build+run, не `nova test` —
println недоступен в test-блоках) через main-репы нетронутый бинарь.
Probe-скрипты (временные, не в репо):
`C:\Users\B7E3~1\AppData\Local\Temp\claude\d--Sources-nv-lang-nova\a48a9f3a-0403-4a44-a6e3-8894781d4b88\scratchpad\probe.nv`
+ `f4r_probe2.nv` (precision-клэмп доп.).

Файлы (spec_tests/conformance/):
- `d422_f4r_baseline_int.nv`, `d422_f4r_baseline_float.nv`,
  `d422_f4r_baseline_strcharboolu64.nv`. Все PASS на нетронутом дереве
  (3/3), пофайлово standalone-прогоняемые.

**Найденные квирки (ПИНУЮТСЯ, НЕ чинятся в рамках Ф.4R):**
1. `${-0.0:.2}` → `"--0.00"` (двойной минус: magnitude-негация `v<0.0`
   false для -0.0 у nova_fmt_f64_body, а nova_fmt_f64_prefix signbit-aware
   отдельно — конкатенация).
2. `${u64.MAX}` (bare) → `"-1"` (primitive_to_str_fn не покрывает u64 —
   generic-fallback `nova_int_to_str((nova_int)(v))` реинтерпретирует
   bit-pattern как signed).
3. `\xHH`-escape БЕЗ фигурных скобок (комментарий в conv.h устарел).
4. f64 precision КЛЭМПИТСЯ на 64 (nova_fmt_f64_body), НЕ на 340 (fmt_f64/
   f64_fmt_into's собственный клэмп) — `.65`+ разошлись бы без реклэмпа.

## Ш1 — DONE (коммит d2ff4d0be, ИСПРАВЛЕН доп. коммитом — архитектура v2)

`std/src/runtime/fmt_buf.nv` — Debug-escape движок: `str_debug_fmt`/
`char_debug_fmt` + хелперы (write_esc2_at/write_hex_esc_at/utf8_encode_at) —
побайтовый порт nova_str_to_debug_str/nova_char_to_debug_str. Остаётся в
fmt_buf.nv (не завязан на StringBuilder, безопасен как есть).

**АРХИТЕКТУРА v1 (в первом коммите d2ff4d0be) ОКАЗАЛАСЬ БАГОВАННОЙ —
ИСПРАВЛЕНА тем же Ш1 (без нового шага):** первая версия положила
`*_display_spec` семейство В `fmt_buf.nv`, что потребовало
`import std.runtime.string_builder.{StringBuilder}` — ВТОРОЙ конец цикла
(`string_builder.nv` уже импортирует `fmt_buf`). Изначально записал в этот
файл вывод «D29 rev-5/Plan 162 явно поддерживает inter-module циклы,
эмпирически подтверждено» — этот вывод был **ПРЕЖДЕВРЕМЕННЫМ**: мои
верификационные прогоны (baseline + string_builder_test + checksums)
СЛУЧАЙНО не попадали в баг-триггерящую комбинацию. Реальная находка (при
подготовке Ш3, см. ниже) — цикл `runtime.fmt_buf ↔ runtime.string_builder`
ЛОМАЕТСЯ ПОРЯДКОЗАВИСИМО: третий файл, импортирующий ИМЕНА из ОБОИХ
модулей, компилируется/падает в зависимости от ТЕКСТОВОГО ПОРЯДКА своих
`import`-строк:
```
import std.runtime.fmt_buf.{Align}            \  ← в ЭТОМ порядке:
import std.runtime.string_builder.{StringBuilder}  CODEGEN-FAIL
                                                     "undefined identifier
                                                      int_fmt_into" ВНУТРИ
                                                      string_builder.nv
import std.runtime.string_builder.{StringBuilder}  \ ← swap →
import std.runtime.fmt_buf.{Align}                  PASS
```
Voспроизведено (1) через `nova build` И `nova test`, (2) в изоляции (один
файл, без других conformance-пиров), (3) НЕ воспроизводится на нетронутом
main-репо (там `fmt_buf` НЕ импортирует `string_builder` — только диамант,
не цикл — работает в любом порядке). Правдоподобный механизм (НЕ
подтверждён чтением всего imports.rs — вне бюджета волны): collect-first
DFS cycle-guard (imports.rs:1634-1650) возвращает `Ok(())` РАНО при
повторном входе в модуль, уже `in_progress` — если внешний файл первым
трогает `fmt_buf`, DFS входит в `fmt_buf`, тот (по v1-архитектуре) тянет
`string_builder`, который тянет `fmt_buf` НАЗАД — `fmt_buf` уже
`in_progress` → ранний возврат ДО того, как `fmt_buf` полностью собрал
СВОИ декларации в этот проход — `string_builder`, «увидев» неполный
`fmt_buf`, теряет `int_fmt_into` и friends.

**ИСПРАВЛЕНИЕ (архитектура v2, тот же Ш1, коммит `<см. git log>`):**
`*_display_spec` семейство + константы ПЕРЕЕХАЛИ из `fmt_buf.nv` В
`string_builder.nv` (у него УЖЕ был безопасный ОДНОСТОРОННИЙ импорт
`fmt_buf` — цикла больше НЕТ вообще). `fmt_buf.nv` лишился импорта
`string_builder`; взамен `export`-нул то, что `string_builder.nv` теперь
использует напрямую: `int_fmt`, `fmt_f64`, `FmtSpec`, `bool_fmt`,
`char_fmt`, `f32_fmt_into`, `str_debug_fmt`, `char_debug_fmt` (были
module-private, кроме int_fmt_into/f64_fmt_shortest_into/
f32_fmt_shortest_into/Align/FloatKind — те УЖЕ были exported с Ф.1).
Тесты семейства переехали из inline `fmt_buf.nv` в
`string_builder_test.nv` (уже существующий peer-файл с ИДЕНТИЧНЫМ
паттерном импорта `{StringBuilder}`+`{Align}`, НО в БЕЗОПАСНОМ порядке
`string_builder` ПЕРЕД `fmt_buf` — что и объясняет, почему ОН годами не
ловил этот баг).

**Побочная находка при фиксе (реальный баг в МОЁМ Ш1-коде, не
компиляторный):** `f64_display_spec` изначально использовал ОДИН и тот же
`neg = v < 0.0` тест И для магнитуды (`nova_fmt_f64_body`-стиль, НЕ
signbit-aware), И для префикса (должен быть `nova_fmt_f64_prefix`-стиль,
signbit-aware: `v<0.0 || (v==0.0 && 1.0/v<0.0)`) — из-за этого `-0.0` с
precision давал ОДИН минус вместо пинованных ДВУХ (`--0.00`). Исправлено:
раздельные `mag_neg`/`prefix_neg`. Также добавлен `zero_pad bool` параметр
(изначально отсутствовал — width+fill='0' НЕ эквивалентно zero-pad-между-
знаком-и-цифрами; f64_display_spec теперь пере-якорит `@pad_in_place` на
`mark+prefix_len` при zero_pad, зеркаля int_display_spec's `int_fmt`-
нативную семантику). Оба бага пойманы inline-тестами до Ш3, не просочились.

Верификация (после ВСЕХ фиксов): baseline 3/3 PASS + string_builder_test
1/0 (8 test-блоков, вкл. zero_pad-квирк для float) + checksums 3/0 +
ПОВТОРНО подтверждён fmt_buf-до-string_builder import-order сценарий —
теперь PASS (ранее ловушка).

## Ш2 — СТОП: компиляторная находка, ловушка-стоп ПОДТВЕРЖДЕНА (2026-07-20)

**Статус: ЗАБЛОКИРОВАНО. Код НЕ закоммичен, дерево возвращено к чистому
Ш1 (коммит d2ff4d0be — АРХИТЕКТУРА v1, до фикса ниже). Патч попытки —
`docs/plans/wip/208-f4r-sh2-blocked-repro.patch` (не применён, для
справки/воспроизведения).**

**ПОСТФАКТУМ (после находки Ш1-архитектурного фикса ниже):** Ш2's находка
здесь — ТА ЖЕ КЛАССА компиляторный баг, что Ш1's (order-dependent
inter-module cycle resolution), просто через ДРУГУЮ пару модулей
(`runtime.fmt_buf ↔ prelude.protocols`, а не `runtime.fmt_buf ↔
runtime.string_builder`). Ш1's архитектурный обход (относить
StringBuilder-зависимый код В string_builder.nv, а не тянуть
StringBuilder-тип В fmt_buf.nv) — та же СТРАТЕГИЯ применима здесь: если
Ш2 когда-то возобновится, вариант «оставить тела в protocols.nv» (см.
рекомендацию ниже) — это ТОЧНО такой же «не тянуть цикл» ход, не хак.
Компиляторный баг САМ по себе (imports.rs collect-order) остаётся
незачиненным — вне рамок этой волны, находка зафиксирована для отдельного
компиляторного трека.

### Что делалось

Примитивные `@display`/`@debug` (int/f64/f32/bool/char + str's `@debug`)
переезжали из `prelude/protocols.nv` в `fmt_buf.nv` (str's `@display` уже
была некруговой — `f.write(@bytes())` — осталась бы на месте нетронутой).
Тела НЕ звали `*_display_spec` (Ш1-семейство) напрямую — та семья требует
КОНКРЕТНЫЙ `mut sb StringBuilder`, а `@display(mut f Fmt)` получает
АБСТРАКТНЫЙ `Fmt`; вместо этого — маленький scratch `[]u8` + сырые
buffer-примитивы (`int_fmt_into`/`f64_fmt_shortest_into`/
`f32_fmt_shortest_into`, довыставленные `export` `bool_fmt`/`char_fmt`/
`str_debug_fmt`/`char_debug_fmt`) + один `f.write(buf)`. Для этого
`fmt_buf.nv` получил новый импорт: `import std.prelude.protocols.{Fmt,
Display, Debug}`.

### Находка (ПОДТВЕРЖДЕНА эмпирически, изолированным бисекшеном)

Импорт `std.prelude.protocols.{Fmt, Display, Debug}` в `fmt_buf.nv`
(ВТОРОЙ inter-module цикл поверх Ш1's — protocols.nv УЖЕ импортирует
fmt_buf.nv для `Align`/`Sign`/`FmtKind`, значит `runtime.fmt_buf ↔
prelude.protocols` становится двусторонним) ЛОМАЕТ НЕСВЯЗАННЫЙ,
существовавший ДО Ф.4R тест `spec_tests/conformance/d374_write_sink_decouple.nv`
(D374 — StringBuilder-as-Write proof) с ошибкой:

```
error: [E7301] cannot pass `StringBuilder (does not satisfy `Write`;
missing: flush() -> Result[(), IoError])` as argument `sink` of type `Write`
  --> d374_write_sink_decouple.nv:54:30    (ro fmt_ctx = FmtCtx.bare(sb, 0, false))
  note: parameter `sink` declared here --> std/src/prelude/protocols.nv:309:23
```

`flush() -> Result[(), IoError]` — это сигнатура `std.io.Write` (Plan 176,
std/src/io/core.nv:51-54), СОВСЕМ ДРУГОЙ протокол, чем fmt-`Write`
(prelude.protocols.nv:235, `mut @write(bytes []u8) -> ()`, без flush).
`d374_write_sink_decouple.nv` НЕ импортирует `std.io` вообще — компилятор
подставляет НЕ ТОТ `Write` в резолве параметра `sink` у `FmtCtx.bare`,
несмотря на то, что ЭТА декларация физически в protocols.nv, где `Write`
однозначен по построению файла.

**Изолированный бисекшен (backup/restore файлов, `git checkout --`):**
- Ш1-only (fmt_buf.nv импортирует ТОЛЬКО `runtime.string_builder`,
  protocols.nv НЕТРОНУТ) → `d374_write_sink_decouple.nv` ОДИН — PASS 1/0.
- Ш1+Ш2 (fmt_buf.nv ДОПОЛНИТЕЛЬНО импортирует `prelude.protocols.{Fmt,
  Display, Debug}`, тела примитивов перенесены) → ТОТ ЖЕ файл ОДИН
  (никаких других файлов в CU, значит НЕ folder-peer-pollution) —
  CODEGEN-FAIL с E7301 выше.

Вывод: причина — ИМЕННО импорт `prelude.protocols` В `fmt_buf.nv`
(конкретно ДВУСТОРОННИЙ цикл `runtime.fmt_buf ↔ prelude.protocols`, где
`prelude.protocols` — часть дефолт-прелюдии), НЕ Ш1's отдельный цикл
`runtime.fmt_buf ↔ runtime.string_builder` (тот подтверждённо безопасен).
Наиболее вероятный механизм (не подтверждён вглубь imports.rs, за
пределами бюджета этой волны): "collect-first DFS" cycle-guard
(imports.rs:1634-1650, Plan 162 Ф.2) возвращает `Ok(())` РАНО, когда
модуль уже `in_progress` — если DFS входит в protocols.nv (для его СОБСТВЕННОГО
`import fmt_buf` на строке ~63), тот запускает fmt_buf.nv, который ТЕПЕРЬ
своим НОВЫМ импортом просит `prelude.protocols` — а protocols.nv УЖЕ
`in_progress` (пауза на строке ~63, ДО того как дошёл до объявления
`Write`/`Fmt`/`FmtCtx` на строках 235-438) → cycle-guard обрывает сбор
РАНЬШЕ, чем protocols.nv успевает объявить свои Write/Fmt-типы в
это конкретное прохождение — какая-то стадия дальше (структурная
проверка `sink Write`) видит НЕПОЛНУЮ/перепутанную картину и падает на
`io.Write`. Это ГИПОТЕЗА о механизме, не подтверждённая чтением полного
imports.rs — для точного диагноза нужен отдельный компиляторный агент.

### Классификация: ЭТО и есть ловушка-стоп брифа

Симптом — НЕ буквально «протокол-диспатч не резолвит МОИ extension-тела»
(ни один из ошибок не упоминает int/f64/bool/char/str @display/@debug
напрямую) — а КОСВЕННЫЙ побочный эффект: тот же класс проблемы
(cross-module цикл через `runtime.fmt_buf ↔ prelude.protocols`,
D267-видимость / collect-order) корродирует РЕЗОЛВ типа `Write` в
СОВЕРШЕННО НЕСВЯЗАННОМ файле того же compile unit. Это компиляторная
находка, попадающая под явное «НЕ городить обход, чекпоинт+СТОП+отчёт»
брифа. Обход (например, дать `Write`/`Fmt` разные локальные алиасы,
или закрыть цикл через промежуточный модуль) НЕ предпринимался
по инструкции.

### Рекомендация интегратору/владельцу (не решено самостоятельно)

Функциональная цель Ш2 («убить циркулярную заглушку + её аллокацию») ДОСТИЖИМА
БЕЗ этого цикла: оставить ТЕЛА `@display`/`@debug` физически в
`protocols.nv` (там, где `Fmt`/`Write`/`Display`/`Debug` УЖЕ однозначны),
просто заменить их ВНУТРЕННОСТЬ на прямые вызовы уже-экспортированных
buffer-примитивов из fmt_buf.nv (`int_fmt_into`/`f64_fmt_shortest_into`/
`f32_fmt_shortest_into`/`bool_fmt`/`char_fmt`/`str_debug_fmt`/
`char_debug_fmt` — ОДНОСТОРОННИЙ импорt protocols→fmt_buf, УЖЕ
существует, БЕЗ нового цикла). Это НЕ обход бага (не пытается ЗАСТАВИТЬ
сломанный путь работать), а другая, некруговая архитектура, дающая ТОТ ЖЕ
результат (некруговые/дешёвые тела). Патч из
`208-f4r-sh2-blocked-repro.patch` можно использовать как основу — минус
блок `import std.prelude.protocols.{...}` и минус физический перенос
объявлений (`#impl` + `fn TYPE @method` секция) обратно в protocols.nv,
плюс их тела заменить на `scratch []u8` + сырой примитив + `f.write(buf)`
(та же логика тел, другое физическое расположение).

Альтернативно (для владельца): если желателен ИМЕННО физический перенос
в fmt_buf.nv (как в исходной карте) — нужен отдельный компиляторный агент
для диагностики imports.rs collect-order бага (или его переклассификация
как отдельный дефект с маркером `[M-...]`, ремонт вне рамок Ф.4R-как-std-волны,
per «нулевая толерантность к багам чинится ТОЙ ЖЕ волной» — здесь волна =
Ф.4R, но фикс — компиляторный, другого рода работы, вероятно другая
волна/владелец-решение).

## Ш3 — НЕ НАЧАТ (заблокирован решением по Ш2)

По брифу «Ш4 — НЕ НАЧИНАТЬ (по умолчанию СТОП после Ш3)» — но Ш3 сам
заблокирован, т.к. зависит от того, ЧЬИ *_display_spec-вызовы эмитить и
откуда (расположение примитив-тел в Ш2 не влияет на Ш3 технически —
Ш3 эмитит вызовы `*_display_spec` из fmt_buf.nv НАПРЯМУЮ из компилятора,
не через `@display`/`@debug` протокол-диспатч вообще — так что Ш3
МОЖНО делать НЕЗАВИСИМО от того, как разрешится Ш2). План Ш3 (не начат,
код не писался):

emit_c.rs fast-path (emit_format_spec_value ~41716-42007 rich-spec;
primitive_to_str_fn в emit_interpolated_str ~41412-41435/~41625-41689
bare-path) эмитит вызовы Ш1's `*_display_spec` (StringBuilder-семья) ПО
РЕЗОЛВУ декларации вместо nova_fmt_*-цепочки. Kill-switch
NOVA_FMT_LEGACY=1 (обе ветки живут одновременно в Ш3). Верификация:
эталоны Ш0 PASS в ОБОИХ режимах + байт-дифф .c выборки 5-6 фикстур
legacy-vs-new. frozen infer_call_ret_c (46293-48883) НЕ трогать.

**Решение владельца нужно:** продолжать на Ш3 СЕЙЧАС (Ш1-семья уже готова
и не зависит от Ш2), оставив Ш2 заблокированным на решение выше? Эта
волна СТОП на Ш2 и ждёт ответа, как предписано брифом («Ш4 — НЕ
НАЧИНАТЬ... по умолчанию СТОП после Ш3 и отчёт» — здесь трактую
консервативно: раз Ш2 не закрыт, не забегаю вперёд на Ш3 без явного
подтверждения, хотя технической зависимости нет).

## Якоря (подтверждены сессией 2026-07-20)

- emit_c.rs `emit_interpolated_str` ~41352; primitive_to_str_fn (bare)
  ~41412-41435 (нет u64/sized-int веток — квирк #2); numeric fallback
  ~41680 `nova_int_to_str((nova_int)(...))`.
- emit_c.rs `emit_format_spec_value` (rich) ~41716-42007; is_int
  классификация ~41746 ВКЛЮЧАЕТ uint64_t/int32_t/etc (шире bare-path).
- conv.h nova_fmt_* — 415-607. Debug-escape: conv.h 222-354 (str_to_debug_str:248,
  char_to_debug_str:317, ptr_to_debug_str:297 — НЕ в scope).
- fmt_buf.nv (ТЕКУЩЕЕ дерево, ПОСЛЕ Ш1, Ш2 НЕ закоммичен): int_fmt(130)/
  bool_fmt(module-private)/char_fmt(module-private), f64_fmt_into
  extern/fmt_f64, str_debug_fmt/char_debug_fmt (module-private, НОВОЕ
  в Ш1), `*_display_spec` семья (export, НОВОЕ в Ш1, ~строки 500-664).
  Примитив-тел `@display`/`@debug` в fmt_buf.nv НЕТ (попытка Ш2
  реверчена — см. секцию Ш2 выше + `208-f4r-sh2-blocked-repro.patch`).
- protocols.nv: Fmt/FmtCtx/Display/Debug protocols ~199-438 НЕТРОНУТЫ;
  примитив-тела int/f64/f32/bool/char/str `@display`/`@debug` НА МЕСТЕ,
  СТАРЫЕ (циркулярные `f.write("${@}".bytes())`) — Ш2 их НЕ тронул (откат);
  Option/Result Display/Debug ~721-826 НЕТРОНУТЫ (единственный вызыватель
  полиморфного пути через `v.debug(f)`/`v.display(f)`).
- НЕТ отдельных u64/i64/i32/... primitive @display тел нигде в std/ —
  подтверждено. Их C-типы в rich-spec классификации — RAW (uint64_t/
  int32_t), не nova_-префикс — перепроверить в Ш3 при построении
  resolved-call dispatch.
