//! D488 rule 2 (owner, 2026-10-01; plan 172.15 Ф.1-бис): a `ro` value is passed by copy up to
//! three machine words (24 bytes) inclusive, by a hidden pointer above that; `mut` is always a
//! pointer. The threshold is ONE named constant (`codegen::emit_c::value_abi`); the program
//! cannot observe the difference, so the rule is pinned on the emitted C signature here.
//!
//! The records are built of `u8` fields so their C size is exactly the field count (alignment
//! 1, no padding): 24 bytes is the last size passed by copy, 25 the first passed by pointer.
//! Before (threshold 16): a 17..24-byte `ro` record went by pointer.

use nova_codegen::codegen::CEmitter;
use nova_codegen::lexer::lex;
use nova_codegen::parser::Parser;

fn emit(src: &str) -> String {
    let tokens = lex(src).expect("lex ok");
    let module = Parser::new(tokens).parse_module().expect("parse ok");
    let emitter = CEmitter::new();
    let (c, _warnings) = emitter.emit_module(&module).expect("emit ok");
    c
}

fn record(name: &str, bytes: usize) -> String {
    let fields: Vec<String> = (0..bytes).map(|i| format!("    ro f{i} u8")).collect();
    format!("type {name} value {{\n{}\n}}\n", fields.join("\n"))
}

fn program() -> String {
    format!(
        "module t\n\n{}\n{}\n{}\n\
         fn take16(r R16) -> int => r.f0 as int\n\
         fn take24(r R24) -> int => r.f0 as int\n\
         fn take25(r R25) -> int => r.f0 as int\n\
         fn bump24(mut r R24) -> int => r.f0 as int\n",
        record("R16", 16),
        record("R24", 24),
        record("R25", 25),
    )
}

/// The C parameter list of the function whose emitted name ends with `name`.
fn params_of(c: &str, name: &str) -> String {
    let needle = format!("{name}(");
    let line = c
        .lines()
        .find(|l| l.contains(&needle) && l.trim_end().ends_with('{'))
        .unwrap_or_else(|| panic!("no definition of {name} in the emitted C"));
    let start = line.find(&needle).unwrap() + needle.len();
    let end = line[start..].find(')').unwrap() + start;
    line[start..end].to_string()
}

#[test]
fn ro_value_up_to_24_bytes_is_a_copy() {
    let c = emit(&program());
    let p16 = params_of(&c, "take16");
    let p24 = params_of(&c, "take24");
    assert!(!p16.contains('*'), "a 16-byte ro record is a copy, got `{p16}`");
    assert!(!p24.contains('*'), "a 24-byte ro record is a copy (D488: three words inclusive), got `{p24}`");
}

#[test]
fn ro_value_above_24_bytes_is_a_pointer() {
    let c = emit(&program());
    let p25 = params_of(&c, "take25");
    assert!(p25.contains("NovaValue_R25") && p25.contains('*'), "a 25-byte ro record goes by pointer, got `{p25}`");
}

#[test]
fn mut_value_is_always_a_pointer() {
    let c = emit(&program());
    let p = params_of(&c, "bump24");
    assert!(p.contains('*'), "a `mut` record parameter is a pointer at any size (D488 rule 3), got `{p}`");
}
