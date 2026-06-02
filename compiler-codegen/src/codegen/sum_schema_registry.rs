//! Plan 62.A.bis Ф.1: declaration-driven schema registry для sum-types.
//!
//! Этот модуль вводит новый layered registry для информации о sum-типах,
//! который **сидит параллельно** существующему `sum_schemas: HashMap<...>`
//! полю в `CEmitter`. В Phase 1 registry заполняется (через
//! `init_hardcoded_baseline()`) но никто (кроме unit-тестов) его не читает —
//! поэтому генерируемый C байт-идентичен baseline'у.
//!
//! Phase 2+ (separate plan-items) будут постепенно мигрировать call-site'ы
//! `sum_schemas.get(...)` / `find_variant(...)` на `lookup_sum_schema(...)`
//! / `find_variant_v2(...)` и в итоге удалят hardcoded dispatch.
//!
//! Архитектура (из docs/plans/62.A.bis-sum-schema-registry.md §«Решение»):
//!
//! ```text
//! SumSchemaRegistry
//!   ├─ entries: Vec<SumSchemaEntry>
//!   │    каждый entry имеет (nova_name, c_name, variants, abi, source,
//!   │    method_routing, origin_module)
//!   └─ variant_to_sum: HashMap<variant_name, Vec<(sum_name, source)>>
//!        sorted by SchemaSource priority (DESC) для O(1) find_variant.
//! ```
//!
//! Lookup precedence (highest first):
//!   `DeclaredFromPrelude` → `DeclaredFromUser` → `HardcodedBaseline`.
//!
//! См. plan doc §«State и lifecycle» для детальной семантики.

use std::collections::HashMap;

// ──────────────────────────────────────────────────────────────────────────
// Public types
// ──────────────────────────────────────────────────────────────────────────

/// Один вариант sum-type'а с информацией о позиционных полях.
///
/// Mirror'ит представление в legacy `sum_schemas: HashMap<sum_name,
/// HashMap<variant_name, Vec<field_c_type>>>` — то есть `field_c_types`
/// это упорядоченный список C-типов позиционных полей (Tuple-variant),
/// либо пустой Vec (Unit-variant), либо для Record-variant'а — упорядоченный
/// список C-типов полей (порядок — из `record_variant_field_order`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VariantInfo {
    /// Variant name: "Some", "None", "Ok", "Err", "DivByZero", ...
    pub variant_name: String,
    /// Ordered list of field C-types. Empty Vec для unit-variant'ов
    /// (None, DivByZero, Overflow). Single-element Vec для tuple-variant'ов
    /// (Some, Ok, Err, TypeMismatch). Multi-element для multi-tuple
    /// (IndexOutOfBounds(int, int)).
    pub field_c_types: Vec<String>,
}

/// Описание ABI / C-представления sum-type'а.
///
/// Phase 1 различает только три категории — этого достаточно чтобы Phase 2+
/// мог разветвить codegen на правильный path без новых полей. Расширения
/// (e.g. `ValueTagPayload { tag_size: usize }`) — out-of-scope для Ф.1.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SumAbi {
    /// `NovaOpt_<T>` value-struct on stack. Accessor `.tag` / `.value`.
    /// Currently used by Option only (см. `nova_rt/array.h`
    /// `NOVA_DECLARE_OPTION_T(T)` macro).
    ValueOptionLike,
    /// `Nova_<Name>*` heap-pointer (via `nova_alloc`). Accessor `->tag` /
    /// `->payload.<V>._<i>`. Used by Result + Error (Error — record-like
    /// pointer but in registry он не появляется; см. §«Что не делаем»).
    PointerErrorLike,
    /// Inline value struct with tag + payload union. Accessor `.tag` /
    /// `.payload.<V>._<i>`. Used by RuntimeError (declared в array.h
    /// как value struct, не pointer).
    ValueTagPayload,
}

/// Источник декларации — определяет lookup precedence.
///
/// `Ord` impl: `DeclaredFromPrelude > DeclaredFromUser > HardcodedBaseline`.
/// `Vec<SumSchemaEntry>` в registry хранится sorted DESC по этому полю.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SchemaSource {
    /// Built-in pre-populated (Option/Result/Error/RuntimeError pre-2026-05).
    /// Lowest priority. Перебивается declared entries при registration.
    HardcodedBaseline = 0,
    /// User-defined в текущей compilation unit. Mid priority.
    /// Возникает при `emit_type_decl` для not-prelude type'ов.
    DeclaredFromUser = 1,
    /// Из `std/prelude/core.nv` / `std/prelude/errors.nv`. Highest priority.
    /// Trumps user declarations с same name (Phase 3 будет emit'ать
    /// `W_PRELUDE_SHADOW` warning при collision).
    DeclaredFromPrelude = 2,
}

/// Как codegen должен dispatch'нуть `obj.<method>(args)` для declared method.
///
/// В Ф.1 все entries для Option/Result/RuntimeError → `HardcodedRuntimeFn`
/// (mirror'ит существующий dispatch в `emit_c.rs:11567-11763`). В Ф.4
/// (отдельный plan-item) добавятся `ExternalFn` / `DeclaredBody` cases —
/// они уже scaffolded чтобы потом расширить без changes полей.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MethodRouting {
    /// Существующий trampoline в `nova_rt/array.h`: `Nova_Result_method_<m>`
    /// (non-per-T) или `Nova_Option_method_<m>_<T_sani>` (per-T).
    ///
    /// `c_name` — base symbol (e.g. `"Nova_Option_method_unwrap_or"`); при
    /// `is_per_t = true` каждый call-site добавит `_<sanitized_T>` суффикс.
    HardcodedRuntimeFn { c_name: String, is_per_t: bool },
    /// External fn декларация с runtime trampoline (no Nova body).
    /// Analog `runtime_registry.rs` entries; Phase 4 будет route declared
    /// `external fn Option[T] @unwrap_or(default T) -> T` сюда.
    ExternalFn { c_name: String },
    /// Nova-side declaration с body — codegen emit'ит method body как
    /// regular method overload через существующий `emit_method_overload`.
    /// `has_nova_body` хранится для Phase 4 dispatch (true → emit body
    /// inline; false → forward decl only).
    DeclaredBody { has_nova_body: bool },
}

/// Один entry в registry. Один sum-type → один entry per `SchemaSource`
/// (т.е. для одного `nova_name` может быть до 3 entries: одна
/// HardcodedBaseline + одна DeclaredFromPrelude + одна DeclaredFromUser).
#[derive(Debug, Clone)]
pub struct SumSchemaEntry {
    /// Nova-level type name: "Option", "Result", "RuntimeError", "MyEnum".
    pub nova_name: String,
    /// Canonical C-name типа (без `*`). Для value-types ("NovaOpt_nova_int")
    /// или heap-pointer types ("Nova_Result"). Phase 2+ будет использовать
    /// для reverse-lookup `c_ty → nova_name`. В Ф.1 хранится но не читается.
    pub c_name: String,
    /// Variants — ordered, потому что variant constructor emit order
    /// влияет на tag IDs. Vec preserves insertion order; legacy
    /// `HashMap<variant_name, Vec<...>>` теряла order — мы фиксируем.
    pub variants: Vec<VariantInfo>,
    /// ABI / C-representation classification.
    pub abi: SumAbi,
    /// Откуда entry пришёл — определяет lookup precedence.
    pub source: SchemaSource,
    /// Module path откуда задекларирован (`["std", "prelude", "core"]` для
    /// prelude entries; `None` для HardcodedBaseline / mono'd instances).
    /// Phase 3 будет использовать для `init_prelude_decls` discovery;
    /// в Ф.1 — `None` везде.
    pub origin_module: Option<Vec<String>>,
    /// Per-method routing. Key = method name (without receiver mangling).
    /// HardcodedBaseline entries содержат entries для всех 11 Option +
    /// 7 Result методов (mirror'ит emit_c.rs:11617-11854 dispatch).
    pub method_routing: HashMap<String, MethodRouting>,
}

/// Layered registry sum-type schemas.
///
/// Phase 1: only `init_hardcoded_baseline()` + lookup API. Phase 2+ добавят
/// `init_prelude_decls(module)` + `register_from_type_decl(...)`.
#[derive(Debug, Clone, Default)]
pub struct SumSchemaRegistry {
    /// Все entries в порядке регистрации. NOTE: entries для одного
    /// `nova_name` могут располагаться в любом порядке внутри Vec'а —
    /// lookup precedence решается через `variant_to_sum` reverse-index
    /// + filter по `SchemaSource`.
    entries: Vec<SumSchemaEntry>,
    /// Reverse-index variant → list of (sum_name, source) candidates.
    /// Sorted DESC by `SchemaSource` priority при каждом `register_schema`.
    /// Ускоряет `find_variant_v2` до O(log K) где K — число
    /// candidates для одного variant_name (обычно 1, для shadowing 2-3).
    variant_to_sum: HashMap<String, Vec<(String, SchemaSource)>>,
}

impl SumSchemaRegistry {
    /// Создаёт пустой registry. Используется `CEmitter::new()` →
    /// `init_hardcoded_baseline()` сразу после.
    pub fn new() -> Self {
        Self::default()
    }

    /// Регистрирует entry. Поддерживает duplicate `nova_name` с разными
    /// `SchemaSource` (для layered shadowing). Идемпотентно для
    /// `(nova_name, source)` pair — повторная регистрация overwrite'ит
    /// existing entry с тем же source.
    ///
    /// Updates `variant_to_sum` reverse-index — для каждого `variant_name`
    /// в entry.variants добавляется (sum_name, source) кортеж, затем
    /// candidates пересортируются DESC by source.
    pub fn register_schema(&mut self, entry: SumSchemaEntry) {
        // Update variant_to_sum reverse-index — добавим все варианты этого
        // entry если ещё не зарегистрированы под этим source'ом.
        let sum_name = entry.nova_name.clone();
        let source = entry.source;
        for v in &entry.variants {
            let candidates = self.variant_to_sum
                .entry(v.variant_name.clone())
                .or_default();
            // Дедуп: (sum_name, source) tuple должен быть уникален.
            if !candidates.iter().any(|(s, src)| s == &sum_name && *src == source) {
                candidates.push((sum_name.clone(), source));
            }
            // Sort DESC by SchemaSource priority — highest source first.
            // Tie-break: shorter sum_name preferred (matches existing
            // find_variant heuristic, emit_c.rs:16805-16812).
            candidates.sort_by(|a, b| {
                b.1.cmp(&a.1)
                    .then_with(|| a.0.len().cmp(&b.0.len()))
            });
        }

        // Idempotent: if (nova_name, source) entry exists — replace it
        // (callers могут refresh, e.g. mono worklist может re-register).
        if let Some(pos) = self.entries.iter().position(|e|
            e.nova_name == sum_name && e.source == source
        ) {
            self.entries[pos] = entry;
        } else {
            self.entries.push(entry);
        }
    }

