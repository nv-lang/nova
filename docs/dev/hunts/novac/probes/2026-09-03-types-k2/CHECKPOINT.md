# hunt: types x K2 (novac) — checkpoint (final)

Цель: К2 — резолв по голой строке через плоские last-wins таблицы. Модуль
`types` и то, чем его ключуют: `sem/defs.nv` (DefTable), `sem/typeref.nv`
(имя -> DeclId -> head интернера), `sem/mangle.nv` (тип -> C-строка).

Отчёт отдан текстом агента (харнесс запрещает писать файлы-отчёты).

## Команды

- смоук (оракул vs novac, stdout байт-в-байт):
  `sh scripts/tools/novac-e1-smoke.sh scratch/hunt-types-k2/<имя>/<имя>.nv`
- оракул отдельно:
  `./nova-cli/target/release/nova.exe build <файл> -o /tmp/x.exe && /tmp/x.exe`
- novac отдельно:
  `./novac/target/novac.exe check <файл>` · `./novac/target/novac.exe emit <файл>`

## Свойство 1 — ОДНА плоская строковая таблица на четыре рода сущностей

`DefTable` (`novac/src/sem/defs.nv:111`, «LAST PUT WINS») держит типы, варианты,
функции и константы в одном пространстве имён `NsDefs`, БЕЗ цепочки одноимённых
рядов (комментарий `defs.nv:104-110` объявляет цепочку ненаблюдаемой).

| проба | результат |
|---|---|
| p_variant_last_wins | РАСХОЖДЕНИЕ: оракул печатает 2, novac даёт два ложных E_NOVAC_SUBSET |
| p_variant_ctl | ok (контроль: варианты не совпадают по написанию) |
| p_variant_order_flip | ok (ОДИН рычаг: переставлен только порядок объявлений) |
| p_type_eaten_by_variant | РАСХОЖДЕНИЕ: оракул печатает 7, novac ICE |
| p_variant_eaten_by_type | ok (обратный порядок безвреден — расхождения нет) |
| p_type_eaten_by_fn | РАСХОЖДЕНИЕ: оракул печатает 7, novac ICE |
| p_variant_vs_fn | РАСХОЖДЕНИЕ: оракул печатает 1, novac ложно отказывает |
| p_const_eats_type | оба отказывают, но novac — ICE вместо диагностики |

## Свойство 2 — C-имя склеивается из РАЗНЫХ пространств через легальный разделитель

| проба | результат |
|---|---|
| p_prim_name_shadow | РАСХОЖДЕНИЕ: оракул 11/22, C от novac не компилируется |
| p_prim_name_ctl | ok (контроль: запись переименована) |
| p_prim_name_shadow_newtype | вторая форма; ОРАКУЛ ломается тем же классом — судить нечем |
| p_fn_name_sep | РАСХОЖДЕНИЕ: оракул 1/11, C от novac не компилируется |
| p_tag_seam | novac даёт невалидный C, но и оракул тоже — расхождения нет |

## Наводка 1 (mono_tuple_name) — носитель есть, следствия нет

| проба | результат |
|---|---|
| p_tuple_newtype_name | ok: `(Row,int)` и `(int,int)` — два ряда интернера, ОДИН typedef |
| p_tuple_newtype_ctl | ok (контроль) |
| p_tuple_ret_overload | ok: невызванные перегрузки не эмитятся (DCE), носителя нет |

## Наводка 2 (ключи интернера) — коллизии не нашёл

`bucket_of(k, head) = head * TYPE_KIND_COUNT + kind_ordinal(k)`
(`novac/src/types/types.nv:146`), `TYPE_KIND_COUNT == 8`
(`novac/src/builtins/builtins.nv:49`) при ровно восьми видах — инъективно для
`head_id >= 0`. Цепочка внутри корзины сравнивает ТОЛЬКО аргументы
(`types.nv:161-168`), вид и голову не перепроверяет.
