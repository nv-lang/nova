//! Registry 221.1 #1598 (D488 rule 2): a `ro @` receiver of a value record is passed by copy
//! up to three machine words (24 bytes) inclusive, by pointer above that; a `mut @` receiver is
//! a pointer at any size (rule 3). The threshold is the same named constant as for a `ro`
//! parameter (`codegen::emit_c::value_abi`). Before, the receiver was a pointer at any size
//! ("receiver ABI A6", plan 172.4) while a parameter already followed the size rule. The
//! program cannot observe the difference, so the rule is pinned on the emitted C here: the
//! method's own receiver parameter and the argument a call passes.
//!
//! The records are built of `u8` fields so their C size is exactly the field count.

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
    let fields: Vec<String> = (0..bytes).map(|i| format!("    f{i} u8")).collect();
    format!("type {name} value {{\n{}\n}}\n", fields.join("\n"))
}

fn program() -> String {
    format!(
        "module t\n\n{}\n{}\n\
         fn R24 @first() -> int => @f0 as int\n\
         fn R25 @first() -> int => @f0 as int\n\
         fn R24 mut @bump() {{\n    @f0 += 1\n}}\n\
         fn use24(r R24) -> int => r.first()\n\
         fn use25(r R25) -> int => r.first()\n",
        record("R24", 24),
        record("R25", 25),
    )
}

/// The first C parameter (the receiver) of the definition of `method` on the type `ty`.
fn receiver_param(c: &str, ty: &str, method: &str) -> String {
    let line = c
        .lines()
        .find(|l| {
            l.contains(method) && l.contains(&format!("NovaValue_{ty}")) && l.contains("nova_self")
                && l.trim_end().ends_with('{')
        })
        .unwrap_or_else(|| panic!("no definition of {ty}.{method} in the emitted C"));
    let start = line.find('(').unwrap() + 1;
    let end = line[start..].find(|ch| ch == ',' || ch == ')').unwrap() + start;
    line[start..end].to_string()
}

#[test]
fn ro_receiver_up_to_24_bytes_is_a_copy() {
    let c = emit(&program());
    let p = receiver_param(&c, "R24", "first");
    assert!(p.contains("NovaValue_R24") && !p.contains('*'), "a 24-byte `ro @` receiver is a copy, got `{p}`");
}

#[test]
fn ro_receiver_above_24_bytes_is_a_pointer() {
    let c = emit(&program());
    let p = receiver_param(&c, "R25", "first");
    assert!(p.contains("NovaValue_R25*"), "a 25-byte `ro @` receiver goes by pointer, got `{p}`");
}

#[test]
fn mut_receiver_is_always_a_pointer() {
    let c = emit(&program());
    let p = receiver_param(&c, "R24", "bump");
    assert!(p.contains("NovaValue_R24*"), "a `mut @` receiver is a pointer at any size (D488 rule 3), got `{p}`");
}