    /// Primary lookup: most authoritative entry для given sum name.
    /// Returns highest-priority entry (Prelude > User > Hardcoded).
    /// Used by future Phase 2+ dispatch sites; в Ф.1 — только unit tests.
    pub fn lookup_sum_schema(&self, name: &str) -> Option<&SumSchemaEntry> {
        self.entries.iter()
            .filter(|e| e.nova_name == name)
            .max_by_key(|e| e.source)
    }

    /// Lookup with explicit fallback chain: возвращает iter'атор всех
    /// entries для given name, sorted DESC by source priority. Используется
    /// в migration steps где hardcoded baseline нужен как safety-net
    /// (e.g. Phase 4 method routing fallback).
    pub fn lookup_sum_schema_layered(&self, name: &str) -> Vec<&SumSchemaEntry> {
        let mut entries: Vec<&SumSchemaEntry> = self.entries.iter()
            .filter(|e| e.nova_name == name)
            .collect();
        entries.sort_by(|a, b| b.source.cmp(&a.source));
        entries
    }

    /// `find_variant` replacement. Возвращает (sum_name, field_c_types,
    /// source). Sort precedence: Prelude entries first → user → hardcoded;
    /// внутри одного source — shorter sum_name preferred (matches existing
    /// `find_variant` heuristic в emit_c.rs:16805-16812).
    pub fn find_variant_v2(&self, variant_name: &str)
        -> Option<(String, Vec<String>, SchemaSource)>
    {
        let candidates = self.variant_to_sum.get(variant_name)?;
        // Vec уже отсортирован DESC by source при register_schema;
        // первый кандидат — winner.
        for (sum_name, src) in candidates {
            if let Some(entry) = self.entries.iter().find(|e|
                e.nova_name == *sum_name && e.source == *src
            ) {
                if let Some(v) = entry.variants.iter().find(|v|
                    v.variant_name == variant_name
                ) {
                    return Some((sum_name.clone(), v.field_c_types.clone(), *src));
                }
            }
        }
        None
    }

    /// Number of entries в registry. Used by tests.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// True если registry пустой.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Iter всех entries (для debug / introspection).
    pub fn iter_entries(&self) -> impl Iterator<Item = &SumSchemaEntry> {
        self.entries.iter()
    }

    // ───────────────────────────────────────────────────────────────────
    // Bootstrap: init_hardcoded_baseline
    // ───────────────────────────────────────────────────────────────────

    /// Populates registry с HardcodedBaseline entries для Option / Result /
    /// RuntimeError.
    ///
    /// **Plan 78 Ф.1 (переклассификация, 2026-05-22).** Этот baseline несёт
    /// две разные по природе вещи, и важно их не путать:
    ///
    /// 1. **`method_routing`** — это **легитимный реестр C-реализации**, НЕ
    ///    хардкод-зеркало `.nv`-декларации. Методы Option/Result физически
    ///    реализованы C-функциями в `nova_rt/array.h`
    ///    (`Nova_Option_method_<m>_<T>` трамплины) либо инлайнятся codegen'ом
    ///    (`<inline>` sentinel). «Трамплин или инлайн», `c_name`, `is_per_t`
    ///    — это implementation-факты рантайма, которых в `.nv`-декларации
    ///    нет и быть не может (ср. `Result.ok()` и `Result.err()` —
    ///    неразличимые сигнатуры `-> Option[...]`, но первый трамплин,
    ///    второй inline). Это тот же класс данных, что `runtime_registry.rs`
    ///    / `external_registry.rs` — Rust-реестр C-backed API. Он **не
    ///    подлежит удалению** Plan 78: набор C-реализованных prelude-типов
    ///    фиксирован (Option/Result/RuntimeError) и не растёт с
    ///    пользовательским кодом — пользовательские типы получают методы с
    ///    Nova-телом через обычный codegen, без routing-таблицы.
    ///
    /// 2. **`variants`** (Some/None, Ok/Err, …) — вот это **действительно
    ///    зеркало** `.nv`-type-деклараций (`std/prelude/core.nv`/`errors.nv`).
    ///    Слоёный registry уже решает дубль приоритетом
    ///    `DeclaredFromPrelude > HardcodedBaseline` — baseline variants
    ///    остаются как fallback. Чистку pre-populate `sum_schemas` (то же
    ///    зеркало в `emit_c.rs`) делает Plan 78 Ф.2.
    ///
    /// **NOT included** (per design doc §«Что не делаем»):
    /// - `Error` record schema — записывается в `record_schemas`, не sum.
    /// - `ChannelPair` — record-only.
    /// - `Fail` / `Time` / `Mem` — effects, not sums.
    pub fn init_hardcoded_baseline(&mut self) {
        // Option — value-type, NovaOpt_<T> templating.
        // Mirror emit_c.rs:960-964:
        //   opt_variants.insert("Some" → vec!["nova_int"]);
        //   opt_variants.insert("None" → vec![]);
        //   sum_schemas.insert("Option" → opt_variants);
        //   sum_schemas.insert("NovaOpt_nova_int" → opt_variants);
        //
        // Один Option entry с c_name "NovaOpt_nova_int" (canonical
        // baseline mono); NovaOpt_nova_int alias регистрируется отдельным
        // entry чтобы lookup'ы по C-type name тоже работали.
        let option_variants = vec![
            VariantInfo {
                variant_name: "Some".to_string(),
                field_c_types: vec!["nova_int".to_string()],
            },
            VariantInfo {
                variant_name: "None".to_string(),
                field_c_types: vec![],
            },
        ];

        // Method routing для Option — mirror emit_c.rs:11617-11725 dispatch.
        // Все methods per-T (suffix `_<T_sani>`), кроме `unwrap` (inline)
        // / `unwrap_or_else` (inline) / `map` (inline) / `ok_or` (inline).
        // Inline-emit methods регистрируются как HardcodedRuntimeFn с
        // placeholder c_name `"<inline>"`; Phase 4 будет refactor чтобы
        // inline methods были спец routing variant.
        let mut option_methods = HashMap::new();
        // Plan 95 Ф.4.2: `is_some` / `is_none` УДАЛЕНЫ из baseline —
        // перенесены на Nova-body в std/prelude/core.nv. Routing для них
        // регистрируется через `init_prelude_decls_from_items` как
        // `MethodRouting::DeclaredBody { has_nova_body: true }`. Сохранение
        // baseline-entry создало бы dead-but-defensive shadow, который
        // в edge-case'е (prelude не загружен — теоретически невозможно,
        // R27 auto-import) мог бы маршрутизировать вызов в удалённый
        // C-трамплин → undefined symbol. Loud-fail «method not found»
        // лучше silent miscompilation.
        // Plan 95.bis Ф.2: `unwrap_or` и `or` УБРАНЫ — перенесены на
        // Nova-body в std/prelude/core.nv; routing получает
        // DeclaredBody через `init_prelude_decls_from_items` (он
        // переопределяет inherited baseline). Если оставить здесь
        // HardcodedRuntimeFn — Prelude override всё равно выигрывает
        // precedence, но baseline-mention вводит в заблуждение.
        option_methods.insert("unwrap".to_string(), MethodRouting::HardcodedRuntimeFn {
            c_name: "<inline>".to_string(),
            is_per_t: false,
        });
        // Plan 99.3 Ф.2: Option.unwrap_or_else УБРАН — на Nova-body.
        // Plan 99.3 Ф.1: Option.map УБРАН — перенесён на Nova-body
        // в std/prelude/core.nv; routing → DeclaredBody через
        // init_prelude_decls_from_items override.
        // Plan 99.3 Ф.3: Option.ok_or УБРАН — на Nova-body.

        self.register_schema(SumSchemaEntry {
            nova_name: "Option".to_string(),
            c_name: "NovaOpt_nova_int".to_string(),
            variants: option_variants.clone(),
            abi: SumAbi::ValueOptionLike,
            source: SchemaSource::HardcodedBaseline,
            origin_module: None,
            method_routing: option_methods.clone(),
        });

        // Дополнительный entry для NovaOpt_nova_int alias — mirror'ит
        // legacy sum_schemas duplicate в emit_c.rs:964. Variant_to_sum
        // получит "Some"/"None" → [(Option, baseline), (NovaOpt_nova_int,
        // baseline)] — но `find_variant_v2` prefer'ит первое (shorter name).
        self.register_schema(SumSchemaEntry {
            nova_name: "NovaOpt_nova_int".to_string(),
            c_name: "NovaOpt_nova_int".to_string(),
            variants: option_variants,
            abi: SumAbi::ValueOptionLike,
            source: SchemaSource::HardcodedBaseline,
            origin_module: None,
            method_routing: option_methods,
        });

        // Result — heap-pointer, hardcoded (Ok: nova_int, Err: nova_str)
        // bootstrap mono compromise. Mirror emit_c.rs:966-969:
        //   res_variants.insert("Ok" → vec!["nova_int"]);
        //   res_variants.insert("Err" → vec!["nova_str"]);
        //   sum_schemas.insert("Result" → res_variants);
        let result_variants = vec![
            VariantInfo {
                variant_name: "Ok".to_string(),
                field_c_types: vec!["nova_int".to_string()],
            },
            VariantInfo {
                variant_name: "Err".to_string(),
                field_c_types: vec!["nova_str".to_string()],
            },
        ];

        // Method routing для Result — mirror emit_c.rs:11727-11854.
        // Non-per-T (single trampoline принимает nova_int compromise).
        //
        // Plan 95 Ф.5.2: `is_ok` / `is_err` УДАЛЕНЫ — перенесены на
        // Nova-body в std/prelude/core.nv, routing регистрируется как
        // `DeclaredBody` через `init_prelude_decls_from_items`.
        let mut result_methods = HashMap::new();
        // Plan 95.bis Ф.2: `ok`, `err`, `unwrap_or` УБРАНЫ — перенесены
        // на Nova-body в std/prelude/core.nv; routing → DeclaredBody
        // через init_prelude_decls_from_items.
        // Plan 99.3 Ф.4: Result.unwrap_or_else УБРАН — на Nova-body.
        // Plan 99.3 Ф.5: Result.map УБРАН — на Nova-body.
        // Plan 99.3 Ф.6: Result.map_err УБРАН — на Nova-body.
        result_methods.insert("unwrap".to_string(), MethodRouting::HardcodedRuntimeFn {
            c_name: "<inline>".to_string(),
            is_per_t: false,
        });

        self.register_schema(SumSchemaEntry {
            nova_name: "Result".to_string(),
            c_name: "Nova_Result".to_string(),
            variants: result_variants,
            abi: SumAbi::PointerErrorLike,
            source: SchemaSource::HardcodedBaseline,
            origin_module: None,
            method_routing: result_methods,
        });

        // Error — НЕ регистрируется как sum entry. См. emit_c.rs:977-985
        // — Error это record (record_schemas), не sum. Plan 62.A.bis
        // §«Что не делаем» явно говорит «records — out of scope».
        // Если в будущем потребуется — добавится отдельный entry.

        // RuntimeError — non-generic value-type, all 6 variants pre-populated.
        // Mirror emit_c.rs:1004-1023:
        //   rt_variants.insert("DivByZero" → vec![]);
        //   rt_variants.insert("Overflow" → vec![]);
        //   rt_variants.insert("IndexOutOfBounds" → vec!["nova_int", "nova_int"]);
        //   rt_variants.insert("TypeMismatch" → vec!["nova_str"]);
        //   rt_variants.insert("AssertFailed" → vec!["nova_str"]);
        //   rt_variants.insert("NoHandler" → vec!["nova_str"]);
        //
        // NOTE: IndexOutOfBounds — record-variant (field names
        // {index, length}); в legacy sum_schemas хранится как Vec без
        // имён. Поля имён живут отдельно в `record_variant_field_order`
        // / `record_variant_field_types` HashMap'ах (in CEmitter, не в
        // registry). Phase 3 (init_prelude_decls) добавит парсинг
        // record-variant'ов declared в файле; в Ф.1 — лишь mirror
        // legacy positional layout.
        let runtime_error_variants = vec![
            VariantInfo {
                variant_name: "DivByZero".to_string(),
                field_c_types: vec![],
            },
            VariantInfo {
                variant_name: "Overflow".to_string(),
                field_c_types: vec![],
            },
            VariantInfo {
                variant_name: "IndexOutOfBounds".to_string(),
                field_c_types: vec!["nova_int".to_string(), "nova_int".to_string()],
            },
            VariantInfo {
                variant_name: "TypeMismatch".to_string(),
                field_c_types: vec!["nova_str".to_string()],
            },
            VariantInfo {
                variant_name: "AssertFailed".to_string(),
                field_c_types: vec!["nova_str".to_string()],
            },
            VariantInfo {
                variant_name: "NoHandler".to_string(),
                field_c_types: vec!["nova_str".to_string()],
            },
        ];

        // RuntimeError methods — пока нет методов на нём (Plan 62.C будет
        // мигрировать declarations в `std/prelude/errors.nv`). Пустой map.
        let runtime_error_methods = HashMap::new();

        self.register_schema(SumSchemaEntry {
            nova_name: "RuntimeError".to_string(),
            c_name: "Nova_RuntimeError".to_string(),
            variants: runtime_error_variants,
            abi: SumAbi::ValueTagPayload,
            source: SchemaSource::HardcodedBaseline,
            origin_module: None,
            method_routing: runtime_error_methods,
        });
    }

