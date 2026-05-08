//! Plan 13: единый реестр runtime-функций которые знает компилятор.
//!
//! Используется для auto-gen `std/runtime/string.nv` и `std/runtime/math.nv`
//! (Plan 13 Ф.3). После migration (Ф.4) вызовы этих функций пойдут через
//! общий builtins.nv-driven dispatch (Plan 12).
//!
//! Single source of truth для **что компилятор реально знает**:
//! - str API (UTF-8 операции).
//! - f64 / f32 math (D74 instance-методы).
//!
//! См. docs/plans/13-runtime-stdlib-and-autogen.md.

/// Описание одной runtime-функции.
#[derive(Debug, Clone)]
pub struct RuntimeFn {
    /// Module path: `"std.runtime.string"`, `"std.runtime.math"`.
    pub module: &'static str,
    /// Receiver type (`Some("str")` для `s.find(...)`, `None` для freefn).
    pub receiver: Option<&'static str>,
    /// `T.method(args)` (static) vs `t.method(args)` (instance).
    pub is_static: bool,
    /// `mut` receiver (`fn T mut @method`).
    pub is_mut: bool,
    /// Method name (без receiver-префикса).
    pub name: &'static str,
    /// Параметры (без receiver'а): `(name, nova_type_name)`.
    pub params: &'static [(&'static str, &'static str)],
    /// Nova return type (`"str"`, `"f64"`, `"bool"`, `"int"`, `"[]byte"`,
    /// `"Option[int]"`, `"Iter[char]"`, etc.).
    pub return_ty: &'static str,
    /// Effects (`"Fail[Error]"` etc.). Пустой массив для total functions.
    pub effects: &'static [&'static str],
    /// Реальное C-имя функции в `nova_rt/`. Plan 12 mangling использует
    /// `Nova_T_method_X`, но legacy str/math используют `nova_str_X`,
    /// `sin`, `cos` etc. Registry хранит **фактическое** имя.
    pub c_name: &'static str,
    /// Doc comment для generated `.nv`.
    pub doc: &'static str,
}

/// Полный реестр runtime-функций. Stable order: by module → by receiver →
/// by name. Acceptance-test depends on this for детерминизма auto-gen.
pub fn all() -> Vec<RuntimeFn> {
    let mut v = Vec::new();
    v.extend(str_runtime());
    v.extend(math_runtime());
    v
}

