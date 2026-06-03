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

use crate::ast::{FnBody, FnDecl, Item, Module, Receiver, ReceiverKind, SyncClass, TypeDecl, TypeDeclKind, TypeRef};
use std::collections::HashMap;

// Re-export SyncClass so callers (emit_c.rs) can import from either place.
pub use crate::ast::SyncClass as SyncClassAlias;

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

    // Plan 83.12: std/net — async TCP/UDP socket stdlib.
    pub const NET_ADDR_SRC: &'static str =
        include_str!("../../../std/net/addr.nv");
    pub const NET_TCP_SRC: &'static str =
        include_str!("../../../std/net/tcp.nv");
    pub const NET_UDP_SRC: &'static str =
        include_str!("../../../std/net/udp.nv");

    /// Парсит per-type .nv файлы (string_builder/write_buffer/read_buffer/
    /// char/sync) и строит unified registry. Вызывается один раз при
    /// инициализации CEmitter.
    ///
    /// Plan 13 Ф.8: builtins.nv декомпозирован — теперь 5 источников.
    /// Все embedded в binary через include_str!.
    /// Plan 83.12: добавлены 3 источника std/net (addr, tcp, udp).
    pub fn load_builtins() -> Result<Self, String> {
        let mut reg = Self::default();
        for (name, src) in &[
            ("string_builder.nv", Self::STRING_BUILDER_SRC),
            ("write_buffer.nv",   Self::WRITE_BUFFER_SRC),
            ("read_buffer.nv",    Self::READ_BUFFER_SRC),
            ("char.nv",           Self::CHAR_SRC),
            ("sync.nv",           Self::SYNC_SRC),
            // Plan 118.1 Ф.1: RawMem intrinsics.
            ("raw_mem.nv",        Self::RAW_MEM_SRC),
            // Plan 83.12: net stdlib.
            ("net/addr.nv",       Self::NET_ADDR_SRC),
            ("net/tcp.nv",        Self::NET_TCP_SRC),
            ("net/udp.nv",        Self::NET_UDP_SRC),
        ] {
            let module = crate::parser::parse(src)
                .map_err(|d| format!("failed to parse {}: {}", name, d.message))?;
            reg.merge_from_module(&module)?;
        }
        Ok(reg)
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
        let mut overload_count: HashMap<(String, String), usize> = HashMap::new();
        for item in &module.items {
            if let Item::Fn(f) = item {
                if !f.is_external { continue; }
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
                Item::Fn(f) if f.is_external => f,
                _ => continue,
            };
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
        let base_c = match (&recv_type_name, is_instance, is_consume_recv) {
            (Some(rt), true, true)  => format!("Nova_{}_consume_{}", rt, f.name),
            (Some(rt), true, false) => format!("Nova_{}_method_{}", rt, f.name),
            (Some(rt), false, _)    => format!("Nova_{}_static_{}", rt, f.name),
            (None, _, _)            => format!("nova_fn_{}", f.name),
        };
        // Suffix builder: использует Nova-type ПОСЛЕДНЕГО параметра (Plan 103.2).
        // Использование last (а не first) позволяет различать overload'ы вида:
        //   store(v i64)  vs  store(v i64, ord MemOrdering)
        // — оба имеют одинаковый первый параметр, но разный последний.
        // Обратная совместимость: все существующие overload'ы однопараметровые
        // (first == last), поэтому с-имена не меняются.
        // []byte → "bytes", char → "char", str → "str", MemOrdering → "MemOrdering".
        let suffix = if !f.params.is_empty() {
            match f.params.last().map(|p| &p.ty) {
                Some(TypeRef::Named { path, .. }) if path.len() == 1 => path[0].clone(),
                Some(TypeRef::Array(inner, _)) => match inner.as_ref() {
                    TypeRef::Named { path, .. } if path.len() == 1 => format!("{}s", path[0]),
                    _ => "arr".into(),
                },
                _ => String::new(),
            }
        } else {
            String::new()
        };
        let c_name = if total_overloads >= 2 && !suffix.is_empty() {
            format!("{}_{}", base_c, suffix)
        } else {
            base_c
        };
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
        })
    }

    /// Plan 83.12: sanitize C-type string for use in `NovaRes_<ok_s>_<err_s>` name.
    /// Mirrors `CEmitter::sanitize_for_novaopt` (defined there as an associated fn).
    fn sanitize_for_novares(c_ty: &str) -> String {
        c_ty.replace('*', "_p").replace(' ', "_")
    }

    /// Type mapping из Nova TypeRef в C-имя. Соответствует
    /// `CEmitter::type_ref_to_c`, но в standalone-форме (не требует
    /// CEmitter state). `Self` резолвится к receiver-типу.
    fn type_ref_to_c(ty: &TypeRef, recv: Option<&str>) -> Result<String, String> {
        match ty {
            TypeRef::Named { path, generics, .. } => {
                let name = path.join("_");
                Ok(match name.as_str() {
                    "int" | "i64" => "nova_int".into(),
                    "i32" => "int32_t".into(),
                    "i16" => "int16_t".into(),
                    "i8"  => "int8_t".into(),
                    "u64" => "uint64_t".into(),
                    // Plan 70.5: uint = alias u64.
                    "uint" => "uint64_t".into(),
                    // Plan 118 / D226 amend: usize = u64 alias (platform
                    // pointer width on 64-bit), isize = i64 alias.
                    "usize" => "uint64_t".into(),
                    "isize" => "nova_int".into(),
                    "u32" => "uint32_t".into(),
                    "u16" => "uint16_t".into(),
                    // Plan 70.4 Ф.4: u8 → nova_byte (unified with byte).
                    "u8"  => "nova_byte".into(),
                    "f64" => "nova_f64".into(),
                    "f32" => "nova_f32".into(),
                    "bool" => "nova_bool".into(),
                    "str" => "nova_str".into(),
                    // Plan 70.3: distinct nova_char typedef (mirror emit_c.rs:2680).
                    "char" => "nova_char".into(),
                    // Plan 115 D214: nova_ptr distinct typedef (mirror
                    // emit_c.rs type_ref_to_c). External fn use this для
                    // ptr-parameter/return ABI.
                    "ptr" => "nova_ptr".into(),
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
                            // void* / empty — erased fallback. nova_ptr ОК.
                            if c.is_empty() || c == "void*" {
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
            TypeRef::Pointer(inner, _) => {
                let inner_c = Self::type_ref_to_c(inner, recv)?;
                Ok(format!("const {}*", inner_c))
            }
            TypeRef::Mut(inner, _) | TypeRef::Unsafe(inner, _) => {
                if let TypeRef::Pointer(p_inner, _) = inner.as_ref() {
                    let p_inner_c = Self::type_ref_to_c(p_inner, recv)?;
                    Ok(format!("{}*", p_inner_c))
                } else {
                    Self::type_ref_to_c(inner, recv)
                }
            }
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