    /// **Plan 62.A follow-up (2026-05-18):** discovery of file-based prelude
    /// method declarations. Scan'ит `items` для `Item::Fn` с receiver
    /// `Option` или `Result` (имена sum-типов из `std.prelude.core`), и
    /// регистрирует `DeclaredFromPrelude` entries в registry с
    /// `method_routing` map'ом, унаследованным от соответствующих
    /// `HardcodedBaseline` entries.
    ///
    /// **Behavior-preserving migration**: `lookup_method_routing("Option",
    /// "unwrap_or")` после prelude scan'а возвращает тот же
    /// `HardcodedRuntimeFn { c_name: "Nova_Option_method_unwrap_or",
    /// is_per_t: true }` что и до scan'а (потому что Prelude entry inherits
    /// routing от Hardcoded). Это позволяет переносить источник правды
    /// (declaration moves from Rust HashMap → Nova file) без изменения
    /// dispatch на call-site'ах.
    ///
    /// **Filter (методы)**: registers Prelude method-routing entry ТОЛЬКО
    /// для methods на `Option` / `Result`. Static fns (без receiver) и
    /// methods на других типах игнорируются.
    ///
    /// **Plan 62.C extension (2026-05-18)**: scan также распознаёт
    /// `Item::Type` с `TypeDeclKind::Sum` для прелюдных error-типов
    /// (`RuntimeError`, `ReadBufferError`). Логика:
    /// - **`RuntimeError`** — HardcodedBaseline уже зарегистрирован
    ///   (см. `init_hardcoded_baseline` lines ~458-497). Проверяем
    ///   variant set strict-equality между prelude-declaration и baseline
    ///   (per Plan 62.A.bis acceptance: «strict equality в 62.A.bis;
    ///   extension requires edition bump (Plan 62.F)»). При совпадении
    ///   регистрируем Prelude entry, наследуя `variants` + `abi` +
    ///   `c_name` + `method_routing` от baseline (currently empty
    ///   routing — RuntimeError не имеет methods в bootstrap'е).
    ///   Mismatch (variant set differs) — silently skip Prelude
    ///   registration (HardcodedBaseline продолжает работать как
    ///   fallback); в будущем сюда добавится `W_PRELUDE_VARIANT_DRIFT`.
    /// - **`ReadBufferError`** — НЕТ HardcodedBaseline (не входит в
    ///   `init_hardcoded_baseline`). Регистрируем Prelude entry с
    ///   variants парсимыми напрямую из AST. ABI = `PointerErrorLike`
    ///   (heap-pointer, по аналогии с Result в bootstrap'е).
    ///   `method_routing` — empty map (нет methods на ReadBufferError).
    ///
    /// **Idempotent**: повторный вызов с тем же items overwrite'ит Prelude
    /// entry с same name; behavior preserved.
    ///
    /// **Type marker**: `items` — `&[Item]` через generic чтобы не вводить
    /// cyclic зависимость registry → ast. Caller (emit_module) передаёт
    /// `&module.items`.
    pub fn init_prelude_decls_from_items(
        &mut self,
        items: &[crate::ast::Item],
    ) {
        use crate::ast::Item;

        // ─────────────────────────────────────────────────────────────────
        // Part 1 (Plan 62.A follow-up): method-routing inheritance для
        // Option / Result (см. doc-comment выше).
        // ─────────────────────────────────────────────────────────────────

        // Собираем имена sum-types, для которых HardcodedBaseline уже зарегистрирован
        // (Option, Result). Только для них имеет смысл создавать Prelude override.
        let prelude_sum_names = ["Option", "Result"];

        for sum_name in prelude_sum_names {
            // Соберём все methods, объявленные на этом sum-type в items'ах.
            // Разделяем на external (C-routing inherited from baseline) и
            // non-external (Nova-body — Plan 95 Ф.1.2, override routing
            // на `DeclaredBody`).
            let mut external_methods: Vec<String> = Vec::new();
            let mut nova_body_methods: Vec<String> = Vec::new();
            for it in items {
                let Item::Fn(f) = it else { continue; };
                let Some(recv) = &f.receiver else { continue; };
                if recv.type_name != sum_name { continue; }
                if f.is_external {
                    external_methods.push(f.name.clone());
                } else {
                    nova_body_methods.push(f.name.clone());
                }
            }

            if external_methods.is_empty() && nova_body_methods.is_empty() {
                // Этот sum-type не задекларирован в файле — оставляем только
                // HardcodedBaseline (skip Prelude registration). Это поддерживает
                // partial declarations (если файл declar'ит только часть methods,
                // остальные будут продолжать работать через HardcodedBaseline).
                continue;
            }

            // Найдём HardcodedBaseline entry для копирования метаданных
            // (variants + abi + method_routing).
            let baseline = self.entries.iter()
                .find(|e| e.nova_name == sum_name && e.source == SchemaSource::HardcodedBaseline)
                .cloned();

            let Some(baseline) = baseline else {
                // Нет baseline — пропускаем (defensive; не должно случиться
                // после init_hardcoded_baseline).
                continue;
            };

            // **Critical**: inherit `method_routing` from HardcodedBaseline.
            // Это behavior-preserving: declared method WITHOUT explicit
            // override routing продолжает использовать тот же
            // `HardcodedRuntimeFn` / `<inline>` sentinel что и раньше.
            //
            // В будущем (Plan 62.B+) можно расширить — например, добавить
            // per-method override через doc-attrs или специальный syntax
            // (`#routing(c_name = "...")`), чтобы Prelude entry могла
            // переопределить routing для нового trampoline'а. Пока — pure
            // inheritance.
            let mut prelude_routing = baseline.method_routing.clone();

            // Plan 95 Ф.1.2: для non-external (Nova-body) methods —
            // **override** routing на `DeclaredBody`. Inherited baseline
            // routing (e.g. `HardcodedRuntimeFn` для `is_some`) перебивается:
            // call-site через `lookup_method_routing` получит `DeclaredBody`
            // вместо `HardcodedRuntimeFn` → перехват `NovaOpt_` (#6) /
            // `is_result_like` (#7) пойдёт в новую ветку, которая
            // monomorphизирует Nova-тело per-T вместо вызова C-трамплина.
            //
            // External methods — routing унаследован из baseline (точно
            // то, что было до декларации), behavior-preserving.
            //
            // Bootstrap-conservative: НЕ добавляем routing entry для
            // unknown external method'ов — оставляем lookup'ы возвращать
            // None (declaration `external fn Option @or(...)` без codegen
            // support даст «method or not found» при call вместо
            // искажённого dispatch'а).
            for m in &nova_body_methods {
                prelude_routing.insert(
                    m.clone(),
                    MethodRouting::DeclaredBody { has_nova_body: true },
                );
            }

            self.register_schema(SumSchemaEntry {
                nova_name: sum_name.to_string(),
                c_name: baseline.c_name.clone(),
                variants: baseline.variants.clone(),
                abi: baseline.abi,
                source: SchemaSource::DeclaredFromPrelude,
                origin_module: Some(vec![
                    "std".into(), "prelude".into(), "core".into(),
                ]),
                method_routing: prelude_routing,
            });
        }

        // ─────────────────────────────────────────────────────────────────
        // Part 2 (Plan 62.C): sum-type Prelude registration для error-типов
        // из std/prelude/errors.nv.
        // ─────────────────────────────────────────────────────────────────

        for item in items {
            let Item::Type(t) = item else { continue; };
            let crate::ast::TypeDeclKind::Sum(variants) = &t.kind else { continue; };

            match t.name.as_str() {
                "RuntimeError" => {
                    self.register_prelude_sum_inheriting_baseline(
                        &t.name, variants,
                        &["std".into(), "prelude".into(), "errors".into()],
                    );
                }
                "ReadBufferError" => {
                    self.register_prelude_sum_from_decl(
                        &t.name, variants,
                        SumAbi::PointerErrorLike,
                        &["std".into(), "prelude".into(), "errors".into()],
                    );
                }
                _ => {
                    // Другие sum-types из user code НЕ регистрируются здесь —
                    // их обрабатывает `register_user_sum` через emit_type_decl
                    // (DeclaredFromUser source). Только known prelude error
                    // types из std/prelude/errors.nv ловятся здесь как Prelude.
                }
            }
        }
    }

