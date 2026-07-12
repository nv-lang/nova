//! Plan 12: builtins.nv-driven external dispatch registry.
//!
//! Hard-coded таблицы для StringBuilder/WriteBuffer/ReadBuffer/str.from(char)
//! заменяются автоматическим выводом из AST `std/runtime/builtins.nv`.
//! Single source of truth — `.nv` декларации; codegen применяет mangling
//! и Nova→C type mapping, никаких ручных таблиц.
//!
//! См. spec/decisions/08-runtime.md → D82 (extended), Plan 12.
//!
//! Plan 103.6 / Plan 113: SyncClass annotation driven by #realtime/#parks/#wakes
//! attributes in sync.nv. Replaces hardcoded is_realtime_blocking lists.

use crate::ast::{FnBody, FnDecl, Item, Module, Param, Receiver, ReceiverKind, SyncClass, TypeDecl, TypeDeclKind, TypeRef};
use std::collections::HashMap;
use std::sync::OnceLock;

// Re-export SyncClass so callers (emit_c.rs) can import from either place.
pub use crate::ast::SyncClass as SyncClassAlias;

/// Plan 172.1.1 (U.1): registry-only builtin `.nv` sources parsed into `'static` `Module`s ONCE.
/// These concrete types (StringBuilder/WriteBuffer/ReadBuffer) are supplied to CODEGEN via
/// `load_builtins` but are ABSENT from the CHECKER's module-built registry → the checker neither
/// knows them as TYPES (`self.types`) nor has their method sigs (`method_table`), so it cannot
/// resolve their call callees (Call-GAP, all `rc=false`, §0.7 — StringBuilder ~14k). MERGED into
/// BOTH checker indexes (types + method sigs) so the checker resolves them → `resolved_callees` →
/// Call-channel (§0/§3 «один реестр для чекера И codegen»). `'static` (OnceLock) → `&FnDecl`/
/// `&TypeDecl` coerce into `SigRegistry<'a>`/`types<'a>` (`'static: 'a`). Parse-once (perf §2).
/// NET excluded (transitive deps, §10 — separate slice). ADDITIVE checker knowledge, NOT
/// `load_builtins` removal (§10 hazard is REMOVAL; adding resolution knowledge is the de-risk gate).
static BUILTIN_SIG_MODULES: OnceLock<Vec<Module>> = OnceLock::new();
pub fn builtin_sig_modules() -> &'static Vec<Module> {
    BUILTIN_SIG_MODULES.get_or_init(|| {
        [
            ExternalRegistry::STRING_BUILDER_SRC,
            ExternalRegistry::WRITE_BUFFER_SRC,
            ExternalRegistry::READ_BUFFER_SRC,
            // Plan 172.1 [M-172.1-d174-sync-consume-registry] (2026-07-02): sync —
            // чекер обязан знать guard-типы (Mutex/MutexGuard/Permit/…) и их
            // method-сигнатуры, чтобы (a) типизировать `mut mu = Mutex.new()` /
            // `consume g = mu.lock()` через КАНАЛ (иначе fail-path эмиссия
            // defer-тел бьётся в P67-пробы на нетипизированном `g`), (b) guard-
            // consume кредитовался из .nv-деклараций. sync.nv самодостаточен
            // (0 import'ов). NET по-прежнему excluded (транзитивный
            // net_last_error, §10 — отдельный срез).
            ExternalRegistry::SYNC_SRC,
        ]
        .iter()
        .filter_map(|src| crate::parser::parse(src).ok())
        .collect()
    })
}

/// Декларация одной external-функции из builtins.nv.
/// Содержит mangled C-name + информацию для emit_call.
#[derive(Debug, Clone)]
pub struct ExternalDecl {
    /// Имя метода/функции (без receiver-префикса).
    pub name: String,
    /// Имя receiver-типа (`StringBuilder`/`WriteBuffer`/...) или None
    /// для свободных функций (`str.from` имеет receiver `str`).
    pub receiver_type: Option<String>,
    /// `true` для instance (`@method`), `false` для static (`Type.method`).
    pub is_instance: bool,
    /// `mut`-receiver — учитывается mangling'ом не отдельно (это не
    /// влияет на C-name), но полезно для emit_call (валидация).
    pub is_mut_receiver: bool,
    /// Параметры (без receiver'а): C-типы для mangling и dispatch.
    pub param_c_types: Vec<String>,
    /// Имена параметров (для генерации читаемого C — необязательно).
    pub param_names: Vec<String>,
    /// Возвращаемый C-тип (`Self` уже резолвлен к receiver'у).
    pub return_c_type: String,
    /// Mangled C-name: `Nova_<RecvType>_static_<name>` /
    /// `Nova_<RecvType>_method_<name>`. Plan 11 mangling по
    /// param-types применяется при коллизии overload'ов.
    pub c_name: String,
    /// Plan 103.6 / Plan 113: Sync interaction class parsed from #realtime/#parks/#wakes.
    /// None = no annotation (conservative: treated as Parks in realtime context).
    pub sync_class: Option<SyncClass>,
    /// Plan 83.12: если return_c_type — `NovaRes_*` с non-trivial ok/err
    /// (т.е. не erased `nova_int_nova_str`), здесь хранится (ok_c, err_c)
    /// чтобы CEmitter мог вызвать `register_novares_decl` при инициализации.
    /// Это необходимо чтобы `NovaRes_Nova_TcpListener_p_nova_str*` и аналоги
    /// были зарегистрированы до первого использования в коде.
    pub result_ok_err: Option<(String, String)>,
    /// [M-compiler-nv-porting-wave] item B1: `true` для namespace pseudo-
    /// receiver'ов (gc/fibers/runtime/bench) чья C-функция возвращает
    /// `void`, но Nova-сигнатура — `-> ()`; вызов в expression-позиции
    /// должен дать значение → эмиттер оборачивает
    /// `(c_name(args), (nova_int)0LL)` вместо голого `c_name(args)`.
    /// `false` (default) для всех обычных external fn (StringBuilder/
    /// WriteBuffer/RawMem/…) — их unit-return уже корректно обрабатывается
    /// без обёртки (bare call), эта обёртка — legacy idiom конкретно этих
    /// namespace-функций, сохранён byte-for-byte при переносе из emit_c.rs
    /// hardcoded match-блоков в registry-driven dispatch.
    pub is_unit_wrap: bool,
}

