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
    v.extend(char_runtime());
    v.extend(string_builder_runtime());
    v.extend(write_buffer_runtime());
    v.extend(read_buffer_runtime());
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
            name: "eq",
            params: &[("other", "str")],
            return_ty: "bool",
            effects: &[],
            c_name: "nova_str_eq",
            doc: "Равенство по контенту (memcmp). O(min). Также вызывается оператором ==.",
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

    // ─── f32 — Plan 13 Ф.8.2 ───
    // Те же функции что f64, но через C `f`-suffixed (sqrtf, sinf, etc.).
    // C-имена соответствуют стандартному <math.h> single-precision API.
    let f32_simple: Vec<(&'static str, &'static str, &'static str)> = vec![
        ("sqrt",  "sqrtf",  "Квадратный корень (single precision)."),
        ("cbrt",  "cbrtf",  "Кубический корень (single precision)."),
        ("abs",   "fabsf",  "Модуль |x| (single precision)."),
        ("ceil",  "ceilf",  "Округление вверх (single precision)."),
        ("floor", "floorf", "Округление вниз (single precision)."),
        ("round", "roundf", "Округление до ближайшего (single precision)."),
        ("trunc", "truncf", "Truncate (single precision)."),
        ("sin",   "sinf",   "Синус radians (single precision)."),
        ("cos",   "cosf",   "Косинус radians (single precision)."),
        ("tan",   "tanf",   "Тангенс radians (single precision)."),
        ("asin",  "asinf",  "Арксинус (single precision)."),
        ("acos",  "acosf",  "Арккосинус (single precision)."),
        ("atan",  "atanf",  "Арктангенс (single precision)."),
        ("sinh",  "sinhf",  "Гиперболический синус (single precision)."),
        ("cosh",  "coshf",  "Гиперболический косинус (single precision)."),
        ("tanh",  "tanhf",  "Гиперболический тангенс (single precision)."),
        ("exp",   "expf",   "e^x (single precision)."),
        ("exp2",  "exp2f",  "2^x (single precision)."),
        ("ln",    "logf",   "Натуральный log (single precision)."),
        ("log2",  "log2f",  "Log2 (single precision)."),
        ("log10", "log10f", "Log10 (single precision)."),
    ];
    for (name, c_name, doc) in &f32_simple {
        v.push(RuntimeFn {
            module: "std.runtime.math",
            receiver: Some("f32"),
            is_static: false, is_mut: false,
            name,
            params: &[],
            return_ty: "f32",
            effects: &[],
            c_name,
            doc,
        });
    }
    // f32 двух-аргументные.
    v.push(RuntimeFn {
        module: "std.runtime.math",
        receiver: Some("f32"),
        is_static: false, is_mut: false,
        name: "atan2",
        params: &[("x", "f32")],
        return_ty: "f32",
        effects: &[],
        c_name: "atan2f",
        doc: "atan2 (single precision).",
    });
    v.push(RuntimeFn {
        module: "std.runtime.math",
        receiver: Some("f32"),
        is_static: false, is_mut: false,
        name: "pow",
        params: &[("exp", "f32")],
        return_ty: "f32",
        effects: &[],
        c_name: "powf",
        doc: "self^exp (single precision).",
    });
    v.push(RuntimeFn {
        module: "std.runtime.math",
        receiver: Some("f32"),
        is_static: false, is_mut: false,
        name: "hypot",
        params: &[("y", "f32")],
        return_ty: "f32",
        effects: &[],
        c_name: "hypotf",
        doc: "hypot (single precision).",
    });
    // f32 predicates: isnan/isfinite/isinf — type-generic в C99 macros,
    // те же имена.
    v.push(RuntimeFn {
        module: "std.runtime.math",
        receiver: Some("f32"),
        is_static: false, is_mut: false,
        name: "is_nan",
        params: &[],
        return_ty: "bool",
        effects: &[],
        c_name: "isnan",
        doc: "True если NaN (single precision).",
    });
    v.push(RuntimeFn {
        module: "std.runtime.math",
        receiver: Some("f32"),
        is_static: false, is_mut: false,
        name: "is_finite",
        params: &[],
        return_ty: "bool",
        effects: &[],
        c_name: "isfinite",
        doc: "True если не ±∞ и не NaN (single precision).",
    });
    v.push(RuntimeFn {
        module: "std.runtime.math",
        receiver: Some("f32"),
        is_static: false, is_mut: false,
        name: "is_infinite",
        params: &[],
        return_ty: "bool",
        effects: &[],
        c_name: "isinf",
        doc: "True если ±∞ (single precision).",
    });
    v
}