    /// **Plan 62.C helper**: регистрирует Prelude entry для sum-type, у
    /// которого уже есть HardcodedBaseline entry — наследует variants +
    /// abi + c_name + method_routing от baseline. **Strict variant-set
    /// equality**: declaration МУСТ содержать ровно тот же набор variant
    /// names что и baseline; mismatch → silent skip (HardcodedBaseline
    /// fallback). См. Plan 62.A.bis acceptance criteria.
    ///
    /// Используется для `RuntimeError` — миграция declaration источника
    /// правды (hardcoded HashMap → std/prelude/errors.nv) при сохранении
    /// behavior (codegen продолжает использовать pre-populated sum_schemas
    /// как ABI-compat fallback).
    fn register_prelude_sum_inheriting_baseline(
        &mut self,
        nova_name: &str,
        decl_variants: &[crate::ast::SumVariant],
        origin_module: &[String],
    ) {
        let baseline = self.entries.iter()
            .find(|e| e.nova_name == nova_name && e.source == SchemaSource::HardcodedBaseline)
            .cloned();

        let Some(baseline) = baseline else {
            // Нет baseline — нечего наследовать. Defensive — для known
            // prelude types (`RuntimeError`) baseline должен быть зарегистрирован.
            return;
        };

        // Strict variant set check — declaration variant names МУСТ совпадать
        // с baseline. Иначе skip (mismatch logged через debug-only path
        // в будущем; в bootstrap'е — silent для regression-safety).
        let decl_names: std::collections::HashSet<&str> = decl_variants.iter()
            .map(|v| v.name.as_str()).collect();
        let baseline_names: std::collections::HashSet<&str> = baseline.variants.iter()
            .map(|v| v.variant_name.as_str()).collect();
        if decl_names != baseline_names {
            // Variant set drift — Plan 62.F edition bump territory.
            // Skip Prelude registration; HardcodedBaseline продолжает
            // работать как fallback. Это означает что user'ская
            // declaration с другим набором variants попадёт как
            // DeclaredFromUser (через emit_type_decl) только если она
            // НЕ skipped по RUNTIME_DEFINED_TYPES (что для RuntimeError
            // ложно — она в skip-list'е). На практике для prelude/errors.nv
            // мы контролируем content, mismatch здесь — bug.
            return;
        }

        self.register_schema(SumSchemaEntry {
            nova_name: nova_name.to_string(),
            c_name: baseline.c_name.clone(),
            variants: baseline.variants.clone(),
            abi: baseline.abi,
            source: SchemaSource::DeclaredFromPrelude,
            origin_module: Some(origin_module.to_vec()),
            method_routing: baseline.method_routing.clone(),
        });
    }

    /// **Plan 62.C helper**: регистрирует Prelude entry для sum-type без
    /// HardcodedBaseline (e.g. `ReadBufferError`). Парсит variant info
    /// напрямую из AST через minimal type-ref→C-type mapper (только
    /// primitives `int` → `nova_int`, `str` → `nova_str`, `bool` → `nova_bool`).
    ///
    /// Variants поддерживают unit / tuple-positional / record-fields формы.
    /// Record-variant field order — из AST insertion order (как в `emit_sum_type`).
    /// `method_routing` — empty HashMap (нет methods).
    ///
    /// **Bootstrap-ограничение**: complex generics в variant полях
    /// (e.g. `Wrapped(Option[T])`) НЕ поддерживаются — попытка вернуть
    /// fallback `"nova_int"` (как `type_ref_to_c` в emit_c.rs). Для
    /// prelude/errors.nv это OK — variants используют только `int` и `str`.
    fn register_prelude_sum_from_decl(
        &mut self,
        nova_name: &str,
        decl_variants: &[crate::ast::SumVariant],
        abi: SumAbi,
        origin_module: &[String],
    ) {
        use crate::ast::{SumVariantKind, TypeRef};

        // Minimal primitive type-ref → C-type mapper. Достаточно для
        // prelude/errors.nv variants (int / str). Сложные generics —
        // fallback на "nova_int" (не должны встречаться в prelude/errors.nv).
        fn type_ref_to_c_minimal(ty: &TypeRef) -> String {
            match ty {
                TypeRef::Named { path, .. } if path.len() == 1 => {
                    match path[0].as_str() {
                        "int" | "i64" | "u64" | "uint" | "size" => "nova_int".to_string(),
                        "i32" | "u32" => "nova_int".to_string(),
                        "i16" | "u16" | "i8" | "u8" => "nova_int".to_string(),
                        "f64" | "f32" => "nova_f64".to_string(),
                        "bool" => "nova_bool".to_string(),
                        "str" => "nova_str".to_string(),
                        "char" => "nova_char".to_string(), // Plan 70.3: char distinct from int
                        // Plan 70 Cat D: unknown named type in prelude/errors.nv schema registration.
                        // This path should not be reached (errors.nv uses only int/str/bool/char).
                        // Fallback to nova_int for ABI-compat baseline schema only; not codegen output.
                        _ => "nova_int".to_string(),
                    }
                }
                TypeRef::Unit(_) => "nova_unit".to_string(),
                // Plan 70 Cat D: non-primitive TypeRef (generic, func, protocol) in prelude schema
                // registration. Should not appear in prelude/errors.nv variants. Defensive fallback.
                _ => "nova_int".to_string(),
            }
        }

        let variants: Vec<VariantInfo> = decl_variants.iter().map(|v| {
            let field_c_types = match &v.kind {
                SumVariantKind::Unit => Vec::new(),
                SumVariantKind::Tuple(tys) => {
                    tys.iter().map(type_ref_to_c_minimal).collect()
                }
                SumVariantKind::Record(fields) => {
                    fields.iter().map(|f| type_ref_to_c_minimal(&f.ty)).collect()
                }
            };
            VariantInfo {
                variant_name: v.name.clone(),
                field_c_types,
            }
        }).collect();

        let c_name = format!("Nova_{}", nova_name);

        self.register_schema(SumSchemaEntry {
            nova_name: nova_name.to_string(),
            c_name,
            variants,
            abi,
            source: SchemaSource::DeclaredFromPrelude,
            origin_module: Some(origin_module.to_vec()),
            method_routing: HashMap::new(),
        });
    }

    /// Legacy stub (compat shim для существующего callsite в emit_module).
    /// Передаёт control в `init_prelude_decls_from_items` если у caller'а
    /// есть items; иначе — no-op (preserve Ф.1 stub semantics).
    ///
    /// `_module_name` — historical placeholder; новая логика триггерится
    /// через items, не через имя модуля. Оставлен в API чтобы не сломать
    /// existing call-sites; будет удалён в Plan 62.A.bis cleanup.
    pub fn init_prelude_decls(&mut self, _module_name: &str) {
        // STUB: real work happens в init_prelude_decls_from_items.
        // Этот wrapper оставлен для backward-compat с emit_module'ным
        // вызовом (который скоро переедет на _from_items вариант).
    }

    // ───────────────────────────────────────────────────────────────────
    // Phase 2 helpers
    // ───────────────────────────────────────────────────────────────────

    /// Phase 2.1 helper: mirror legacy `sum_schemas.insert(name, schema)` в
    /// registry как `DeclaredFromUser` entry. Используется на trois call-site'ах
    /// в `emit_c.rs` (emit_type_decl / generic mono / emit_sum_type) чтобы
    /// registry содержал тот же набор entries что legacy `sum_schemas`.
    ///
    /// `name` — sum-type C-key (e.g. "MyEnum", "Slot____nova_str__nova_int").
    /// `legacy_schema` — variant_name → ordered Vec field C-types.
    /// `c_name` — canonical C-type name (без `*`), e.g. "Nova_MyEnum".
    /// `abi` — typically `PointerErrorLike` для user sums (heap-alloc).
    /// `variant_order` — ordered variant names (legacy HashMap теряет порядок,
    /// caller передаёт оригинальный AST order).
    ///
    /// Идемпотент: повторный register с тем же `(name, source)` overwrite'ит.
    pub fn register_user_sum(
        &mut self,
        name: &str,
        legacy_schema: &HashMap<String, Vec<String>>,
        c_name: &str,
        abi: SumAbi,
        variant_order: &[String],
    ) {
        // Build VariantInfo Vec preserving order. Если variant_order пустой
        // (caller не имеет порядка) — fall back на iteration HashMap (deterministic
        // for tests но user-visible order undefined).
        let variants: Vec<VariantInfo> = if !variant_order.is_empty() {
            variant_order.iter()
                .filter_map(|vname| {
                    legacy_schema.get(vname).map(|fields| VariantInfo {
                        variant_name: vname.clone(),
                        field_c_types: fields.clone(),
                    })
                })
                .collect()
        } else {
            legacy_schema.iter()
                .map(|(vname, fields)| VariantInfo {
                    variant_name: vname.clone(),
                    field_c_types: fields.clone(),
                })
                .collect()
        };

        self.register_schema(SumSchemaEntry {
            nova_name: name.to_string(),
            c_name: c_name.to_string(),
            variants,
            abi,
            source: SchemaSource::DeclaredFromUser,
            origin_module: None,
            method_routing: HashMap::new(),
        });
    }

