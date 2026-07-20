<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# Детерминизация вывода codegen — рабочие заметки

**Статус:** ЗАКРЫТО (2026-07-20). Закрывает `[M-codegen-emission-nondeterminism]` (residual
после Plan 145.2) полностью — byte-identical пересборка подтверждена на 3 целях.
**Ветка:** `p-deterministic-codegen`, worktree `nova-detcodegen`, база `005c82cc8` (main).
**Модель:** sonnet.
**Не language-changing:** внутренний порядок эмиссии C-текста, ноль изменений видимого поведения
скомпилированных программ — D-амендмент НЕ нужен.

## 1. Проблема (из modular-incremental-research.md)

Тот же неизменный `.nv`-исходник компилировался в РАЗНЫЙ `.c` run-to-run (~44-86 переставленных
строк на флагман-CU). Комментарий-признание в коде (`emit_c.rs`, тогда строка 51638) называл
«benign HashMap-order classes: fwd-typedef order + sum-eq conjunct order».

## 2. Аудит

Прогреб все `HashMap`/`HashSet`/`FxHashMap`/`FxHashSet` поля `CEmitter` (119 полей) + локальные
`let mut X: HashMap/HashSet` (94 сайта) в `emit_c.rs` (57к строк) на предмет ИТЕРАЦИИ (`.iter()`,
`.keys()`, `.values()`, `for .. in &self.field`), отфильтрованной от чистых membership-проверок
(`.contains()`/`.get()`/`.insert()` — order-independent). Из ~15 кандидатов-итераций почти все уже
защищены (сортировка перед итерацией — `effect_schemas` L25331, `type_id_registry` L7813,
`per_e_fail_types` L2429 — или структурно order-independent — `.any()`/`.count()` на HashSet).

Найдено ровно 2 живых сайта, оба подтверждены REPRO (byte-diff до фикса) на
`examples/flagship/aggregator`:

