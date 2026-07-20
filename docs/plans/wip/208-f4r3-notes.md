# Plan 208 Ф.4R §10R-Д3 — упразднение суффикса `_into` (checkpoint)

Worktree: `d:/Sources/nv-lang/nova-f4r3`, branch `p208-f4r-no-into` (from `main` `bb5cae073`,
уже вкл. Ф.4R Ш1-Ш3 + владелец-поправки №1 value-first/№2 type-first). Модель: sonnet.

Канон: `docs/plans/208-unified-formatter.md` §10R-Д (source of truth для сигнатур).

## Проба (ПЕРВЫМ, до Д3-п.1)

**Вердикт: ЗЕЛЁНАЯ.** Standalone-probe (`fn add_with_default(a int, b int = 10) -> int`,
вызов `add_with_default(5)`, module `probe_freefn_default.main`, свежий release-бинарь
worktree) — скомпилирован и выполнен, вывод `15` (5+10, дефолт применился). Плюс
независимое подтверждение по коду: `callnorm.rs::collect_sigs` — `Sigs.free` bucket
обрабатывает FREE-функции ИДЕНТИЧНО `static_methods`/`instance_by_name` (тот же
backfill-механизм, `normalize_expr` на `param.default` рекурсивно). Плюс — уже
СУЩЕСТВУЮЩИЙ зелёный conformance-фикстур `spec_tests/conformance/d102_named_args_default_params.nv`
буквально тестирует free-fn `d102_port(host, port int = 8080)` с опущенным `port` —
уже часть главного гейта. **Путь: ПОЛНОЕ СХЛОПЫВАНИЕ (следствия 1-3), fallback НЕ нужен,
маркер `[M-freefn-default-arg-backfill-gap]` НЕ создаётся.**

## Сделано

1. **`nova_rt.h`** (`compiler-codegen/nova_rt/nova_rt.h`): `f64_fmt_into` → `nova_f64_fmt`,
   `f32_fmt_into` → `nova_f32_fmt` (тела + doc-комментарии).
2. **`fmt_buf/core.nv`**:
   - `int_fmt` — добавлен `unsafe` + default `spec FmtSpec = FmtSpec.new()` + `requires cap >= 0`
     (совмещён с прежним radix-requires через `&&`).
   - `f64_fmt` — добавлен `unsafe` + default `kind FloatKind = FloatKind.Shortest, prec int = -1`
     + `requires cap >= 0`; тело зовёт переименованный extern `nova_f64_fmt`.
   - `f32_fmt` — НОВАЯ `unsafe fn` (взамен связки extern `f32_fmt_into` + мост
     `f32_fmt_shortest_into`) — `requires cap >= 0`, тело `unsafe { nova_f32_fmt(v,buf,cap) }`.
   - extern-декларации `nova_f64_fmt`/`nova_f32_fmt` — module-private (без `export`;
     `f32_fmt_into` РАНЬШЕ был `export`, теперь публичная поверхность — только `f32_fmt`).
   - Мосты `int_fmt_into`/`f64_fmt_shortest_into`/`f32_fmt_shortest_into` — УДАЛЕНЫ.
   - Шапка модуля — добавлена норма семьи (Д3 п.3): "все `*_fmt` рендерят в (buf, cap);
     запись-в-буфер — определение семьи, не суффикс; простые режимы = default-арги".
3. **`fmt_buf/core_test.nv`**: все вызовы `int_fmt(...)`/`f64_fmt(...)` обёрнуты в
   `unsafe { }` (функции стали `unsafe fn` — унаследовали unsafe-статус бывших мостов);
   `f64_fmt_into` → `nova_f64_fmt` в тесте прямого extern-вызова; добавлен новый тест
   "int_fmt: default spec" (вызов БЕЗ `spec`) + доп. ассерт в `f64_fmt`-тесте (вызов БЕЗ
   `kind`/`prec`) — доказывают именно default-arg-схлопывание, не просто рефакторинг имён.
4. **`string_builder.nv`**: import-строка — убраны `int_fmt_into`/`f64_fmt_shortest_into`/
   `f32_fmt_shortest_into`/`f32_fmt_into`, добавлен `f32_fmt`; `@append(int/f64/f32)` и
   `f32_display_spec` — прямые вызовы `int_fmt`/`f64_fmt`/`f32_fmt` (с default-аргами для
   int/f64), обёртки `unsafe { }` на call-сайтах СОХРАНЕНЫ (были и раньше).
5. **`external_registry.rs`** — touch (include_str! snapshot string_builder.nv).
6. **`spec/decisions/02-types.md`** — новая строка Ф.4R в "Статус реализации" таблице
   (после Ф.4-строки), фиксирующая факт Д1-Д3 (уже влитых value-first/type-first +
   этой волны `_into`-упразднения), со ссылкой на 208-unified-formatter.md §10R-Д как
   source of truth (§5/§7 code-примеры в САМОМ 02-types.md помечены как доредизайновые,
   не переписаны построчно — сознательно узкий скоуп, не полная Ф.4-документная волна).
7. `emit_c.rs` — грепнут на `int_fmt_into`/`f64_fmt_shortest_into`/`f32_fmt_shortest_into`/
   `f32_fmt_into`/`f64_fmt_into`: **0 хитов** (fast-path зовёт `*_display_spec`-семью через
   `free_fn_c_name`-резолв, не мосты напрямую) — emit_c.rs трогать НЕ пришлось.

## Греп `_into` (std/src + nova_rt.h)

Живой код — 0. Остаток — ТОЛЬКО прозаические upominания в комментариях (история/поясняющие
сноски о РЕТРАКТИРОВАННЫХ именах): `fmt_buf/core.nv` (6 мест), `fmt_buf/core_test.nv`
(5 мест), `string_builder.nv` (4 места), `nova_rt.h` (2 места) — везде текст explicitно
говорит "renamed from"/"retired"/"former". `spec_tests/conformance/d422_f4r_baseline_float.nv`
(1 комментарий, Ш0-фикстура — НЕ трогал, вне скоупа/риска правки байт-в-байт эталона).

## Гейты (см. отчёт агента — здесь дублируются кратко после прогона)

(заполняется по ходу; см. итоговый отчёт)
