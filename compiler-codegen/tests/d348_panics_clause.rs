// SPDX-License-Identifier: MIT OR Apache-2.0
//! Plan 173 Ф.6 / D348 — `panics`-клаузула тест-блока: инверсия PASS/FAIL.
//!
//! Мета-FAIL-кейсы («нет паники → FAIL», «неверный паттерн → FAIL») НЕ
//! выражаются `.nv`-фикстурами (сделали бы suite красным навсегда) — этот
//! Rust-интеграционный тест проверяет их на уровне parse→emit (hermetic,
//! без вызова C-компилятора):
//!
//! 1. Парсер: `test "имя" panics "паттерн" { … }` → `TestDecl.panics ==
//!    Some(паттерн)`; обычный тест → `None`; `panics` без строки — ошибка;
//!    `panics` как обычный идентификатор вне клаузульной позиции — свободен.
//! 2. Codegen test-runner: panics-тест эмитит ИНВЕРСИЮ (happy-path → FAIL
//!    «expected panic», catch-path → PANIC-дискриминатор + substring-матч
//!    `nova_test_msg_contains` + обе FAIL-ветки «did not contain» /
//!    «failed without panic») и `nova_runtime_reset()` в эпилоге;
//!    обычный тест инверсию НЕ получает.

use nova_codegen::codegen::CEmitter;
use nova_codegen::lexer::lex;
use nova_codegen::parser::Parser;

fn parse(src: &str) -> nova_codegen::ast::Module {
    let tokens = lex(src).expect("lex ok");
    let mut parser = Parser::new(tokens);
    parser.parse_module().expect("parse ok")
}

fn emit(src: &str) -> String {
    let module = parse(src);
    let emitter = CEmitter::new();
    let (c, _warnings) = emitter.emit_module(&module).expect("emit ok");
    c
}

// ───────────────────────────── 1. parser ─────────────────────────────

#[test]
fn parser_panics_clause_captured() {
    let m = parse(
        "module t\n\ntest \"boom\" panics \"needle\" {\n    panic(\"needle in msg\")\n}\n",
    );
    let tests: Vec<_> = m
        .items
        .iter()
        .filter_map(|i| match i {
            nova_codegen::ast::Item::Test(t) => Some(t),
            _ => None,
        })
        .collect();
    assert_eq!(tests.len(), 1);
    assert_eq!(tests[0].panics.as_deref(), Some("needle"));
}

#[test]
fn parser_plain_test_has_no_panics() {
    let m = parse("module t\n\ntest \"ok\" {\n    assert(true)\n}\n");
    let tests: Vec<_> = m
        .items
        .iter()
        .filter_map(|i| match i {
            nova_codegen::ast::Item::Test(t) => Some(t),
            _ => None,
        })
        .collect();
    assert_eq!(tests.len(), 1);
    assert_eq!(tests[0].panics, None);
}

#[test]
fn parser_panics_requires_string_pattern() {
    let tokens = lex("module t\n\ntest \"bad\" panics {\n    assert(true)\n}\n").expect("lex ok");
    let mut parser = Parser::new(tokens);
    let err = parser.parse_module().expect_err("panics без строки — ошибка");
    assert!(
        err.message.contains("panics"),
        "диагностика должна упоминать клаузулу: {}",
        err.message
    );
}

#[test]
fn parser_panics_is_contextual_identifier() {
    // Вне клаузульной позиции `panics` — обычный идентификатор (как raw/bench).
    let m = parse(
        "module t\n\nfn panics(x int) -> int => x + 1\n\ntest \"uses ident\" {\n    assert(panics(1) == 2)\n}\n",
    );
    assert!(m.items.iter().any(|i| matches!(
        i,
        nova_codegen::ast::Item::Fn(f) if f.name == "panics"
    )));
}

// ───────────────────────────── 2. codegen ─────────────────────────────

#[test]
fn codegen_panics_test_emits_inversion_and_reset() {
    let c = emit(
        "module t\n\ntest \"boom\" panics \"needle\" {\n    panic(\"needle in msg\")\n}\n",
    );
    // Happy-path инвертирован: завершение без паники = FAIL.
    assert!(
        c.contains("expected panic containing \\\"needle\\\" but test completed normally"),
        "нет FAIL-ветки «expected panic» в сгенерированном C"
    );
    // PANIC-дискриминатор + substring-матч.
    assert!(c.contains("NOVA_THROW_PANIC"), "нет PANIC-дискриминатора");
    assert!(
        c.contains("nova_test_msg_contains(_p_msg, _p_len, \"needle\")"),
        "нет substring-матча паттерна"
    );
    // Мета-FAIL-ветки: неверный паттерн / не-паника.
    assert!(
        c.contains("panic message did not contain \\\"needle\\\""),
        "нет FAIL-ветки «wrong panic message»"
    );
    assert!(
        c.contains("failed without panic (throw/cancel/exit is not a panic)"),
        "нет FAIL-ветки «failed without panic»"
    );
    // Ф.5 п.6: сброс runtime-состояния после panics-теста.
    assert!(
        c.contains("nova_runtime_reset();"),
        "нет nova_runtime_reset() в эпилоге panics-теста"
    );
}

#[test]
fn codegen_plain_test_not_inverted() {
    let c = emit("module t\n\ntest \"ok\" {\n    assert(true)\n}\n");
    assert!(
        !c.contains("expected panic containing"),
        "обычный тест НЕ должен получать инверсию"
    );
    // ОБРАЩЕНО 2026-09-01 (№854): раньше здесь требовалось ОТСУТСТВИЕ reset-
    // эпилога у обычного теста — по первоначальной норме плана 173 Ф.5 п.6.
    // Норма сменена НАМЕРЕННО коммитом 1f3650e57 (2026-07-13, race-198):
    // обычный тест тоже разматывается через longjmp (`with Fail[E] = |e| interrupt`,
    // или провал `assert()` вглуби вызовов) и оставляет грязным TLS-
    // состояние, которое в слитом CU текло в СЛЕДУЮЩИЙ тест-блок — вплоть до
    // access violation (причина расписана в `emit_c.rs` около стр. 29758).
    // Ассерт НЕ УДАЛЁН, а обращён: безусловный reset сам по себе —
    // свойство, которое стоит стеречь, а простое удаление оставило бы его без охраны.
    assert!(
        c.contains("nova_runtime_reset();"),
        "обычный тест ОБЯЗАН получать reset-эпилог (race-198): без него грязное TLS-состояние течёт в следующий тест-блок"
    );
}

#[test]
fn codegen_empty_pattern_matches_any_panic() {
    let c = emit("module t\n\ntest \"any\" panics \"\" {\n    panic(\"whatever\")\n}\n");
    assert!(
        c.contains("nova_test_msg_contains(_p_msg, _p_len, \"\")"),
        "пустой паттерн должен эмититься как \"\" (матчит любую панику)"
    );
}
