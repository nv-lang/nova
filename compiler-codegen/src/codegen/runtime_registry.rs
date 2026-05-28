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
    /// Plan 73 (D131): `consume` receiver (`fn T consume @method`).
    /// После вызова такого метода переменная-источник инвалидируется.
    /// Взаимоисключающий с `is_mut`.
    pub is_consume: bool,
    /// Method name (без receiver-префикса).
    pub name: &'static str,
    /// Параметры (без receiver'а): `(name, nova_type_name)`.
    pub params: &'static [(&'static str, &'static str)],
    /// Nova return type (`"str"`, `"f64"`, `"bool"`, `"int"`, `"[]u8"`,
    /// `"Option[int]"`, `"Iter[char]"`, etc.).
    pub return_ty: &'static str,
    /// Effects (`"Fail[Error]"` etc.). Пустой массив для total functions.
    pub effects: &'static [&'static str],
    /// Реальное C-имя функции в `nova_rt/`. Plan 12 mangling использует
    /// `Nova_T_method_X`, но legacy str/math используют `nova_str_X`,
    /// `sin`, `cos` etc. Registry хранит **фактическое** имя.
    /// Для записей с `nova_body == Some(...)` (Nova-implemented method)
    /// `c_name` игнорируется (но обычно ставится в `""`).
    pub c_name: &'static str,
    /// Doc comment для generated `.nv`.
    pub doc: &'static str,
    /// Для записей **с body** (Nova-impl, не external):
    ///   `Some("@append(s)")` → `=> @append(s)` в emitted .nv.
    /// Для external — `None`. Plan 13 Ф.9.2.
    pub nova_body: Option<&'static str>,
}

/// Полный реестр runtime-функций. Stable order: by module → by receiver →
/// by name. Acceptance-test depends on this for детерминизма auto-gen.
pub fn all() -> Vec<RuntimeFn> {
    let mut v = Vec::new();
    v.extend(str_runtime());
    v.extend(math_runtime());
    v.extend(numeric_runtime());
    v.extend(char_runtime());
    v.extend(string_builder_runtime());
    v.extend(write_buffer_runtime());
    v.extend(read_buffer_runtime());
    v
}

/// `std.runtime.numeric` — Plan 74: IEEE 754 primitive bit-cast.
/// `f64 ↔ u64`, `f32 ↔ u32` reinterpret-cast. C-реализация —
/// `nova_rt/numeric.h` (memcpy-based, zero-cost). Codegen dispatch'ит
/// эти методы hard-coded (primitive-type methods, как D74 math), а не
/// через external_registry — registry-запись здесь даёт canonical
/// Nova-side декларацию для `nova doc` / IDE discovery.
fn numeric_runtime() -> Vec<RuntimeFn> {
    vec![
        RuntimeFn {
            module: "std.runtime.numeric",
            receiver: Some("f64"),
            is_static: false, is_mut: false, is_consume: false,
            name: "to_bits",
            params: &[],
            return_ty: "u64",
            effects: &[],
            c_name: "Nova_f64_to_bits",
            doc: "IEEE 754 bit-pattern double как u64 (reinterpret-cast).",
            nova_body: None,
        },
        RuntimeFn {
            module: "std.runtime.numeric",
            receiver: Some("f64"),
            is_static: true, is_mut: false, is_consume: false,
            name: "from_bits",
            params: &[("bits", "u64")],
            return_ty: "f64",
            effects: &[],
            c_name: "Nova_f64_from_bits",
            doc: "Восстановить f64 из IEEE 754 bit-pattern (reinterpret-cast).",
            nova_body: None,
        },
        RuntimeFn {
            module: "std.runtime.numeric",
            receiver: Some("f32"),
            is_static: false, is_mut: false, is_consume: false,
            name: "to_bits",
            params: &[],
            return_ty: "u32",
            effects: &[],
            c_name: "Nova_f32_to_bits",
            doc: "IEEE 754 bit-pattern float как u32 (reinterpret-cast).",
            nova_body: None,
        },
        RuntimeFn {
            module: "std.runtime.numeric",
            receiver: Some("f32"),
            is_static: true, is_mut: false, is_consume: false,
            name: "from_bits",
            params: &[("bits", "u32")],
            return_ty: "f32",
            effects: &[],
            c_name: "Nova_f32_from_bits",
            doc: "Восстановить f32 из IEEE 754 bit-pattern (reinterpret-cast).",
            nova_body: None,
        },
    ]
}