/// `std.runtime.char` — char ↔ str (UTF-8 encode/decode).
fn char_runtime() -> Vec<RuntimeFn> {
    vec![
        RuntimeFn {
            module: "std.runtime.char",
            receiver: Some("str"),
            is_static: true, is_mut: false,
            name: "from",
            params: &[("c", "char")],
            return_ty: "str",
            effects: &[],
            c_name: "Nova_str_static_from_char",
            doc: "UTF-8 encode codepoint в 1-4 байта (D73 auto-derive: char.into() -> str).",
        },
    ]
}

/// `std.runtime.string_builder` — UTF-8 string accumulator (Plan 04).
fn string_builder_runtime() -> Vec<RuntimeFn> {
    let m = "std.runtime.string_builder";
    let recv = Some("StringBuilder");
    vec![
        RuntimeFn { module: m, receiver: recv, is_static: true,  is_mut: false,
            name: "new", params: &[], return_ty: "StringBuilder", effects: &[],
            c_name: "Nova_StringBuilder_static_new",
            doc: "Создать пустой StringBuilder с initial capacity 16." },
        RuntimeFn { module: m, receiver: recv, is_static: true,  is_mut: false,
            name: "with_capacity", params: &[("n", "int")], return_ty: "StringBuilder", effects: &[],
            c_name: "Nova_StringBuilder_static_with_capacity",
            doc: "Создать StringBuilder с pre-allocated capacity n." },
        RuntimeFn { module: m, receiver: recv, is_static: true,  is_mut: false,
            name: "from", params: &[("s", "str")], return_ty: "StringBuilder", effects: &[],
            c_name: "Nova_StringBuilder_static_from_str",
            doc: "Создать StringBuilder из существующей строки (copy)." },
        RuntimeFn { module: m, receiver: recv, is_static: true,  is_mut: false,
            name: "from", params: &[("c", "char")], return_ty: "StringBuilder", effects: &[],
            c_name: "Nova_StringBuilder_static_from_char",
            doc: "Создать StringBuilder из одного codepoint (UTF-8 encode 1-4 байта)." },
        RuntimeFn { module: m, receiver: recv, is_static: false, is_mut: true,
            name: "append", params: &[("s", "str")], return_ty: "()", effects: &[],
            c_name: "Nova_StringBuilder_method_append_str",
            doc: "Append UTF-8 bytes из str. Append-only, capacity grow 2x." },
        RuntimeFn { module: m, receiver: recv, is_static: false, is_mut: true,
            name: "append", params: &[("c", "char")], return_ty: "()", effects: &[],
            c_name: "Nova_StringBuilder_method_append_char",
            doc: "Append codepoint как UTF-8 (1-4 байта)." },
        RuntimeFn { module: m, receiver: recv, is_static: false, is_mut: false,
            name: "len", params: &[], return_ty: "int", effects: &[],
            c_name: "Nova_StringBuilder_method_len",
            doc: "Текущий размер в байтах (UTF-8 байты, не codepoint'ы)." },
        RuntimeFn { module: m, receiver: recv, is_static: false, is_mut: false,
            name: "capacity", params: &[], return_ty: "int", effects: &[],
            c_name: "Nova_StringBuilder_method_capacity",
            doc: "Allocated capacity в байтах (>= len)." },
        RuntimeFn { module: m, receiver: recv, is_static: false, is_mut: false,
            name: "clone", params: &[], return_ty: "StringBuilder", effects: &[],
            c_name: "Nova_StringBuilder_method_clone",
            doc: "Создать независимую копию (deep copy buffer)." },
        RuntimeFn { module: m, receiver: recv, is_static: false, is_mut: false,
            name: "into", params: &[], return_ty: "str", effects: &[],
            c_name: "Nova_StringBuilder_method_into",
            doc: "Финализировать в str. Infallible (UTF-8 invariant поддерживается append'ами). После consume mutating методы → runtime panic." },
    ]
}

