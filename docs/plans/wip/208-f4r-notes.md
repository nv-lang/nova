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

## Ш3 — DONE (частично, задокументированный скоуп; коммит следующий)

Решение: Ш3 технически НЕ зависит от Ш2 (эмитит вызовы `*_display_spec`
НАПРЯМУЮ из компилятора, не через `@display`/`@debug` протокол-диспатч) —
продолжил, Ш2 остаётся заблокированным отдельно (см. выше).

### Скоуп ЭТОЙ волны (осознанно сужен, не полный Ш3)

- **Rich-spec путь** (`emit_format_spec_value`): int radix (`x`/`X`/`b`/`o`)
  + int decimal + float — ВСЕ non-Debug — переведены на `int_display_spec`/
  `f64_display_spec`. int/float Debug-kind rich-spec (`${x:10?}`) и
  str/char/bool ЛЮБОЙ rich-spec — остаются на СТАРОМ движке.
- **Bare-path** (`emit_interpolated_str`): int/f64/f32 (Display И Debug —
  для этих трёх debug==display побайтово, ОДНА точка вызова покрывает
  оба) — переведены. str/char/bool bare — остаются на СТАРОМ движке.
- Composite/user-type путь (FmtCtx/@display/@debug диспатч) — НЕТРОНУТ.

Причина сужения: время/токен-бюджет волны исчерпан находками Ш1/Ш2
(две архитектурные компиляторные находки съели большую часть бюджета);
int/float — наибольшая часть D374/152.7-B width/precision/align/fill/
sign/alt/radix поверхности и голова "carrier swap"-мотивации плана;
str/char/bool — низкий риск на старом движке (простые conv.h-интринзики,
не циркулярные). Follow-up для str/char/bool — задокументирован, не
начат.

### Механизм (по карте, resolved-каналы — compiler-conventions §3)

- `Self::fmt_legacy_enabled()` — kill-switch (`NOVA_FMT_LEGACY=1`).
- `Self::align_ctor_c(align_code)` — `nova_make_Align_{Left,Right,Center}()`
  (компилятор-широкая enum-ctor конвенция, не Ф.4R-специфичный хардкод).
- `self.free_fn_c_name("int_display_spec")` / `"f64_display_spec"` /
  `"f32_display_spec"` — РЕЗОЛВ-канал (fn_module_map → mangle_free_fn),
  НЕ C-строка имени вручную.
- `emit_int_display_spec_call`/`emit_f64_display_spec_call` — rich-spec
  helper'ы: рендерят через temp `StringBuilder` (тот же паттерн, что УЖЕ
  использует composite/user-type fallback чуть ниже в том же файле) →
  `Nova_StringBuilder_consume_into_str`.
- Bare-путь эмитит ПРЯМОЙ statement-вызов в РЕАЛЬНЫЙ interp `sb` (БЕЗ
  temp — `*_display_spec` уже пишет в данный ему sb) — `continue` сразу
  после (established паттерн в этой функции).

### DCE-находка (предвиденный риск, устранён ДО прогона)

`compiler-codegen/src/lints.rs`'s `collect_used_names` (`ExprKind::
InterpolatedStr` seed-блок, ~1262-1296) — добавлены `int_display_spec`/
`f64_display_spec`/`f32_display_spec` в seed-set. Без этого —
reachability-DCE (Plan 81 Ф.7.2/159) вырезала бы эти НОВЫЕ free-функции
из ЛЮБОЙ программы, где ничто в пользовательском исходнике не называет
их по имени явно (т.е. из ПОЧТИ каждой программы) — codegen эмитил бы
вызов НЕОБЪЯВЛЕННОЙ C-функции при ПЕРВОЙ `${int}`/`${float}` интерполяции.

### Найден и исправлен РЕАЛЬНЫЙ баг (Ш1-код, не компиляторный) — cap для radix

`DISPLAY_INT_CAP=20` (размер под decimal: `"-9223372036854775808"`)
использовался ДЛЯ ВСЕХ радиксов в non-zero-pad ветке `int_display_spec` —
для radix=8 (octal) 64-битное значение требует ДО 22 цифр, radix=2
(binary) — ДО 64 цифр (+2 alt-префикс каждый) — `int_fmt` "truncates
defensively", так `${int.MIN:o}` (22 цифры) молча ТЕРЯЛ последние 2 цифры
вместо ошибки — эталон `d422_f4r_baseline_int.nv`/`_float.nv`/
`_strcharboolu64.nv` поймал (RUN-FAIL, `<expr> == "1000000000000000000000"`
не совпало). Исправлено: `int_display_natural_cap(radix)` — 66 (bin) / 24
(oct) / 18 (hex) / 20 (dec) — в `string_builder.nv`. НЕ пойман моими
собственными inline-юнит-тестами (Ш1) — я не тестировал octal/binary с
БОЛЬШИМ значением там, только `-1` в hex (укладывается в 20). Именно
ПОЭТОМУ эталоны Ш0 (снятые с реального движка, MIN/MAX явно в списке)
существуют — поймали за секунды.

### Верификация

- `d422_f4r_baseline_{int,float,strcharboolu64}.nv` — PASS 3/3 в ОБОИХ
  режимах (default = новый движок; `NOVA_FMT_LEGACY=1` = старый) — на
  ОДНОМ бинаре (собственный worktree-бинарь, cargo build --release).
- `string_builder_test.nv` 1/0 + `checksums` 3/0 — PASS (новый режим).
- Байт-дифф `.c` (`nova test --keep-artifacts` в обоих режимах,
  `diff` построчно) — фикстуры фактически мёржатся в ПОЛНЫЙ
  `spec_tests.conformance` compile unit (не только мои 3 файла — ВЕСЬ
  корпус ~169 файлов, судя по масштабу диффа), так что байт-дифф покрыл
  ГОРАЗДО больше заявленных "5-6 фикстур". Диф — ЧИСТЫЙ, укладывается
  ровно в 2 ожидаемых класса: (1) bare-path — 1-в-1 замена
  `Nova_StringBuilder_method_append(sb, nova_int_to_str(v))` →
  `nova_fn_..._int_display_spec(sb, ...)` (и f64/f32 аналогично); (2)
  rich-spec path — доп. temp-StringBuilder statements (было — одно
  `nova_fmt_pad(...)`-выражение, стало 3 statements) сдвигает нумерацию
  ПОСЛЕДУЮЩИХ `_nv_interp_sb_N`/`_nv_interp_str_N` временных переменных в
  ТОМ ЖЕ файле — семантически инертно (подтверждено идентичным PASS в
  обоих режимах на ВСЕХ ассертах). Никаких диффов ВНЕ этих двух классов
  не найдено.
- `frozen infer_call_ret_c` (46293-48883 в исходной нумерации) НЕ
  тронут — все правки в ~41352-42007 (emit_c.rs) + lints.rs seed-блок +
  string_builder.nv.

### Follow-up (НЕ этой волной)

- str/char/bool bare+rich-spec — на СТАРОМ движке (conv.h) намеренно.
- int/float Debug-kind rich-spec (`${x:10?}`) — на СТАРОМ движке.
- Ш4 (снос `conv.h` nova_fmt_*, 16 сайтов + kill-switch) — НЕ начат по
  брифу («Ш4 — НЕ НАЧИНАТЬ»).
- Ш2 (перенос примитив-тел) — остаётся заблокирован, см. выше.

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