/// `std.runtime.string` — UTF-8 операции на str.
fn str_runtime() -> Vec<RuntimeFn> {
    vec![
        RuntimeFn {
            module: "std.runtime.string",
            receiver: Some("str"),
            is_static: false, is_mut: false, is_consume: false,
            name: "len",
            params: &[],
            return_ty: "int",
            effects: &[],
            c_name: "nova_str_byte_len",
            doc: "Длина строки в байтах. O(1). (Plan 108 D26 rev: len = bytes, char_len = codepoints).",
        nova_body: None,
    },
        RuntimeFn {
            module: "std.runtime.string",
            receiver: Some("str"),
            is_static: false, is_mut: false, is_consume: false,
            name: "char_len",
            params: &[],
            return_ty: "int",
            effects: &[],
            c_name: "nova_str_char_len",
            doc: "Длина строки в codepoint'ах. O(n). Используй для итерации по символам.",
        nova_body: None,
    },
        // Plan 90: O(1) доступ к байту — примитив для str-алгоритмов на Nova.
        RuntimeFn {
            module: "std.runtime.string",
            receiver: Some("str"),
            is_static: false, is_mut: false, is_consume: false,
            name: "byte_at",
            params: &[("i", "int")],
            return_ty: "u8",
            effects: &[],
            c_name: "nova_str_byte_at",
            doc: "UTF-8 байт по индексу. O(1). Panic при выходе за границы. Plan 90 — неустранимый примитив для byte-алгоритмов (lexer/find/trim) на Nova.",
            nova_body: None,
        },
        // Plan 75: @is_empty — логичный спутник @len; у всех коллекций есть, у str не было.
        RuntimeFn {
            module: "std.runtime.string",
            receiver: Some("str"),
            is_static: false, is_mut: false, is_consume: false,
            name: "is_empty",
            params: &[],
            return_ty: "bool",
            effects: &[],
            c_name: "",
            doc: "True если строка пустая. O(n) через @len (codepoints); для bootstrap приемлемо.",
            nova_body: Some("@len() == 0"),
        },
        RuntimeFn {
            module: "std.runtime.string",
            receiver: Some("str"),
            is_static: false, is_mut: false, is_consume: false,
            name: "starts_with",
            params: &[("prefix", "str")],
            return_ty: "bool",
            effects: &[],
            c_name: "nova_str_starts_with",
            doc: "True если строка начинается с prefix.",
        nova_body: None,
    },
        RuntimeFn {
            module: "std.runtime.string",
            receiver: Some("str"),
            is_static: false, is_mut: false, is_consume: false,
            name: "ends_with",
            params: &[("suffix", "str")],
            return_ty: "bool",
            effects: &[],
            c_name: "nova_str_ends_with",
            doc: "True если строка заканчивается на suffix.",
        nova_body: None,
    },
        RuntimeFn {
            module: "std.runtime.string",
            receiver: Some("str"),
            is_static: false, is_mut: false, is_consume: false,
            name: "contains",
            params: &[("needle", "str")],
            return_ty: "bool",
            effects: &[],
            c_name: "nova_str_contains",
            doc: "True если needle встречается в строке.",
        nova_body: None,
    },
        RuntimeFn {
            module: "std.runtime.string",
            receiver: Some("str"),
            is_static: false, is_mut: false, is_consume: false,
            name: "find",
            params: &[("needle", "str")],
            return_ty: "Option[int]",
            effects: &[],
            c_name: "nova_str_find",
            doc: "Codepoint-offset первого вхождения needle. None если нет.",
        nova_body: None,
    },
        RuntimeFn {
            module: "std.runtime.string",
            receiver: Some("str"),
            is_static: false, is_mut: false, is_consume: false,
            name: "rfind",
            params: &[("needle", "str")],
            return_ty: "Option[int]",
            effects: &[],
            c_name: "nova_str_rfind",
            doc: "Codepoint-offset последнего вхождения needle.",
        nova_body: None,
    },
        RuntimeFn {
            module: "std.runtime.string",
            receiver: Some("str"),
            is_static: false, is_mut: false, is_consume: false,
            name: "char_at",
            params: &[("idx", "int")],
            return_ty: "Option[char]",
            effects: &[],
            c_name: "nova_str_char_at",
            doc: "Codepoint по индексу (codepoint-indexed). None при OOB.",
        nova_body: None,
    },
        // Plan 96.1: метод @slice удалён в пользу bracket-формы s[a..b]
        // (Plan 96 D-str-slice, D9 «один очевидный путь»). Bracket-form
        // codegen — emit_c.rs ExprKind::Index с Range index → nova_str_slice_panic.
        RuntimeFn {
            module: "std.runtime.string",
            receiver: Some("str"),
            is_static: false, is_mut: false, is_consume: false,
            name: "trim",
            params: &[],
            return_ty: "str",
            effects: &[],
            c_name: "nova_str_trim",
            doc: "Убирает whitespace с начала и конца.",
        nova_body: None,
    },
        RuntimeFn {
            module: "std.runtime.string",
            receiver: Some("str"),
            is_static: false, is_mut: false, is_consume: false,
            name: "to_lower",
            params: &[],
            return_ty: "str",
            effects: &[],
            c_name: "nova_str_to_lower",
            doc: "Lowercase копия (ASCII).",
        nova_body: None,
    },
        RuntimeFn {
            module: "std.runtime.string",
            receiver: Some("str"),
            is_static: false, is_mut: false, is_consume: false,
            name: "to_upper",
            params: &[],
            return_ty: "str",
            effects: &[],
            c_name: "nova_str_to_upper",
            doc: "Uppercase копия (ASCII).",
        nova_body: None,
    },
        RuntimeFn {
            module: "std.runtime.string",
            receiver: Some("str"),
            is_static: false, is_mut: false, is_consume: false,
            name: "concat",
            params: &[("other", "str")],
            return_ty: "str",
            effects: &[],
            c_name: "nova_str_concat",
            doc: "Конкатенация двух строк (создаёт новую). O(a+b).",
            nova_body: None,
        },
        // Plan 13 Ф.9.2: оператор `+` через метод @plus.
        // Body delegates на @concat — общее правило routing'а в codegen
        // (D46 operator overload) превращает `s1 + s2` в `s1.@plus(s2)`.
        RuntimeFn {
            module: "std.runtime.string",
            receiver: Some("str"),
            is_static: false, is_mut: false, is_consume: false,
            name: "plus",
            params: &[("other", "str")],
            return_ty: "str",
            effects: &[],
            c_name: "",
            doc: "Оператор `+`: `s1 + s2 == s1.@plus(s2)` → @concat (D46).",
            nova_body: Some("@concat(other)"),
        },
        RuntimeFn {
            module: "std.runtime.string",
            receiver: Some("str"),
            is_static: false, is_mut: false, is_consume: false,
            name: "eq",
            params: &[("other", "str")],
            return_ty: "bool",
            effects: &[],
            c_name: "nova_str_eq",
            doc: "Равенство по контенту (memcmp). O(min). Также вызывается оператором ==.",
        nova_body: None,
    },
        // D109 (Plan 48 Ф.8): FNV-1a hash для str — ключ HashMap.
        // Codegen vector: `prim_builtin_method` dispatch перехватывает
        // вызов до общего resolver'а (emit_c.rs); declaration здесь —
        // для AI/IDE discovery + sanity-check.
        RuntimeFn {
            module: "std.runtime.string",
            receiver: Some("str"),
            is_static: false, is_mut: false, is_consume: false,
            name: "hash",
            params: &[],
            return_ty: "u64",
            effects: &[],
            c_name: "nova_str_hash",
            doc: "FNV-1a хеш по байтам строки. Используется в std.collections.HashMap.",
            nova_body: None,
        },
        // 2026-05-12: lex byte-wise compare для nova_str. Bootstrap MVP —
        // ASCII-correct; UTF-8 partial. Полное Unicode collation —
        // production milestone.
        RuntimeFn {
            module: "std.runtime.string",
            receiver: Some("str"),
            is_static: false, is_mut: false, is_consume: false,
            name: "lt",
            params: &[("other", "str")],
            return_ty: "bool",
            effects: &[],
            c_name: "nova_str_lt",
            doc: "Lexicographic less-than (byte-wise). Также вызывается оператором `<`.",
            nova_body: None,
        },
        RuntimeFn {
            module: "std.runtime.string",
            receiver: Some("str"),
            is_static: false, is_mut: false, is_consume: false,
            name: "le",
            params: &[("other", "str")],
            return_ty: "bool",
            effects: &[],
            c_name: "nova_str_le",
            doc: "Lexicographic less-or-equal. Также вызывается оператором `<=`.",
            nova_body: None,
        },
        RuntimeFn {
            module: "std.runtime.string",
            receiver: Some("str"),
            is_static: false, is_mut: false, is_consume: false,
            name: "gt",
            params: &[("other", "str")],
            return_ty: "bool",
            effects: &[],
            c_name: "nova_str_gt",
            doc: "Lexicographic greater-than. Также вызывается оператором `>`.",
            nova_body: None,
        },
        RuntimeFn {
            module: "std.runtime.string",
            receiver: Some("str"),
            is_static: false, is_mut: false, is_consume: false,
            name: "ge",
            params: &[("other", "str")],
            return_ty: "bool",
            effects: &[],
            c_name: "nova_str_ge",
            doc: "Lexicographic greater-or-equal. Также вызывается оператором `>=`.",
            nova_body: None,
        },
        RuntimeFn {
            module: "std.runtime.string",
            receiver: Some("str"),
            is_static: false, is_mut: false, is_consume: false,
            name: "bytes",
            params: &[],
            return_ty: "[]u8",
            effects: &[],
            c_name: "nova_str_bytes",
            doc: "UTF-8 bytes как []u8 (copy).",
        nova_body: None,
    },
        RuntimeFn {
            module: "std.runtime.string",
            receiver: Some("str"),
            is_static: false, is_mut: false, is_consume: false,
            name: "as_bytes",
            params: &[],
            return_ty: "readonly []u8",
            effects: &[],
            c_name: "nova_str_as_bytes",
            doc: "Plan 108 D176: zero-copy view of str UTF-8 bytes as readonly []u8 (no memcpy).",
        nova_body: None,
    },
        RuntimeFn {
            module: "std.runtime.string",
            receiver: Some("str"),
            is_static: false, is_mut: false, is_consume: false,
            name: "chars",
            params: &[],
            return_ty: "[]char",
            effects: &[],
            c_name: "nova_str_chars",
            doc: "Codepoints как []char (eager). Future: Iter[char] для лениво.",
        nova_body: None,
    },
        RuntimeFn {
            module: "std.runtime.string",
            receiver: Some("str"),
            is_static: false, is_mut: false, is_consume: false,
            name: "split",
            params: &[("sep", "str")],
            return_ty: "[]str",
            effects: &[],
            c_name: "nova_str_split",
            doc: "Split по separator. Eager в bootstrap.",
        nova_body: None,
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
            is_static: false, is_mut: false, is_consume: false,
            name,
            params: &[],
            return_ty: "f64",
            effects: &[],
            c_name: c_name_static,
            doc,
        nova_body: None,
    });
    }
    // Двух-аргументные f64 math.
    v.push(RuntimeFn {
        module: "std.runtime.math",
        receiver: Some("f64"),
        is_static: false, is_mut: false, is_consume: false,
        name: "atan2",
        params: &[("x", "f64")],
        return_ty: "f64",
        effects: &[],
        c_name: "atan2",
        doc: "atan2(y, x) — angle от positive x-axis. Self = y.",
    nova_body: None,
});
    v.push(RuntimeFn {
        module: "std.runtime.math",
        receiver: Some("f64"),
        is_static: false, is_mut: false, is_consume: false,
        name: "pow",
        params: &[("exp", "f64")],
        return_ty: "f64",
        effects: &[],
        c_name: "pow",
        doc: "self^exp.",
    nova_body: None,
});
    v.push(RuntimeFn {
        module: "std.runtime.math",
        receiver: Some("f64"),
        is_static: false, is_mut: false, is_consume: false,
        name: "hypot",
        params: &[("y", "f64")],
        return_ty: "f64",
        effects: &[],
        c_name: "hypot",
        doc: "sqrt(self^2 + y^2) без overflow.",
    nova_body: None,
});
    // Predicate methods (return bool).
    v.push(RuntimeFn {
        module: "std.runtime.math",
        receiver: Some("f64"),
        is_static: false, is_mut: false, is_consume: false,
        name: "is_nan",
        params: &[],
        return_ty: "bool",
        effects: &[],
        c_name: "isnan",
        doc: "True если NaN.",
    nova_body: None,
});
    v.push(RuntimeFn {
        module: "std.runtime.math",
        receiver: Some("f64"),
        is_static: false, is_mut: false, is_consume: false,
        name: "is_finite",
        params: &[],
        return_ty: "bool",
        effects: &[],
        c_name: "isfinite",
        doc: "True если не ±∞ и не NaN.",
    nova_body: None,
});
    v.push(RuntimeFn {
        module: "std.runtime.math",
        receiver: Some("f64"),
        is_static: false, is_mut: false, is_consume: false,
        name: "is_infinite",
        params: &[],
        return_ty: "bool",
        effects: &[],
        c_name: "isinf",
        doc: "True если ±∞.",
    nova_body: None,
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
            is_static: false, is_mut: false, is_consume: false,
            name,
            params: &[],
            return_ty: "f32",
            effects: &[],
            c_name,
            doc,
        nova_body: None,
    });
    }
    // f32 двух-аргументные.
    v.push(RuntimeFn {
        module: "std.runtime.math",
        receiver: Some("f32"),
        is_static: false, is_mut: false, is_consume: false,
        name: "atan2",
        params: &[("x", "f32")],
        return_ty: "f32",
        effects: &[],
        c_name: "atan2f",
        doc: "atan2 (single precision).",
    nova_body: None,
});
    v.push(RuntimeFn {
        module: "std.runtime.math",
        receiver: Some("f32"),
        is_static: false, is_mut: false, is_consume: false,
        name: "pow",
        params: &[("exp", "f32")],
        return_ty: "f32",
        effects: &[],
        c_name: "powf",
        doc: "self^exp (single precision).",
    nova_body: None,
});
    v.push(RuntimeFn {
        module: "std.runtime.math",
        receiver: Some("f32"),
        is_static: false, is_mut: false, is_consume: false,
        name: "hypot",
        params: &[("y", "f32")],
        return_ty: "f32",
        effects: &[],
        c_name: "hypotf",
        doc: "hypot (single precision).",
    nova_body: None,
});
    // f32 predicates: isnan/isfinite/isinf — type-generic в C99 macros,
    // те же имена.
    v.push(RuntimeFn {
        module: "std.runtime.math",
        receiver: Some("f32"),
        is_static: false, is_mut: false, is_consume: false,
        name: "is_nan",
        params: &[],
        return_ty: "bool",
        effects: &[],
        c_name: "isnan",
        doc: "True если NaN (single precision).",
    nova_body: None,
});
    v.push(RuntimeFn {
        module: "std.runtime.math",
        receiver: Some("f32"),
        is_static: false, is_mut: false, is_consume: false,
        name: "is_finite",
        params: &[],
        return_ty: "bool",
        effects: &[],
        c_name: "isfinite",
        doc: "True если не ±∞ и не NaN (single precision).",
    nova_body: None,
});
    v.push(RuntimeFn {
        module: "std.runtime.math",
        receiver: Some("f32"),
        is_static: false, is_mut: false, is_consume: false,
        name: "is_infinite",
        params: &[],
        return_ty: "bool",
        effects: &[],
        c_name: "isinf",
        doc: "True если ±∞ (single precision).",
    nova_body: None,
});
    v
}

