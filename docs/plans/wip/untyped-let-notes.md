# [M-vec-ext-method-untyped-let-breaks-chain-dispatch] — фикс-заметки

Worktree: `d:/Sources/nv-lang/nova-untypedlet`, ветка `p-fix-untyped-let-chain`.

## Корень (найден)

`compiler-codegen/src/types/mod.rs`, функция `f3_check_member_ctx`, блок
"Метод?" (было ~11951-11983, после патча см. текущие номера строк —
патч вставлен между `slice_elem_has_method` и `has_method`).

Механизм — представленческое рассогласование канала (196-территория),
ТРИ разные конвенции регистрации slice-методов, из которых чекер знал
только про две:

1. `t_provides_method(tname="Vec", name)` — bare "Vec"-key (native
   `Vec[T]`-методы).
2. `slice_elem_has_method` — литеральный ключ `"[]<конкретный-элемент>"`
   (напр. `"[]str"`), для КОНКРЕТНЫХ slice-ресиверов (`fn []str @join(...)`).
3. **ОТСУТСТВОВАЛА (найдена сейчас):** литеральный ключ `"[]T"` — где `T`
   это СОБСТВЕННЫЙ generic-параметр декларации (`fn[T] []T @method(...)`,
   пример — ровно `std/src/collections/vec_seq.nv`'s `@map[U]`/`@filter`/
   `@fold[Acc]`, и наш mini-repro `my_map_ch`/`my_filter_ch`).

Почему аннотация чинит, а без неё — ломает:
- `ro x []int = v.map(f)` — `d.ty` (аннотация) даёт `TypeRef::Array(int)`
  НАПРЯМУЮ. `f3_check_member_ctx`'s `let TypeRef::Named{..} = &obj_tr else
  {return;}` (строка ~11703) НЕ матчит `Array` → функция бейлится РАНЬШЕ
  метод-чека — проверка вообще не выполняется (permissive).
- `ro x = v.map(f)` (без аннотации) — тип биндинга материализуется через
  КАНАЛ (`f1_stmt`'s `chain_ty`, читает `resolved_types_buf`, который
  `f1_expr` заполнил через `infer_method_call_channel_type` →
  `ResolvedType::from_type_ref` на `[]int`). `from_type_ref` КАНОНИЗИРУЕТ
  `TypeRef::Array` → `ResolvedType::Named{"Vec", [int]}` (D239, "единое
  каноническое представление"). `resolved_to_typeref_tp` конвертирует
  ОБРАТНО в TypeRef — и восстанавливает `TypeRef::Named{["Vec"],[int]}`
  (НЕ `Array`!) — другую форму TypeRef для СЕМАНТИЧЕСКИ того же типа.
  Эта форма ДОХОДИТ до метод-чека (матчит `TypeRef::Named` на 11703) —
  и там падает, т.к. `my_filter_ch` зарегистрирован под "[]T", а чекер
  пробовал только "Vec" и "[]int".

## Фикс

Добавлен третий гейт `prefix_generic_slice_method` рядом с
`slice_elem_has_method` в `f3_check_member_ctx`: когда `tname=="Vec"` и
`recv_type_args` несёт ровно один конкретный элемент — реконструируем
`TypeRef::Array(elem)` и зовём уже существующую (и уже протестированную,
0 false-positives/707K вызовов корпуса, Plan 177 Ф.3)
`self.prefix_generic_method_exists(&synthetic_array, name)`. Она уже умеет
искать `"[]<T>"`-ключи method_table, где T — генерик-параметр самой
декларации.

Никаких изменений в frozen-зоне `infer_call_ret_c` (emit_c.rs) — фикс
целиком в checker (`types/mod.rs`), в стороне от codegen return-inference.

## RED → GREEN

Мини-репро (scratchpad, 3 файла — unannotated/chained-one-expr/annotated):
- unannotated (`ro mapped = v.my_map_ch(f); mapped.my_filter_ch(p)`) —
  RED (`[E7320] no field or method my_filter_ch on type Vec`) → GREEN.
- chained-one-expr (`v.my_map_ch(f).my_filter_ch(p)`) — тот же симптом,
  RED → GREEN.
- annotated (`ro mapped []int = ...`) — был GREEN (контроль, не трогали),
  остался GREEN.

## Дальше по плану — ЗАКРЫТО

- `nova check nova_tests/generics/mono_basic.nv` (несёт `plan101_1_vec_chained.nv:20`
  `my_filter_ch`) — GREEN. Полный `nova test` на этой folder-module остаётся
  CODEGEN-FAIL, но по ДРУГОЙ orthogonal причине: co-equal-пир
  `plan101_1_vec_map_int_str.nv` зовёт ретрактированный (Plan 174.2) `str.from(x)`.
  Пробовал тривиально мигрировать на `.to_str()` — вскрыло ЕЩЁ один, глубже и не
  связанный с этим маркером codegen-баг (byte/str confusion в generic-closure-теле,
  `Nova_Vec____nova_byte*` вместо `nova_str`) — правку ОТКАТИЛ, файл не трогал,
  вне объёма этой волны.
- ВАЖНАЯ ПОПРАВКА: "v3_user_generic_newtype_ok.nv (chained .debug/.display на
  Vec[f32].from)" из исходного задания — МИСАТРИБУЦИЯ. Реальный файл
  `v3_user_generic_newtype_ok.nv` не содержит такого контента вообще (только
  newtype-тесты, коммит 99f0021f9 был чисто unsafe-обёрточной гигиеной). Текст
  "chained .debug/.display на Vec[f32]" на самом деле принадлежит
  `spec_tests/conformance/vec_f32_chained_debug.nv` — но ЭТО отдельный,
  уже триажированный P1-маркер `[M-208-vec-chained-debug-display-red]`
  ("208-волна", Vec Fmt-миграция), НЕ пересекается корнем с этим фиксом
  (codegen Display/Debug-диспетч, не checker E7320/method-registration).
  Прогнал его с моим фиксом — остаётся RED (ожидаемо, не в моей зоне, не трогал).
- δ0 GREEN: `std/src/collections` (vec_seq.nv, реальный прод-риск),
  `std/src/checksums/{adler32,crc32,fnv}_test.nv`,
  `std/src/runtime/{char,sync}_test.nv`.
- Пин-фикстура standalone в conformance:
  `spec_tests/conformance/vec_ext_method_untyped_let_chain_ok.nv` (реальные
  `std.collections.vec_seq.map/filter`) — верифицирована через изолированную
  module-renamed копию (spec_tests/conformance — ОДИН CU, любой файл внутри
  тянет весь каталог; приём уже задокументирован в M-208-generic-interp-
  display-dispatch-gap записи simplifications.md) — PASS 1/0.
- Маркер закрыт: `docs/plans/backlog-followups.md` + `docs/dev/simplifications.md`.
- Коммиты: `7f397016f` (фикс + чекпоинт), `d13c24e0a` (пин-фикстура + docs).