/// Registry всех external-функций из builtins.nv.
/// Key: `(receiver_type, method_name)` → Vec overloads (Plan 11).
#[derive(Debug, Default, Clone)]
pub struct ExternalRegistry {
    /// Ключ `(recv_type_name, method_name)`.
    /// Для свободных функций (нет receiver'а) — recv_type_name = "".
    pub by_key: HashMap<(String, String), Vec<ExternalDecl>>,
    /// Set всех receiver-типов которые встречаются в декларациях.
    /// Используется для record_schemas init (Plan 12 Ф.2).
    pub receiver_types: Vec<String>,
    /// Plan 103.5: TypeDecl entries from external .nv files (sync.nv etc.).
    /// Includes both runtime-defined sum types (OnceState) and generic
    /// opaque types (OnceCell[T], Lazy[T]). Used by emit_c.rs to register
    /// them in generic_types/generic_type_templates/sum_schemas so that
    /// type inference and dispatch work correctly without them being declared
    /// in the user module.
    pub type_decls: Vec<TypeDecl>,
}

impl ExternalRegistry {
    /// Plan 13 Ф.8: builtins.nv удалён, заменён на per-type
    /// auto-generated файлы. ExternalRegistry загружает все 4 модуля
    /// (string_builder, write_buffer, read_buffer, char) — они embedded
    /// в binary через include_str!.
    pub const STRING_BUILDER_SRC: &'static str =
        include_str!("../../../std/runtime/string_builder.nv");
    pub const WRITE_BUFFER_SRC: &'static str =
        include_str!("../../../std/runtime/write_buffer.nv");
    pub const READ_BUFFER_SRC: &'static str =
        include_str!("../../../std/runtime/read_buffer.nv");
    pub const CHAR_SRC: &'static str =
        include_str!("../../../std/runtime/char.nv");
    pub const SYNC_SRC: &'static str =
        include_str!("../../../std/runtime/sync.nv");
    // Plan 118.1 Ф.1: byte-level memory intrinsics для FFI / driver work.
    pub const RAW_MEM_SRC: &'static str =
        include_str!("../../../std/runtime/raw_mem.nv");
    // Plan 172.1 U.1.3b срез 1: `FFI_CSTR_SRC` (include_str! `std/ffi/cstr.nv`) УДАЛЁН —
    // cstr больше не pre-load'ится в codegen-реестр, приходит через `import` (inline).

    // Plan 83.12: std/net — async TCP/UDP socket stdlib.
    pub const NET_ADDR_SRC: &'static str =
        include_str!("../../../std/net/addr.nv");
    pub const NET_TCP_SRC: &'static str =
        include_str!("../../../std/net/tcp.nv");
    pub const NET_UDP_SRC: &'static str =
        include_str!("../../../std/net/udp.nv");

    // [M-compiler-nv-porting-wave] item B1: gc/fibers/runtime/bench —
    // namespace pseudo-receiver API (Plan 32/44.2/44/57). Embedded так же,
    // как raw_mem/net — эти .nv уже source of truth для checker'а (обычный
    // `import`), теперь ЖЕ source питает codegen c-name dispatch через
    // ExternalRegistry (NAMESPACE_OVERRIDES выше), убирая hardcoded
    // match-блоки emit_c.rs.
    pub const GC_SRC: &'static str =
        include_str!("../../../std/runtime/gc.nv");
    pub const FIBERS_SRC: &'static str =
        include_str!("../../../std/runtime/fibers.nv");
    // Plan 175 (owner TODO closure, 2026-07-10): virtual-clock auto-idle-
    // advance coordination hook — see std/runtime/vclock.nv.
    pub const VCLOCK_SRC: &'static str =
        include_str!("../../../std/runtime/vclock.nv");
    pub const RUNTIME_SRC: &'static str =
        include_str!("../../../std/runtime/runtime.nv");
    pub const BENCH_SRC: &'static str =
        include_str!("../../../std/bench.nv");
    // [M-compiler-nv-porting-wave] item B2: f64/f32 IEEE 754 bit-cast —
    // c-name dispatch via PRIMITIVE_BITCAST_OVERRIDES above.
    pub const NUMERIC_SRC: &'static str =
        include_str!("../../../std/runtime/numeric.nv");