/// `std.runtime.string` — UTF-8 операции на str.
fn str_runtime() -> Vec<RuntimeFn> {
    vec![
        RuntimeFn {
            module: "std.runtime.string",
            receiver: Some("str"),
            is_static: false, is_mut: false,
            name: "char_len",
            params: &[],
            return_ty: "int",
            effects: &[],
            c_name: "nova_str_char_len",
            doc: "Длина строки в codepoint'ах (D26 школа B). O(n) — обходит UTF-8.",
        },
        RuntimeFn {
            module: "std.runtime.string",
            receiver: Some("str"),
            is_static: false, is_mut: false,
            name: "byte_len",
            params: &[],
            return_ty: "int",
            effects: &[],
            c_name: "nova_str_byte_len",
            doc: "Длина в байтах. O(1). Для FFI / буферных операций.",
        },
        RuntimeFn {
            module: "std.runtime.string",
            receiver: Some("str"),
            is_static: false, is_mut: false,
            name: "is_empty",
            params: &[],
            return_ty: "bool",
            effects: &[],
            c_name: "nova_str_is_empty",
            doc: "True если byte_len == 0.",
        },
        RuntimeFn {
            module: "std.runtime.string",
            receiver: Some("str"),
            is_static: false, is_mut: false,
            name: "starts_with",
            params: &[("prefix", "str")],
            return_ty: "bool",
            effects: &[],
            c_name: "nova_str_starts_with",
            doc: "True если строка начинается с prefix.",
        },
        RuntimeFn {
            module: "std.runtime.string",
            receiver: Some("str"),
            is_static: false, is_mut: false,
            name: "ends_with",
            params: &[("suffix", "str")],
            return_ty: "bool",
            effects: &[],
            c_name: "nova_str_ends_with",
            doc: "True если строка заканчивается на suffix.",
        },
        RuntimeFn {
            module: "std.runtime.string",
            receiver: Some("str"),
            is_static: false, is_mut: false,
            name: "contains",
            params: &[("needle", "str")],
            return_ty: "bool",
            effects: &[],
            c_name: "nova_str_contains",
            doc: "True если needle встречается в строке.",
        },
        RuntimeFn {
            module: "std.runtime.string",
            receiver: Some("str"),
            is_static: false, is_mut: false,
            name: "find",
            params: &[("needle", "str")],
            return_ty: "Option[int]",
            effects: &[],
            c_name: "nova_str_find",
            doc: "Codepoint-offset первого вхождения needle. None если нет.",
        },
        RuntimeFn {
            module: "std.runtime.string",
            receiver: Some("str"),
            is_static: false, is_mut: false,
            name: "rfind",
            params: &[("needle", "str")],
            return_ty: "Option[int]",
            effects: &[],
            c_name: "nova_str_rfind",
            doc: "Codepoint-offset последнего вхождения needle.",
        },
        RuntimeFn {
            module: "std.runtime.string",
            receiver: Some("str"),
            is_static: false, is_mut: false,
            name: "char_at",
            params: &[("idx", "int")],
            return_ty: "Option[int]",
            effects: &[],
            c_name: "nova_str_char_at",
            doc: "Codepoint по индексу (codepoint-indexed). None при OOB.",
        },
        RuntimeFn {
            module: "std.runtime.string",
            receiver: Some("str"),
            is_static: false, is_mut: false,
            name: "slice",
            params: &[("from", "int"), ("to", "int")],
            return_ty: "str",
            effects: &[],
            c_name: "nova_str_slice",
            doc: "Codepoint-indexed slice (D26 школа B).",
        },
        RuntimeFn {
            module: "std.runtime.string",
            receiver: Some("str"),
            is_static: false, is_mut: false,
            name: "trim",
            params: &[],
            return_ty: "str",
            effects: &[],
            c_name: "nova_str_trim",
            doc: "Убирает whitespace с начала и конца.",
        },
        RuntimeFn {
            module: "std.runtime.string",
            receiver: Some("str"),
            is_static: false, is_mut: false,
            name: "to_lower",
            params: &[],
            return_ty: "str",
            effects: &[],
            c_name: "nova_str_to_lower",
            doc: "Lowercase копия (ASCII).",
        },
        RuntimeFn {
            module: "std.runtime.string",
            receiver: Some("str"),
            is_static: false, is_mut: false,
            name: "to_upper",
            params: &[],
            return_ty: "str",
            effects: &[],
            c_name: "nova_str_to_upper",
            doc: "Uppercase копия (ASCII).",
        },
        RuntimeFn {
            module: "std.runtime.string",
            receiver: Some("str"),
            is_static: false, is_mut: false,
            name: "concat",
            params: &[("other", "str")],
            return_ty: "str",
            effects: &[],
            c_name: "nova_str_concat",
            doc: "Конкатенация двух строк (создаёт новую). O(a+b).",
        },
        RuntimeFn {
            module: "std.runtime.string",
            receiver: Some("str"),
            is_static: false, is_mut: false,
            name: "bytes",
            params: &[],
            return_ty: "[]byte",
            effects: &[],
            c_name: "nova_str_bytes",
            doc: "UTF-8 bytes как []byte (copy).",
        },
        RuntimeFn {
            module: "std.runtime.string",
            receiver: Some("str"),
            is_static: false, is_mut: false,
            name: "chars",
            params: &[],
            return_ty: "[]int",
            effects: &[],
            c_name: "nova_str_chars",
            doc: "Codepoints как []int (eager bootstrap; production будет Iter[char]).",
        },
        RuntimeFn {
            module: "std.runtime.string",
            receiver: Some("str"),
            is_static: false, is_mut: false,
            name: "split",
            params: &[("sep", "str")],
            return_ty: "[]str",
            effects: &[],
            c_name: "nova_str_split",
            doc: "Split по separator. Eager в bootstrap.",
        },
    ]
}

