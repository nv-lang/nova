<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# Внутреннее устройство строкового модуля (`runtime.string`)

> Карта реализации `str` для контрибьюторов. Plan 152.0. Публичный API строк —
> в [strings.md](strings.md) (Plan 152.1+). Здесь — как это устроено внутри.

## Раскладка модуля

`str` — value-record lang-item `{ptr *ro u8, len int}` (Plan 139/139.1, объявлен в
`std/prelude/core.nv`). Методы живут в папке-модуле:

```
std/runtime/
  string/                  ← ОДИН модуль `runtime.string` (co-equal файлы)
    core.nv                @len/@byte_len/@char_len/@byte_at/@is_empty/@as_bytes/
                           @to_bytes/@to_chars/@compare/@hash/str.new/from_bytes_*/
                           alloc_copy + helpers (cp_to_char/validate_utf8/is_cont)
    search.nv              @find/@rfind/@contains/@starts_with/@ends_with/@split/@sub_view
    transform.nv           @trim/@to_lower/@to_upper/@concat/@plus/@pad_*/@repeat/@replace
    parse.nv               @parse_int/@try_parse_int/ParseIntError
    chars.nv               @char_at (+ Plan 152.1: CharsIter)
  string_builder.nv        ← `runtime.string_builder`: тонкий consume-wrapper над Vec[u8]
```

> **`_buffer` ≡ `Vec[u8]`** (D-R5, [findings](plans/152-findings.md)). Отдельный
> `string_buffer.nv`/`StrBuf` НЕ вводится — `Vec[u8]` уже RawMem-буфер (Plan 131), а
> `StringBuilder` уже обёртка над ним. Дублировать grow/alloc Vec'а = нарушить DRY.

### Почему папка из co-equal файлов, а не facade-файл

Резолвер Nova **запрещает** сосуществование файла `string.nv` и папки `string/` с тем
же именем (`ambiguous module: both single-file and folder-module exist`). Поэтому facade-
файл невозможен. Вместо него — **папка = один модуль**: все файлы внутри объявляют
`module runtime.string` и сливаются (прецедент: `sync.nv`+`sync_test.nv` =
`runtime.sync`). Следствие: `import std.runtime.string.{X}` и prelude
`export import std.runtime.string.{…}` работают без изменений — модуль тот же.
(См. [docs/plans/152-findings.md](plans/152-findings.md) F4.)

### Видимость / internal

В Nova нет keyword `internal`. Внутри модуля `runtime.string` приватность — через
отсутствие `export` (module-private хелперы видны всем co-equal файлам папки, но не
снаружи). `string_buffer` сделан **отдельным** модулем (не submodule папки), чтобы:
(1) не попасть в публичную поверхность `runtime.string`; (2) быть импортируемым из
`runtime.string_builder`. Он не реэкспортируется в prelude → de-facto internal
(конвенция `_buffer`/`_`-naming; см. umbrella Q-module-internal-visibility).

## `Vec[u8]` — единый дом аллокаций (RawMem) (D-R5)

Историческая проблема: `@trim`/`@to_upper`/`@concat`/`@to_bytes` строили результат
**push-loop-копипастом** по `[]u8` (`with_capacity(n)` + ручной цикл `push`). Решение
(Ф.3): чистые копии (`@trim`/`@concat`/`@to_bytes`) идут через `Vec[u8].@append`
(`RawMem.copy` memmove — один проход, [vec_owned.nv:568](../std/collections/vec_owned.nv#L568))
+ `str.from_bytes_unchecked_steal` (reuse буфера, без второй копии). `Vec[u8]` —
единственный RawMem-буфер (Plan 131), `StringBuilder` = тонкая обёртка `{mut buf []u8}`
над ним. **Отдельный `StrBuf`/`string_buffer.nv` не вводится** (дублировал бы Vec —
DRY/минимализм, см. D-R5). Остаточные push-loop'ы (`@to_lower`/`@to_upper`/
`from_bytes_lossy`/`@to_chars`/`@split`) — это per-byte ТРАНСФОРМ / decode /
list-of-views, не copy-paste alloc/grow/NUL.

## Ацикличный модульный граф (DAG)

Интерполяция `"${e}"`/`"${e:?}"` (D44) десугарится codegen'ом в StringBuilder-цепочку
(`@display`/`@debug`, D183/D229). Чтобы не было цикла `str ↔ StringBuilder`:

```
string_buffer (RawMem, leaf, #no_prelude)
    ↑                       ↑
runtime.string (core, ...) → StringBuilder (consume-wrapper над string_buffer)
    ↑                       ↑
str.@display / Display-impl ─► StringBuilder.append   (leaf-ward, без обратного ребра)
```

`pad_*`/интерполяция продолжают работать; новых перекрёстных рёбер
`str ↔ StringBuilder` сверх DAG нет.

## Что компилятор знает про str (вне `.nv`)

Резолв str-методов — из распарсенного `.nv` (НЕ из `runtime_registry.rs`; реестровые
str-Nova-body записи удалены в Plan 152.0 Ф.2.5 как вестигиальные). Компилятор хардкодит
лишь: операторы `==`/`!=`/`+`/`<`/`<=`/`>`/`>=` → C `nova_str_eq`/`concat`/`lt`/…
(`emit_c.rs`, маркер `[M-139.1-operator-lowered-methods]` — декомиссия в Plan 152.5a) +
`@hash` (`nova_str_hash` — SipHash с DoS-стойким per-process seed, намеренно в C). См.
[docs/plans/152-findings.md](plans/152-findings.md) F2/D-R2..D-R4.