    /// Парсит per-type .nv файлы (string_builder/write_buffer/read_buffer/
    /// char/sync) и строит unified registry. Вызывается один раз при
    /// инициализации CEmitter.
    ///
    /// Plan 13 Ф.8: builtins.nv декомпозирован — теперь 5 источников.
    /// Все embedded в binary через include_str!.
    /// Plan 83.12: добавлены 3 источника std/net (addr, tcp, udp).
    pub fn load_builtins() -> Result<Self, String> {
        let mut reg = Self::default();
        // Plan 172.1 U.1.3a: `char.nv` УБРАН из этого захардкоженного списка —
        // `str.from`/`str.from_codepoint` теперь приходят через prelude
        // (`import std.runtime.char`, минимальный-prelude §3) и регистрируются в
        // реестре из import-resolved модуля (`from_module`). char.nv безопасен
        // для переноса сейчас: у него нет `Item::Type` (только 2 extern), поэтому
        // `by_key`-merge достаточно.
        //
        // Plan 172.1 U.1.3b СРЕЗ 1 (cstr — наименьший, 2026-06-25): `ffi/cstr.nv` УБРАН
        // из списка. cstr — листовая FFI-библиотека: 0 транзитивных зависимостей в
        // prelude/core (grep std/ — CStr/to_cstr только в самом cstr.nv), 0 usage без
        // `import` в корпусе (все cstr-тесты уже `import std.ffi.cstr`), а её
        // инлайн-путь ЗВУЧЕН (Nova-body `to_cstr` возвращает `CStr`, не self-extern-bool
        // — не требует Gap A/B; recon PASS). Под `import` cstr приходит через
        // import-resolved модуль (`from_module` merge :2157 несёт extern-сигнатуры +
        // `CStr` type_decl, U.1.3a). Предусловие §10 (звучность инлайн-пути ПЕРЕД
        // удалением снабжения) выполнено для cstr. Без import → чистая «undefined».
        //
        // Оставшиеся 8 файлов — БИБЛИОТЕЧНЫЕ; их удаление — продолжение U.1.3b
        // (sync/net требуют Gap A ✅ + Gap B primitive/U.4.3 tuple ПЕРЕД удалением,
        // [M-172.1-U1-lib-import-needs-U4]; net — ещё транзитивный net_last_error).
        for module in Self::builtin_modules() {
            reg.merge_from_module(module)?;
        }
        Ok(reg)
    }

