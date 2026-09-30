# REPORT-p1444 — алиас типа при импорте (№1444)

КОРЕНЬ: резолвер импортов (`imports.rs`, слияние модуля, `rename_map`) для алиаса ТИПА `import m.{T as U}` и для любого алиаса `export import m.{x as y}` переименовывал само объявление на всю единицу компиляции; фикс №1419 снял это только для функций/констант обычного импорта. Модуль-источник и другие импортёры после этого называли `T`/`x`, которого в CU нет. /

ФИКС:
- `compiler-codegen/src/imports.rs` (слияние, ≈2352) — объявление сливается под своим именем для любого алиаса; `rename_item` удалён; файлу видны и алиас, и объявленное имя; в ветке реэкспорта (≈2277) потребителю фасада видно и объявленное имя.
- `compiler-codegen/src/import_alias/type_rewrite.rs` (новый) — `rewrite_type_aliases`, вызывается в конце `resolve_imports_inline_ex`: в ИМПОРТИРУЮЩЕМ файле все позиции типа `U` → `T` (аннотации, параметры, возвраты, обобщённые аргументы, литерал `U { .. }`, головы `U.Variant`/`U.method()`, образцы `match`, приёмник `fn U @m()`, списки `impl`); обход исчерпывающий, обобщённый параметр с именем алиаса затеняет его; ссылки — в `Module::import_alias_refs` (линт unused-import).
- `compiler-codegen/src/import_alias.rs` — `aliases_where` + `import_names_file`: карта алиасов файла включает собственные `{x as y}` (и `export import`) и имена, импортированные из фасада, который реэкспортировал их с `as` — для типов через перепись выше, для функций/констант через `file_aliases` → `alpha_rename` (№1419); путь ссылки — импорт фасада, т. е. модуль объявления. C-символ не меняется. /

КЛЕТКИ (`nova test spec_tests --full --filter p1444`, выключатель `NOVA_KILL_1444` — только для замера, в коммиты не входил):
- `p1444_a_peer_first` (пир импортирован первым) -> до: CODEGEN-FAIL `unknown type 'Crate' in record literal` (минимальная проба из строки реестра до фикса: CC-FAIL `incomplete definition of type 'struct Nova_Crate'`) / после: PASS
- `p1444_b_alias_first` (алиас первым) -> до: CODEGEN-FAIL `[E_UNKNOWN_TYPE] unknown type 'Box' in record literal` в `ta_src` / после: PASS
- `p1444_c_facade` (`export import ./ta_src.{Box as Crate, mk as make_box}`, потребитель `{Crate, make_box}`) -> до: CODEGEN-FAIL то же / после: PASS
- `p1444_d_facade_alias` (`{Crate as Kiste, make_box as pack}`) -> до: CODEGEN-FAIL / после: PASS
- функция через фасад (проба вне фикстуры, `export import ./fsrc.{mk as make}`) -> до: CC-FAIL `undefined reference to nova_fn_..fsrc2mk` / после: PASS
- `neg/p1444_type_import_conflict`, `neg/p1444_type_import_import_conflict` -> до: PASS / после: PASS (D29 для типов уже держал код №1234) /

ЦЕНА: std 0, novac 0, conformance 0 — `nova check` по `std/src`, `novac`, `spec_tests/conformance` новым бинарём и бинарём `origin/integrate` на c775a975d (до слияния), вывод std/novac побайтно равен, в conformance различаются только новые фикстуры; сателлиты — выборочных `as`-импортов и `export import … as` нет ни в одном. /

ФИКСТУРЫ: `spec_tests/conformance/standalone/p1444_type_import_alias/` (источник `ta_src`, пир `ta_peer`, фасад `ta_facade`, входы `p1444_a_peer_first`, `p1444_b_alias_first`, `p1444_c_facade`, `p1444_d_facade_alias`); `spec_tests/conformance/neg/p1444_type_import_conflict/`, `spec_tests/conformance/neg/p1444_type_import_import_conflict/` (построчные `nova:expect E_IMPORT_NAME_CONFLICT`). Соседи зелёные: p1419, p1234, p1390, import, alias, crossmod, d78_, same_name, facade, export. Линт своих `.nv` — 0 находок. /

КАРИНА: самосборка да (`nova build novac/src/main.nv`, до и после слияния integrate) /
КРЕЙТ: compiler-codegen --lib 1275/0 /
ХРАПОВИК: emit_c строк 68130 (база — arch-ratchet.baseline lines=68857), `emit_c.rs` не тронут /

КОММИТЫ: 76967f159 fix(#1444); f5bb05033 merge origin/integrate (конфликт — только реестр: строки integrate переставлены, взята его версия и в неё заново вписана строка №1444); коммит с этим отчётом. Ветка `p1444-type-import-alias` запушена в origin. /

ОТКРЫТО:
- `facade.U` в КВАЛИФИЦИРОВАННОЙ позиции типа (`ro c facade.U`) перепись делает верно, но квалифицированный тип в оракуле не собирается и без алиаса (CC-FAIL `Nova_ta_src_Box`) — это №1077, новой строки не заводил.
- Алиас как лекарство D29 для типа, совпадающего с типом импортирующего модуля (`type Box` + `{Box as Crate}`), упирается в №705: такой CU падает и БЕЗ алиаса (литерал `Box { v }` модуля-источника резолвится к чужому `Box`). Новой строки не заводил — носитель №705.
- Строки реестра №1444 — «ИСПРАВЛЕНО В ВЕТКЕ»; вердикт слияния ставит интегратор. /

ВОПРОСЫ ИНТЕГРАТОРУ: нет. Две вещи решены по спеке и названы, чтобы их было видно: (1) методы для алиаса в импортирующем файле (`fn Crate @double()`) разрешены — это extension-метод на чужом типе, D287 его допускает, алиас — имя того же типа в этом файле; (2) фикс покрывает и алиас реэкспорта ФУНКЦИЙ/констант (`export import m.{f as g}`) — строка №1419 в «ГРАНИЦЕ» отнесла этот остаток к №1444, и он чинится тем же механизмом.