    /// Phase 2.1 compat shim: drop-in замена legacy `CEmitter::find_variant`.
    /// Возвращает `(sum_name, field_c_types)` — тот же tuple что legacy.
    /// Internally дёргает `find_variant_v2` и discards `SchemaSource`.
    ///
    /// **Semantic equivalence guarantee:** для каждого `variant_name`,
    /// присутствующего в legacy `sum_schemas` HashMap, registry должен
    /// содержать matching entry (после `init_hardcoded_baseline` +
    /// `register_user_sum` вызовов на каждом legacy insert-сайте). Если
    /// registry возвращает None но `sum_schemas.get(variant_name)` дал
    /// бы что-то — это registration gap, не lookup bug.
    pub fn find_variant_compat(&self, variant_name: &str)
        -> Option<(String, Vec<String>)>
    {
        self.find_variant_v2(variant_name)
            .map(|(sum_name, fields, _src)| (sum_name, fields))
    }

    // ───────────────────────────────────────────────────────────────────
    // Phase 3 helpers — method routing dispatch
    // ───────────────────────────────────────────────────────────────────

    /// Phase 3 lookup: возвращает `MethodRouting` для `method_name` на
    /// sum-type'е `nova_name`. Returns None если sum-type не зарегистрирован
    /// или метод отсутствует в `method_routing` map'е.
    ///
    /// Используется в `emit_c.rs` method-dispatch path для замены hardcoded
    /// if-chain'ов на declaration-driven dispatch. Highest-priority entry
    /// (Prelude > User > Hardcoded) выигрывает — это позволяет Plan 62.A
    /// follow-up'у переопределить routing через prelude declaration без
    /// changes codegen-логики.
    pub fn lookup_method_routing(&self, nova_name: &str, method_name: &str)
        -> Option<&MethodRouting>
    {
        self.lookup_sum_schema(nova_name)?
            .method_routing
            .get(method_name)
    }

    /// Phase 3 reverse-lookup: преобразует C-type name (e.g. `"NovaOpt_nova_int"`,
    /// `"Nova_Result*"`) в Nova-level sum-type name (`"Option"`, `"Result"`).
    ///
    /// Strategy: iterate entries и проверяет три pattern'а matching:
    /// 1. **Template-like value sums** (`SumAbi::ValueOptionLike`): entry'шный
    ///    `c_name` это canonical baseline mono (e.g. `"NovaOpt_nova_int"`).
    ///    Реальный `obj_ty` может быть любой mono — мы match'им по prefix
    ///    `"NovaOpt_"` (выводим из baseline c_name'а через `strip_after_underscore`).
    /// 2. **Pointer sums** (`SumAbi::PointerErrorLike`): entry'шный `c_name`
    ///    это base type без `*` (e.g. `"Nova_Result"`). Реальный `obj_ty`
    ///    добавляет `*` суффикс — match по `obj_ty == format!("{}*", c_name)`
    ///    или trim'аем `*` и сравниваем.
    /// 3. **Inline value-tag-payload sums** (`SumAbi::ValueTagPayload`): exact
    ///    match `obj_ty == c_name`.
    ///
    /// Multiple entries для одного nova_name (e.g. alias `NovaOpt_nova_int`
    /// + canonical `Option`) — winner выбирается по precedence:
    /// matches на entries с distinct nova_name'ами — `Option` matches раньше
    /// `NovaOpt_nova_int` если оба entry в registry'е имеют тот же c_name
    /// (потому что мы iter'им; первый match — Option в insertion order).
    /// Для определённости выбираем entry с **shorter nova_name** (matches
    /// existing `find_variant` heuristic). Это даёт `"Option"` а не алиас.
    pub fn lookup_sum_for_c_type(&self, c_ty: &str) -> Option<&SumSchemaEntry> {
        let c_ty_trimmed = c_ty.trim_end_matches('*').trim();
        let mut best: Option<&SumSchemaEntry> = None;
        for entry in &self.entries {
            let matches = match entry.abi {
                SumAbi::ValueOptionLike => {
                    // Template prefix matching: NovaOpt_<T>. Берём prefix до
                    // первого '_' включая, e.g. "NovaOpt_". Если entry c_name
                    // == "NovaOpt_nova_int", prefix будет "NovaOpt_".
                    if let Some(idx) = entry.c_name.find('_') {
                        let prefix = &entry.c_name[..=idx];
                        c_ty.starts_with(prefix) || c_ty_trimmed.starts_with(prefix)
                    } else {
                        false
                    }
                }
                SumAbi::PointerErrorLike => {
                    // Pointer sums: obj_ty имеет `*` суффикс.
                    c_ty_trimmed == entry.c_name
                }
                SumAbi::ValueTagPayload => {
                    // Inline value struct: exact match.
                    c_ty == entry.c_name || c_ty_trimmed == entry.c_name
                }
            };
            if matches {
                // Prefer shorter nova_name (matches existing find_variant
                // heuristic: "Option" preferred over "NovaOpt_nova_int" alias).
                match best {
                    None => best = Some(entry),
                    Some(cur) => {
                        if entry.nova_name.len() < cur.nova_name.len() {
                            best = Some(entry);
                        }
                    }
                }
            }
        }
        best
    }
}

