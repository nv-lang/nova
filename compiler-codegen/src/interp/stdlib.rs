//! Stdlib — нативные функции, доступные из любого Nova-исходника.
//!
//! Минимум: print/println, panic, assert, file I/O для компилятора.

use super::env::Env;
use super::value::*;
use super::Interpreter;
use crate::ast::Expr;
use crate::diag::{Diagnostic, Span};
use std::cell::RefCell;
use std::rc::Rc;

// Plan 45 Ф.24.8: thread-local output capture for expect_output doc-tests.
// When active (Some), print/println write to buffer instead of stdout.
// Doc-tests run sequentially, so no concurrency concern.
thread_local! {
    static OUTPUT_CAPTURE: RefCell<Option<String>> = RefCell::new(None);
}

/// Begin capturing stdout output for this thread. Replaces any previous capture.
pub fn capture_output_start() {
    OUTPUT_CAPTURE.with(|c| *c.borrow_mut() = Some(String::new()));
}

/// Stop capturing and return accumulated output (without trailing newline).
pub fn capture_output_finish() -> String {
    OUTPUT_CAPTURE.with(|c| {
        c.borrow_mut()
            .take()
            .unwrap_or_default()
            .trim_end_matches('\n')
            .to_string()
    })
}

/// Write text through the output system (capture buffer or stdout).
fn output_write(text: &str) {
    let captured = OUTPUT_CAPTURE.with(|c| {
        let mut b = c.borrow_mut();
        if let Some(buf) = b.as_mut() {
            buf.push_str(text);
            true
        } else {
            false
        }
    });
    if !captured {
        print!("{}", text);
    }
}

