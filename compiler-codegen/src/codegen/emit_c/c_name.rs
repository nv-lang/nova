//! Registry 221.1 #1440 / #1446 -- the ONE door "Nova name -> C identifier".
//!
//! A name written in Nova source (a local, a parameter, a capture, a record /
//! value-record field, a sum variant and its payload fields, a protocol method,
//! an effect op and its parameters) becomes a C token in the generated unit.
//! Taken verbatim it can collide with something the C side already owns:
//!
//! * a C keyword (`long`, `auto`, `register`, C23 `bool`/`true`/`typeof`);
//! * a platform macro from a header the runtime includes -- `near`, `far`,
//!   `interface`, `IN`, `OUT`, `min`, `max` in the Windows headers (#1440: a
//!   protocol method `near` became the vtable slot `(*near)`, and `windef.h`
//!   defines `near` as nothing); `linux`, `unix`, `errno`, `stdin` on Linux;
//! * a name the code generator itself emits next to user names -- the spawn /
//!   `parallel for` ctx pointer `_c` (#1446: `ro _c = ...` in a `parallel for`
//!   body shadowed it, `_c->ok` read a `nova_int`), `_co`, `_nv_tmp_N`,
//!   `_nova_*`, the runtime's `nova_*` / `Nova*` / `NOVA_*` types and macros,
//!   the fixed struct members `ctx` (effect vtable) and `schedlink` (spawn ctx).
//!
//! THE RULE (one sentence, so that Carina can repeat it byte for byte): a name
//! is emitted as `nv_` + name when it is a C keyword, a listed platform macro,
//! a listed fixed member of a code-generator struct, an ALL-CAPS name of two or
//! more characters (the macro convention), or starts with `_`, `nv_`, `nova_`,
//! `Nova` or `NOVA` -- except the compiler's own namespaces `_nv_`, `_nova_`,
//! `_at_`, `__`, which it synthesizes itself and where a program may not
//! declare a name (D487, `E_RESERVED_NAME`); every other name is emitted
//! unchanged.
//!
//! Why it is safe: an escaped name starts with `nv_`, and a name that starts
//! with `nv_` is always escaped, so no two source names meet in C (the mapping
//! is injective -- before #1440 `long` and `nv_long` both became `nv_long`).
//! Nothing the generator or the runtime names begins with `nv_` + an escaped
//! word (runtime `nv_*` symbols are `nv_panic`, `nv_exit`, ... -- `panic`,
//! `exit` are not escaped), and an unescaped name never starts with `_`,
//! `nova_`, `Nova`, `NOVA` or `nv_`, the prefixes the generator reserves.
//!
//! NOT through this door: names that are ABI -- `extern "C"` symbols,
//! `#export` symbols, runtime-defined types' C structs (`RUNTIME_DEFINED_TYPES`)
//! and the runtime-owned effect vtables. Those keep their spelling; free
//! functions and methods get module-qualified symbols of their own
//! (`mangle_free_fn`, `mangle_fn`), never a bare user name.

use super::CEmitter;

/// C keywords: C89..C23 plus the extensions the supported compilers accept
/// in their default mode. `_Bool`-style reserved words start with `_` and are
/// caught by the prefix rule, but are kept here so the list reads complete.
const C_KEYWORDS: &[&str] = &[
    "auto", "break", "case", "char", "const", "continue", "default", "do",
    "double", "else", "enum", "extern", "float", "for", "goto", "if",
    "inline", "int", "long", "register", "restrict", "return", "short",
    "signed", "sizeof", "static", "struct", "switch", "typedef", "union",
    "unsigned", "void", "volatile", "while",
    // C23
    "alignas", "alignof", "bool", "constexpr", "false", "nullptr",
    "static_assert", "thread_local", "true", "typeof", "typeof_unqual",
    // C11 reserved spellings
    "_Alignas", "_Alignof", "_Atomic", "_BitInt", "_Bool", "_Complex",
    "_Generic", "_Imaginary", "_Noreturn", "_Static_assert", "_Thread_local",
    // GNU / clang / MSVC extensions
    "asm", "fortran",
];

/// Lower-case macros of the headers a generated unit includes (ALL-CAPS ones
/// are covered by the ALL-CAPS rule). Windows: `windef.h`, `minwindef.h`,
/// `rpcndr.h`, `objbase.h` (#1440). Linux / glibc: the lower-case names of
/// `clang -dM -E` over `nova_rt/nova_rt.h` (2026-09-30), plus the gnu-mode
/// predefined `linux` / `unix` / `i386`. A missing entry is a defect of this
/// list, not a new mechanism: add the name here.
const PLATFORM_MACROS: &[&str] = &[
    // Windows
    "near", "far", "interface", "small", "hyper", "pascal", "cdecl", "min",
    "max",
    // predefined in gnu mode
    "linux", "unix", "i386",
    // libc / POSIX / glibc
    "alloca", "assert", "errno", "stdin", "stdout", "stderr", "offsetof",
    "setjmp", "sigsetjmp", "h_addr", "h_errno", "fpclassify", "isfinite",
    "isgreater", "isgreaterequal", "isinf", "isless", "islessequal",
    "islessgreater", "isnan", "isnormal", "isunordered", "signbit",
    "math_errhandling", "isalnum", "isalpha", "isascii", "isblank", "iscntrl",
    "isdigit", "isgraph", "islower", "isprint", "ispunct", "isspace",
    "isupper", "isxdigit", "toascii", "st_atime", "st_ctime", "st_mtime",
    "d_fileno", "s6_addr", "s6_addr16", "s6_addr32", "sa_handler",
    "sa_sigaction", "sched_priority", "sigmask", "howmany", "roundup",
    "powerof2", "setbit", "clrbit", "isset", "isclr", "dlopen",
    "be16toh", "be32toh", "be64toh", "le16toh", "le32toh", "le64toh",
    "htobe16", "htobe32", "htobe64", "htole16", "htole32", "htole64",
    "pthread_create", "pthread_join", "pthread_detach", "pthread_exit",
    "pthread_cancel", "pthread_sigmask", "pthread_cleanup_push",
    "pthread_cleanup_pop",
    // Windows sockets (`winsock2.h` in_addr / hostent member macros)
    "s_addr", "s_host", "s_net", "s_imp", "s_impno", "s_lh",
];

