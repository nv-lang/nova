# variants-sum-literals — заметки окна (221.1 №156 + №155а)

Модель: sonnet. Оба дефекта — про то, как codegen обрабатывает конструкции
вариантов сумм; корни РАЗНЫЕ функции, общий фикс не найден (см. ниже).

## №156 [M-bare-unit-variant-eq-invalid-cast]

**Симптом:** `s == MySign.Pos` (голый zero-field variant литерал) → CC-FAIL
`member reference type 'nova_int' is not a pointer`.

**Корень (emit-side, ДВЕ стороны):**
- Эмиттер: `emit_c.rs` Path-арм ~33518-33586 (`D109: qualified unit variant
  constructor`) — конструктор голого zero-field варианта `Type.Variant`
  ВСЕГДА кастуется в `(nova_int)(intptr_t)nova_make_<Type>_<Variant>()`.
  Найдено происхождение (Plan 48, коммит `2ffcf1a11`, D109 HashMap Slot):
  каст нужен, когда `type_name_raw` — generic-шаблон (`self.generic_type_
  templates.contains_key`), т.е. значение уходит в ERASED generic-контекст
  (пример: `buckets.push(Slot.Empty)` в `std/src/collections/hashmap/core.nv:622`
  внутри ещё-generic `new_buckets[K,V]`). Для ПРОСТОГО non-generic суммного
  типа (`MySign`) та же ветка ошибочно применяла тот же каст.
- Приёмник: `emit_field_eq`'s `is_single_nova_ptr` ветка (~emit_c.rs:20913,
  `structural_eq_body_for_ptr` ~21054) разыменовывает операнд как НАСТОЯЩИЙ
  `Nova_X*` (`(l)->tag`), не проверяя форму текста.

**Почему приёмная сторона, а не эмиссионная:** `infer_expr_c_type`'s D406-
ветка для `Path(["Type","Variant"])` (emit_c.rs ~60539-60552) УЖЕ корректно
объявляет тип этого выражения как `Nova_Type*` (указатель) — то есть
type-инференс и C-эмиссия для ОДНОГО И ТОГО ЖЕ AST-узла расходятся: канал
говорит «указатель», эмиттер даёт «скаляр». Пока не выяснено конкретно,
нужен ли каст `(nova_int)` вообще в НЕ-generic случае buckets.push (Vec[Slot[K,V]]
после мономорфизации хранит РЕАЛЬНЫЙ типизированный `Nova_Slot____K__V*`,
а не эрейзд-скаляр) — распутывание этого потребовало бы более глубокой
регрессии по generic-эрейзеру, вне бюджета этого окна. Вместо демонтажа
каста в эмиттере (риск сломать Plan-48 HashMap-путь) фикс — defensive
recast на приёмной стороне: `((cty)(l))`/`((cty)(r))` перед ЛЮБЫМ
использованием внутри `is_single_nova_ptr`-ветки. No-op для настоящего
указателя, round-trip для erased-scalar формы; покрывает разом tag-
рекурсию, @equal/@compare диспетч и named-eq-fn (cyclic recursion) —
единая точка входа.

**Фикс:** `emit_c.rs` ~20913-20919 (внутри `emit_field_eq`).
**Гипотеза (НЕ проверено, для будущего окна):** возможно, D109-каст в
эмиттере (~33580) излишен ДАЖЕ для generic-erased случая (Vec[Slot[K,V]]
после mono хранит типизированный указатель, не скаляр) — если так, каст
можно снести целиком и приёмный фикс тоже не нужен. Не трогал — вне
бюджета, риск для Plan-48 HashMap-пути без глубокой регрессии.

## №155а [M-flagship-anon-record-literal-enum-payload]

**Симптом:** `TaskStatus.Done({ id, payload: .. })` (голый record-литерал
как payload-аргумент варианта) → codegen error `anonymous record literal:
expected struct 'TaskStatus' not in record_schemas`.

**Живой сайт обхода (стало устаревшим после фикса — задача указывала
`aggregate_test.nv:52`, но фактически обход был в
`examples/flagship/aggregator/src/api/report_json_test.nv:63`;
`aggregate_test.nv:52` на момент этого окна — просто `for r in
report.results {`, никакого обхода там нет и не было; проверено grep'ом
по всему flagship — единственное совпадение `TaskStatus.Done(SourceData`
было в `report_json_test.nv`).**

