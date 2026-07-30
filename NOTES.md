# NOTES — окно mangle-module-qualification (№154/№151/№136)

## Разведка: одна точка или несколько?
НЕСКОЛЬКО, но общий класс подтверждён: недостаточная квалификация модуля
в C-символах/резолве имён, три РАЗНЫЕ конкретные точки:

1. **№154 форма (а), protocol vtable**: `emit_protocol_box_typedef`
   (`compiler-codegen/src/codegen/emit_c.rs`) лежит ВНЕ
   `current_emit_file_id`-гейта, которым уже пользуется D381
   (`colliding_type_names`/`ref_type_base`/`def_type_base`) для СТРУКТ/СУММ.
   `Fmt.sign() -> Sign` (std.runtime.fmt_buf) — ссылка на СОБСТВЕННЫЙ `Sign`
   протокола лоуэрится ДО того, как известно, из какого файла эта сигнатура,
   поэтому `ref_type_base` не может квалифицировать и возвращает голое имя.
   ФИКС: `protocol_decl_file: HashMap<String, FileId>` + временная установка
   `current_emit_file_id` на время лоуринга param/return типов методов
   протокола (тот же save/restore идиом, что уже используется в
   `emit_monomorphized_method_scoped_inner`).

2. **№154 форма (б), Newtype**: D381 (`record_def`/`qualifiable` в
   `emit_module`) явно ИСКЛЮЧАЛ Newtype из квалификации ("distinct axis,
   followup"). Из-за этого `type Sign(i8)` пользователя И
   `std.runtime.fmt_buf.Sign` (Sum) НЕ считались коллидирующими вовсе (Sum
   один, Newtype не считался → `mods.len()==1`) → оба typedef эмитятся под
   голым `Nova_Sign` → `typedef redefinition`.
   ФИКС: добавлен `plain_newtype` в `qualifiable` (минус
   `debt_is_runtime_backed_newtype`); typedef Newtype-эмиссии переведён на
   `def_base` вместо `t.name`. `type_aliases` остаётся keyed по bare-имени
   (конвенция, как у sum_schemas/record_schemas).

3. **№151, export const**: `private_const_c_names` (единственный механизм
   квалификации const-символов) по дизайну НЕ покрывал `export const`
   ("имя стабильно как cross-module API; коллизия — ambiguity error чекера"
   — чекер эту проверку никогда не реализовывал). Символ `_nova_const_
   <name>_value` — голый везде для export. ФИКС: `colliding_const_names`
   (счёт различных declaring-модулей по имени, >=2 = коллизия, тот же
   паттерн что D381 для типов) + `const_qualified_by_name` (глобальная по
   имени, "последний обработанный побеждает" — НАМЕРЕННО зеркалит уже
   существующее поведение `var_types` для инференса типа того же имени,
   гарантируя согласованность ТИП<->СИМВОЛ; генуинно неоднозначная ссылка
   — один файл импортирует ОБА коллидирующих кандидата без квалификатора —
   остаётся processing-order-зависимой, это НЕ новое свойство, а
   расширение уже существующего на выбор символа).
   ОСТАТОК (честно не сделан): чекер-диагностика неоднозначности импорта
   (E_AMBIGUOUS_IMPORT) — заявленный в комментарии оригинальный дизайн-
   план ("collision между export'нутыми consts — ambiguity error type-
   checker'а уровня D29") никогда не был реализован; я тоже его не
   реализовала — codegen-фикс достаточен для измеренного CC-FAIL и не
   идёт вразрез с §0/196 (не наращивает легаси-эвристику резолва ИМЁН —
   это чистое манглинг-расширение по образцу D381, а не новый резолв).

4. **№136, bare variant vs newtype**: `type_aliases: HashMap<String,String>`
   — ПЛОСКИЙ реестр по голому имени через ВЕСЬ CU (любой Newtype/Alias
   откуда угодно пишет туда). `emit_call`'s newtype-identity-cast intercept
   (Plan 115 D214, `type_aliases.get(name)`) срабатывал ПЕРВЫМ, ДО проверки
   на sum-variant-конструктор (`debt_find_variant_ctx`) — голая конструкция
   `Tagged("kind")` молча кастовалась в inner-тип НЕСВЯЗАННОГО
   `type Tagged[T,U](int)`. ФИКС: gate — newtype-путь берётся, только если
   `debt_find_variant_ctx(name, arity)` НЕ находит вариант; тот же
   arity/hint/return-sum дизамбигуатор, что уже используется для
   one-name-across-colliding-sums (D381). Байт-идентично, если ни один sum
   в CU не объявляет вариант с этим именем.

## Попутный дефект (НЕ чинить, отдельная запись реестра нужна)
`[M-bare-unit-variant-eq-invalid-cast]` (гипотеза-имя маркера): `==`/`!=`
между значением sum-типа и ГОЛЫМ `Type.Variant`-литералом (zero-field
variant) эмитит `(nova_int)(intptr_t)nova_make_X_Variant())->tag` —
скаляр с `->`. НЕ зависит от коллизии имён (control-репро с НЕколлидирующим
именем `MySign`/`MySign2` даёт ИДЕНТИЧНУЮ ошибку, БЕЗ участия value-record
даже). Корень: `emit_expr`'s "D109 qualified unit variant constructor" ветка
(Path `Type.Variant`, ~emit_c.rs:33428 область) безусловно кастует в
`(nova_int)(intptr_t)...`, а BinOp Eq/Neq для sum-типов (`emit_field_eq`)
ожидает, что ОБА операнда — настоящие указатели. Заявка реестра №154
("ПРИЧИНА НЕ В value-record ... без пользовательского Sign те же репро
зелёные") ОПРОВЕРГНУТА моим измерением — control БЕЗ коллизии тоже RED.
Репро: `scratch_repro/control_sign2.nv` (минимальный, без value-record).
