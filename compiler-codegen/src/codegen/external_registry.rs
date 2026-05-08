//! Plan 12: builtins.nv-driven external dispatch registry.
//!
//! Hard-coded таблицы для StringBuilder/WriteBuffer/ReadBuffer/str.from(char)
//! заменяются автоматическим выводом из AST `std/runtime/builtins.nv`.
//! Single source of truth — `.nv` декларации; codegen применяет mangling
//! и Nova→C type mapping, никаких ручных таблиц.
//!
//! См. spec/decisions/08-runtime.md → D82 (extended), Plan 12.

use crate::ast::{FnBody, FnDecl, Item, Module, Receiver, ReceiverKind, TypeRef};
use std::collections::HashMap;

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
}

impl ExternalRegistry {
    /// Builtins source — embedded в binary через include_str!. Это
    /// гарантирует что codegen и `std/runtime/builtins.nv` синхронны
    /// без зависимости от файловой системы.
    pub const BUILTINS_SRC: &'static str = include_str!("../../../std/runtime/builtins.nv");

    /// Парсит `builtins.nv` и строит registry. Вызывается один раз
    /// при инициализации CEmitter.
    ///
    /// Plan 13: runtime_registry (str/math) загружается **отдельно** через
    /// `emit-runtime-stubs` для генерации `std/runtime/*.nv`. Loading в
    /// codegen-runtime registry (с merge'ингом сюда) — следующий шаг
    /// Plan 13 Ф.4 (отдельная итерация после full migration special-case
    /// dispatch'ей).
    pub fn load_builtins() -> Result<Self, String> {
        let module = crate::parser::parse(Self::BUILTINS_SRC)
            .map_err(|d| format!("failed to parse builtins.nv: {}", d.message))?;
        Self::from_module(&module)
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
        Ok(reg)
    }

    fn decl_from_fn(f: &FnDecl, total_overloads: usize) -> Result<ExternalDecl, String> {
        let (recv_type_name, is_instance, is_mut_recv) = match &f.receiver {
            Some(Receiver { type_name, kind, mutable, .. }) => {
                let inst = matches!(kind, ReceiverKind::Instance);
                (Some(type_name.clone()), inst, *mutable)
            }
            None => (None, false, false),
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
        let base_c = match (&recv_type_name, is_instance) {
            (Some(rt), true)  => format!("Nova_{}_method_{}", rt, f.name),
            (Some(rt), false) => format!("Nova_{}_static_{}", rt, f.name),
            (None, _)         => format!("nova_fn_{}", f.name),
        };
        // Suffix builder: использует Nova-type первого параметра (path[0]).
        // []byte → "bytes", char → "char", str → "str", etc.
        let suffix = if !f.params.is_empty() {
            match &f.params[0].ty {
                TypeRef::Named { path, .. } if path.len() == 1 => path[0].clone(),
                TypeRef::Array(inner, _) => match inner.as_ref() {
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
        Ok(ExternalDecl {
            name: f.name.clone(),
            receiver_type: recv_type_name,
            is_instance,
            is_mut_receiver: is_mut_recv,
            param_c_types,
            param_names,
            return_c_type,
            c_name,
        })
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
                    "u32" => "uint32_t".into(),
                    "u16" => "uint16_t".into(),
                    "u8"  => "uint8_t".into(),
                    "f64" => "nova_f64".into(),
                    "f32" => "nova_f32".into(),
                    "bool" => "nova_bool".into(),
                    "str" => "nova_str".into(),
                    "byte" => "nova_byte".into(),
                    "char" => "nova_int".into(),
                    "Self" => match recv {
                        Some("str") => "nova_str".into(),
                        Some(t) => format!("Nova_{}*", t),
                        None => return Err("Self in non-receiver context".into()),
                    },
                    "Result" => {
                        let _ = generics;
                        "Nova_Result*".into()
                    }
                    "Option" => "NovaOpt_nova_int".into(),
                    _ => format!("Nova_{}*", name),
                })
            }
            TypeRef::Unit(_) => Ok("nova_unit".into()),
            TypeRef::Array(inner, _) => {
                if let TypeRef::Named { path, .. } = inner.as_ref() {
                    if path.len() == 1 {
                        return Ok(match path[0].as_str() {
                            "str" => "NovaArray_nova_str*".into(),
                            "byte" | "u8" => "NovaArray_nova_byte*".into(),
                            "bool" => "NovaArray_nova_bool*".into(),
                            "f64" | "f32" => "NovaArray_nova_f64*".into(),
                            _ => "NovaArray_nova_int*".into(),
                        });
                    }
                }
                Ok("NovaArray_nova_int*".into())
            }
            TypeRef::Tuple(elems, _) => Ok(format!("_NovaTuple{}", elems.len())),
            TypeRef::Func { .. } => Ok("void*".into()),
            TypeRef::FixedArray(_, inner, _) => Self::type_ref_to_c(inner, recv),
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