/// `std.runtime.math` — D74 instance-методы для f64 (и subset f32).
fn math_runtime() -> Vec<RuntimeFn> {
    let f64_fns: Vec<&'static str> = vec![
        "sqrt", "cbrt", "abs", "ceil", "floor", "round", "trunc",
        "sin", "cos", "tan", "asin", "acos", "atan",
        "sinh", "cosh", "tanh",
        "exp", "exp2", "ln", "log2", "log10",
    ];
    let mut v: Vec<RuntimeFn> = Vec::new();
    for name in &f64_fns {
        let c_name = match *name {
            "abs" => "fabs",       // C name отличается
            "ln"  => "log",        // C `log` это natural log
            other => other,
        };
        let doc: &'static str = match *name {
            "sqrt" => "Квадратный корень. NaN на отрицательном.",
            "cbrt" => "Кубический корень.",
            "abs"  => "Модуль (|x|).",
            "ceil" => "Округление вверх (toward +∞).",
            "floor"=> "Округление вниз (toward -∞).",
            "round"=> "Округление до ближайшего целого (half away from zero).",
            "trunc"=> "Отбрасывание дробной части (toward zero).",
            "sin"  => "Синус (radians).",
            "cos"  => "Косинус (radians).",
            "tan"  => "Тангенс (radians).",
            "asin" => "Арксинус. Result в [-π/2, π/2].",
            "acos" => "Арккосинус. Result в [0, π].",
            "atan" => "Арктангенс. Result в (-π/2, π/2).",
            "sinh" => "Гиперболический синус.",
            "cosh" => "Гиперболический косинус.",
            "tanh" => "Гиперболический тангенс.",
            "exp"  => "e^x.",
            "exp2" => "2^x.",
            "ln"   => "Натуральный log (по основанию e).",
            "log2" => "Log по основанию 2.",
            "log10"=> "Log по основанию 10.",
            _ => "",
        };
        // Leak имена через Box::leak для 'static lifetime.
        let c_name_static: &'static str = Box::leak(c_name.to_string().into_boxed_str());
        v.push(RuntimeFn {
            module: "std.runtime.math",
            receiver: Some("f64"),
            is_static: false, is_mut: false,
            name,
            params: &[],
            return_ty: "f64",
            effects: &[],
            c_name: c_name_static,
            doc,
        });
    }
    // Двух-аргументные f64 math.
    v.push(RuntimeFn {
        module: "std.runtime.math",
        receiver: Some("f64"),
        is_static: false, is_mut: false,
        name: "atan2",
        params: &[("x", "f64")],
        return_ty: "f64",
        effects: &[],
        c_name: "atan2",
        doc: "atan2(y, x) — angle от positive x-axis. Self = y.",
    });
    v.push(RuntimeFn {
        module: "std.runtime.math",
        receiver: Some("f64"),
        is_static: false, is_mut: false,
        name: "pow",
        params: &[("exp", "f64")],
        return_ty: "f64",
        effects: &[],
        c_name: "pow",
        doc: "self^exp.",
    });
    v.push(RuntimeFn {
        module: "std.runtime.math",
        receiver: Some("f64"),
        is_static: false, is_mut: false,
        name: "hypot",
        params: &[("y", "f64")],
        return_ty: "f64",
        effects: &[],
        c_name: "hypot",
        doc: "sqrt(self^2 + y^2) без overflow.",
    });
    // Predicate methods (return bool).
    v.push(RuntimeFn {
        module: "std.runtime.math",
        receiver: Some("f64"),
        is_static: false, is_mut: false,
        name: "is_nan",
        params: &[],
        return_ty: "bool",
        effects: &[],
        c_name: "isnan",
        doc: "True если NaN.",
    });
    v.push(RuntimeFn {
        module: "std.runtime.math",
        receiver: Some("f64"),
        is_static: false, is_mut: false,
        name: "is_finite",
        params: &[],
        return_ty: "bool",
        effects: &[],
        c_name: "isfinite",
        doc: "True если не ±∞ и не NaN.",
    });
    v.push(RuntimeFn {
        module: "std.runtime.math",
        receiver: Some("f64"),
        is_static: false, is_mut: false,
        name: "is_infinite",
        params: &[],
        return_ty: "bool",
        effects: &[],
        c_name: "isinf",
        doc: "True если ±∞.",
    });
    v
}

/// Group registry by module path. Stable ordering preserved.
pub fn group_by_module(reg: &[RuntimeFn]) -> Vec<(&'static str, Vec<&RuntimeFn>)> {
    let mut groups: Vec<(&'static str, Vec<&RuntimeFn>)> = Vec::new();
    for f in reg {
        if let Some(last) = groups.last_mut() {
            if last.0 == f.module {
                last.1.push(f);
                continue;
            }
        }
        groups.push((f.module, vec![f]));
    }
    groups
}

/// Convert module path `std.runtime.math` → file path `std/runtime/math.nv`.
pub fn module_to_path(module: &str) -> String {
    format!("{}.nv", module.replace('.', "/"))
}

/// Render single .nv file content for a module.
pub fn render_nv(module: &str, fns: &[&RuntimeFn]) -> String {
    let mut out = String::new();
    out.push_str("// AUTO-GENERATED by `nova-codegen emit-runtime-stubs`.\n");
    out.push_str("// Do not edit manually — changes will be overwritten.\n");
    out.push_str("// Source of truth: compiler-codegen/src/codegen/runtime_registry.rs\n");
    out.push_str("//\n");
    out.push_str("// См. docs/plans/13-runtime-stdlib-and-autogen.md.\n");
    out.push('\n');
    out.push_str(&format!("module {}\n", module));
    out.push('\n');
    out.push('\n');
    let mut last_recv: Option<&str> = None;
    for f in fns {
        if last_recv != f.receiver {
            if last_recv.is_some() { out.push('\n'); }
            if let Some(r) = f.receiver {
                out.push_str(&format!("// ─── {} ───\n\n", r));
            }
            last_recv = f.receiver;
        }
        // doc-comment.
        if !f.doc.is_empty() {
            out.push_str(&format!("// {}\n", f.doc));
        }
        // signature.
        out.push_str("export external fn ");
        if let Some(recv) = f.receiver {
            out.push_str(recv);
            out.push(' ');
            if f.is_mut { out.push_str("mut "); }
            if f.is_static {
                out.push('.');
            } else {
                out.push('@');
            }
            out.push_str(f.name);
        } else {
            out.push_str(f.name);
        }
        out.push('(');
        let parts: Vec<String> = f.params.iter()
            .map(|(n, ty)| format!("{} {}", n, ty))
            .collect();
        out.push_str(&parts.join(", "));
        out.push(')');
        // effects.
        for eff in f.effects {
            out.push(' ');
            out.push_str(eff);
        }
        // return.
        out.push_str(" -> ");
        out.push_str(f.return_ty);
        out.push('\n');
    }
    out
}