/// `std.runtime.write_buffer` — binary serialization buffer (Plan 04).
fn write_buffer_runtime() -> Vec<RuntimeFn> {
    let m = "std.runtime.write_buffer";
    let recv = Some("WriteBuffer");
    let mut v: Vec<RuntimeFn> = Vec::new();
    // Создание.
    v.push(RuntimeFn { module: m, receiver: recv, is_static: true, is_mut: false,
        name: "new", params: &[], return_ty: "WriteBuffer", effects: &[],
        c_name: "Nova_WriteBuffer_static_new",
        doc: "Создать пустой WriteBuffer." });
    v.push(RuntimeFn { module: m, receiver: recv, is_static: true, is_mut: false,
        name: "with_capacity", params: &[("n", "int")], return_ty: "WriteBuffer", effects: &[],
        c_name: "Nova_WriteBuffer_static_with_capacity",
        doc: "Создать WriteBuffer с pre-allocated capacity." });
    v.push(RuntimeFn { module: m, receiver: recv, is_static: true, is_mut: false,
        name: "from", params: &[("b", "[]byte")], return_ty: "WriteBuffer", effects: &[],
        c_name: "Nova_WriteBuffer_static_from",
        doc: "Создать WriteBuffer из существующих байт." });
    // Базовые write.
    v.push(RuntimeFn { module: m, receiver: recv, is_static: false, is_mut: true,
        name: "write_byte", params: &[("v", "byte")], return_ty: "()", effects: &[],
        c_name: "Nova_WriteBuffer_method_write_byte",
        doc: "Append один byte." });
    v.push(RuntimeFn { module: m, receiver: recv, is_static: false, is_mut: true,
        name: "write_bytes", params: &[("src", "[]byte")], return_ty: "()", effects: &[],
        c_name: "Nova_WriteBuffer_method_write_bytes",
        doc: "Append массив байт (memcpy)." });
    v.push(RuntimeFn { module: m, receiver: recv, is_static: false, is_mut: true,
        name: "write_zero", params: &[("n", "int")], return_ty: "()", effects: &[],
        c_name: "Nova_WriteBuffer_method_write_zero",
        doc: "Append n нулевых байт (memset)." });
    // Text → UTF-8 bytes (Plan 04 Этап 6.1).
    v.push(RuntimeFn { module: m, receiver: recv, is_static: false, is_mut: true,
        name: "write_char", params: &[("c", "char")], return_ty: "()", effects: &[],
        c_name: "Nova_WriteBuffer_method_write_char",
        doc: "UTF-8 encode codepoint (1-4 байта). Mixed text+binary." });
    v.push(RuntimeFn { module: m, receiver: recv, is_static: false, is_mut: true,
        name: "write_str", params: &[("s", "str")], return_ty: "()", effects: &[],
        c_name: "Nova_WriteBuffer_method_write_str",
        doc: "Append UTF-8 байты из str (memcpy)." });
    // 18 numeric × LE/BE.
    let numeric_specs: Vec<(&'static str, &'static str, &'static str)> = vec![
        ("write_u8",     "u8",  "1 byte unsigned, без endianness."),
        ("write_i8",     "i8",  "1 byte signed, без endianness."),
        ("write_u16_le", "u16", "u16 little-endian (2 байта)."),
        ("write_u16_be", "u16", "u16 big-endian (2 байта)."),
        ("write_u32_le", "u32", "u32 little-endian (4 байта)."),
        ("write_u32_be", "u32", "u32 big-endian (4 байта)."),
        ("write_u64_le", "u64", "u64 little-endian (8 байт)."),
        ("write_u64_be", "u64", "u64 big-endian (8 байт)."),
        ("write_i16_le", "i16", "i16 little-endian (2 байта)."),
        ("write_i16_be", "i16", "i16 big-endian (2 байта)."),
        ("write_i32_le", "i32", "i32 little-endian (4 байта)."),
        ("write_i32_be", "i32", "i32 big-endian (4 байта)."),
        ("write_i64_le", "i64", "i64 little-endian (8 байт)."),
        ("write_i64_be", "i64", "i64 big-endian (8 байт)."),
        ("write_f32_le", "f32", "f32 little-endian IEEE 754."),
        ("write_f32_be", "f32", "f32 big-endian IEEE 754."),
        ("write_f64_le", "f64", "f64 little-endian IEEE 754."),
        ("write_f64_be", "f64", "f64 big-endian IEEE 754."),
    ];
    for (name, arg_ty, doc) in &numeric_specs {
        // c_name с нашим mangling'ом одного нумерик-аргумента.
        let c_name_static: &'static str = Box::leak(
            format!("Nova_WriteBuffer_method_{}", name).into_boxed_str(),
        );
        let params_static: &'static [(&'static str, &'static str)] = Box::leak(Box::new([("v", *arg_ty)]));
        v.push(RuntimeFn { module: m, receiver: recv, is_static: false, is_mut: true,
            name, params: params_static, return_ty: "()", effects: &[],
            c_name: c_name_static, doc });
    }
    // Финализация / read-only.
    v.push(RuntimeFn { module: m, receiver: recv, is_static: false, is_mut: false,
        name: "len", params: &[], return_ty: "int", effects: &[],
        c_name: "Nova_WriteBuffer_method_len",
        doc: "Текущий размер в байтах." });
    v.push(RuntimeFn { module: m, receiver: recv, is_static: false, is_mut: false,
        name: "capacity", params: &[], return_ty: "int", effects: &[],
        c_name: "Nova_WriteBuffer_method_capacity",
        doc: "Allocated capacity в байтах." });
    v.push(RuntimeFn { module: m, receiver: recv, is_static: false, is_mut: false,
        name: "clone", params: &[], return_ty: "WriteBuffer", effects: &[],
        c_name: "Nova_WriteBuffer_method_clone",
        doc: "Создать независимую копию (deep copy buffer)." });
    v.push(RuntimeFn { module: m, receiver: recv, is_static: false, is_mut: false,
        name: "into", params: &[], return_ty: "[]byte", effects: &[],
        c_name: "Nova_WriteBuffer_method_into",
        doc: "Финализировать в []byte. Infallible." });
    v
}