/// Fixed members the generator declares in a struct that also carries
/// user-named members: `void* ctx;` of an effect vtable (next to the op
/// slots), `mco_coro* schedlink;` of a spawn ctx (next to the captures).
const CODEGEN_MEMBERS: &[&str] = &["ctx", "schedlink"];

/// Prefixes the generator and the runtime reserve for their own names.
const RESERVED_PREFIXES: &[&str] = &["_", "nv_", "nova_", "Nova", "NOVA"];

/// The compiler's OWN namespaces inside `_`: names the parser, the desugar
/// passes and the generator synthesize and then read through the same
/// `Ident` path as user names (`_nv_tmp_N`, `_nova_decr_old`, `_at_<F>`,
/// `__nv_p0`). They pass the door unchanged. A program may not declare a name
/// there (D487, `E_RESERVED_NAME`), so nothing the programmer wrote reaches C
/// raw through this exemption -- the list is the ONE list of
/// `types/reserved_names.rs`, so the door and the rule cannot drift apart.
use crate::types::reserved_names::COMPILER_NAMESPACES;

/// Does `name` have to be escaped? (The rule of the module doc, clause by clause.)
pub(crate) fn c_name_needs_escape(name: &str) -> bool {
    C_KEYWORDS.contains(&name)
        || PLATFORM_MACROS.contains(&name)
        || CODEGEN_MEMBERS.contains(&name)
        || is_all_caps(name)
        || (RESERVED_PREFIXES.iter().any(|p| name.starts_with(p))
            && !COMPILER_NAMESPACES.iter().any(|p| name.starts_with(p)))
}

/// `IN`, `OUT`, `ERROR`, `EOF`, `INT_MAX`: two or more characters, starting
/// with a capital letter, with no lower-case letter -- the C macro convention.
fn is_all_caps(name: &str) -> bool {
    let b = name.as_bytes();
    b.len() >= 2
        && b[0].is_ascii_uppercase()
        && b.iter().all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || *c == b'_')
}

/// THE door: the C identifier of a Nova-source name.
pub(crate) fn c_ident(name: &str) -> String {
    if c_name_needs_escape(name) {
        format!("nv_{}", name)
    } else {
        name.to_string()
    }
}

impl CEmitter {
    /// Historical name of the door (it began with record fields, Plan 172.13
    /// `[M-c-keyword-ident-collision]`); every user-name site calls it.
    pub(super) fn mangle_field_name(name: &str) -> String {
        c_ident(name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escapes_each_clause() {
        for n in ["long", "auto", "register", "bool", "typeof"] { assert_eq!(c_ident(n), format!("nv_{n}")); }
        for n in ["near", "far", "interface", "min", "linux", "errno", "stdin"] { assert_eq!(c_ident(n), format!("nv_{n}")); }
        for n in ["IN", "OUT", "VOID", "CONST", "ERROR", "DELETE", "EOF", "INT_MAX", "V4"] { assert_eq!(c_ident(n), format!("nv_{n}")); }
        for n in ["_c", "_co", "_env", "_r", "nv_long", "nova_int", "NovaOpt_x", "NOVA_TAG_X"] {
            assert_eq!(c_ident(n), format!("nv_{n}"));
        }
        for n in ["ctx", "schedlink"] { assert_eq!(c_ident(n), format!("nv_{n}")); }
    }

    #[test]
    fn leaves_ordinary_names() {
        for n in ["x", "total", "longer", "A", "B", "Ok", "Err", "Some", "count_1", "nearby", "In", "panic", "exit",
                  "_nv_tmp_1", "_nova_decr_old", "_at_x", "__nv_p0"] {
            assert_eq!(c_ident(n), n);
        }
    }

    /// Injective: `long` and `nv_long` no longer meet (both were `nv_long`).
    #[test]
    fn injective_on_the_escape_prefix() {
        assert_ne!(c_ident("long"), c_ident("nv_long"));
        assert_ne!(c_ident("_c"), c_ident("nv__c"));
        for n in ["long", "_c", "IN", "near", "x", "nv_x", "nv_long"] {
            let c = c_ident(n);
            assert!(!C_KEYWORDS.contains(&c.as_str()) && !PLATFORM_MACROS.contains(&c.as_str()));
        }
    }
}