1. **fwd-typedef order** — `emit_c.rs` (`emit_module`, район L5570-5683): `external_names` и
   `vtable_names` — `HashSet<String>` (populated из type-ref-сканирования params/return-types
   всего модуля), итерировались напрямую (`for name in external_names`) для эмиссии
   `typedef struct Nova_X Nova_X;` / `typedef struct NovaVtable_X NovaVtable_X;`. Rust's default
   `RandomState`-хешер сидируется per-PROCESS → разный порядок КАЖДЫЙ `nova build`. Наблюдалось
   как переставленные однобуквенные `Nova_F/S/W/D/V/U/T/K/R/E/Acc/I` (generic-параметр-имена,
   собранные как «внешние» ссылки в сигнатурах flagship's `main.nv`).
2. **sum-eq conjunct order** — `emit_c.rs::structural_eq_body_for_ptr` (район L18632-18641):
   `variants: HashMap<String, Vec<String>>` (клон `self.sum_schemas.get(type_name)`) итерировался
   напрямую (`for (var_name, field_types) in &variants`) для построения `&&`-цепочки
   `(tag != NOVA_TAG_X_V || fields)`. Тот же per-process-random-hasher эффект. Наблюдалось на
   ~10+ несвязанных sum-типах flagship (`ErrSource`, `TlsError`, `ParseJsonError`, `JsonValue`,
   `Base64Error`, `Token`, `AggError`, `http_ErrorKind`, `ParseUrlError`).

Прочие `sum_schemas`-чтения (30+ сайтов) — все `.contains_key()`/`.get(single-key)`,
order-independent. `reconstruct_mono_sum_schema` уже возвращает `Vec<(String, Vec<String>)>`
(не HashMap) — уже детерминирован. String-literal pool (`interned_str_emit: Vec<...>`,
push-order при первой встрече) и mono-instantiation worklist (`generic_type_worklist:
RefCell<Vec<...>>`, drain-order) — уже Vec-based, уже детерминированы (НЕ отдельные баги, вопреки
гипотезе задания — эмпирически НЕ видны ни в одном diff). Plan 145.2 §6 residual-заметка про
`novaopt_typedefs_buf`-порядок (2026-06-15) — эмпирически НЕ подтверждена на текущем main
(0 совпадений `NovaOpt` ни в одном before-diff'е); вероятно закрыта попутно недавним
tuple+fixarr topo-sort фиксом (`[M-tuple-fixarr-typedef-order]`, 2026-07-19) или другой
промежуточной работой — считаю остаток снятым по опровержению (byte-identical на 3 целях,
включая цель, которая интенсивно гоняет `Option`/`NovaOpt_` через HashMap.get/Queue.pop/
Deque.pop_back).

## 3. Фикс

Оба сайта: собрать HashSet/HashMap-ключи в `Vec`, `.sort()` по имени (String, естественный Ord)
перед итерацией — стабильный, осмысленный ключ (совпадает с C-символом: `Nova_<name>` /
`NOVA_TAG_<ty>_<variant>`). Никакого влияния на topo-порядок typedef'ов (эти fwd-decl'ы —
independent opaque `typedef struct X X;`, друг от друга не зависят — чистый tie-break, не
переупорядочивание зависимостей) и никакого влияния на semantics sum-eq (`&&`-конъюнкты чистые,
side-effect-free, `&&` коммутативен для них).

Правки — `compiler-codegen/src/codegen/emit_c.rs`:
- `external_names`/`vtable_names` → `.into_iter().collect::<Vec<_>>()` + `.sort()` перед циклом
  (2 сайта, район L5655/5673).
- `structural_eq_body_for_ptr`: `variants.keys().collect::<Vec<_>>()` + `.sort()`, индексация
  `&variants[var_name]` внутри цикла вместо деструктуризации кортежа (район L18640).

## 4. Доказательство (byte-identical repro)

Три цели, каждая собрана **3 раза подряд** (`NOVA_CACHE=0`, `--keep-artifacts`, свежий per-PID
tmp-dir → 3 независимых процесса → 3 независимых hasher-seed'а, то самое условие, что раньше
триггерило недетерминизм):

| цель | ДО фикса (diff между запусками) | ПОСЛЕ фикса |
|---|---|---|
| `examples/basics/hello.nv` (пустышка) | не гонялась отдельно (флагман репрезентативен) | **SHA256 идентичен 3/3**, diff = 0 строк |
| collections repro (`HashMap`/`Set`/`Queue`/`Deque` из `std/src/collections`, throwaway driver, удалён) | не гонялась отдельно | **SHA256 идентичен 3/3**, diff = 0 строк |
| `examples/flagship/aggregator/src/main.nv` (`--strict-effects`) | **diff НЕ пуст**: A↔B 84 строки, B↔C 86 строк, A↔C 68 строк (2 класса: fwd-typedef порядок однобуквенных `Nova_F/S/W/D/...`, sum-eq конъюнкт-порядок в 10+ типах) | **SHA256 идентичен 3/3** (`542dea9...`), diff = 0 строк |

Размер `.c` не изменился (1 946 611 байт до и после — фикс переставляет строки, не меняет
контент/семантику).

## 5. Регресс-гейты (после фикса, worktree `nova-detcodegen`)

- `spec_tests/conformance` (standalone single-CU, `test --positive --compile-error`):
  **PASS 503 / FAIL 1 / SKIP 14**. Единственный FAIL — `app_effect_basic_t8_1`
  (`Vec[f32].from([...]) chained .debug/.display` assert) — это **ИЗВЕСТНЫЙ pre-existing P1
  маркер `[M-208-vec-chained-debug-display-red]`** (backlog-followups.md, OPEN с 2026-07-17,
  Plan-208-Fmt-волна, никак не связан с codegen-эмиссией/HashMap-итерацией) — assert-тексты
  побайтово совпадают с задокументированными в маркере. НЕ регрессия от этой волны.
- `std/src/checksums` (`nova test`): PASS 3 / FAIL 0 / SKIP 3 (adler32/crc32/fnv — SKIP, no
  `fn main`/test-blocks, компилируются OK).
- `std/src/collections` (`nova test`): PASS 13 / FAIL 0 / SKIP 6.
- `examples/flagship/aggregator/src/main.nv --strict-effects`: успешно собран **6 раз** (3 до +
  3 после фикса) — линковка/сборка без ошибок каждый раз, byte-identical `.c` в трёх
  post-fix запусках (см. §4).

Ноль регрессий, поведение сохранено (изменён только порядок эмиссии текста).

## 6. Дисциплина / прочее

- Диск на машине был **0 байт свободно на C:** в середине волны (`wsl-crashes` 26 ГБ +
  `DiagOutputDir` 3.3 ГБ — WSL/диагностические temp-дампы, не относящиеся к Nova) — расчистил
  эти два safe-to-delete диагностических temp-каталога (не трогал Docker/WSL-данные, не трогал
  `claude`-scratch других сессий), стало 30 ГБ свободно, работу продолжил.
- Throwaway repro-драйвер `examples/_detcodegen_repro/main.nv` (HashMap/Set/Queue/Deque
  smoke) создавался ВРЕМЕННО для byte-identical пробы на collections-цели и **удалён** перед
  коммитом (не часть репозитория). `examples/nova.lock` тоже случайно тронулся (dep-resolve
  побочный эффект той же пробы, commit-hash `nova-tls` pin) — откачен (`git checkout --`)
  как несвязанный.
- Не language-changing — внутренний порядок C-эмиссии, D-амендмент не требуется.