    /// Plan 172.1 [M-172.1-d174-sync-consume-registry]: ЕДИНЫЙ список embedded
    /// builtin `.nv`-источников — тот же, что кормит `load_builtins`. Второй
    /// потребитель — checker'ские Linearity/ConsumeRegistry (consume-метаданные
    /// guard-типов D174: `MutexGuard consume` + `fn MutexGuard consume @unlock()`
    /// обязаны прийти из .nv-деклараций, не из хардкода §3). Один список — одно
    /// окно; сжимается по мере U.1.3b (файл ушёл на `import` → строка удаляется
    /// здесь, оба потребителя обновляются синхронно).
    pub fn builtin_sources() -> &'static [(&'static str, &'static str)] {
        &[
            ("string_builder.nv", Self::STRING_BUILDER_SRC),
            ("write_buffer.nv",   Self::WRITE_BUFFER_SRC),
            ("read_buffer.nv",    Self::READ_BUFFER_SRC),
            ("sync.nv",           Self::SYNC_SRC),
            // Plan 118.1 Ф.1: RawMem intrinsics.
            ("raw_mem.nv",        Self::RAW_MEM_SRC),
            // Plan 83.12: net stdlib.
            ("net/addr.nv",       Self::NET_ADDR_SRC),
            ("net/tcp.nv",        Self::NET_TCP_SRC),
            ("net/udp.nv",        Self::NET_UDP_SRC),
            // [M-compiler-nv-porting-wave] item B1: gc/fibers/runtime/bench
            // namespace pseudo-receivers — c-name dispatch теперь registry-
            // driven (NAMESPACE_OVERRIDES) вместо emit_c.rs hardcoded match.
            ("runtime/gc.nv",      Self::GC_SRC),
            ("runtime/fibers.nv",  Self::FIBERS_SRC),
            ("runtime/vclock.nv",  Self::VCLOCK_SRC),
            ("runtime/runtime.nv", Self::RUNTIME_SRC),
            ("bench.nv",           Self::BENCH_SRC),
            // [M-compiler-nv-porting-wave] item B2: f64/f32 bit-cast.
            ("runtime/numeric.nv", Self::NUMERIC_SRC),
        ]
    }

    /// Распарсенные builtin-модули (см. `builtin_sources`) — parse один раз на
    /// процесс (OnceLock, §2). Ошибка парса embedded-источника = баг сборки —
    /// паникуем громко (эти файлы компилируются в бинарь и парсятся в
    /// `load_builtins` тем же парсером).
    pub fn builtin_modules() -> &'static [crate::ast::Module] {
        static MODULES: std::sync::OnceLock<Vec<crate::ast::Module>> =
            std::sync::OnceLock::new();
        MODULES.get_or_init(|| {
            Self::builtin_sources()
                .iter()
                .map(|(name, src)| {
                    crate::parser::parse(src).unwrap_or_else(|d| {
                        panic!("failed to parse embedded builtin {}: {}", name, d.message)
                    })
                })
                .collect()
        })
    }

    /// Merge entries из одного модуля в self. Используется для
    /// multi-file load_builtins. Сохраняет cumulative receiver_types
    /// и by_key.
    fn merge_from_module(&mut self, module: &Module) -> Result<(), String> {
        let other = Self::from_module(module)?;
        for rt in other.receiver_types {
            if !self.receiver_types.contains(&rt) {
                self.receiver_types.push(rt);
            }
        }
        for (k, v) in other.by_key {
            self.by_key.entry(k).or_default().extend(v);
        }
        // Plan 103.5: merge type_decls (sum types + generic opaque types).
        for td in other.type_decls {
            if !self.type_decls.iter().any(|t| t.name == td.name) {
                self.type_decls.push(td);
            }
        }
        Ok(())
    }

    /// Строит registry из произвольного модуля (для тестов / sanity).
    /// Двухпроходный алгоритм: сначала подсчёт overload'ов, затем
    /// генерация имён с правильным mangling'ом.
    pub fn from_module(module: &Module) -> Result<Self, String> {
        // Pass 1: подсчёт overload'ов per ключ.
        // Skip `extern "C" fn` — они не идут в registry (literal C name, no nova_fn_ prefix).
        let mut overload_count: HashMap<(String, String), usize> = HashMap::new();
        for item in &module.items {
            if let Item::Fn(f) = item {
                if !f.is_external || f.extern_abi.as_deref() == Some("C") { continue; }
                let recv_ty = f.receiver.as_ref().map(|r| r.type_name.clone()).unwrap_or_default();
                let key = (recv_ty, f.name.clone());
                *overload_count.entry(key).or_insert(0) += 1;
            }
        }
        // Pass 2: построить registry.
        let mut reg = Self::default();
        let mut seen_types: std::collections::HashSet<String> = Default::default();
        for item in &module.items {
            let f = match item {
                // Plan 91.12 Ф.-1 (D282): `extern "C" fn` — literal C name,
                // NOT registered in ExternalRegistry (no nova_fn_ prefix).
                // Only `extern "nova" fn` and legacy `external fn` go through registry.
                Item::Fn(f) if f.is_external && f.extern_abi.as_deref() != Some("C") => f,
                _ => continue,
            };
            // [M-compiler-nv-porting-wave] item B1: fn-level generics (`fn
            // bench.opaque[T](v T) -> T`, receiver НЕ generic — generics
            // здесь на самой fn, не на Receiver.generics как у OnceCell[T]/
            // Lazy[T]) не резолвятся `type_ref_to_c` (T — не конкретный
            // тип) → skip. `bench.opaque` остаётся hardcoded в emit_c.rs
            // (generic pass-through macro, вне registry-driven dispatch).
            if !f.generics.is_empty() { continue; }
            debug_assert!(matches!(&f.body, FnBody::External));
            let recv_ty_str = f.receiver.as_ref().map(|r| r.type_name.clone()).unwrap_or_default();
            let total_overloads = *overload_count
                .get(&(recv_ty_str.clone(), f.name.clone()))
                .unwrap_or(&1);
            let decl = Self::decl_from_fn(f, total_overloads)?;
            if let Some(ref rt) = decl.receiver_type {
                if !seen_types.contains(rt) {
                    seen_types.insert(rt.clone());
                    reg.receiver_types.push(rt.clone());
                }
            }
            let key = (
                decl.receiver_type.clone().unwrap_or_default(),
                decl.name.clone(),
            );
            reg.by_key.entry(key).or_default().push(decl);
        }
        // Plan 103.5: collect Item::Type declarations (sum types + generic
        // opaque types from sync.nv) for later registration in emit_c.rs.
        // These drive sum_schemas (OnceState), generic_types (OnceCell, Lazy),
        // and generic_type_templates needed for dispatch + type inference.
        //
        // Plan 91.12 V2 (D126 retract — sync types migration): runtime-backed
        // newtype declarations (`type X[T](ptr)` для OnceCell/Lazy/Condvar)
        // тоже должны попадать в type_decls — иначе method-dispatch для
        // `Lazy[int].new(...)` не находит receiver type и codegen падает
        // с misrouted `Nova_int_method_new(Lazy, ...)`. Type_decls collection
        // unifies Opaque (legacy) и Newtype (post-V2) paths.
        for item in &module.items {
            if let Item::Type(t) = item {
                // Only collect sum types, opaque types, and runtime-backed
                // newtypes relevant to codegen.
                match &t.kind {
                    TypeDeclKind::Sum(_) => reg.type_decls.push(t.clone()),
                    TypeDeclKind::Opaque => reg.type_decls.push(t.clone()),
                    // Plan 91.12 V2: newtype-over-ptr declarations с generics
                    // или с runtime-backed именами need same dispatch path.
                    // Без generics non-runtime-backed newtypes (e.g.
                    // `type SqHandle(ptr)`) НЕ нуждаются — codegen ходит
                    // через регистрацию external fn напрямую (Plan 115).
                    TypeDeclKind::Newtype(_)
                        if !t.generics.is_empty()
                            || matches!(t.name.as_str(),
                                "OnceCell" | "Lazy" | "Condvar")
                    => reg.type_decls.push(t.clone()),
                    _ => {}
                }
            }
        }
        Ok(reg)
    }

    /// Plan 172.1 U.2.2 (§0 один источник mangling): Plan 11 c-name для метода/fn.
    /// ЕДИНЫЙ источник, используемый и `decl_from_fn` (ExternalRegistry), и
    /// `SigRegistry` — раньше логика жила только здесь inline. Byte-identical с
    /// прежним inline-кодом (тот же base + overload-suffix).
    ///
    /// - `(Some(rt), instance, consume)` → `Nova_{rt}_consume_{name}` (D174)
    /// - `(Some(rt), instance, !consume)` → `Nova_{rt}_method_{name}`
    /// - `(Some(rt), static)` → `Nova_{rt}_static_{name}`
    /// - `(None)` → `nova_fn_{name}`
    /// + `_{last_suffix}` при `total_overloads >= 2 && !last_suffix.is_empty()`.
    pub(crate) fn mangle_method_c_name(
        receiver: Option<&str>,
        is_instance: bool,
        is_consume_recv: bool,
        name: &str,
        total_overloads: usize,
        last_suffix: &str,
    ) -> String {
        let base_c = match (receiver, is_instance, is_consume_recv) {
            (Some(rt), true, true)  => format!("Nova_{}_consume_{}", rt, name),
            (Some(rt), true, false) => format!("Nova_{}_method_{}", rt, name),
            (Some(rt), false, _)    => format!("Nova_{}_static_{}", rt, name),
            (None, _, _)            => format!("nova_fn_{}", name),
        };
        if total_overloads >= 2 && !last_suffix.is_empty() {
            format!("{}_{}", base_c, last_suffix)
        } else {
            base_c
        }
    }

    /// Plan 172.1 U.2.2 (§0): Nova-type имя ПОСЛЕДНЕГО параметра для overload-
    /// suffix (Plan 103.2). `[]byte → "bytes"`, `char → "char"`, …; пусто если
    /// нет параметров / non-simple. ЕДИНЫЙ источник (был inline в decl_from_fn).
    pub(crate) fn last_param_suffix(params: &[Param]) -> String {
        match params.last().map(|p| &p.ty) {
            Some(TypeRef::Named { path, .. }) if path.len() == 1 => path[0].clone(),
            Some(TypeRef::Array(inner, _)) => match inner.as_ref() {
                TypeRef::Named { path, .. } if path.len() == 1 => format!("{}s", path[0]),
                _ => "arr".into(),
            },
            _ => String::new(),
        }
    }

    /// [M-compiler-nv-porting-wave] item B1: gc/fibers/runtime/bench —
    /// lowercase pseudo-receiver namespaces (Plan 32/44.2/44/57 — «namespace»,
    /// не opaque-тип). Их `.nv`-декларации (std/runtime/{gc,fibers,runtime}.nv,
    /// std/bench.nv) уже source of truth для checker'а (импортируются
    /// обычным `import`), но РЕАЛЬНЫЕ C-имена этого read/introspection API —
    /// прямые runtime internals (`nova_gc_heap_size`, `nova_fiber_yield`, …),
    /// НЕ производные Nova_{Recv}_static_{name} mangling'а — стандартный
    /// `mangle_method_c_name` даёт неверное имя. Раньше расхождение жило как
    /// 4 отдельных match-блока в emit_c.rs (arity-check + C-имя, задублировано
    /// ручной синхронизацией — см. бывшие предупреждения в .nv). Теперь —
    /// ОДНА таблица здесь; `decl_from_fn` применяет override после обычного
    /// mangling. `unit_wrap = true` — C-функция `void`, Nova `-> ()`,
    /// emit-время сохраняет legacy `(call, (nova_int)0LL)` idiom (byte-
    /// identity с прежним hardcoded выводом). `bench.opaque[T]` НЕ здесь —
    /// класс C: compiler intrinsic black-box barrier (anti-optimization
    /// macro, не C-function call), fn-level generic `T` вдобавок не
    /// резолвится `type_ref_to_c` (skip по generics guard в `from_module`
    /// ниже) — остаётся hardcoded в emit_c.rs НАВСЕГДА по природе, как
    /// panic()/exit()/assert() (не registry-гэп).
    const NAMESPACE_OVERRIDES: &[(&str, &str, &str, bool)] = &[
        // gc.* (Plan 32).
        ("gc", "heap_size",     "nova_gc_heap_size",     false),
        ("gc", "live_count",    "nova_gc_live_count",    false),
        ("gc", "alloc_count",   "nova_gc_alloc_count",   false),
        ("gc", "collect",       "nova_gc_collect",       true),
        ("gc", "reset_stats",   "nova_gc_reset_stats",   true),
        ("gc", "last_pause_ns", "nova_gc_last_pause_ns", false),
        // fibers.* (Plan 44.2 Этап 3).
        ("fibers", "virtual_reserved", "nova_fibers_virtual_reserved", false),
        ("fibers", "slot_count",       "nova_fibers_slot_count",       false),
        ("fibers", "slots_active",     "nova_fibers_slots_active",     false),
        ("fibers", "high_water",       "nova_fibers_high_water",       false),
        ("fibers", "compact",          "nova_fibers_compact",          true),
        // runtime.* (Plan 44 Этап 0).
        ("runtime", "init",               "nova_runtime_init",               true),
        ("runtime", "shutdown",           "nova_runtime_shutdown",           true),
        ("runtime", "worker_count",       "nova_runtime_worker_count",       false),
        ("runtime", "maxprocs",           "nova_runtime_maxprocs",           false),
        ("runtime", "is_initialized",     "nova_runtime_is_initialized",     false),
        ("runtime", "current_worker_id",  "nova_runtime_current_worker_id",  false),
        ("runtime", "drain_orphans",      "nova_runtime_drain_orphans",      true),
        // Irregular: `runtime.yield()` — actual C symbol is the general
        // fiber-yield primitive, NOT `nova_runtime_yield`.
        ("runtime", "yield",              "nova_fiber_yield",                true),
        // vclock.* (Plan 175, owner TODO closure, 2026-07-10) — virtual-
        // clock auto-idle-advance coordination hook (std/runtime/vclock.nv).
        // `false`: C fn returns `nova_unit` already (not bare `void`), no
        // expr-position wrap needed (unlike gc/fibers/runtime `true` entries
        // above, whose C fns are bare `void`).
        ("vclock", "park_until",          "nova_vclock_park_until",          false),
        // bench.* (Plan 57) — `opaque[T]` excluded: class C compiler
        // intrinsic (black-box barrier), permanently hardcoded in emit_c.rs.
        ("bench", "iterations",  "nova_bench_iterations", false),
        ("bench", "reset_timer", "nova_bench_reset_timer", true),
        // Irregular: Nova method name ≠ C function name (setter naming).
        ("bench", "bytes",    "nova_bench_set_throughput_bytes",    true),
        ("bench", "elements", "nova_bench_set_throughput_elements", true),
        ("bench", "allocs",   "nova_bench_alloc_count_snapshot",    false),
        ("bench", "now_ns",   "nova_bench_now_ns",                  false),
        ("bench", "metric",   "nova_bench_emit_metric",             true),
    ];

    /// [M-ptr-raw-access-contract-and-unaligned] item 3 (2026-07-08):
    /// `PRIMITIVE_BITCAST_OVERRIDES` (f64/f32 `from_bits`/`to_bits` C-name
    /// override, ex-[M-compiler-nv-porting-wave] item B2) REMOVED — these are
    /// no longer `extern "nova"` declarations at all. `std/runtime/numeric.nv`
    /// now declares them as ordinary Nova-body methods (`unsafe { (&@ as
    /// *u64).read_unaligned() }` etc., поверх item 2's pointer methods),
    /// resolved through the normal Nova-body method dispatch (same channel as
    /// `char.from`/`u8.try_from` in `std/runtime/char.nv`) — no `ExternalDecl`,
    /// no c_name override needed. `nova_rt/numeric.h`'s `Nova_f64_to_bits`
    /// C helpers are dead code, removed with this change. `int.to_bits(f f64)
    /// -> int` (legacy Plan 04 helper, read_buffer round-trip) is unrelated —
    /// has no std/runtime/numeric.nv declaration, stays hardcoded in
    /// emit_c.rs as-is.
    fn decl_from_fn(f: &FnDecl, total_overloads: usize) -> Result<ExternalDecl, String> {
        let (recv_type_name, is_instance, is_mut_recv, is_consume_recv) = match &f.receiver {
            Some(Receiver { type_name, kind, mutable, consume, .. }) => {
                let inst = matches!(kind, ReceiverKind::Instance);
                (Some(type_name.clone()), inst, *mutable, *consume)
            }
            None => (None, false, false, false),
        };
        // Resolve param types к C-типам.
        let mut param_c_types: Vec<String> = Vec::new();
        let mut param_names: Vec<String> = Vec::new();
        for p in &f.params {
            let cty = Self::type_ref_to_c(&p.ty, recv_type_name.as_deref())?;
            param_c_types.push(cty);
            param_names.push(p.name.clone());
        }
        let return_c_type = match &f.return_type {
            Some(t) => Self::type_ref_to_c(t, recv_type_name.as_deref())?,
            None => "nova_unit".into(),
        };
        // Mangling: если ключ имеет ≥2 overload'ов, добавляется suffix
        // по Nova-type первого параметра (`_str`/`_char`/`_bytes`/...).
        // Это compatible с runtime naming.
        // Plan 103.9 (D174): consume-receiver methods → Nova_{T}_consume_{name}
        // to match the D164 ABI used by emit_c.rs for user-defined consume methods.
        // Plan 172.1 U.2.2: base + overload-suffix через ЕДИНЫЕ хелперы (§0).
        // Suffix — Nova-type ПОСЛЕДНЕГО параметра (Plan 103.2, различает
        // `store(v)` vs `store(v, ord)`). consume-receiver → Nova_{T}_consume_{m}
        // (D174). Byte-identical с прежним inline-кодом.
        let suffix = Self::last_param_suffix(&f.params);
        let c_name = Self::mangle_method_c_name(
            recv_type_name.as_deref(),
            is_instance,
            is_consume_recv,
            &f.name,
            total_overloads,
            &suffix,
        );
        // Plan 83.12: для Result-возвратов с non-trivial ok-типом
        // сохраняем (ok_c, err_c) чтобы CEmitter мог зарегистрировать
        // `NovaRes_<ok_s>_<err_s>` struct через `register_novares_decl`.
        // Нужно только для `NovaRes_*` отличных от erased `nova_int_nova_str`.
        let result_ok_err: Option<(String, String)> = if return_c_type.starts_with("NovaRes_")
            && return_c_type != "NovaRes_nova_int_nova_str*"
        {
            // Восстанавливаем (ok_c, err_c) напрямую из TypeRef return_type.
            if let Some(TypeRef::Named { path, generics: ret_generics, .. }) = &f.return_type {
                if path.len() == 1 && path[0] == "Result" && ret_generics.len() >= 2 {
                    let ok_c = Self::type_ref_to_c(&ret_generics[0], recv_type_name.as_deref()).ok();
                    let err_c = Self::type_ref_to_c(&ret_generics[1], recv_type_name.as_deref()).ok();
                    match (ok_c, err_c) {
                        (Some(ok), Some(err)) => Some((ok, err)),
                        _ => None,
                    }
                } else { None }
            } else { None }
        } else {
            None
        };
        // [M-compiler-nv-porting-wave] item B1: c_name override lookup —
        // namespace pseudo-receivers (gc/fibers/runtime/bench, NAMESPACE_
        // OVERRIDES). PRIMITIVE_BITCAST_OVERRIDES (f64/f32 bit-cast) removed
        // [M-ptr-raw-access-contract-and-unaligned] item 3 — no longer extern,
        // nothing to override here.
        let (c_name, is_unit_wrap) = match recv_type_name.as_deref() {
            Some(ns) => {
                let found = Self::NAMESPACE_OVERRIDES.iter()
                    .find(|(n, m, _, _)| *n == ns && *m == f.name);
                match found {
                    Some((_, _, override_c_name, unit_wrap)) => (override_c_name.to_string(), *unit_wrap),
                    None => (c_name, false),
                }
            }
            None => (c_name, false),
        };
        Ok(ExternalDecl {
            name: f.name.clone(),
            receiver_type: recv_type_name,
            is_instance,
            is_mut_receiver: is_mut_recv,
            param_c_types,
            param_names,
            return_c_type,
            c_name,
            sync_class: f.sync_class,
            result_ok_err,
            is_unit_wrap,
        })
    }

    /// Plan 83.12: sanitize C-type string for use in `NovaRes_<ok_s>_<err_s>` name.
    /// Mirrors `CEmitter::sanitize_for_novaopt` (defined there as an associated fn).
    fn sanitize_for_novares(c_ty: &str) -> String {
        c_ty.replace('*', "_p").replace(' ', "_")
    }

    /// Type mapping из Nova TypeRef в C-имя. Соответствует
    /// `CEmitter::type_ref_to_c`, но в standalone-форме (не требует
    /// CEmitter state — эта функция запускается ДО того, как CEmitter
    /// вообще существует, во время построения registry). `Self` резолвится
    /// к receiver-типу.
    // Plan 172.1 U.2.2: pub(crate) — SigRegistry переиспользует ЭТОТ standalone
    // type→C mapping (а не копирует), реализуя §0 «один type→C mapping».
    pub(crate) fn type_ref_to_c(ty: &TypeRef, recv: Option<&str>) -> Result<String, String> {
        match ty {
            TypeRef::Named { path, generics, .. } => {
                let name = path.join("_");
                // 196.3 D315: primitives resolve through THE single shared table
                // `CEmitter::primitive_name_to_c` (emit_c.rs, made `pub(crate)` for
                // this call) instead of a 4th hand-copied primitive list — mirrors
                // the U.6.1.b dedup already done for `apply_type_subst_to_ref`
                // (missing-`u32`-drift precedent: Plan 152.8). Only the
                // caller-specific bits (removed usize/isize/ptr, Self/Result/
                // Option, opaque user-type fallback) stay below.
                if let Some(c) = super::emit_c::CEmitter::primitive_name_to_c(&name) {
                    return Ok(c.into());
                }
                Ok(match name.as_str() {
                    // Plan 133: usize/isize removed — use int/uint instead.
                    "usize" => return Err("type `usize` is removed — use `int` (Plan 133)".into()),
                    "isize" => return Err("type `isize` is removed — use `int` (Plan 133)".into()),
                    // Plan 134: `ptr` builtin type REMOVED — use `*()` (Plan 134).
                    // *() parsed as TypeRef::Pointer(TypeRef::Unit) → handled below.
                    "ptr" => return Err("type `ptr` is removed — use `*()` (Plan 134)".into()),
                    "Self" => match recv {
                        Some("str") => "nova_str".into(),
                        Some(t) => format!("Nova_{}*", t),
                        None => return Err("Self in non-receiver context".into()),
                    },
                    "Result" => {
                        // Plan 83.12: вычисляем конкретный mono-тип из generic args.
                        // Раньше — всегда erased `NovaRes_nova_int_nova_str*`.
                        // Теперь: `Result[TcpListener, str]` →
                        //   `NovaRes_Nova_TcpListener_p_nova_str*`
                        // чтобы `unwrap()` давал `Nova_TcpListener*` и методы
                        // на нём диспатчились через ExternalRegistry.
                        // Для `Result[int, str]` / `Result[str, str]` /
                        // `Result[u16, str]` вычисляем аналогично.
                        let ok_c = if !generics.is_empty() {
                            Self::type_ref_to_c(&generics[0], recv)?
                        } else {
                            "nova_int".to_string()
                        };
                        let err_c = if generics.len() > 1 {
                            Self::type_ref_to_c(&generics[1], recv)?
                        } else {
                            "nova_str".to_string()
                        };
                        if ok_c == "nova_int" && err_c == "nova_str" {
                            // Canonical erased pair — pre-defined in array.h.
                            "NovaRes_nova_int_nova_str*".into()
                        } else {
                            let ok_s = Self::sanitize_for_novares(&ok_c);
                            let err_s = Self::sanitize_for_novares(&err_c);
                            format!("NovaRes_{}_{}*", ok_s, err_s)
                        }
                    }
                    "Option" => {
                        // Plan 103.5: preserve inner type param for generic methods.
                        // Option[T] in generic context → "NovaOpt_T" so that
                        // type substitution (T → nova_str etc.) works in infer_expr_c_type.
                        if !generics.is_empty() {
                            if let TypeRef::Named { path, generics: ig, .. } = &generics[0] {
                                if ig.is_empty() && path.len() == 1 {
                                    let inner = &path[0];
                                    // Preserve: return "NovaOpt_<inner>" regardless of whether
                                    // inner is a type param or a concrete type. The substitution
                                    // fold in infer_expr_c_type handles T → concrete replacement.
                                    let inner_c = match inner.as_str() {
                                        "int" | "i64" => "nova_int".to_string(),
                                        "str"  => "nova_str".to_string(),
                                        "bool" => "nova_bool".to_string(),
                                        "char" => "nova_char".to_string(),
                                        other  => other.to_string(), // type param like T, E, V
                                    };
                                    return Ok(format!("NovaOpt_{}", inner_c));
                                }
                            }
                        }
                        "NovaOpt_nova_int".into()
                    }
                    _ => format!("Nova_{}*", name),
                })
            }
            TypeRef::Unit(_) => Ok("nova_unit".into()),
            TypeRef::Array(inner, _) => {
                if let TypeRef::Named { path, .. } = inner.as_ref() {
                    if path.len() == 1 {
                        return Ok(match path[0].as_str() {
                            "str" => "NovaArray_nova_str*".into(),
                            "u8" => "NovaArray_nova_byte*".into(),
                            "bool" => "NovaArray_nova_bool*".into(),
                            "f64" => "NovaArray_nova_f64*".into(),
                            // Plan 70.4: f32 distinct from f64 (ABI: 4 vs 8 bytes).
                            "f32" => "NovaArray_nova_f32*".into(),
                            // Plan 70.3: distinct array element type for char.
                            "char" => "NovaArray_nova_char*".into(),
                            // Plan 70.4 Ф.2: sized-int arrays — distinct packed storage.
                            "i32" => "NovaArray_int32_t*".into(),
                            "i16" => "NovaArray_int16_t*".into(),
                            "i8"  => "NovaArray_int8_t*".into(),
                            "i64" => "NovaArray_nova_int*".into(), // i64 == nova_int (int64_t)
                            "u32"  => "NovaArray_uint32_t*".into(),
                            "u16"  => "NovaArray_uint16_t*".into(),
                            "u64"  => "NovaArray_uint64_t*".into(),
                            // Plan 70.5: uint = alias u64.
                            "uint" => "NovaArray_uint64_t*".into(),
                            _ => "NovaArray_nova_int*".into(),
                        });
                    }
                }
                Ok("NovaArray_nova_int*".into())
            }
            // Plan 115 D214: external fn tuple-by-value returns. Compute
            // mono'd `_NovaTuple_<arity>_<L1>_<T1>_<L2>_<T2>...` mangled
            // name matching CEmitter::compute_mono_tuple_c_name. C ABI
            // handles struct return per platform (register vs hidden-out).
            // Fallback на legacy `_NovaTupleN` если элементы non-concrete
            // (generic-erased).
            TypeRef::Tuple(elems, _) => {
                let mut elem_cs: Vec<String> = Vec::with_capacity(elems.len());
                let mut all_concrete = true;
                for el in elems {
                    match Self::type_ref_to_c(el, recv) {
                        Ok(c) => {
                            // Empty string = unresolved/erased type → fallback.
                            // Plan 134: void* (from *()) IS concrete — do not
                            // treat as erased; sanitize_c_for_ident("void*")
                            // → "void_p" matching the shim header's mangled name.
                            if c.is_empty() {
                                all_concrete = false;
                                break;
                            }
                            elem_cs.push(c);
                        }
                        Err(_) => { all_concrete = false; break; }
                    }
                }
                if all_concrete && !elem_cs.is_empty() {
                    // Mirror CEmitter::compute_mono_tuple_c_name.
                    let mut out = String::from("_NovaTuple_");
                    out.push_str(&elem_cs.len().to_string());
                    for c_ty in &elem_cs {
                        let sanitized = c_ty
                            .replace("* ", "_p_")
                            .replace('*', "_p")
                            .replace(' ', "_")
                            .replace('[', "_arr_")
                            .replace(']', "")
                            .replace('-', "_");
                        out.push('_');
                        out.push_str(&sanitized.len().to_string());
                        out.push('_');
                        out.push_str(&sanitized);
                    }
                    Ok(out)
                } else {
                    Ok(format!("_NovaTuple{}", elems.len()))
                }
            }
            TypeRef::Func { .. } => Ok("void*".into()),
            TypeRef::FixedArray(_, inner, _) => Self::type_ref_to_c(inner, recv),
            // Plan 97 Ф.2 (D142): анонимный protocol-тип в external-fn
            // сигнатуре не имеет concrete C-репрезентации — value-erased.
            // External-FFI обычно не использует protocol-параметры, но
            // arm нужен для exhaustiveness.
            TypeRef::Protocol { .. } => Ok("void*".into()),
            // D176 (Plan 108): readonly T — transparent for codegen.
            TypeRef::Readonly(inner, _) => Self::type_ref_to_c(inner, recv),
            // Plan 118.5 D216 V2 §V2.3: typed pointer `*T` is canonical
            // read-only — emit C `const T*`. `mut`/`unsafe` are first-class
            // wrappers; when they wrap a Pointer they strip the `const`
            // (→ `T*`); otherwise they are transparent for codegen.
            // Plan 134: *() = pointer-to-unit = void* (replaces `ptr` builtin).
            // Plan 138.5 final model: pointee-mut is POSTFIX, so the canonical
            // form is `Pointer(Mut(T))` / `Pointer(Unsafe(T))` (= `*mut T` /
            // `*unsafe T`). The pointee-mut wrapper INSIDE the Pointer strips
            // the `const` (writable pointee → `T*`); a bare `Pointer(T)` (≡
            // `*ro T`) stays `const T*`. This mirrors emit_c.rs `type_ref_to_c`
            // so the registry's mangled C-symbol matches the call-site symbol
            // (otherwise `*mut u8` would mangle to `const nova_byte*` here but
            // `nova_byte*` at the call site → undefined-symbol link error).
            TypeRef::Pointer(inner, _) => {
                let (is_mutable_ptr, base_inner) = match inner.as_ref() {
                    TypeRef::Mut(ti, _) | TypeRef::Uninit(ti, _) => (true, ti.as_ref()),
                    _ => (false, inner.as_ref()),
                };
                if matches!(base_inner, TypeRef::Unit(_)) {
                    return Ok("void*".into());
                }
                let inner_c = Self::type_ref_to_c(base_inner, recv)?;
                if is_mutable_ptr {
                    Ok(format!("{}*", inner_c))
                } else {
                    Ok(format!("const {}*", inner_c))
                }
            }
            TypeRef::Mut(inner, _) | TypeRef::Uninit(inner, _) => {
                if let TypeRef::Pointer(p_inner, _) = inner.as_ref() {
                    // Plan 134: *mut () = void*.
                    if matches!(p_inner.as_ref(), TypeRef::Unit(_)) {
                        return Ok("void*".into());
                    }
                    let p_inner_c = Self::type_ref_to_c(p_inner, recv)?;
                    Ok(format!("{}*", p_inner_c))
                } else {
                    Self::type_ref_to_c(inner, recv)
                }
            }
            // Plan 184: `ref T` не появляется на extern-границе (Р9 — только
            // сырые `*`/`*mut`); транспарентно резолвим цель (Р6 для heap уже
            // выполнена, value-ref → указатель-алиас эмитится на месте локала).
            TypeRef::Ref(inner, _) => Self::type_ref_to_c(inner, recv),
        }
    }

    /// Lookup overloads по (receiver_type, method_name).
    pub fn lookup(&self, recv_type: &str, method: &str) -> Option<&[ExternalDecl]> {
        self.by_key
            .get(&(recv_type.to_string(), method.to_string()))
            .map(|v| v.as_slice())
    }

    /// True если у opaque-типа есть метод (для type-checker gate, Ф.6).
    #[allow(dead_code)]
    pub fn has_method(&self, recv_type: &str, method: &str) -> bool {
        self.by_key
            .contains_key(&(recv_type.to_string(), method.to_string()))
    }
}