/// `std.runtime.char` — char ↔ str (UTF-8 encode/decode).
fn char_runtime() -> Vec<RuntimeFn> {
    vec![
        RuntimeFn {
            module: "std.runtime.char",
            receiver: Some("str"),
            is_static: true, is_mut: false, is_consume: false,
            name: "from",
            params: &[("c", "char")],
            return_ty: "str",
            effects: &[],
            c_name: "Nova_str_static_from_char",
            doc: "UTF-8 encode codepoint в 1-4 байта (D73 auto-derive: char.into() -> str).",
        nova_body: None,
    },
    ]
}

/// `std.runtime.string_builder` — UTF-8 string accumulator (Plan 04).
fn string_builder_runtime() -> Vec<RuntimeFn> {
    let m = "std.runtime.string_builder";
    let recv = Some("StringBuilder");
    vec![
        RuntimeFn { module: m, receiver: recv, is_static: true,  is_mut: false, is_consume: false,
            name: "new", params: &[], return_ty: "Self", effects: &[],
            c_name: "Nova_StringBuilder_static_new",
            doc: "Создать пустой StringBuilder с initial capacity 16.",
            nova_body: None,
        },
        RuntimeFn { module: m, receiver: recv, is_static: true,  is_mut: false, is_consume: false,
            name: "with_capacity", params: &[("n", "int")], return_ty: "Self", effects: &[],
            c_name: "Nova_StringBuilder_static_with_capacity",
            doc: "Создать StringBuilder с pre-allocated capacity n.",
            nova_body: None,
        },
        RuntimeFn { module: m, receiver: recv, is_static: true,  is_mut: false, is_consume: false,
            name: "from", params: &[("s", "str")], return_ty: "Self", effects: &[],
            c_name: "Nova_StringBuilder_static_from_str",
            doc: "Создать StringBuilder из существующей строки (copy).",
            nova_body: None,
        },
        RuntimeFn { module: m, receiver: recv, is_static: true,  is_mut: false, is_consume: false,
            name: "from", params: &[("c", "char")], return_ty: "Self", effects: &[],
            c_name: "Nova_StringBuilder_static_from_char",
            doc: "Создать StringBuilder из одного codepoint (UTF-8 encode 1-4 байта).",
            nova_body: None,
        },
        RuntimeFn { module: m, receiver: recv, is_static: false, is_mut: true, is_consume: false,
            name: "append", params: &[("s", "str")], return_ty: "Self", effects: &[],
            c_name: "Nova_StringBuilder_method_append_str",
            doc: "Append UTF-8 bytes из str. Возвращает self для chaining (Ф.9.1).",
            nova_body: None,
        },
        RuntimeFn { module: m, receiver: recv, is_static: false, is_mut: true, is_consume: false,
            name: "append", params: &[("c", "char")], return_ty: "Self", effects: &[],
            c_name: "Nova_StringBuilder_method_append_char",
            doc: "Append codepoint как UTF-8 (1-4 байта). Возвращает self для chaining.",
            nova_body: None,
        },
        // Plan 13 Ф.9.2: оператор `+` через метод @plus.
        // sb + str  → sb.@plus(s) → @append(s)
        // sb + char → sb.@plus(c) → @append(c)
        RuntimeFn { module: m, receiver: recv, is_static: false, is_mut: true, is_consume: false,
            name: "plus", params: &[("s", "str")], return_ty: "Self", effects: &[],
            c_name: "",
            doc: "Оператор `+`: `sb + s == sb.@plus(s)` → @append (D46).",
            nova_body: Some("@append(s)"),
        },
        RuntimeFn { module: m, receiver: recv, is_static: false, is_mut: true, is_consume: false,
            name: "plus", params: &[("c", "char")], return_ty: "Self", effects: &[],
            c_name: "",
            doc: "Оператор `+`: `sb + c == sb.@plus(c)` → @append (D46, char overload).",
            nova_body: Some("@append(c)"),
        },
        RuntimeFn { module: m, receiver: recv, is_static: false, is_mut: false, is_consume: false,
            name: "len", params: &[], return_ty: "int", effects: &[],
            c_name: "Nova_StringBuilder_method_len",
            doc: "Длина в codepoint'ах (D26 школа B). O(n) UTF-8 walk.",
            nova_body: None,
        },
        RuntimeFn { module: m, receiver: recv, is_static: false, is_mut: false, is_consume: false,
            name: "byte_len", params: &[], return_ty: "int", effects: &[],
            c_name: "Nova_StringBuilder_method_byte_len",
            doc: "Размер в UTF-8 байтах. O(1). Для FFI / capacity-планирования.",
            nova_body: None,
        },
        RuntimeFn { module: m, receiver: recv, is_static: false, is_mut: false, is_consume: false,
            name: "capacity", params: &[], return_ty: "int", effects: &[],
            c_name: "Nova_StringBuilder_method_capacity",
            doc: "Allocated capacity в байтах (>= byte_len).",
            nova_body: None,
        },
        RuntimeFn { module: m, receiver: recv, is_static: false, is_mut: false, is_consume: false,
            name: "clone", params: &[], return_ty: "Self", effects: &[],
            c_name: "Nova_StringBuilder_method_clone",
            doc: "Создать независимую копию (deep copy buffer).",
            nova_body: None,
        },
        // Plan 73 (D131): `consume @into` — после @into() переменная-источник
        // недоступна; повторное использование — compile error, не runtime panic.
        RuntimeFn { module: m, receiver: recv, is_static: false, is_mut: false, is_consume: true,
            name: "into", params: &[], return_ty: "str", effects: &[],
            c_name: "Nova_StringBuilder_method_into",
            doc: "Финализировать в str. Infallible (UTF-8 invariant поддерживается append'ами). consume-метод (D131): после @into() StringBuilder недоступен — повторное использование = compile error.",
            nova_body: None,
        },
        RuntimeFn { module: m, receiver: recv, is_static: false, is_mut: false, is_consume: false,
            name: "starts_with", params: &[("prefix", "str")], return_ty: "bool", effects: &[],
            c_name: "Nova_StringBuilder_method_starts_with",
            doc: "Non-consuming: проверить prefix буфера. Возвращает false после @into() (consumed).",
            nova_body: None,
        },
        RuntimeFn { module: m, receiver: recv, is_static: false, is_mut: false, is_consume: false,
            name: "ends_with", params: &[("suffix", "str")], return_ty: "bool", effects: &[],
            c_name: "Nova_StringBuilder_method_ends_with",
            doc: "Non-consuming: проверить suffix буфера. Возвращает false после @into() (consumed).",
            nova_body: None,
        },
        RuntimeFn { module: m, receiver: recv, is_static: false, is_mut: false, is_consume: false,
            name: "is_empty", params: &[], return_ty: "bool", effects: &[],
            c_name: "Nova_StringBuilder_method_is_empty",
            doc: "Non-consuming: true если буфер пуст (или consumed).",
            nova_body: None,
        },
        RuntimeFn { module: m, receiver: recv, is_static: false, is_mut: false, is_consume: false,
            name: "peek", params: &[], return_ty: "str", effects: &[],
            c_name: "Nova_StringBuilder_method_peek",
            doc: "Non-consuming snapshot буфера как str. ВАЖНО: pointer на тот же buffer — subsequent append может realloc'нуть. Использовать только immediately (sb.peek().ends_with(...)).",
            nova_body: None,
        },
    ]
}