/// `std.runtime.read_buffer` — cursor-style binary reader (Plan 04).
/// Pair `@read_*` (Fail-form) / `@try_read_*` (Result-form) — auto-derive
/// на C-runtime уровне (одна C-функция, две Nova-обёртки).
fn read_buffer_runtime() -> Vec<RuntimeFn> {
    let m = "std.runtime.read_buffer";
    let recv = Some("ReadBuffer");
    let mut v: Vec<RuntimeFn> = Vec::new();
    // Создание.
    v.push(RuntimeFn { module: m, receiver: recv, is_static: true, is_mut: false,
        name: "from", params: &[("b", "[]byte")], return_ty: "ReadBuffer", effects: &[],
        c_name: "Nova_ReadBuffer_static_from",
        doc: "Создать ReadBuffer из []byte (view, no copy)." });
    // Cursor metadata (read-only).
    v.push(RuntimeFn { module: m, receiver: recv, is_static: false, is_mut: false,
        name: "position", params: &[], return_ty: "int", effects: &[],
        c_name: "Nova_ReadBuffer_method_position",
        doc: "Текущий offset cursor'а в байтах." });
    v.push(RuntimeFn { module: m, receiver: recv, is_static: false, is_mut: false,
        name: "remaining", params: &[], return_ty: "int", effects: &[],
        c_name: "Nova_ReadBuffer_method_remaining",
        doc: "Сколько байт осталось до конца буфера." });
    v.push(RuntimeFn { module: m, receiver: recv, is_static: false, is_mut: false,
        name: "has_remaining", params: &[("n", "int")], return_ty: "bool", effects: &[],
        c_name: "Nova_ReadBuffer_method_has_remaining",
        doc: "True если осталось ≥ n байт." });
    v.push(RuntimeFn { module: m, receiver: recv, is_static: false, is_mut: false,
        name: "remaining_bytes", params: &[], return_ty: "[]byte", effects: &[],
        c_name: "Nova_ReadBuffer_method_remaining_bytes",
        doc: "Скопировать оставшиеся байты как []byte." });
    // 18 numeric × LE/BE × Fail-form / Try-form.
    let read_specs: Vec<(&'static str, &'static str, &'static str)> = vec![
        ("read_byte",  "byte", "1 byte."),
        ("read_bytes", "[]byte", "n байт (с n int параметром)."),
        ("read_u8",    "u8",  "1 byte unsigned."),
        ("read_i8",    "i8",  "1 byte signed."),
        ("read_u16_le","u16", "u16 little-endian."),
        ("read_u16_be","u16", "u16 big-endian."),
        ("read_u32_le","u32", "u32 little-endian."),
        ("read_u32_be","u32", "u32 big-endian."),
        ("read_u64_le","u64", "u64 little-endian."),
        ("read_u64_be","u64", "u64 big-endian."),
        ("read_i16_le","i16", "i16 little-endian."),
        ("read_i16_be","i16", "i16 big-endian."),
        ("read_i32_le","i32", "i32 little-endian."),
        ("read_i32_be","i32", "i32 big-endian."),
        ("read_i64_le","i64", "i64 little-endian."),
        ("read_i64_be","i64", "i64 big-endian."),
        ("read_f32_le","f32", "f32 little-endian IEEE 754."),
        ("read_f32_be","f32", "f32 big-endian IEEE 754."),
        ("read_f64_le","f64", "f64 little-endian IEEE 754."),
        ("read_f64_be","f64", "f64 big-endian IEEE 754."),
    ];
    // Fail-form (Fail[ReadBufferError]) и try-form (Result[T, ReadBufferError]).
    for (name, ret_ty, doc) in &read_specs {
        let c_name_fail: &'static str = Box::leak(
            format!("Nova_ReadBuffer_method_{}", name).into_boxed_str(),
        );
        let try_name: &'static str = Box::leak(format!("try_{}", name).into_boxed_str());
        let c_name_try: &'static str = Box::leak(
            format!("Nova_ReadBuffer_method_try_{}", name).into_boxed_str(),
        );
        let try_ret: &'static str = Box::leak(
            format!("Result[{}, ReadBufferError]", ret_ty).into_boxed_str(),
        );
        // read_bytes имеет int параметр; остальные — без параметров.
        let (params_fail, params_try): (&'static [(&'static str, &'static str)], &'static [(&'static str, &'static str)]) =
            if *name == "read_bytes" {
                (Box::leak(Box::new([("n", "int")])) as _,
                 Box::leak(Box::new([("n", "int")])) as _)
            } else {
                (&[], &[])
            };
        let doc_fail: &'static str = Box::leak(
            format!("{} Throw'ит ReadBufferError при недостатке байт.", doc).into_boxed_str(),
        );
        let doc_try: &'static str = Box::leak(
            format!("{} Result-форма (Ok(value) или Err(UnexpectedEnd)).", doc).into_boxed_str(),
        );
        v.push(RuntimeFn { module: m, receiver: recv, is_static: false, is_mut: true,
            name, params: params_fail, return_ty: ret_ty, effects: &["Fail[ReadBufferError]"],
            c_name: c_name_fail, doc: doc_fail });
        v.push(RuntimeFn { module: m, receiver: recv, is_static: false, is_mut: true,
            name: try_name, params: params_try, return_ty: try_ret, effects: &[],
            c_name: c_name_try, doc: doc_try });
    }
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
        // Static: `Type.method` (точка без пробела) — D35 convention.
        // Instance: `Type [mut] @method` (с пробелом перед @, mut между ними).
        out.push_str("export external fn ");
        if let Some(recv) = f.receiver {
            out.push_str(recv);
            if f.is_static {
                // No space before dot.
                out.push('.');
            } else {
                out.push(' ');
                if f.is_mut { out.push_str("mut "); }
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
        out.push_str("\n\n");
    }
    out
}
