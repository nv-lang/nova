# Plan 208 Ф.4R — прогресс (worktree nova-f4r, branch p208-f4r)

Карта: docs/plans/208-unified-formatter.md §10R «Ф.4R». Модель: sonnet.

## Ш0 — эталон-фикстуры (в процессе → done после зелёного прогона)

Сняты байт-тексты ТЕКУЩЕГО (нетронутого) вывода прогоном probe.nv через
main-репы бинарь (`d:/Sources/nv-lang/nova/nova-cli/target/release/nova.exe`,
build+run, НЕ `nova test` — println недоступен в test-блоках).

Probe-скрипт: `C:\Users\B7E3~1\AppData\Local\Temp\claude\d--Sources-nv-lang-nova\a48a9f3a-0403-4a44-a6e3-8894781d4b88\scratchpad\probe.nv`
(временный, не в репо).

Файлы (в nova-f4r/spec_tests/conformance/):
- `d422_f4r_baseline_int.nv` — decimal/hex/oct/bin (MIN/MAX/-1), zero-pad/width/sign/alt.
- `d422_f4r_baseline_float.nv` — precision+width, `.0`, no-precision shortest edge magnitudes.
- `d422_f4r_baseline_strcharboolu64.nv` — str/char Debug-escape as-is, bool, u64 MAX quirk.

**Найденные квирки (ПИНУЮТСЯ как есть, НЕ чинятся в рамках Ф.4R):**
1. `${-0.0:.2}` → `"--0.00"` (двойной минус: `nova_fmt_f64_body` не signbit-aware
   для магнитуды при v<0.0 false для -0.0, а `nova_fmt_f64_prefix` signbit-aware
   отдельно — конкатенация двух минусов).
2. `${u64.MAX}` (bare, без спека) → `"-1"` (primitive_to_str_fn в emit_c.rs не
   покрывает u64/sized-int C-типы — только "nova_int"; падает в generic-fallback
   `nova_int_to_str((nova_int)(v))`, реинтерпретирует bit-pattern как signed).
3. `\xHH`-escape (control bytes) БЕЗ фигурных скобок — комментарий в conv.h
   врёт (`\x{HH}`), реальный код emits `\xHH`.

Статус верификации: прогон fixtures на нетронутом дереве — ЗАПУЩЕН (task id
ba5tt2yi0), жду результата.

## Ш1-Ш3 — план (не начаты)

Ш1 (std, аддитивно): Debug-escape движок (str/char) в fmt_buf.nv (портируемый
цикл, повторяет conv.h nova_str_to_debug_str/nova_char_to_debug_str побайтово)
+ `*_display_spec` семейство поверх int_fmt/fmt_f64 (плоские аргументы).

Ключевое расхождение с нынешним conv.h, которое `*_display_spec` ОБЯЗАН
воспроизвести:
- float precision-path: prefix(sign) + body(magnitude via %.*f) РАЗДЕЛЬНО
  (двойной минус на -0.0 — квирк #1 выше).
- float no-precision path: body = fmt_f64(Shortest) уже несёт свой знак,
  prefix пустой ИЛИ условный '+' только если dv>=0.0 (эта ветка asymmetric
  относительно precision-path).
- int: int_fmt (fmt_buf.nv, уже спек-полный) — сверить 1:1 с nova_fmt_int_body/
  nova_fmt_int_radix_body/nova_fmt_int_prefix/nova_fmt_radix_prefix/nova_fmt_pad
  (похоже уже совпадает по логике — юнит-тесты в fmt_buf.nv это подтверждают).

Ш2: primitive @display/@debug тела (int/f64/f32/bool/char/str — protocols.nv
~661-694) переезжают в fmt_buf extension-методы, зовут *_display_spec.
СТОП-ловушка: если #impl/extension-dispatch не резолвит тела из
runtime.fmt_buf (D267 privacy/visibility) — чекпоинт+СТОП+отчёт, НЕ обходить.

Ш3: emit_c.rs fast-path (emit_format_spec_value ~41716-42007, и
primitive_to_str_fn в emit_interpolated_str ~41412-41435/~41625-41689) эмитит
вызовы *_display_spec по резолву декларации вместо nova_fmt_*-цепочки.
Kill-switch NOVA_FMT_LEGACY=1.

## Якоря (подтверждены сессией 2026-07-20)

- emit_c.rs `emit_interpolated_str` — начало ~41352; primitive_to_str_fn
  (bare-path) ~41412-41435 (нет u64/sized-int веток — квирк #2); numeric
  fallback ~41680 `nova_int_to_str((nova_int)(...))`.
- emit_c.rs `emit_format_spec_value` (rich-spec path) — ~41716-42007;
  is_int классификация ~41746 ВКЛЮЧАЕТ uint64_t/int32_t/etc (шире чем bare-path).
- conv.h nova_fmt_* — строки 415-607 (encode_fill/char_count/bytes_for_chars/
  pad/int_body/int_radix_body/int_prefix/radix_prefix/f64_body/f64_prefix/
  str_precision). Debug-escape: conv.h:222-354 (bool/int/f64/f32 debug ==
  display; str_to_debug_str:248; char_to_debug_str:317; ptr_to_debug_str:297
  — НЕ в scope Ф.4R, вне списка потребителей).
- fmt_buf.nv: int_fmt (130), bool_fmt(225), char_fmt(252, уже покрывает
  UTF-8 encode — НЕ debug-escape), f64_fmt_into extern (295)/fmt_f64(328)
  wrapper, unsafe-мосты int_fmt_into/f64_fmt_shortest_into/f32_fmt_shortest_into.
  НЕТ debug-escape для str/char — это Находка А, строим в Ш1.
- protocols.nv: Fmt/FmtCtx/Display/Debug protocols ~199-438; примитив-тела
  ~661-694 (int/f64/f32/bool/char/str — циркулярные `f.write("${@}".bytes())`);
  Option/Result Display/Debug ~721-826 (НЕ трогать — не примитивы).
- НЕТ отдельных u64/i64/i32/... primitive @display тел нигде в std/ —
  подтверждено grep. Их C-типы (nova_i8/nova_u64/etc) ОТЛИЧАЮТСЯ от
  emit_format_spec_value's is_int list (uint64_t/int32_t/... — RAW C names,
  не nova_-префикс) — видимо sized-int переменные инферятся как
  "uint64_t"/"int32_t" НЕ "nova_u64"/"nova_i32" в этом контексте (нужно
  перепроверить в Ш1/Ш3 при построении display_spec dispatch — resolved-channel
  должен покрыть ОБА написания, если они оба реально встречаются).