pub fn install(env: &Env) {
    env.define(
        "print",
        Value::Native(Rc::new(NativeFn {
            name: "print".into(),
            func: Box::new(|args| {
                for a in args {
                    output_write(&format!("{}", a));
                }
                Ok(Value::Unit)
            }),
        })),
    );
    env.define(
        "println",
        Value::Native(Rc::new(NativeFn {
            name: "println".into(),
            func: Box::new(|args| {
                for a in args {
                    output_write(&format!("{}", a));
                }
                output_write("\n");
                Ok(Value::Unit)
            }),
        })),
    );
    env.define(
        "panic",
        Value::Native(Rc::new(NativeFn {
            name: "panic".into(),
            func: Box::new(|args| {
                let msg: String = args.iter().map(|a| format!("{}", a)).collect();
                Err(NativeError {
                    message: format!("panic: {}", msg),
                })
            }),
        })),
    );
    // exit(code int, msg str) -> Never — D13: смерть всего процесса.
    // В test-runner'е перехватывается как fail теста (через NativeError);
    // в production-runtime гасит процесс с указанным exit code.
    env.define(
        "exit",
        Value::Native(Rc::new(NativeFn {
            name: "exit".into(),
            func: Box::new(|args| {
                let code = match args.first() {
                    Some(Value::Int(c)) => *c,
                    _ => return Err(NativeError {
                        message: "exit: expected (int, str)".into(),
                    }),
                };
                let msg = match args.get(1) {
                    Some(Value::Str(s)) => s.clone(),
                    _ => return Err(NativeError {
                        message: "exit: expected (int, str)".into(),
                    }),
                };
                // В interp-mode (тесты, скрипты) — отдаём как ошибку,
                // чтобы test-runner мог сообщить о провале с кодом и сообщением.
                Err(NativeError {
                    message: format!("exit({}): {}", code, msg),
                })
            }),
        })),
    );
    env.define(
        "assert",
        Value::Native(Rc::new(NativeFn {
            name: "assert".into(),
            func: Box::new(|args| match args.first() {
                Some(Value::Bool(true)) => Ok(Value::Unit),
                Some(Value::Bool(false)) => Err(NativeError {
                    message: "assertion failed".into(),
                }),
                _ => Err(NativeError {
                    message: "assert: expected bool".into(),
                }),
            }),
        })),
    );
    env.define(
        "read_file",
        Value::Native(Rc::new(NativeFn {
            name: "read_file".into(),
            func: Box::new(|args| {
                let Some(Value::Str(path)) = args.first() else {
                    return Err(NativeError {
                        message: "read_file: expected str".into(),
                    });
                };
                match std::fs::read_to_string(path) {
                    Ok(s) => Ok(Value::Str(s)),
                    Err(e) => Err(NativeError {
                        message: format!("read_file: {}", e),
                    }),
                }
            }),
        })),
    );
    env.define(
        "write_file",
        Value::Native(Rc::new(NativeFn {
            name: "write_file".into(),
            func: Box::new(|args| {
                let (Some(Value::Str(path)), Some(Value::Str(contents))) =
                    (args.first(), args.get(1))
                else {
                    return Err(NativeError {
                        message: "write_file: expected (str, str)".into(),
                    });
                };
                match std::fs::write(path, contents) {
                    Ok(()) => Ok(Value::Unit),
                    Err(e) => Err(NativeError {
                        message: format!("write_file: {}", e),
                    }),
                }
            }),
        })),
    );
    env.define(
        "to_str",
        Value::Native(Rc::new(NativeFn {
            name: "to_str".into(),
            func: Box::new(|args| {
                let Some(v) = args.first() else {
                    return Err(NativeError {
                        message: "to_str: missing argument".into(),
                    });
                };
                Ok(Value::Str(format!("{}", v)))
            }),
        })),
    );
    env.define(
        "len",
        Value::Native(Rc::new(NativeFn {
            name: "len".into(),
            func: Box::new(|args| match args.first() {
                Some(Value::Str(s)) => Ok(Value::Int(s.chars().count() as i64)),
                Some(Value::Array(arr)) => Ok(Value::Int(arr.borrow().len() as i64)),
                _ => Err(NativeError {
                    message: "len: expected str or array".into(),
                }),
            }),
        })),
    );
    // try_run — для тестов транзакционных handler'ов: запускает closure,
    // конвертирует Throw в Err.
    env.define(
        "try_run",
        Value::Native(Rc::new(NativeFn {
            name: "try_run".into(),
            func: Box::new(|_args| {
                // В bootstrap не проброшено — для compiler этого не нужно.
                // Тесты ORM используют упрощённую форму без try_run.
                Ok(Value::Variant {
                    type_name: Some("Result".into()),
                    name: "Ok".into(),
                    payload: VariantPayload::Tuple(vec![Value::Unit]),
                })
            }),
        })),
    );
}

