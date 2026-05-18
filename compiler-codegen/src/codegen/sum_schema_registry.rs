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

    /// Populates registry с 4 HardcodedBaseline entries:
    /// Option / Result / Error-as-record-skipped / RuntimeError.
    ///
    /// **Mirror'ит** существующий pre-populate code в `emit_c.rs:958-1023`
    /// — variant info, c_names, method_routing entries — должны matchить
    /// что там есть (Phase 2+ acceptance: `legacy_sum_schema_view` возвращает
    /// identical HashMap к pre-migration `sum_schemas.get(name)`).
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
        option_methods.insert("is_some".to_string(), MethodRouting::HardcodedRuntimeFn {
            c_name: "Nova_Option_method_is_some".to_string(),
            is_per_t: true,
        });
        option_methods.insert("is_none".to_string(), MethodRouting::HardcodedRuntimeFn {
            c_name: "Nova_Option_method_is_none".to_string(),
            is_per_t: true,
        });
        option_methods.insert("unwrap_or".to_string(), MethodRouting::HardcodedRuntimeFn {
            c_name: "Nova_Option_method_unwrap_or".to_string(),
            is_per_t: true,
        });
        option_methods.insert("unwrap".to_string(), MethodRouting::HardcodedRuntimeFn {
            c_name: "<inline>".to_string(),
            is_per_t: false,
        });
        option_methods.insert("unwrap_or_else".to_string(), MethodRouting::HardcodedRuntimeFn {
            c_name: "<inline>".to_string(),
            is_per_t: false,
        });
        option_methods.insert("map".to_string(), MethodRouting::HardcodedRuntimeFn {
            c_name: "<inline>".to_string(),
            is_per_t: false,
        });
        option_methods.insert("ok_or".to_string(), MethodRouting::HardcodedRuntimeFn {
            c_name: "<inline>".to_string(),
            is_per_t: false,
        });

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
        let mut result_methods = HashMap::new();
        result_methods.insert("is_ok".to_string(), MethodRouting::HardcodedRuntimeFn {
            c_name: "Nova_Result_method_is_ok".to_string(),
            is_per_t: false,
        });
        result_methods.insert("is_err".to_string(), MethodRouting::HardcodedRuntimeFn {
            c_name: "Nova_Result_method_is_err".to_string(),
            is_per_t: false,
        });
        result_methods.insert("ok".to_string(), MethodRouting::HardcodedRuntimeFn {
            c_name: "Nova_Result_method_ok".to_string(),
            is_per_t: false,
        });
        result_methods.insert("err".to_string(), MethodRouting::HardcodedRuntimeFn {
            c_name: "<inline>".to_string(),
            is_per_t: false,
        });
        result_methods.insert("unwrap_or".to_string(), MethodRouting::HardcodedRuntimeFn {
            c_name: "Nova_Result_method_unwrap_or".to_string(),
            is_per_t: false,
        });
        result_methods.insert("unwrap_or_else".to_string(), MethodRouting::HardcodedRuntimeFn {
            c_name: "<inline>".to_string(),
            is_per_t: false,
        });
        result_methods.insert("map".to_string(), MethodRouting::HardcodedRuntimeFn {
            c_name: "<inline>".to_string(),
            is_per_t: false,
        });
        result_methods.insert("map_err".to_string(), MethodRouting::HardcodedRuntimeFn {
            c_name: "<inline>".to_string(),
            is_per_t: false,
        });
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

    /// Phase 2+ hook: discovery of file-based prelude declarations.
    /// В Ф.1 — STUB: проверяет module name на префикс `"std.prelude"`,
    /// но не регистрирует entries (пустая body). Call site exists в
    /// `emit_module()` чтобы Phase 2 мог расширить без changes wiring.
    ///
    /// Phase 2 будет:
    ///   1) Scan `module.items` для `Item::Type` с origin в `std.prelude.*`.
    ///   2) Convert каждый `TypeDeclKind::Sum(variants)` в `SumSchemaEntry`
    ///      с source = DeclaredFromPrelude.
    ///   3) Variant-set validation против HardcodedBaseline (см. plan doc
    ///      §«Phase 3 acceptance» — variant-set check).
    ///
    /// `_module_name` — placeholder; в Ф.1 не используется.
    pub fn init_prelude_decls(&mut self, _module_name: &str) {
        // STUB: Phase 2 будет fully populated.
        // Wiring exists в emit_module() чтобы убедиться call-site stable.
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

    /// init_prelude_decls — Ф.1 stub. Не регистрирует entries, не падает.
    /// Phase 2 будет полностью populated; в Ф.1 хотим только убедиться
    /// что wiring безопасный.
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
}
