//! Общие хелперы для интеграционных тестов.

use nova::interp::value::Value;
use nova::interp::Interpreter;
use nova::parser::parse;
use nova::types::check_module;

/// Парсит, проверяет и запускает `main`. Возвращает результат `main`.
pub fn run_main(src: &str) -> Result<Value, String> {
    let module = parse(src).map_err(|d| d.message.clone())?;
    check_module(&module).map_err(|errs| {
        errs.iter()
            .map(|d| d.message.clone())
            .collect::<Vec<_>>()
            .join("\n")
    })?;
    let mut interp = Interpreter::new();
    interp.load_module(&module).map_err(|d| d.message.clone())?;
    interp.run_main().map_err(|d| d.message.clone())
}

/// Утверждает, что `main` возвращает ожидаемое значение.
pub fn assert_returns(src: &str, expected: Value) {
    match run_main(src) {
        Ok(v) => assert_eq!(v, expected, "src:\n{}", src),
        Err(e) => panic!("error: {}\nsrc:\n{}", e, src),
    }
}

/// Утверждает, что `main` возвращает int.
pub fn assert_int(src: &str, expected: i64) {
    assert_returns(src, Value::Int(expected));
}

/// Утверждает, что `main` возвращает bool.
pub fn assert_bool(src: &str, expected: bool) {
    assert_returns(src, Value::Bool(expected));
}

/// Утверждает, что `main` возвращает str.
pub fn assert_str(src: &str, expected: &str) {
    assert_returns(src, Value::Str(expected.to_string()));
}

/// Запуск падает с ошибкой (parse / type / runtime). Возвращает текст ошибки.
pub fn assert_fails(src: &str) -> String {
    match run_main(src) {
        Ok(v) => panic!("expected failure, got {:?}\nsrc:\n{}", v, src),
        Err(e) => e,
    }
}

/// Парсинг проходит без ошибок (мы не запускаем).
pub fn assert_parses(src: &str) {
    match parse(src) {
        Ok(_) => {}
        Err(d) => panic!("parse error: {} (span {})\nsrc:\n{}", d.message, d.span, src),
    }
}

/// Парсинг падает.
pub fn assert_parse_fails(src: &str) -> String {
    match parse(src) {
        Ok(_) => panic!("expected parse failure, src:\n{}", src),
        Err(d) => d.message,
    }
}

/// Run all tests in the module. Returns (passed, failed).
pub fn run_module_tests(src: &str) -> Result<(usize, usize), String> {
    let module = parse(src).map_err(|d| d.message.clone())?;
    let mut interp = Interpreter::new();
    interp.load_module(&module).map_err(|d| d.message.clone())?;
    let (passed, failed, _names) = interp.run_tests().map_err(|d| d.message.clone())?;
    Ok((passed, failed))
}
