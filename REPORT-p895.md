# Отчёт по №895 — облачная сессия, 2026-09-30, ветка `p895-static-via-param`

КОРЕНЬ: два разных дефекта под одной строкой.
(1) Мономорфизация свободной generic-функции брала подстановку из C-строки (`I → "nova_int"`, дальше `ResolvedType::Raw`), хотя канал чекера на том же вызове уже знал ответ. Замер временным зондом на месте вызова (зонд снят): `ZOND895 fn=mk subst=[("I", "nova_int")] node=Some([("I", Named { name: "FnRow", .. })])`, для `b_record` — `subst=[("I", "Nova_Cell*")] node=[("I", Named { name: "Wrap" })]`. Имя newtype стиралось в ЭМИТТЕРЕ, а не в канале. Из-за этого стоял и ключ экземпляра: `mk[FnRow]` и `mk[int]` давали один экземпляр. Проба `p895_newtype_mono_distinct` без правки печатает `row=2 int=2 row2=3`: newtype молча получал статик `int`. Это тихая мискомпиляция, а не только отказ сборки.
(2) Статический set-blanket: в `types/mod.rs` Path-вызов на примитиве делал ранний `return` (при отсутствии своих overload'ов). Поэтому `i64.from_ordinal(3)` проходил чекер без callee и без типа в канале, а эмиттер искал ключ `("i64", m)`, тогда как blanket зарегистрирован под `("T", m)`.

ФИКС:
- `compiler-codegen/src/codegen/emit_c.rs:48713` — место вызова свободной generic-функции берёт номинальные слоты из A1″-канала (`mono_nominal_slots`). Имя экземпляра строится с Nova-типом (`mk____Nova_FnRow`), тело получает реальный RT; C-параметр остаётся представлением. Помощники — в `codegen/emit_c/mono_nominal.rs`.
- `emit_c.rs:48241` — статический диспатч `T.m()` берёт имя newtype из подстановки (`subst_static_recv_name`).
- `types/mod.rs:18023` + `types/static_blanket.rs` — чекер находит статический set-blanket, чей type-set содержит примитив (тот же предикат членства, что у №930). Он пишет callee и тип `T := prim` в канал.
- `emit_c.rs:48269` + `codegen/emit_c/static_blanket.rs` — эмиттер мономорфизирует callee из канала как `Nova_<prim>_static_<m>`.

КЛЕТКИ:
- `newtype_bound`: до — `E_UNKNOWN_STATIC_METHOD int.from_ordinal`; после — собран, печатает `42`.
- `set_blanket_static`: до — `[INTERNAL-PANIC] [E_CODEGEN_TYPE_UNKNOWN]`; после — собран, печатает `3`.
- `b_record`: до — `undefined reference to Nova_Cell_static_from_ordinal`; после — собран, печатает `42`.
- `c_plain_record`: `42` → `42`.
- `d_instance_bound`: `42` → `42`.
- `direct`: `42` → `42`.
- `e_neg_int`: `E_BOUND_NOT_SATISFIED` → `E_BOUND_NOT_SATISFIED`.
- `mk[FnRow]` / `mk[int]`: до — один экземпляр; после — `nova_fn_…mk____Nova_FnRow(nova_int n)` вызывает `Nova_FnRow_static_from_ordinal`, `nova_fn_…mk____nova_int(nova_int n)` вызывает `Nova_int_static_from_ordinal`. Снято с `--keep-artifacts`.
- Вне приёмки, случай Карины `IdxVec[FnRow, str]` с `push -> I` и `I.from_ordinal(n)` в методе: до — `E_UNKNOWN_STATIC_METHOD`; после — то же, НЕ ПОЧИНЕНО (см. ОТКРЫТО).

ФИКСТУРЫ (проба в обе стороны на одном дереве: правка компилятора снята `git apply -R`, пересобрано, затем возвращена):
- `spec_tests/conformance/standalone/p895_static_via_newtype_param.nv`: без правки — `CODEGEN-FAIL [E_UNKNOWN_STATIC_METHOD] int.from_ordinal(...)`; с правкой — `PASS`. Клетки: newtype над int, newtype над записью, простая запись, вложенный `outer[I]` → `mk[I]`.
- `spec_tests/conformance/standalone/p895_newtype_mono_distinct.nv`: без правки — `NEG-WRONG-STDOUT … not found in: row=2 int=2 row2=3`; с правкой — `PASS`.
- `spec_tests/conformance/standalone/p895_static_set_blanket.nv`: без правки — `CODEGEN-FAIL [INTERNAL-PANIC] [E_CODEGEN_TYPE_UNKNOWN] Path call return type unknown for method=p895_from_ordinal`; с правкой — `PASS`.

КРЕЙТ: `compiler-codegen --lib`: 1275 прошло / 0 упало (`RUST_MIN_STACK=134217728 cargo test --release --lib`).
Точечные прогоны:
- `nova test spec_tests/conformance/standalone --filter generic`: 27/0.
- То же с `--filter`: `static` 10/0, `blanket` 5/0 (1 skip), `newtype` 5/0, `p895` 3/0.
- `spec_tests/conformance/neg --full --filter`: `blanket` 7/0, `set_` 7/0, `static` 10/0, `newtype` 7/0, `bound` 26/0.
- Мега-CU, полный `nova test`, `gate.sh` не гонялись.

ХРАПОВИК: `emit_c.rs` 68851 строк, база 68857 (`scripts/guards/arch-ratchet.baseline`); до правки было 68843, рост +8. `infer` 246 при базе 247. Новый код — в дочерних модулях.

КОММИТЫ:
- `b497f04` fix(#895): keep a newtype's identity in fn-level mono; emit static set-blankets
- `ab03bbc` registry(#895): fixed in branch p895-static-via-param; generic-type cell stays open
- (этот отчёт — последним отдельным коммитом)

ОТКРЫТО:
1. Экземпляр generic-ТИПА: `type IdxVec[I Ordinal, T]`, метод `IdxVec[I, T] mut @push(v T) -> I` с `I.from_ordinal(n)`, вызов на `IdxVec[FnRow, str]`. Это ровно `IndexVec` из архитектуры novac. Он по-прежнему даёт `E_UNKNOWN_STATIC_METHOD int.from_ordinal`. Производитель другой: аргументы типа ключуются C-строкой в `generic_type_instance_info` / `generic_type_worklist` (A1‴ перевёл носитель на `Vec<ResolvedType>`, но наполняет его `Raw` через `args_lift`). Имя структуры экземпляра (`Nova_IdxVec____nova_int__nova_str`) строится из C-строк в нескольких местах; `____` разбирается строкой примерно в 30 местах. Проба (не закоммичена, по правилу «пробы вне репо»):
   ```
   type IdxVec[I Ordinal, T] { mut items Vec[T] }
   fn IdxVec[I, T].new() -> IdxVec[I, T] => { items: Vec.new() }
   fn IdxVec[I, T] mut @push(v T) -> I { ro n = @items.len(); @items.push(v); I.from_ordinal(n) }
   // main: mut a = IdxVec[FnRow, str].new(); a.push("x")
   ```
2. `str.from_ordinal(3)` при `fn[T SignedInts] T.from_ordinal` чекер по-прежнему пропускает молча, а сборка падает прежним `E_CODEGEN_TYPE_UNKNOWN`. Отказ (`E_TYPE_NOT_IN_SET`, зеркально №930) не добавлен: у примитивов есть статики рантайма, которых чекер не видит, и отказ по имени мог бы ложно сработать на `int.<рантайм-статик>`. Это отдельный ряд.
3. Статический set-blanket, вызванный ЧЕРЕЗ параметр типа (`fn f[T SignedInts]() -> T => T.from_ordinal(1)`), не проверялся: чекер там идёт по ветке `gs`, а не по новой.
4. Цена дублей экземпляров на большом корпусе не замерена. Номинальный ключ появляется только у слотов-newtype. Без newtype-аргументов имя и подстановка прежние по построению: при пустом `nominal` помощник и есть `compute_mono_name`, а подмена worklist — no-op. На мега-CU это не проверено.
5. `type Al alias Cell` с `mk[Al]` отвергается чекером `E_BOUND_NOT_SATISFIED` («`Al` is missing ordinal»), хотя alias по D52 прозрачен. Найдено попутно, не исследовано, в реестр не заведено — решение за интегратором.

ВОПРОСЫ ИНТЕГРАТОРУ:
1. №895, клетка generic-ТИПА (`IdxVec[FnRow, T]`, IndexVec Карины). Для функций починка сделана, для экземпляров generic-типов — нет: там аргументы типа ключуются C-строкой в `generic_type_instance_info`. Варианты:
   (а) номинальное имя экземпляра типа (`Nova_IdxVec____Nova_FnRow__nova_str`, раскладка та же). Все производители имени экземпляра и подстановки тел методов получают `Named{FnRow}` вместо `Raw("nova_int")`. Это и есть шаг A1‴/A1″ плана 172.12 для реестров типов. Затронуты `resolved_named_to_c`, `register_generic_instances_in_typeref`, `args_lift` и его 61 место вызова, около 30 мест разбора `____`, `emit_monomorphized_method`. Объём — несколько дней, `emit_c.rs` почти наверняка вырастет, нужен прогон мега-CU.
   (б) структуру оставить общей, а номинально разводить только экземпляры МЕТОДОВ, беря тип получателя из канала чекера. Это уже, но трогает около 15 мест `register_mono_method_instance` и создаёт второй ключ рядом с именем структуры.
   Рекомендую (а) отдельным шагом плана 172.12 после 0.2, как и маршрутизировал владелец. Это продолжение направления владельца («Nova-тип в имя») без второго дома правды. (б) заводит ровно тот параллельный носитель, от которого владелец ушёл 2026-09-18. Пока у Карины остаётся обход — строить индекс снаружи по `len()`.
2. Статус строки №895 я поставил «ИСПРАВЛЕНО В ВЕТКЕ», как велел бриф, и записал в строке открытую клетку generic-типа. Варианты: оставить так и завести клетку generic-типа отдельной строкой реестра (номер ваш, я его не беру), либо держать №895 открытой до решения по вопросу 1. Рекомендую отдельную строку: класс уровня функций закрыт фикстурами, а тип-уровень — другой производитель и другой объём работы.
3. Ветка. Бриф велел выкладывать в `p895-static-via-param`, а облачная среда этой сессии назначала `claude/funny-fermat-ll6daj`. Я выложила туда, куда велел бриф. Если среда не пустила этот пуш, строкой ниже будет сказано, куда ушло.