/// Native-методы на встроенных типах.
pub fn try_native_method(
    interp: &Interpreter,
    recv: &Value,
    method: &str,
    args: &[Expr],
    env: &Env,
    span: Span,
) -> Result<Option<Value>, Diagnostic> {
    match (recv, method) {
        // Array methods
        (Value::Array(arr), "len") => {
            require_args(method, args, 0, span)?;
            Ok(Some(Value::Int(arr.borrow().len() as i64)))
        }
        // Plan 60 / D112: array size-accessor methods. `capacity()`,
        // не `cap()` — консистентно с Rust/C++/Swift; D29 явность.
        (Value::Array(arr), "capacity") => {
            require_args(method, args, 0, span)?;
            Ok(Some(Value::Int(arr.borrow().capacity() as i64)))
        }
        (Value::Array(arr), "is_empty") => {
            require_args(method, args, 0, span)?;
            Ok(Some(Value::Bool(arr.borrow().is_empty())))
        }
        (Value::Array(arr), "push") => {
            require_args(method, args, 1, span)?;
            let v = eval_arg(interp, &args[0], env)?;
            arr.borrow_mut().push(v);
            Ok(Some(Value::Unit))
        }
        (Value::Array(arr), "pop") => {
            require_args(method, args, 0, span)?;
            let popped = arr.borrow_mut().pop();
            match popped {
                Some(v) => Ok(Some(Value::Variant {
                    type_name: Some("Option".into()),
                    name: "Some".into(),
                    payload: VariantPayload::Tuple(vec![v]),
                })),
                None => Ok(Some(Value::Variant {
                    type_name: Some("Option".into()),
                    name: "None".into(),
                    payload: VariantPayload::Unit,
                })),
            }
        }
        (Value::Array(arr), "get") => {
            require_args(method, args, 1, span)?;
            let idx_v = eval_arg(interp, &args[0], env)?;
            let Value::Int(i) = idx_v else {
                return Err(Diagnostic::new("get: expected int index", span));
            };
            let arr_ref = arr.borrow();
            if i < 0 || (i as usize) >= arr_ref.len() {
                return Ok(Some(Value::Variant {
                    type_name: Some("Option".into()),
                    name: "None".into(),
                    payload: VariantPayload::Unit,
                }));
            }
            Ok(Some(Value::Variant {
                type_name: Some("Option".into()),
                name: "Some".into(),
                payload: VariantPayload::Tuple(vec![arr_ref[i as usize].clone()]),
            }))
        }
        // String methods
        (Value::Str(s), "len") => {
            require_args(method, args, 0, span)?;
            Ok(Some(Value::Int(s.chars().count() as i64)))
        }
        // Plan 60 / D112: str size-accessor methods.
        // byte_len уже могут быть в другой ветке — добавляем здесь для полноты.
        (Value::Str(s), "byte_len") => {
            require_args(method, args, 0, span)?;
            Ok(Some(Value::Int(s.len() as i64)))
        }
        (Value::Str(s), "is_empty") => {
            require_args(method, args, 0, span)?;
            Ok(Some(Value::Bool(s.is_empty())))
        }
        (Value::Str(s), "starts_with") => {
            require_args(method, args, 1, span)?;
            let Value::Str(prefix) = eval_arg(interp, &args[0], env)? else {
                return Err(Diagnostic::new("starts_with: expected str", span));
            };
            Ok(Some(Value::Bool(s.starts_with(&prefix))))
        }
        (Value::Str(s), "ends_with") => {
            require_args(method, args, 1, span)?;
            let Value::Str(suffix) = eval_arg(interp, &args[0], env)? else {
                return Err(Diagnostic::new("ends_with: expected str", span));
            };
            Ok(Some(Value::Bool(s.ends_with(&suffix))))
        }
        (Value::Str(s), "contains") => {
            require_args(method, args, 1, span)?;
            let Value::Str(sub) = eval_arg(interp, &args[0], env)? else {
                return Err(Diagnostic::new("contains: expected str", span));
            };
            Ok(Some(Value::Bool(s.contains(&sub))))
        }
        (Value::Str(s), "to_lower") => {
            require_args(method, args, 0, span)?;
            Ok(Some(Value::Str(s.to_lowercase())))
        }
        (Value::Str(s), "to_upper") => {
            require_args(method, args, 0, span)?;
            Ok(Some(Value::Str(s.to_uppercase())))
        }
        (Value::Str(s), "trim") => {
            require_args(method, args, 0, span)?;
            Ok(Some(Value::Str(s.trim().to_string())))
        }
        _ => Ok(None),
    }
}

fn require_args(
    method: &str,
    args: &[Expr],
    expected: usize,
    span: Span,
) -> Result<(), Diagnostic> {
    if args.len() != expected {
        Err(Diagnostic::new(
            format!(
                "method `{}` expects {} arg(s), got {}",
                method,
                expected,
                args.len()
            ),
            span,
        ))
    } else {
        Ok(())
    }
}

fn eval_arg(interp: &Interpreter, arg: &Expr, env: &Env) -> Result<Value, Diagnostic> {
    interp
        .eval_expr(arg, env)
        .and_then(|f| match f {
            super::Flow::Value(v) => Ok(v),
            _ => Err(Diagnostic::new("invalid control flow in argument", arg.span)),
        })
}