// ──────────────────────────────────────────────────────────────────────────
// Unit tests
// ──────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    /// Bootstrap: registry after `init_hardcoded_baseline()` должен иметь
    /// ровно 4 entries (Option + NovaOpt_nova_int alias + Result +
    /// RuntimeError). Error НЕ регистрируется (это record).
    #[test]
    fn test_init_hardcoded_baseline_creates_4_entries() {
        let mut reg = SumSchemaRegistry::new();
        assert!(reg.is_empty(), "fresh registry must be empty");
        reg.init_hardcoded_baseline();
        assert_eq!(reg.len(), 4,
            "expected 4 entries (Option, NovaOpt_nova_int alias, Result, RuntimeError); got {}",
            reg.len());

        let names: Vec<&str> = reg.iter_entries()
            .map(|e| e.nova_name.as_str())
            .collect();
        assert!(names.contains(&"Option"), "Option missing");
        assert!(names.contains(&"NovaOpt_nova_int"), "NovaOpt_nova_int alias missing");
        assert!(names.contains(&"Result"), "Result missing");
        assert!(names.contains(&"RuntimeError"), "RuntimeError missing");
        // Error explicitly NOT registered (records out of scope).
        assert!(!names.contains(&"Error"), "Error should not be registered (record, not sum)");
    }

    /// Lookup: `lookup_sum_schema("Option")` возвращает Option entry
    /// с правильными variants (Some/None) и ABI (ValueOptionLike).
    #[test]
    fn test_lookup_sum_schema_returns_hardcoded_for_option() {
        let mut reg = SumSchemaRegistry::new();
        reg.init_hardcoded_baseline();

        let entry = reg.lookup_sum_schema("Option")
            .expect("Option entry must exist after baseline init");
        assert_eq!(entry.nova_name, "Option");
        assert_eq!(entry.c_name, "NovaOpt_nova_int");
        assert_eq!(entry.abi, SumAbi::ValueOptionLike);
        assert_eq!(entry.source, SchemaSource::HardcodedBaseline);
        assert!(entry.origin_module.is_none());

        // Variant set check: Some + None.
        let variant_names: Vec<&str> = entry.variants.iter()
            .map(|v| v.variant_name.as_str())
            .collect();
        assert_eq!(variant_names, vec!["Some", "None"]);

        // Some(nova_int) — single-field tuple-variant.
        let some_v = entry.variants.iter().find(|v| v.variant_name == "Some").unwrap();
        assert_eq!(some_v.field_c_types, vec!["nova_int".to_string()]);

        // None — unit-variant, empty fields.
        let none_v = entry.variants.iter().find(|v| v.variant_name == "None").unwrap();
        assert!(none_v.field_c_types.is_empty());

        // Method routing для unwrap_or: HardcodedRuntimeFn с per-T mangling.
        let unwrap_or_routing = entry.method_routing.get("unwrap_or")
            .expect("Option.unwrap_or must be routed");
        match unwrap_or_routing {
            MethodRouting::HardcodedRuntimeFn { c_name, is_per_t } => {
                assert_eq!(c_name, "Nova_Option_method_unwrap_or");
                assert!(*is_per_t, "Option methods must be per-T mangled");
            }
            other => panic!("expected HardcodedRuntimeFn for Option.unwrap_or, got {:?}", other),
        }
    }

    /// Variant resolution: `find_variant_v2("Some")` должен резолвиться
    /// в Option (shorter name preferred при tie на source level).
    #[test]
    fn test_variant_to_sum_resolves_some_to_option() {
        let mut reg = SumSchemaRegistry::new();
        reg.init_hardcoded_baseline();

        let (sum_name, fields, source) = reg.find_variant_v2("Some")
            .expect("Some variant must resolve");
        assert_eq!(sum_name, "Option",
            "Some should resolve to Option (shorter than NovaOpt_nova_int)");
        assert_eq!(fields, vec!["nova_int".to_string()]);
        assert_eq!(source, SchemaSource::HardcodedBaseline);

        // Ok → Result.
        let (sum_name, fields, source) = reg.find_variant_v2("Ok")
            .expect("Ok variant must resolve");
        assert_eq!(sum_name, "Result");
        assert_eq!(fields, vec!["nova_int".to_string()]);
        assert_eq!(source, SchemaSource::HardcodedBaseline);

        // Err → Result, with nova_str payload.
        let (sum_name, fields, source) = reg.find_variant_v2("Err")
            .expect("Err variant must resolve");
        assert_eq!(sum_name, "Result");
        assert_eq!(fields, vec!["nova_str".to_string()]);
        assert_eq!(source, SchemaSource::HardcodedBaseline);

        // DivByZero → RuntimeError, unit variant.
        let (sum_name, fields, source) = reg.find_variant_v2("DivByZero")
            .expect("DivByZero variant must resolve");
        assert_eq!(sum_name, "RuntimeError");
        assert!(fields.is_empty());
        assert_eq!(source, SchemaSource::HardcodedBaseline);

        // IndexOutOfBounds → RuntimeError, 2-field positional.
        let (sum_name, fields, source) = reg.find_variant_v2("IndexOutOfBounds")
            .expect("IndexOutOfBounds variant must resolve");
        assert_eq!(sum_name, "RuntimeError");
        assert_eq!(fields, vec!["nova_int".to_string(), "nova_int".to_string()]);
        assert_eq!(source, SchemaSource::HardcodedBaseline);

        // Unknown variant: None resolution.
        assert!(reg.find_variant_v2("NotAVariant").is_none());
    }

    /// Priority: DeclaredFromPrelude entry trumps HardcodedBaseline.
    /// `lookup_sum_schema` возвращает highest-priority entry; layered
    /// возвращает обе entries DESC by source.
    #[test]
    fn test_register_schema_priority_prelude_over_user() {
        let mut reg = SumSchemaRegistry::new();
        reg.init_hardcoded_baseline();

        // Register a DeclaredFromPrelude Option with same variants but
        // distinct c_name to verify lookup picks it.
        let prelude_option = SumSchemaEntry {
            nova_name: "Option".to_string(),
            c_name: "Nova_Option_prelude".to_string(),  // distinct
            variants: vec![
                VariantInfo { variant_name: "Some".to_string(), field_c_types: vec!["nova_int".to_string()] },
                VariantInfo { variant_name: "None".to_string(), field_c_types: vec![] },
            ],
            abi: SumAbi::ValueOptionLike,
            source: SchemaSource::DeclaredFromPrelude,
            origin_module: Some(vec!["std".into(), "prelude".into(), "core".into()]),
            method_routing: HashMap::new(),
        };
        reg.register_schema(prelude_option);

        // Primary lookup: должен быть Prelude entry.
        let entry = reg.lookup_sum_schema("Option").unwrap();
        assert_eq!(entry.source, SchemaSource::DeclaredFromPrelude);
        assert_eq!(entry.c_name, "Nova_Option_prelude");
        assert!(entry.origin_module.is_some());

        // Register user shadowing — should be mid-priority (between
        // baseline and prelude).
        let user_option = SumSchemaEntry {
            nova_name: "Option".to_string(),
            c_name: "Nova_Option_user".to_string(),
            variants: vec![
                VariantInfo { variant_name: "Some".to_string(), field_c_types: vec!["nova_int".to_string()] },
                VariantInfo { variant_name: "None".to_string(), field_c_types: vec![] },
            ],
            abi: SumAbi::ValueOptionLike,
            source: SchemaSource::DeclaredFromUser,
            origin_module: None,
            method_routing: HashMap::new(),
        };
        reg.register_schema(user_option);

        // Primary lookup STILL Prelude (highest priority).
        let entry = reg.lookup_sum_schema("Option").unwrap();
        assert_eq!(entry.source, SchemaSource::DeclaredFromPrelude);

        // Idempotent re-register с тем же source — overwrite.
        let prelude_v2 = SumSchemaEntry {
            nova_name: "Option".to_string(),
            c_name: "Nova_Option_prelude_v2".to_string(),
            variants: vec![
                VariantInfo { variant_name: "Some".to_string(), field_c_types: vec!["nova_int".to_string()] },
                VariantInfo { variant_name: "None".to_string(), field_c_types: vec![] },
            ],
            abi: SumAbi::ValueOptionLike,
            source: SchemaSource::DeclaredFromPrelude,
            origin_module: Some(vec!["std".into(), "prelude".into(), "core".into()]),
            method_routing: HashMap::new(),
        };
        reg.register_schema(prelude_v2);

        let entry = reg.lookup_sum_schema("Option").unwrap();
        assert_eq!(entry.c_name, "Nova_Option_prelude_v2",
            "re-registration с тем же (nova_name, source) должна overwrite");
    }

    /// Layered lookup: returns all entries для name, sorted DESC by source.
    #[test]
    fn test_lookup_sum_schema_layered_returns_all_sources() {
        let mut reg = SumSchemaRegistry::new();
        reg.init_hardcoded_baseline();

        // Add Prelude и User entries for "Option".
        let prelude_option = SumSchemaEntry {
            nova_name: "Option".to_string(),
            c_name: "Nova_Option_prelude".to_string(),
            variants: vec![
                VariantInfo { variant_name: "Some".to_string(), field_c_types: vec!["nova_int".to_string()] },
                VariantInfo { variant_name: "None".to_string(), field_c_types: vec![] },
            ],
            abi: SumAbi::ValueOptionLike,
            source: SchemaSource::DeclaredFromPrelude,
            origin_module: Some(vec!["std".into(), "prelude".into(), "core".into()]),
            method_routing: HashMap::new(),
        };
        reg.register_schema(prelude_option);

        let user_option = SumSchemaEntry {
            nova_name: "Option".to_string(),
            c_name: "Nova_Option_user".to_string(),
            variants: vec![
                VariantInfo { variant_name: "Some".to_string(), field_c_types: vec!["nova_int".to_string()] },
                VariantInfo { variant_name: "None".to_string(), field_c_types: vec![] },
            ],
            abi: SumAbi::ValueOptionLike,
            source: SchemaSource::DeclaredFromUser,
            origin_module: None,
            method_routing: HashMap::new(),
        };
        reg.register_schema(user_option);

        let entries = reg.lookup_sum_schema_layered("Option");
        assert_eq!(entries.len(), 3,
            "expected 3 layered entries (Prelude + User + Baseline); got {}", entries.len());

        // Sorted DESC by source: Prelude (2) → User (1) → Baseline (0).
        assert_eq!(entries[0].source, SchemaSource::DeclaredFromPrelude);
        assert_eq!(entries[1].source, SchemaSource::DeclaredFromUser);
        assert_eq!(entries[2].source, SchemaSource::HardcodedBaseline);

        // RuntimeError still has 1 entry.
        let rt_entries = reg.lookup_sum_schema_layered("RuntimeError");
        assert_eq!(rt_entries.len(), 1);
        assert_eq!(rt_entries[0].source, SchemaSource::HardcodedBaseline);

        // Unknown name — empty Vec.
        let none = reg.lookup_sum_schema_layered("NoSuchSum");
        assert!(none.is_empty());
    }

    /// Variant priority sort: при collision (DeclaredFromPrelude
    /// registers variant что и HardcodedBaseline), `find_variant_v2`
    /// предпочитает higher priority source.
    #[test]
    fn test_find_variant_v2_priority_sort() {
        let mut reg = SumSchemaRegistry::new();
        reg.init_hardcoded_baseline();

        // Register a Prelude Option (shadows Hardcoded baseline Option).
        // `Some` теперь имеет 2 candidates: Option (baseline) +
        // Option (prelude) + NovaOpt_nova_int (baseline alias). Highest
        // priority candidate — Prelude.
        let prelude_option = SumSchemaEntry {
            nova_name: "Option".to_string(),
            c_name: "Nova_Option_prelude".to_string(),
            variants: vec![
                VariantInfo {
                    variant_name: "Some".to_string(),
                    field_c_types: vec!["nova_int".to_string()],
                },
                VariantInfo {
                    variant_name: "None".to_string(),
                    field_c_types: vec![],
                },
            ],
            abi: SumAbi::ValueOptionLike,
            source: SchemaSource::DeclaredFromPrelude,
            origin_module: Some(vec!["std".into(), "prelude".into(), "core".into()]),
            method_routing: HashMap::new(),
        };
        reg.register_schema(prelude_option);

        let (sum_name, _, source) = reg.find_variant_v2("Some")
            .expect("Some must resolve");
        // Must pick Prelude over Baseline.
        assert_eq!(source, SchemaSource::DeclaredFromPrelude,
            "find_variant_v2 must prefer DeclaredFromPrelude over HardcodedBaseline");
        assert_eq!(sum_name, "Option");
    }

    /// Phase 3: `lookup_method_routing` возвращает HardcodedRuntimeFn для
    /// существующих методов Option/Result и None для несуществующих.
    #[test]
    fn test_lookup_method_routing_option_and_result() {
        let mut reg = SumSchemaRegistry::new();
        reg.init_hardcoded_baseline();

        // Plan 95 Ф.4.2: is_some УДАЛЕНО из baseline — перенесено на
        // Nova-body. Без prelude scan'а baseline-only lookup → None
        // (DeclaredBody-entry регистрируется через `init_prelude_decls_
        // from_items`, не здесь). Verify Option.unwrap_or как пример
        // оставшегося per-T trampoline.
        assert!(reg.lookup_method_routing("Option", "is_some").is_none(),
            "is_some убран из baseline — регистрируется DeclaredBody через prelude scan");
        let r = reg.lookup_method_routing("Option", "unwrap_or")
            .expect("Option.unwrap_or must be routed");
        match r {
            MethodRouting::HardcodedRuntimeFn { c_name, is_per_t } => {
                assert_eq!(c_name, "Nova_Option_method_unwrap_or");
                assert!(*is_per_t);
            }
            other => panic!("expected HardcodedRuntimeFn, got {:?}", other),
        }

        // Option.unwrap → inline sentinel.
        let r = reg.lookup_method_routing("Option", "unwrap").unwrap();
        match r {
            MethodRouting::HardcodedRuntimeFn { c_name, is_per_t } => {
                assert_eq!(c_name, "<inline>");
                assert!(!*is_per_t);
            }
            other => panic!("expected HardcodedRuntimeFn<inline>, got {:?}", other),
        }

        // Result.unwrap_or — non-per-T trampoline.
        let r = reg.lookup_method_routing("Result", "unwrap_or").unwrap();
        match r {
            MethodRouting::HardcodedRuntimeFn { c_name, is_per_t } => {
                assert_eq!(c_name, "Nova_Result_method_unwrap_or");
                assert!(!*is_per_t);
            }
            other => panic!("expected HardcodedRuntimeFn non-per-T, got {:?}", other),
        }

        // Result.unknown_method → None.
        assert!(reg.lookup_method_routing("Result", "no_such_method").is_none());

        // Unknown sum.
        assert!(reg.lookup_method_routing("NoSuchSum", "any").is_none());
    }

    /// Phase 3: `lookup_sum_for_c_type` reverse-maps C-type → entry.
    #[test]
    fn test_lookup_sum_for_c_type_reverse_mapping() {
        let mut reg = SumSchemaRegistry::new();
        reg.init_hardcoded_baseline();

        // NovaOpt_<T> template — должен резолвиться в Option (shorter name
        // preferred над alias NovaOpt_nova_int).
        let e = reg.lookup_sum_for_c_type("NovaOpt_nova_int")
            .expect("NovaOpt_nova_int must resolve");
        assert_eq!(e.nova_name, "Option");

        // NovaOpt_nova_str (other mono) — также Option.
        let e = reg.lookup_sum_for_c_type("NovaOpt_nova_str")
            .expect("NovaOpt_nova_str must resolve");
        assert_eq!(e.nova_name, "Option");

        // Nova_Result* → Result.
        let e = reg.lookup_sum_for_c_type("Nova_Result*")
            .expect("Nova_Result* must resolve");
        assert_eq!(e.nova_name, "Result");
        assert_eq!(e.abi, SumAbi::PointerErrorLike);

        // Without star.
        let e = reg.lookup_sum_for_c_type("Nova_Result")
            .expect("Nova_Result must resolve (no star)");
        assert_eq!(e.nova_name, "Result");

        // Nova_RuntimeError — value-tag-payload.
        let e = reg.lookup_sum_for_c_type("Nova_RuntimeError")
            .expect("Nova_RuntimeError must resolve");
        assert_eq!(e.nova_name, "RuntimeError");
        assert_eq!(e.abi, SumAbi::ValueTagPayload);

        // Unknown.
        assert!(reg.lookup_sum_for_c_type("NoSuchType").is_none());
        assert!(reg.lookup_sum_for_c_type("NovaArray_nova_int*").is_none());
    }

    /// init_prelude_decls — Ф.1 stub. Не регистрирует entries, не падает.
    /// Phase 2 будет полностью populated; в Ф.1 хотим только убедиться
    /// что wiring безопасный. **Plan 62.A follow-up (2026-05-18):**
    /// `init_prelude_decls(&str)` остаётся wrapper-stub'ом — real work
    /// moved в `init_prelude_decls_from_items`.
    #[test]
    fn test_init_prelude_decls_stub_is_safe() {
        let mut reg = SumSchemaRegistry::new();
        reg.init_hardcoded_baseline();
        let baseline_len = reg.len();
        reg.init_prelude_decls("std.prelude.core");
        assert_eq!(reg.len(), baseline_len,
            "init_prelude_decls (Ф.1 stub) must not change entry count");
        // Random module name — также no-op.
        reg.init_prelude_decls("user.code");
        assert_eq!(reg.len(), baseline_len);
    }

    /// Plan 62.A follow-up: empty items slice — no-op (нет prelude decls →
    /// нет Prelude entries регистрируется).
    #[test]
    fn test_init_prelude_decls_from_items_empty_is_noop() {
        let mut reg = SumSchemaRegistry::new();
        reg.init_hardcoded_baseline();
        let baseline_len = reg.len();
        reg.init_prelude_decls_from_items(&[]);
        assert_eq!(reg.len(), baseline_len,
            "пустой items list не должен добавлять entries");
    }

    /// Plan 62.A follow-up: single Option.is_some declaration → Prelude
    /// entry для Option, inheriting method_routing от HardcodedBaseline.
    #[test]
    fn test_init_prelude_decls_from_items_option_inherits_routing() {
        use crate::ast::{FnDecl, Item, Receiver, ReceiverKind};
        use crate::diag::Span;

        let mut reg = SumSchemaRegistry::new();
        reg.init_hardcoded_baseline();

        // Plan 123 baseline-fix (2026-06-02): Default::default() spread.
        // Robust to future AST field additions — set only what matters.
        let opt_is_some = FnDecl {
            is_external: true,
            name: "is_some".to_string(),
            receiver: Some(Receiver {
                type_name: "Option".to_string(),
                generics: vec![],
                kind: ReceiverKind::Instance,
                mutable: false,
                consume: false,
                span: Span::default(),
            }),
            ..Default::default()
        };
        let items = vec![Item::Fn(opt_is_some)];

        reg.init_prelude_decls_from_items(&items);

        // Lookup должен теперь возвращать Prelude entry (higher priority).
        let entry = reg.lookup_sum_schema("Option")
            .expect("Option must resolve");
        assert_eq!(entry.source, SchemaSource::DeclaredFromPrelude,
            "Prelude entry должен выигрывать lookup precedence");
        assert!(entry.origin_module.is_some(),
            "Prelude entry должен иметь origin_module = Some(std.prelude.core)");

        // **Critical**: method_routing inherited от Hardcoded — Option.unwrap_or
        // (которая NOT в declared items, но В Hardcoded routing) всё ещё
        // findable через registry.
        let routing = reg.lookup_method_routing("Option", "unwrap_or")
            .expect("Option.unwrap_or должен оставаться routable после Prelude scan'а");
        match routing {
            MethodRouting::HardcodedRuntimeFn { c_name, is_per_t } => {
                assert_eq!(c_name, "Nova_Option_method_unwrap_or",
                    "routing must inherit от HardcodedBaseline");
                assert!(*is_per_t,
                    "is_per_t flag должен сохраниться");
            }
            other => panic!("expected HardcodedRuntimeFn, got {:?}", other),
        }

        // Plan 95 Ф.4.2: `is_some` УДАЛЁН из baseline (перенесён на
        // Nova-body, регистрируется через DeclaredBody). Для external
        // декларации без baseline-entry routing не регистрируется
        // (inheritance-conservative — см. комментарий в
        // init_prelude_decls_from_items). lookup → None.
        let routing = reg.lookup_method_routing("Option", "is_some");
        assert!(routing.is_none(),
            "is_some убран из baseline (Plan 95) — external декларация без baseline-entry не регистрирует routing");
    }

    /// Plan 62.A follow-up: Result method declaration → Prelude Result
    /// entry. Verifies routing inherited (non-per-T for Result).
    #[test]
    fn test_init_prelude_decls_from_items_result_inherits_routing() {
        use crate::ast::{FnDecl, Item, Receiver, ReceiverKind};
        use crate::diag::Span;

        let mut reg = SumSchemaRegistry::new();
        reg.init_hardcoded_baseline();

        let res_is_ok = FnDecl {
            is_external: true,
            name: "is_ok".to_string(),
            receiver: Some(Receiver {
                type_name: "Result".to_string(),
                generics: vec![],
                kind: ReceiverKind::Instance,
                mutable: false,
                consume: false,
                span: Span::default(),
            }),
            ..Default::default()
        };
        let items = vec![Item::Fn(res_is_ok)];

        reg.init_prelude_decls_from_items(&items);

        let entry = reg.lookup_sum_schema("Result")
            .expect("Result must resolve");
        assert_eq!(entry.source, SchemaSource::DeclaredFromPrelude);

        // Routing для Result.unwrap_or — non-per-T (`is_per_t = false`).
        let routing = reg.lookup_method_routing("Result", "unwrap_or").unwrap();
        match routing {
            MethodRouting::HardcodedRuntimeFn { c_name, is_per_t } => {
                assert_eq!(c_name, "Nova_Result_method_unwrap_or");
                assert!(!*is_per_t, "Result methods — non-per-T в bootstrap'е");
            }
            other => panic!("expected HardcodedRuntimeFn non-per-T, got {:?}", other),
        }
    }

    /// **Plan 95 Ф.1.2**: Nova-body метод на `Option`/`Result` (с `body !=
    /// FnBody::External`, `is_external == false`) → routing должен быть
    /// **`DeclaredBody`** (override baseline `HardcodedRuntimeFn`). Не
    /// перенесённые методы того же типа продолжают возвращать унаследованный
    /// `HardcodedRuntimeFn` (behavior-preserving).
    #[test]
    fn test_init_prelude_decls_from_items_nova_body_overrides_to_declared_body() {
        use crate::ast::{FnDecl, FnBody, Item, Receiver, ReceiverKind, Expr, ExprKind};
        use crate::diag::Span;

        let mk_fn = |name: &str, recv_type: &str, external: bool| FnDecl {
            is_external: external,
            name: name.to_string(),
            receiver: Some(Receiver {
                type_name: recv_type.to_string(),
                generics: vec![],
                kind: ReceiverKind::Instance,
                mutable: false, consume: false,
                span: Span::default(),
            }),
            // Stub Nova-body для теста — `=> false` (контент не важен,
            // важно только `is_external == false` и `body != External`).
            body: if external { FnBody::External } else {
                FnBody::Expr(Expr::new(
                    ExprKind::BoolLit(false), Span::default()
                ))
            },
            ..Default::default()
        };

        let mut reg = SumSchemaRegistry::new();
        reg.init_hardcoded_baseline();

        // Mix: Option.is_some — Nova-body (мигрированный), Option.unwrap —
        // external (остаётся C-routed).
        let items = vec![
            Item::Fn(mk_fn("is_some", "Option", false)),
            Item::Fn(mk_fn("unwrap", "Option", true)),
        ];

        reg.init_prelude_decls_from_items(&items);

        // Sanity: Prelude entry зарегистрирован.
        let entry = reg.lookup_sum_schema("Option").expect("Option resolves");
        assert_eq!(entry.source, SchemaSource::DeclaredFromPrelude);

        // is_some — **override на DeclaredBody**.
        let routing = reg.lookup_method_routing("Option", "is_some")
            .expect("is_some routing must exist");
        match routing {
            MethodRouting::DeclaredBody { has_nova_body } => {
                assert!(*has_nova_body, "Nova-body метод → has_nova_body = true");
            }
            other => panic!(
                "expected DeclaredBody for migrated is_some, got {:?}", other),
        }

        // unwrap — НЕ переопределён, остаётся HardcodedRuntimeFn (наследован).
        // Baseline routing для unwrap — `<inline>` sentinel (Fail-dispatch),
        // важно что это **не** DeclaredBody — Plan 95 не трогает unwrap.
        let routing = reg.lookup_method_routing("Option", "unwrap")
            .expect("unwrap routing must exist");
        assert!(matches!(routing, MethodRouting::HardcodedRuntimeFn { .. }),
            "external method `unwrap` остаётся C-routed (не DeclaredBody), got {:?}",
            routing);

        // unwrap_or — вообще не declared в items, но в Hardcoded routing
        // map'е — findable через inheritance (нерегрессионный sanity).
        let routing = reg.lookup_method_routing("Option", "unwrap_or")
            .expect("unwrap_or routing inherited from baseline");
        assert!(matches!(routing, MethodRouting::HardcodedRuntimeFn { .. }));
    }

    /// **Plan 95 Ф.1.2 (Result)**: Nova-body `is_ok` на `Result` → `DeclaredBody`.
    #[test]
    fn test_init_prelude_decls_from_items_result_nova_body_overrides() {
        use crate::ast::{FnDecl, FnBody, Item, Receiver, ReceiverKind, Expr, ExprKind};
        use crate::diag::Span;

        let res_is_ok = FnDecl {
            name: "is_ok".to_string(),
            receiver: Some(Receiver {
                type_name: "Result".to_string(),
                generics: vec![],
                kind: ReceiverKind::Instance,
                mutable: false, consume: false,
                span: Span::default(),
            }),
            body: FnBody::Expr(Expr::new(
                ExprKind::BoolLit(false), Span::default()
            )),
            ..Default::default()
        };

        let mut reg = SumSchemaRegistry::new();
        reg.init_hardcoded_baseline();
        reg.init_prelude_decls_from_items(&vec![Item::Fn(res_is_ok)]);

        let routing = reg.lookup_method_routing("Result", "is_ok")
            .expect("is_ok routing must exist");
        match routing {
            MethodRouting::DeclaredBody { has_nova_body } => {
                assert!(*has_nova_body);
            }
            other => panic!("expected DeclaredBody for is_ok, got {:?}", other),
        }
    }

    /// **Plan 62.C**: RuntimeError sum-declaration в items → Prelude entry,
    /// наследующий variants + method_routing от HardcodedBaseline. Variant
    /// set strict-equality enforced.
    #[test]
    fn test_runtime_error_prelude_decl_inherits_baseline_variants() {
        use crate::ast::{
            Item, TypeDecl, TypeDeclKind, SumVariant, SumVariantKind, RecordField,
            TypeRef,
        };
        use crate::diag::Span;

        let mut reg = SumSchemaRegistry::new();
        reg.init_hardcoded_baseline();

        // Baseline должен быть зарегистрирован init_hardcoded_baseline'ом.
        let baseline_entry = reg.lookup_sum_schema("RuntimeError")
            .expect("RuntimeError HardcodedBaseline must exist").clone();
        assert_eq!(baseline_entry.source, SchemaSource::HardcodedBaseline);
        assert_eq!(baseline_entry.variants.len(), 6);

        // Helper для построения TypeRef::Named("int" / "str").
        let mk_named = |name: &str| TypeRef::Named {
            path: vec![name.to_string()],
            generics: vec![],
            span: Span::default(),
        };
        let mk_field = |name: &str, ty_name: &str| RecordField {
            name: name.to_string(),
            ty: mk_named(ty_name),
            ..Default::default()
        };
        let mk_variant = |name: &str, kind: SumVariantKind| SumVariant {
            name: name.to_string(),
            kind,
            discriminant: None,
            span: Span::default(),
        };

        // Конструируем prelude-declaration RuntimeError — все 6 variants,
        // matching std/prelude/errors.nv.
        let runtime_error_decl = TypeDecl {
            is_export: true,
            name: "RuntimeError".to_string(),
            kind: TypeDeclKind::Sum(vec![
                mk_variant("DivByZero", SumVariantKind::Unit),
                mk_variant("Overflow", SumVariantKind::Unit),
                mk_variant("IndexOutOfBounds", SumVariantKind::Record(vec![
                    mk_field("index", "int"),
                    mk_field("length", "int"),
                ])),
                mk_variant("TypeMismatch", SumVariantKind::Tuple(vec![mk_named("str")])),
                mk_variant("AssertFailed", SumVariantKind::Tuple(vec![mk_named("str")])),
                mk_variant("NoHandler", SumVariantKind::Tuple(vec![mk_named("str")])),
            ]),
            ..Default::default()
        };
        let items = vec![Item::Type(runtime_error_decl)];

        reg.init_prelude_decls_from_items(&items);

        // Primary lookup теперь должен вернуть Prelude entry (higher priority).
        let entry = reg.lookup_sum_schema("RuntimeError")
            .expect("RuntimeError must resolve");
        assert_eq!(entry.source, SchemaSource::DeclaredFromPrelude,
            "Prelude entry должен выигрывать lookup precedence");
        assert!(entry.origin_module.is_some(),
            "Prelude entry должен иметь origin_module = Some(std.prelude.errors)");
        assert_eq!(entry.origin_module.as_ref().unwrap(),
            &vec!["std".to_string(), "prelude".to_string(), "errors".to_string()]);

        // **Critical**: variants inherited от baseline (с правильными
        // field_c_types: positional layout).
        assert_eq!(entry.variants.len(), 6);
        let by_name: std::collections::HashMap<&str, &VariantInfo> =
            entry.variants.iter().map(|v| (v.variant_name.as_str(), v)).collect();
        assert!(by_name["DivByZero"].field_c_types.is_empty());
        assert!(by_name["Overflow"].field_c_types.is_empty());
        assert_eq!(by_name["IndexOutOfBounds"].field_c_types,
            vec!["nova_int".to_string(), "nova_int".to_string()]);
        assert_eq!(by_name["TypeMismatch"].field_c_types,
            vec!["nova_str".to_string()]);
        assert_eq!(by_name["AssertFailed"].field_c_types,
            vec!["nova_str".to_string()]);
        assert_eq!(by_name["NoHandler"].field_c_types,
            vec!["nova_str".to_string()]);

        // ABI inherited (ValueTagPayload per baseline).
        assert_eq!(entry.abi, SumAbi::ValueTagPayload);

        // c_name inherited (Nova_RuntimeError).
        assert_eq!(entry.c_name, "Nova_RuntimeError");

        // find_variant_v2 для всех variants должен вернуть Prelude source.
        for vn in &["DivByZero", "Overflow", "IndexOutOfBounds",
                    "TypeMismatch", "AssertFailed", "NoHandler"] {
            let (sum, _, src) = reg.find_variant_v2(vn)
                .unwrap_or_else(|| panic!("variant {} must resolve", vn));
            assert_eq!(sum, "RuntimeError");
            assert_eq!(src, SchemaSource::DeclaredFromPrelude,
                "variant {} must resolve через Prelude (higher priority)", vn);
        }
    }

    /// **Plan 62.C**: variant-set drift между prelude declaration и
    /// HardcodedBaseline → silent skip Prelude registration (baseline
    /// продолжает работать как fallback). Plan 62.F edition territory.
    #[test]
    fn test_runtime_error_prelude_decl_variant_drift_skipped() {
        use crate::ast::{
            Item, TypeDecl, TypeDeclKind, SumVariant, SumVariantKind,
        };
        use crate::diag::Span;

        let mut reg = SumSchemaRegistry::new();
        reg.init_hardcoded_baseline();

        let mk_variant = |name: &str| SumVariant {
            name: name.to_string(),
            kind: SumVariantKind::Unit,
            discriminant: None,
            span: Span::default(),
        };

        // Drift: declaration с extra variant "MyExtra" + missing 5 baseline'ных.
        let drifted_decl = TypeDecl {
            is_export: true,
            name: "RuntimeError".to_string(),
            kind: TypeDeclKind::Sum(vec![
                mk_variant("DivByZero"),  // matches baseline
                mk_variant("MyExtra"),    // extra — not in baseline
            ]),
            ..Default::default()
        };

        reg.init_prelude_decls_from_items(&[Item::Type(drifted_decl)]);

        // Primary lookup STILL HardcodedBaseline — Prelude registration skipped.
        let entry = reg.lookup_sum_schema("RuntimeError").unwrap();
        assert_eq!(entry.source, SchemaSource::HardcodedBaseline,
            "variant drift должен trigger silent skip; baseline остаётся primary");
        // Layered count: ровно 1 entry (baseline; no Prelude registered).
        let layered = reg.lookup_sum_schema_layered("RuntimeError");
        assert_eq!(layered.len(), 1, "Prelude entry не должен быть зарегистрирован");
    }

    /// **Plan 62.C**: ReadBufferError sum-declaration → Prelude entry с
    /// variants парсимыми из AST (HardcodedBaseline для ReadBufferError
    /// отсутствует). PointerErrorLike ABI.
    #[test]
    fn test_read_buffer_error_prelude_decl_registers_from_ast() {
        use crate::ast::{
            Item, TypeDecl, TypeDeclKind, SumVariant, SumVariantKind, RecordField,
            TypeRef,
        };
        use crate::diag::Span;

        let mut reg = SumSchemaRegistry::new();
        reg.init_hardcoded_baseline();

        // ReadBufferError НЕ должен быть в baseline.
        assert!(reg.lookup_sum_schema("ReadBufferError").is_none(),
            "ReadBufferError не должен быть в HardcodedBaseline");

        let mk_named = |name: &str| TypeRef::Named {
            path: vec![name.to_string()],
            generics: vec![],
            span: Span::default(),
        };
        let mk_field = |name: &str, ty_name: &str| RecordField {
            name: name.to_string(),
            ty: mk_named(ty_name),
            ..Default::default()
        };

        let rbe_decl = TypeDecl {
            is_export: true,
            name: "ReadBufferError".to_string(),
            kind: TypeDeclKind::Sum(vec![
                SumVariant {
                    name: "UnexpectedEnd".to_string(),
                    kind: SumVariantKind::Record(vec![
                        mk_field("wanted", "int"),
                        mk_field("available", "int"),
                    ]),
                    discriminant: None,
                    span: Span::default(),
                },
            ]),
            ..Default::default()
        };

        reg.init_prelude_decls_from_items(&[Item::Type(rbe_decl)]);

        let entry = reg.lookup_sum_schema("ReadBufferError")
            .expect("ReadBufferError должен быть зарегистрирован");
        assert_eq!(entry.source, SchemaSource::DeclaredFromPrelude);
        assert_eq!(entry.abi, SumAbi::PointerErrorLike);
        assert_eq!(entry.c_name, "Nova_ReadBufferError");
        assert_eq!(entry.origin_module.as_ref().unwrap(),
            &vec!["std".to_string(), "prelude".to_string(), "errors".to_string()]);

        // UnexpectedEnd variant с 2 int полями.
        assert_eq!(entry.variants.len(), 1);
        let v = &entry.variants[0];
        assert_eq!(v.variant_name, "UnexpectedEnd");
        assert_eq!(v.field_c_types,
            vec!["nova_int".to_string(), "nova_int".to_string()]);

        // find_variant_v2 для UnexpectedEnd должен резолвиться в ReadBufferError.
        let (sum, _, src) = reg.find_variant_v2("UnexpectedEnd")
            .expect("UnexpectedEnd должен резолвиться");
        assert_eq!(sum, "ReadBufferError");
        assert_eq!(src, SchemaSource::DeclaredFromPrelude);
    }

    /// Plan 62.A follow-up: non-Option/Result method declaration — ignored
    /// (не создаёт Prelude entry).
    #[test]
    fn test_init_prelude_decls_from_items_ignores_other_types() {
        use crate::ast::{FnDecl, Item, Receiver, ReceiverKind};
        use crate::diag::Span;

        let mut reg = SumSchemaRegistry::new();
        reg.init_hardcoded_baseline();
        let baseline_len = reg.len();

        // Method on Error (record, не sum в registry) — должен игнорироваться.
        let error_method = FnDecl {
            is_external: true,
            name: "render".to_string(),
            receiver: Some(Receiver {
                type_name: "Error".to_string(),
                generics: vec![], kind: ReceiverKind::Instance,
                mutable: false, consume: false, span: Span::default(),
            }),
            ..Default::default()
        };
        let items = vec![Item::Fn(error_method)];

        reg.init_prelude_decls_from_items(&items);
        assert_eq!(reg.len(), baseline_len,
            "method on non-Option/Result type не должен добавлять entries");
    }
}
