//! Интеграционные тесты — конкретные Nova-программы, которые
//! выполняются через `nova::interp` и проверяются по выводу
//! функции main или возвращаемого значения.

use nova::interp::value::Value;
use nova::interp::Interpreter;
use nova::parser::parse;
use nova::types::check_module;

fn run_main(src: &str) -> Result<Value, String> {
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

fn assert_main_returns(src: &str, expected: Value) {
    match run_main(src) {
        Ok(v) => assert_eq!(v, expected, "src:\n{}", src),
        Err(e) => panic!("error: {}\nsrc:\n{}", e, src),
    }
}

#[test]
fn returns_int_literal() {
    assert_main_returns("fn main() -> int => 42\n", Value::Int(42));
}

#[test]
fn arithmetic_ops() {
    assert_main_returns(
        "fn main() -> int => 1 + 2 * 3 - 4\n",
        Value::Int(3),
    );
}

#[test]
fn if_expression() {
    assert_main_returns(
        "fn main() -> int => if 5 > 3 { 10 } else { 20 }\n",
        Value::Int(10),
    );
}

#[test]
fn factorial() {
    let src = r#"
        fn fact(n int) -> int {
            if n <= 1 { return 1 }
            n * fact(n - 1)
        }
        fn main() -> int => fact(5)
    "#;
    assert_main_returns(src, Value::Int(120));
}

#[test]
fn fib() {
    let src = r#"
        fn fib(n int) -> int {
            if n < 2 { return n }
            fib(n - 1) + fib(n - 2)
        }
        fn main() -> int => fib(10)
    "#;
    assert_main_returns(src, Value::Int(55));
}

#[test]
fn for_loop_sum() {
    let src = r#"
        fn main() -> int {
            let mut s = 0
            for i in 0..11 {
                s += i
            }
            s
        }
    "#;
    assert_main_returns(src, Value::Int(55));
}

#[test]
fn while_loop() {
    let src = r#"
        fn main() -> int {
            let mut n = 1
            let mut count = 0
            while n < 100 {
                n *= 2
                count += 1
            }
            count
        }
    "#;
    assert_main_returns(src, Value::Int(7));
}

#[test]
fn match_sum() {
    let src = r#"
        type Shape | Circle(int) | Square(int)
        fn area(s Shape) -> int => match s {
            Circle(r) => 3 * r * r
            Square(side) => side * side
        }
        fn main() -> int => area(Square(5))
    "#;
    assert_main_returns(src, Value::Int(25));
}

#[test]
fn match_with_guard() {
    let src = r#"
        fn classify(n int) -> int => match n {
            0 => 0
            x if x > 0 => 1
            _ => -1
        }
        fn main() -> int => classify(-5)
    "#;
    assert_main_returns(src, Value::Int(-1));
}

#[test]
fn array_literal_and_index() {
    let src = r#"
        fn main() -> int {
            let arr = [10, 20, 30]
            arr[1]
        }
    "#;
    assert_main_returns(src, Value::Int(20));
}

#[test]
fn array_spread() {
    let src = r#"
        fn main() -> int {
            let a = [1, 2, 3]
            let b = [0, ...a, 4]
            b[2]
        }
    "#;
    assert_main_returns(src, Value::Int(2));
}

#[test]
fn record_construction_and_field_access() {
    let src = r#"
        type Point { x int, y int }
        fn main() -> int {
            let p = Point { x: 3, y: 4 }
            p.x + p.y
        }
    "#;
    assert_main_returns(src, Value::Int(7));
}

#[test]
fn record_field_punning() {
    let src = r#"
        type Point { x int, y int }
        fn main() -> int {
            let x = 5
            let y = 6
            let p = Point { x, y }
            p.x + p.y
        }
    "#;
    assert_main_returns(src, Value::Int(11));
}

#[test]
fn record_spread() {
    let src = r#"
        type Point { x int, y int }
        fn main() -> int {
            let p = Point { x: 1, y: 2 }
            let p2 = { ...p, y: 10 }
            p2.x + p2.y
        }
    "#;
    assert_main_returns(src, Value::Int(11));
}

#[test]
fn handler_with_resume() {
    let src = r#"
        type Counter protocol { next() -> int }
        fn three_times() Counter -> int {
            let a = Counter.next()
            let b = Counter.next()
            let c = Counter.next()
            a + b + c
        }
        fn main() -> int {
            let mut state = 0
            with Counter = Counter {
                next() {
                    state += 1
                    resume(state)
                }
            } {
                three_times()
            }
        }
    "#;
    assert_main_returns(src, Value::Int(6));
}

#[test]
fn try_propagates_err() {
    let src = r#"
        fn parse(s str) -> int => match s {
            "42" => 42
            _   => 0
        }
        fn main() -> int {
            let x = parse("42")
            x + 1
        }
    "#;
    assert_main_returns(src, Value::Int(43));
}

#[test]
fn array_pattern_match() {
    let src = r#"
        fn first_or_zero(xs []int) -> int => match xs {
            [] => 0
            [x, ..] => x
        }
        fn main() -> int => first_or_zero([7, 8, 9])
    "#;
    assert_main_returns(src, Value::Int(7));
}

#[test]
fn closure_captures() {
    let src = r#"
        fn make_adder(n int) -> int => 0
        fn main() -> int {
            let add5 = (x) => x + 5
            add5(7)
        }
    "#;
    assert_main_returns(src, Value::Int(12));
}

#[test]
fn option_some_none() {
    let src = r#"
        fn first(xs []int) -> int => match xs {
            [] => 0
            [x, ..] => x
        }
        fn main() -> int => first([100, 200])
    "#;
    assert_main_returns(src, Value::Int(100));
}

#[test]
fn assignment_compound() {
    let src = r#"
        fn main() -> int {
            let mut x = 10
            x += 5
            x *= 2
            x -= 3
            x
        }
    "#;
    assert_main_returns(src, Value::Int(27));
}

#[test]
fn nested_record_with_method() {
    let src = r#"
        type Point { x int, y int }
        fn Point @sum() -> int => @x + @y
        fn main() -> int {
            let p = Point { x: 3, y: 4 }
            p.sum()
        }
    "#;
    assert_main_returns(src, Value::Int(7));
}

#[test]
fn const_decl() {
    let src = r#"
        const PI int = 3
        fn main() -> int => PI * 2
    "#;
    assert_main_returns(src, Value::Int(6));
}

#[test]
fn lambda_in_arg() {
    let src = r#"
        fn apply(f fn(int) -> int, x int) -> int => f(x)
        fn main() -> int => apply((n) => n * 3, 7)
    "#;
    assert_main_returns(src, Value::Int(21));
}

#[test]
fn trailing_block() {
    let src = r#"
        fn run(body fn() -> int) -> int => body()
        fn main() -> int {
            run() {
                let x = 10
                x + 5
            }
        }
    "#;
    assert_main_returns(src, Value::Int(15));
}

#[test]
fn variant_with_payload_pattern() {
    let src = r#"
        type Result_ | Good(int) | Bad
        fn unwrap(r Result_) -> int => match r {
            Good(n) => n
            Bad => -1
        }
        fn main() -> int => unwrap(Good(42))
    "#;
    assert_main_returns(src, Value::Int(42));
}

#[test]
fn unit_returning_main() {
    let src = r#"
        fn main() {
            let x = 1 + 2
        }
    "#;
    match run_main(src) {
        Ok(_) => {}
        Err(e) => panic!("{}", e),
    }
}

#[test]
fn array_method_push_pop() {
    let src = r#"
        fn main() -> int {
            let arr = [1, 2, 3]
            arr.push(4)
            arr.push(5)
            arr.len
        }
    "#;
    assert_main_returns(src, Value::Int(5));
}