/// `std.runtime.write_buffer` — binary serialization buffer (Plan 04).
fn write_buffer_runtime() -> Vec<RuntimeFn> {
    let m = "std.runtime.write_buffer";
    let recv = Some("WriteBuffer");
    let mut v: Vec<RuntimeFn> = Vec::new();
    // Создание (Self для единообразия — Plan 13 Ф.9.1).
    v.push(RuntimeFn { module: m, receiver: recv, is_static: true, is_mut: false, is_consume: false,
        name: "new", params: &[], return_ty: "Self", effects: &[],
        c_name: "Nova_WriteBuffer_static_new",
        doc: "Создать пустой WriteBuffer.",
        nova_body: None,
    });
    v.push(RuntimeFn { module: m, receiver: recv, is_static: true, is_mut: false, is_consume: false,
        name: "with_capacity", params: &[("n", "int")], return_ty: "Self", effects: &[],
        c_name: "Nova_WriteBuffer_static_with_capacity",
        doc: "Создать WriteBuffer с pre-allocated capacity.",
        nova_body: None,
    });
    v.push(RuntimeFn { module: m, receiver: recv, is_static: true, is_mut: false, is_consume: false,
        name: "from", params: &[("b", "[]u8")], return_ty: "Self", effects: &[],
        c_name: "Nova_WriteBuffer_static_from",
        doc: "Создать WriteBuffer из существующих байт.",
        nova_body: None,
    });
    // Базовые write. Все mut @write_* возвращают Self для chaining (Ф.9.1).
    v.push(RuntimeFn { module: m, receiver: recv, is_static: false, is_mut: true, is_consume: false,
        name: "write_byte", params: &[("v", "u8")], return_ty: "Self", effects: &[],
        c_name: "Nova_WriteBuffer_method_write_byte",
        doc: "Append один byte. Returns self for chaining.",
        nova_body: None,
    });
    v.push(RuntimeFn { module: m, receiver: recv, is_static: false, is_mut: true, is_consume: false,
        name: "write_bytes", params: &[("src", "[]u8")], return_ty: "Self", effects: &[],
        c_name: "Nova_WriteBuffer_method_write_bytes",
        doc: "Append массив байт (memcpy). Returns self.",
        nova_body: None,
    });
    v.push(RuntimeFn { module: m, receiver: recv, is_static: false, is_mut: true, is_consume: false,
        name: "write_zero", params: &[("n", "int")], return_ty: "Self", effects: &[],
        c_name: "Nova_WriteBuffer_method_write_zero",
        doc: "Append n нулевых байт (memset). Returns self.",
        nova_body: None,
    });
    // Text → UTF-8 bytes (Plan 04 Этап 6.1).
    v.push(RuntimeFn { module: m, receiver: recv, is_static: false, is_mut: true, is_consume: false,
        name: "write_char", params: &[("c", "char")], return_ty: "Self", effects: &[],
        c_name: "Nova_WriteBuffer_method_write_char",
        doc: "UTF-8 encode codepoint (1-4 байта). Returns self.",
        nova_body: None,
    });
    v.push(RuntimeFn { module: m, receiver: recv, is_static: false, is_mut: true, is_consume: false,
        name: "write_str", params: &[("s", "str")], return_ty: "Self", effects: &[],
        c_name: "Nova_WriteBuffer_method_write_str",
        doc: "Append UTF-8 байты из str (memcpy). Returns self.",
        nova_body: None,
    });
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
        v.push(RuntimeFn { module: m, receiver: recv, is_static: false, is_mut: true, is_consume: false,
            name, params: params_static, return_ty: "Self", effects: &[],
            c_name: c_name_static, doc,
            nova_body: None,
        });
    }
    // Финализация / read-only.
    v.push(RuntimeFn { module: m, receiver: recv, is_static: false, is_mut: false, is_consume: false,
        name: "len", params: &[], return_ty: "int", effects: &[],
        c_name: "Nova_WriteBuffer_method_len",
        doc: "Текущий размер в байтах.",
        nova_body: None,
    });
    v.push(RuntimeFn { module: m, receiver: recv, is_static: false, is_mut: false, is_consume: false,
        name: "capacity", params: &[], return_ty: "int", effects: &[],
        c_name: "Nova_WriteBuffer_method_capacity",
        doc: "Allocated capacity в байтах.",
        nova_body: None,
    });
    v.push(RuntimeFn { module: m, receiver: recv, is_static: false, is_mut: false, is_consume: false,
        name: "clone", params: &[], return_ty: "Self", effects: &[],
        c_name: "Nova_WriteBuffer_method_clone",
        doc: "Создать независимую копию (deep copy buffer).",
        nova_body: None,
    });
    v.push(RuntimeFn { module: m, receiver: recv, is_static: false, is_mut: false, is_consume: false,
        name: "into", params: &[], return_ty: "[]u8", effects: &[],
        c_name: "Nova_WriteBuffer_method_into",
        doc: "Финализировать в []u8. Infallible.",
        nova_body: None,
    });
    v
}

