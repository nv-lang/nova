//! Минимальный sanity-тест, чтобы быстрая команда `cargo test --test
//! integration` всё ещё работала. Полное покрытие фич — в отдельных
//! тематических test-файлах (lexer_tests, parser_tests, eval_*, etc.).

mod common;

use common::*;
use nova::interp::value::Value;

#[test]
fn returns_int_literal() {
    assert_returns("fn main() -> int => 42\n", Value::Int(42));
}

#[test]
fn arithmetic_compiles_and_runs() {
    assert_int("fn main() -> int => 1 + 2 * 3 - 4\n", 3);
}
