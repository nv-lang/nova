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

/// `std.runtime.numeric` — Plan 74: IEEE 754 primitive bit-cast
/// (`f64 ↔ u64`, `f32 ↔ u32` reinterpret-cast).
///
/// [M-ptr-raw-access-contract-and-unaligned] item 3 (2026-07-08): NOT
/// auto-gen anymore. Was `extern "nova"` (C-side `nova_rt/numeric.h`,
/// memcpy-based); owner decision retires the C wrappers — `to_bits`/
/// `from_bits` are now PURE `.nv` bodies built on top of item 2's
/// `.read_unaligned()`/`.write_unaligned()` typed pointer methods
/// (`unsafe { (&@ as *u64).read_unaligned() }`). Same precedent as
/// `char_runtime()` (std/runtime/char.nv, [M-compiler-nv-porting-wave]
/// item A): auto-gen writes exactly ONE file per module and cannot
/// coexist with a hand-maintained sibling for the same module path — so
/// this registry returns `vec![]` and `std/runtime/numeric.nv` is entirely
/// hand-maintained (single file, single source of truth).
fn numeric_runtime() -> Vec<RuntimeFn> {
    vec![]
}

/// `std.runtime.string` — UTF-8 операции на str.
fn str_runtime() -> Vec<RuntimeFn> {
    // D-R2 (Plan 152.0 Ф.2.5): vestigial Nova-body str entries removed —
    // str method resolution is .nv-driven (std/runtime/string/*.nv), NOT the
    // registry (F2). Only the C-dispatch entries that back operator lowering
    // (emit_c.rs) + the DoS-seed hash remain. eq/lt/le/gt/ge go in 152.5a (D-R4,
    // operator decommission) -> registry ends with only @hash.
    // Plan 152.5a D-R4 DONE: eq/lt/le/gt/ge removed — `==`/`!=`/`<`/`<=`/`>`/`>=`
    // now synthesize from the Nova-body `str @eq`/`@compare` (core.nv) via the
    // emit_c.rs nova_str BinOp arm; `+` from `@concat`. Method-form `s.eq(t)` etc.
    // resolve to those Nova bodies too. Registry now ends with only @hash
    // (irreducible: SipHash + crypto-seed). Closes [M-139.1-operator-lowered-methods].
    //
    // Plan 172 (porting-wave-tails): @hash is hard-coded in emit_c.rs (line 38670),
    // no registry entry needed. Empty registry prevents emit-runtime-stubs from
    // writing std/runtime/string.nv (mirroring char_runtime pattern).
    vec![]
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

    // ─── int — Plan 196.3 (D109/D74 checker-visibility migration) ───
    // [Числовой паритет-2, 2026-07-20] `int @abs()` RETRACTED from this
    // hardcoded extern-registry entry (was `c_name: "llabs"` — C `llabs` is
    // UB on `LLONG_MIN`, `-LLONG_MIN` not representable). Replaced by a real
    // `.nv` `fn[T SignedInts] T @abs() -> T` blanket (std/prelude/
    // protocols.nv) covering `int` AND the narrow signed widths
    // (i8/i16/i32/i64) that never had `abs` at all — same "retract the
    // concrete hardcode, cover with a blanket" precedent as Plan 200 Step 0
    // (`@clamp`)/numeric-parity-1 (`@signum`/`@is_negative`/`@is_positive`).
    // The blanket traps on `T.MIN` via the ALREADY-existing unary-negate
    // trap guard (D427 §R2) — not a new overflow policy, just no longer
    // routed through UB-prone `llabs`. The matching `emit_c.rs::
    // int_method_to_c` hardcode + its `emit_call` interception arm are
    // ALSO removed (see emit_c.rs) — an `.nv`-blanket-provided `abs` on
    // `int` would otherwise never be reached (that arm ran BEFORE normal
    // `.nv`-method dispatch for any `nova_int` receiver).
    v
}

/// `std.runtime.char` — char ↔ str (UTF-8 encode/decode).
///
/// [M-compiler-nv-porting-wave] (2026-07-07) item A: was mixed (2 external
/// entries auto-gen'd into std/runtime/char.nv + 2 Nova-implemented entries
/// whose body lived as a Rust string literal, `nova_body: Some(...)`,
/// rendered verbatim by `render_nv`). Registry-driven auto-gen writes ONE
/// file per module (`module_to_path`, single_file form) — it cannot
/// coexist with a hand-maintained sibling under the SAME module (Nova
/// import resolution is single-file XOR folder per module path, see
/// `resolve_module_paths` in imports.rs — it does NOT scan a directory for
/// arbitrary same-named-module sibling files; `sync.nv`+`sync_test.nv`
/// only merge via the `*_test.nv` test-peer special case, not a general
/// mechanism; confirmed experimentally — a `char_convert.nv` sibling was
/// silently never imported). Fix: same path StringBuilder/WriteBuffer/
/// ReadBuffer already took (Plan 91.12/109) — char_runtime() now returns
/// `vec![]` (empty, like write_buffer_runtime/read_buffer_runtime below);
/// ALL FOUR declarations (str.from/str.from_codepoint externs +
/// char.from/u8.try_from Nova bodies) live together, hand-maintained, in
/// std/runtime/char.nv — single file, single source of truth, no more
/// auto-gen involvement for this module.
fn char_runtime() -> Vec<RuntimeFn> {
    vec![]
}

/// `std.runtime.string_builder` — Plan 109: StringBuilder is now a Nova-defined type.
/// All methods are implemented in std/runtime/string_builder.nv as regular Nova functions.
/// This registry is intentionally empty — no external C dispatch needed.
fn string_builder_runtime() -> Vec<RuntimeFn> {
    vec![]
}

/// Plan 91.12 (D126 retract): WriteBuffer is now a Nova-defined consume type.
/// All methods are implemented in std/runtime/write_buffer.nv as regular Nova
/// functions over `mut buf []u8` (push / append / extend_from primitives).
/// This registry is intentionally empty — no external C dispatch needed.
fn write_buffer_runtime() -> Vec<RuntimeFn> {
    vec![]
}

/// Plan 91.12 (D126 retract): ReadBuffer is now a Nova-defined cursor record
/// `{ ro data []u8, mut pos int }`. All methods are implemented in
/// std/runtime/read_buffer.nv. This registry is intentionally empty — no
/// external C dispatch needed.
fn read_buffer_runtime() -> Vec<RuntimeFn> {
    vec![]
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
            out.push_str("export extern \"nova\" fn ");
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
        // Plan 13 Ф.9.2 / Plan 91 Ф.2: тело для записей с nova_body.
        // Если body начинается с `{` — эмитируем `fn { ... }` (block form).
        // Иначе — `fn => expr` (expression form).
        if let Some(body) = f.nova_body {
            if body.trim_start().starts_with('{') {
                out.push(' ');
                out.push_str(body.trim_start());
            } else {
                out.push_str(" => ");
                out.push_str(body);
            }
        }
        out.push_str("\n\n");
    }
    out
}