/// `std.runtime.read_buffer` — cursor-style binary reader (Plan 04).
/// Pair `@read_*` (Fail-form) / `@try_read_*` (Result-form) — auto-derive
/// на C-runtime уровне (одна C-функция, две Nova-обёртки).
fn read_buffer_runtime() -> Vec<RuntimeFn> {
    let m = "std.runtime.read_buffer";
    let recv = Some("ReadBuffer");
    let mut v: Vec<RuntimeFn> = Vec::new();
    // Создание (Self — Plan 13 Ф.9.1 unification).
    v.push(RuntimeFn { module: m, receiver: recv, is_static: true, is_mut: false, is_consume: false,
        name: "from", params: &[("b", "[]u8")], return_ty: "Self", effects: &[],
        c_name: "Nova_ReadBuffer_static_from",
        doc: "Создать ReadBuffer из []u8 (view, no copy).",
        nova_body: None,
    });
    // Cursor metadata (read-only).
    v.push(RuntimeFn { module: m, receiver: recv, is_static: false, is_mut: false, is_consume: false,
        name: "position", params: &[], return_ty: "int", effects: &[],
        c_name: "Nova_ReadBuffer_method_position",
        doc: "Текущий offset cursor'а в байтах.",
        nova_body: None,
    });
    v.push(RuntimeFn { module: m, receiver: recv, is_static: false, is_mut: false, is_consume: false,
        name: "remaining", params: &[], return_ty: "int", effects: &[],
        c_name: "Nova_ReadBuffer_method_remaining",
        doc: "Сколько байт осталось до конца буфера.",
        nova_body: None,
    });
    v.push(RuntimeFn { module: m, receiver: recv, is_static: false, is_mut: false, is_consume: false,
        name: "has_remaining", params: &[("n", "int")], return_ty: "bool", effects: &[],
        c_name: "Nova_ReadBuffer_method_has_remaining",
        doc: "True если осталось ≥ n байт.",
        nova_body: None,
    });
    v.push(RuntimeFn { module: m, receiver: recv, is_static: false, is_mut: false, is_consume: false,
        name: "remaining_bytes", params: &[], return_ty: "[]u8", effects: &[],
        c_name: "Nova_ReadBuffer_method_remaining_bytes",
        doc: "Скопировать оставшиеся байты как []u8.",
        nova_body: None,
    });
    // 18 numeric × LE/BE × Fail-form / Try-form.
    let read_specs: Vec<(&'static str, &'static str, &'static str)> = vec![
        ("read_byte",  "u8", "1 byte."),
        ("read_bytes", "[]u8", "n байт (с n int параметром)."),
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
        v.push(RuntimeFn { module: m, receiver: recv, is_static: false, is_mut: true, is_consume: false,
            name, params: params_fail, return_ty: ret_ty, effects: &["Fail[ReadBufferError]"],
            c_name: c_name_fail, doc: doc_fail,
            nova_body: None,
        });
        v.push(RuntimeFn { module: m, receiver: recv, is_static: false, is_mut: true, is_consume: false,
            name: try_name, params: params_try, return_ty: try_ret, effects: &[],
            c_name: c_name_try, doc: doc_try,
            nova_body: None,
        });
    }
    // Plan 13 Ф.9.4: codepoint-уровневые reads.
    v.push(RuntimeFn { module: m, receiver: recv, is_static: false, is_mut: true, is_consume: false,
        name: "read_char", params: &[],
        return_ty: "char", effects: &["Fail[ReadBufferError]"],
        c_name: "Nova_ReadBuffer_method_read_char",
        doc: "Один codepoint (UTF-8 1-4 байта). Throw'ит UnexpectedEnd / InvalidUtf8.",
        nova_body: None,
    });
    v.push(RuntimeFn { module: m, receiver: recv, is_static: false, is_mut: true, is_consume: false,
        name: "try_read_char", params: &[],
        return_ty: "Result[char, ReadBufferError]", effects: &[],
        c_name: "Nova_ReadBuffer_method_try_read_char",
        doc: "Result-форма @read_char (Ok(char) или Err(UnexpectedEnd|InvalidUtf8)).",
        nova_body: None,
    });
    v.push(RuntimeFn { module: m, receiver: recv, is_static: false, is_mut: true, is_consume: false,
        name: "read_str", params: &[("n", "int")],
        return_ty: "str", effects: &["Fail[ReadBufferError]"],
        c_name: "Nova_ReadBuffer_method_read_str",
        doc: "n codepoint'ов как str. Throw'ит UnexpectedEnd / InvalidUtf8.",
        nova_body: None,
    });
    v.push(RuntimeFn { module: m, receiver: recv, is_static: false, is_mut: true, is_consume: false,
        name: "try_read_str", params: &[("n", "int")],
        return_ty: "Result[str, ReadBufferError]", effects: &[],
        c_name: "Nova_ReadBuffer_method_try_read_str",
        doc: "Result-форма @read_str (Ok(str) или Err).",
        nova_body: None,
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
    // Plan 62.D.bis (D126, 2026-05-18): для opaque types StringBuilder /
    // WriteBuffer / ReadBuffer canonical type-declaration живёт в
    // std/prelude/collections.nv через `external type` (D126); этот файл
    // содержит ТОЛЬКО methods через `external fn` (D82). Связь по
    // receiver-type name.
    if matches!(
        module,
        "std.runtime.string_builder"
            | "std.runtime.write_buffer"
            | "std.runtime.read_buffer"
    ) {
        out.push_str("//\n");
        out.push_str("// Plan 62.D.bis (D126, 2026-05-18): type declaration — see\n");
        out.push_str("// std/prelude/collections.nv (`external type`, D126).\n");
        out.push_str("// This file declares ONLY methods via `external fn` (D82).\n");
    }
    out.push('\n');
    // D29 rev-3 (2026-05-13) `parent.target` rule: module declaration ==
    // `<parent_of_target>.<target_name>` (2 segments), не full filesystem
    // path. Registry хранит canonical full path (`std.runtime.string`),
    // render эмитит short-form (`runtime.string`) per spec. См.
    // `spec/decisions/07-modules.md` D29 «Объявление модуля».
    let decl = {
        let parts: Vec<&str> = module.split('.').collect();
        if parts.len() >= 2 {
            format!("{}.{}", parts[parts.len() - 2], parts[parts.len() - 1])
        } else {
            module.to_string()
        }
    };
    out.push_str(&format!("module {}\n", decl));
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
        // Plan 13 Ф.9.2: записи с nova_body — обычный `export fn` (не external),
        // тело идёт через `=> {body}` после возвращаемого типа.
        if f.nova_body.is_some() {
            out.push_str("export fn ");
        } else {
            out.push_str("export external fn ");
        }
        if let Some(recv) = f.receiver {
            out.push_str(recv);
            if f.is_static {
                // No space before dot.
                out.push('.');
            } else {
                out.push(' ');
                // Plan 73 (D131): `mut` / `consume` — взаимоисключающие.
                if f.is_mut { out.push_str("mut "); }
                if f.is_consume { out.push_str("consume "); }
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
        // Plan 77 (D132): fluent builder-метод (`mut` instance, возвращает
        // `Self` = сам receiver) рендерится как `-> @`. Исключение —
        // записи с `nova_body` (напр. `@plus => @append`): они остаются
        // `-> Self`, тело не обязано буквально быть `@`.
        let is_fluent = !f.is_static && f.is_mut
            && f.return_ty == "Self" && f.nova_body.is_none();
        if is_fluent {
            out.push_str(" -> @");
        } else {
            out.push_str(" -> ");
            out.push_str(f.return_ty);
        }
        // Plan 13 Ф.9.2: тело для записей с nova_body.
        if let Some(body) = f.nova_body {
            out.push_str(" => ");
            out.push_str(body);
        }
        out.push_str("\n\n");
    }
    out
}