**Корень:** `try_emit_explicit_variant_ctor` (~emit_c.rs:4412) — ЕДИНСТВЕННАЯ
точка эмиссии для explicit-receiver payload-variant call `Sum.Variant(args)`.
Эмитила каждый arg голым `emit_expr` БЕЗ scoping `expected_record_type` (D55
inferred-type-context сигнал, нужный `emit_record_lit`'у чтобы понять, в
какой struct кастовать голый `{ .. }`). Без него `expected_record_type`
оставался тем, что выставил ВНЕШНИЙ контекст (обычно ничего либо имя суммы),
никогда — реальный payload-тип варианта (`SourceData`).

Зеркальный паттерн уже существовал для `Ok(v)`/`Err(e)` — [M-181-anon-
record-in-ctor-arg-codegen] (~emit_c.rs:37829, другая функция того же
семейства, `emit_call`'s `Ident` arm) — но НЕ распространялся на общий
`Sum.Variant(args)` explicit-receiver путь.

**Фикс:** `try_emit_explicit_variant_ctor` теперь достаёт `field_c_types`
найденного варианта (уже резолвлены для проверки арности — `owns`→
`find_fields`) и scopes `expected_record_type` ПЕРЕД эмиссией КАЖДОГО arg
из `debt_struct_name_from_c_type(field_c_types[i])`; restore после цикла.
No-op для скалярных/nova_str полей (функция вернёт `None`).

**Обход снят:** `report_json_test.nv:62-63` теперь `TaskStatus.Done({ id,
payload: []u8.new() })` — bare-форма, без явного `SourceData` префикса.

## Общий ли корень

НЕТ. Разные функции (`emit_field_eq` vs `try_emit_explicit_variant_ctor`),
разные механизмы (приёмный ре-каст типа vs propagation `expected_record_type`).
Общее — обе бьют по konstrukция́м вариантов сумм, оба гэпа обнаружены в одном
семействе D109/D55 механизмов кодогена.

## arch-ratchet рост

emit_c.rs: 63739 (baseline) → 63751 = **+12 строк** (после двух раундов
сокращения комментариев: было +40 сразу после первых фиксов, затем +22
после первого сокращения, теперь +12 — минимум, который сохраняет
маркер-комментарий формата `[M-NNN] (реестр, файл/коммит)` для будущих
агентов, без чего маркер-конвенция (§ zero-tolerance-bugs) не соблюдается).
Baseline НЕ трогал — решение об апдейте за интегратором.

Разбивка +12 (по `wc -l`):
- `try_emit_explicit_variant_ctor` (№155а): чистый рост ~+2..+5 строк —
  `owns: bool`-closure заменён на `find_fields: Option<Vec<String>>`
  (нужны сами field_c_types, не только факт владения) + 2 строки
  save/restore `expected_record_type` вокруг цикла + короткий
  маркер-комментарий (3 строки).
- `emit_field_eq` (№156): +7 строк — 4 строки кода (2×`format!` recast +
  2×`.as_str()` шедоуинг) + 3-строчный маркер-комментарий.

## Прочие находки (НЕ чинил, вне периметра задачи)

- **Флаки-тест флагмана** (маркер отсутствует, репро нестабильно): `nova
  test-build examples/flagship/aggregator/src/app/aggregate_test.nv` —
  тест `"aggregate: done/failed/cancelled settle correctly on a mixed
  fan-out"` иногда падает на `assert(report.done == 2)` (timing-
  чувствительная конкурентная проверка, реальные дедлайны/fiber-scheduling).
  Один прогон `nova test examples/flagship/aggregator` дал RUN-FAIL,
  повторный `nova test-build` того же файла — PASS. Не связано с sum-
  литералами; не трогал.
- **`test-build` не умеет изолировать один файл folder-module** (тулинг-
  гэп, не мой код): `nova test-build spec_tests/conformance/<любой-файл>.nv`
  для folder-module `spec_tests.conformance` даёт `lld-link: error:
  undefined symbol: Tagged` — ПРОВЕРЕНО на уже зелёной baseline-фикстуре
  `m178_variant_ctor_crosssum_option_collision.nv` (та же ошибка, не
  связана с моими правками). Точечную проверку новых conformance-фикстур
  сделал через `nova check <file>` (type-check, без линковки — PASS: 1
  FAIL: 0 для обеих) + эквивалентные standalone-репро в scratchpad
  (build+run, GREEN) — так как `nova test <dir>` на всю папку = мега-CU,
  запрещённый в этом окне.
- **`sum_per_variant.nv`** использует legacy leading-`|` форму (`type Node3
  | SpvLeaf | ...`) вместо канонической `type X enum A | B | C` (D406,
  memory-note `feedback-sum-enum-marker-d406` — «тихо проскакивает,
  грепать»). Пре-существующее, не трогал.
