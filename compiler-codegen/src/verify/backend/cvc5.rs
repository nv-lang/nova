//! Plan 33.14 Ф.2: CVC5 runner — `SmtBackend` через текстовый SMT-LIB.
//!
//! Вторая, полностью независимая от Z3-FFI, линия проверки VC. Не
//! линкуется через FFI: каждый `check_sat` рендерит текущее состояние
//! `SmtLibEmitter` в SMT-LIB v2 и скармливает его подпроцессу `cvc5`.
//!
//! ## Безопасность для soundness
//!
//! - `cvc5` не найден → `check_sat` возвращает `Unknown` (graceful skip,
//!   не паника). Cross-check вырождается в «только Z3», но компиляция
//!   не ломается.
//! - Любая ошибка `cvc5` (parse-error, crash, пустой вывод) → `Unknown`,
//!   **никогда** не definite-ответ. Cross-check классифицирует
//!   `Unknown` как OK — ложного disagreement из-за сбоя cvc5 не будет.
//! - Непереведённая формула (`SmtLibEmitter::translation_failed`) →
//!   `Unknown` (зеркало Plan 33.8 Ф.6.2 для Z3).
//!
//! ## Несовместимость с инкрементальным API
//!
//! `cvc5` тут запускается non-incremental: один подпроцесс на один
//! `check_sat`, со всем живым SMT-LIB-скриптом. `push`/`pop` ведут
//! скоуп внутри `SmtLibEmitter`, а не в решателе. Для пары
//! `assert (not goal); check_sat; pop` из `try_prove` этого достаточно.

use std::io::Write;
use std::process::{Command, Stdio};
use std::sync::OnceLock;
use std::collections::HashMap;

use super::super::ir::*;
use super::super::smtlib::SmtLibEmitter;
use super::SmtBackend;

/// Разрешить путь к бинарнику `cvc5`: env `NOVA_CVC5`, иначе `cvc5` из
/// `PATH`. Проба через `--version`; кешируется на процесс.
fn resolve_cvc5() -> Option<String> {
    let candidate = std::env::var("NOVA_CVC5")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| "cvc5".to_string());
    match Command::new(&candidate)
        .arg("--version")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
    {
        Ok(status) if status.success() => Some(candidate),
        _ => None,
    }
}

static CVC5_PATH: OnceLock<Option<String>> = OnceLock::new();

/// Путь к `cvc5`, либо `None` если бинарник недоступен.
pub fn cvc5_path() -> Option<&'static str> {
    CVC5_PATH.get_or_init(resolve_cvc5).as_deref()
}

/// Доступен ли `cvc5` в этом окружении.
pub fn cvc5_available() -> bool {
    cvc5_path().is_some()
}

/// SMT backend поверх подпроцесса `cvc5`.
pub struct Cvc5Backend {
    emitter: SmtLibEmitter,
    timeout_ms: u32,
    /// Модель последнего `Sat` (для diff-репорта cross-check'а).
    last_model: Model,
    /// SMT-LIB-скрипт последнего `check_sat` (для diff-репорта).
    last_script: String,
}

impl Cvc5Backend {
    pub fn new(timeout_ms: u32) -> Self {
        Cvc5Backend {
            emitter: SmtLibEmitter::new(),
            timeout_ms,
            last_model: Model { bindings: HashMap::new() },
            last_script: String::new(),
        }
    }

    /// SMT-LIB-скрипт последнего запуска — для diff-репорта.
    pub fn last_script(&self) -> &str {
        &self.last_script
    }

    /// Запустить `cvc5` на скрипте; вернуть `(stdout, stderr)`.
    fn run(path: &str, timeout_ms: u32, script: &str) -> Result<(String, String), String> {
        let mut child = Command::new(path)
            .arg("--lang=smt2")
            // Мягкий лимит времени на запрос; cvc5 надёжно его соблюдает.
            .arg(format!("--tlimit={}", timeout_ms))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| format!("не удалось запустить cvc5: {}", e))?;

        // stdin пишем в отдельном потоке — защита от взаимной блокировки
        // pipe'ов, если скрипт окажется крупнее буфера ОС.
        let mut stdin = child.stdin.take().ok_or("cvc5: нет stdin pipe")?;
        let script_owned = script.to_string();
        let writer = std::thread::spawn(move || {
            let _ = stdin.write_all(script_owned.as_bytes());
            // stdin закрывается на drop → cvc5 видит EOF.
        });

        let out = child
            .wait_with_output()
            .map_err(|e| format!("cvc5 wait: {}", e))?;
        let _ = writer.join();

        Ok((
            String::from_utf8_lossy(&out.stdout).into_owned(),
            String::from_utf8_lossy(&out.stderr).into_owned(),
        ))
    }
}

impl SmtBackend for Cvc5Backend {
    fn name(&self) -> &'static str {
        "cvc5"
    }

    fn declare_var(&mut self, name: &str, sort: SortRef) {
        self.emitter.declare_var(name, sort);
    }

    fn declare_function(&mut self, name: &str, param_sorts: &[SortRef], return_sort: SortRef) {
        self.emitter.declare_function(name, param_sorts, return_sort);
    }

    fn assert(&mut self, assertion: Assertion) {
        self.emitter.assert(&assertion);
    }

    fn push(&mut self) {
        self.emitter.push();
    }

    fn pop(&mut self) {
        self.emitter.pop();
    }

    fn get_witness(&mut self, var_name: &str) -> Option<ModelValue> {
        self.last_model.bindings.get(var_name).cloned()
    }

    fn check_sat(&mut self) -> SatResult {
        // Зеркало Plan 33.8 Ф.6.2: непереведённую формулу нельзя
        // молча отбрасывать — иначе ложный Unsat. Раз эмиттер не
        // справился, решателю доверять нельзя.
        if self.emitter.translation_failed() {
            return SatResult::Unknown(UnknownReason::BackendError(
                "формула не транслировалась в SMT-LIB — результат cvc5 \
                 не определён (Plan 33.14 / Plan 33.8 Ф.6.2)"
                    .into(),
            ));
        }

        let path = match cvc5_path() {
            Some(p) => p,
            None => {
                return SatResult::Unknown(UnknownReason::BackendError(
                    "cvc5 не найден (env NOVA_CVC5 либо PATH) — cross-check \
                     для этой VC пропущен"
                        .into(),
                ))
            }
        };

        let script = self.emitter.render_with_get_model();
        self.last_script = script.clone();

        let (stdout, stderr) = match Self::run(path, self.timeout_ms, &script) {
            Ok(pair) => pair,
            Err(e) => {
                return SatResult::Unknown(UnknownReason::BackendError(format!("cvc5: {}", e)))
            }
        };

        let verdict = parse_verdict(&stdout);
        match verdict {
            Some(Verdict::Unsat) => SatResult::Unsat(UnsatCore::default()),
            Some(Verdict::Sat) => {
                self.last_model = parse_model(&stdout);
                SatResult::Sat(self.last_model.clone())
            }
            Some(Verdict::Unknown) => {
                let combined = format!("{}\n{}", stdout, stderr).to_ascii_lowercase();
                let reason = if combined.contains("timeout")
                    || combined.contains("resource")
                    || combined.contains("interrupt")
                {
                    UnknownReason::Timeout
                } else {
                    UnknownReason::NotAttempted("cvc5 returned unknown".into())
                };
                SatResult::Unknown(reason)
            }
            // cvc5 не дал вердикта: parse-error / crash / пустой вывод.
            // КЛАССИФИЦИРУЕМ КАК Unknown — никогда не как definite-ответ,
            // иначе сбой cvc5 превратился бы в ложный disagreement.
            None => {
                let snippet = first_meaningful_line(&stderr)
                    .or_else(|| first_meaningful_line(&stdout))
                    .unwrap_or_else(|| "(пустой вывод)".to_string());
                SatResult::Unknown(UnknownReason::BackendError(format!(
                    "cvc5 не вернул sat/unsat/unknown: {}",
                    snippet
                )))
            }
        }
    }
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
enum Verdict {
    Sat,
    Unsat,
    Unknown,
}

/// Найти первую строку, точно равную `sat` / `unsat` / `unknown`.
fn parse_verdict(stdout: &str) -> Option<Verdict> {
    for line in stdout.lines() {
        match line.trim() {
            "unsat" => return Some(Verdict::Unsat),
            "sat" => return Some(Verdict::Sat),
            "unknown" => return Some(Verdict::Unknown),
            _ => {}
        }
    }
    None
}

fn first_meaningful_line(s: &str) -> Option<String> {
    s.lines()
        .map(|l| l.trim())
        .find(|l| !l.is_empty())
        .map(|l| {
            let mut t = l.to_string();
            if t.len() > 200 {
                t.truncate(200);
                t.push('…');
            }
            t
        })
}

/// Распарсить `(get-model)`-вывод cvc5 в `Model`.
///
/// Best-effort: используется только для отображения counterexample в
/// diff-репорте, никогда — для soundness-решений. Непонятные значения
/// → `ModelValue::Unknown`.
fn parse_model(stdout: &str) -> Model {
    let mut bindings = HashMap::new();
    // Формат cvc5: `(define-fun NAME () SORT VALUE)`. Скаляры —
    // одной строкой; этого достаточно для Int/Bool/Str-констант.
    for line in stdout.lines() {
        let line = line.trim();
        let Some(rest) = line.strip_prefix("(define-fun ") else {
            continue;
        };
        let toks: Vec<&str> = rest.split_whitespace().collect();
        // toks: NAME () SORT VALUE...   (минимум 4 токена)
        if toks.len() < 4 {
            continue;
        }
        let name = toks[0].to_string();
        // toks[1] == "()" (нет параметров), toks[2] == sort.
        let value_raw = toks[3..].join(" ");
        // Снять РОВНО ОДНУ закрывающую скобку всего define-fun
        // (значение само может оканчиваться на `)`, напр. `(- 7)`).
        let trimmed = value_raw.trim();
        let value = trimmed.strip_suffix(')').unwrap_or(trimmed).trim();
        bindings.insert(name, parse_model_value(value));
    }
    Model { bindings }
}

fn parse_model_value(v: &str) -> ModelValue {
    let v = v.trim();
    match v {
        "true" => return ModelValue::Bool(true),
        "false" => return ModelValue::Bool(false),
        _ => {}
    }
    if let Ok(n) = v.parse::<i64>() {
        return ModelValue::Int(n);
    }
    // `(- N)` — отрицательное целое.
    if let Some(inner) = v.strip_prefix("(-").and_then(|s| s.strip_suffix(')')) {
        if let Ok(n) = inner.trim().parse::<i64>() {
            return ModelValue::Int(-n);
        }
    }
    // Строковый литерал.
    if v.starts_with('"') && v.ends_with('"') && v.len() >= 2 {
        return ModelValue::Str(v[1..v.len() - 1].replace("\"\"", "\""));
    }
    ModelValue::Unknown
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn verdict_unsat() {
        assert_eq!(parse_verdict("unsat\n"), Some(Verdict::Unsat));
    }

    #[test]
    fn verdict_sat_then_model() {
        assert_eq!(parse_verdict("sat\n(\n(define-fun x () Int 1)\n)\n"), Some(Verdict::Sat));
    }

    #[test]
    fn verdict_unknown() {
        assert_eq!(parse_verdict("unknown\n"), Some(Verdict::Unknown));
    }

    #[test]
    fn verdict_ignores_error_lines() {
        // get-model после unsat печатает (error ...) — первой всё равно
        // должна найтись строка вердикта.
        let out = "unsat\n(error \"cannot get model\")\n";
        assert_eq!(parse_verdict(out), Some(Verdict::Unsat));
    }

    #[test]
    fn verdict_none_on_parse_error() {
        let out = "(error \"parse error at line 3\")\n";
        assert_eq!(parse_verdict(out), None);
    }

    #[test]
    fn model_parsing_scalars() {
        let out = "sat\n(\n(define-fun x () Int 42)\n\
                   (define-fun y () Int (- 7))\n\
                   (define-fun b () Bool true)\n\
                   (define-fun s () String \"hi\")\n)\n";
        let m = parse_model(out);
        assert!(matches!(m.bindings.get("x"), Some(ModelValue::Int(42))));
        assert!(matches!(m.bindings.get("y"), Some(ModelValue::Int(-7))));
        assert!(matches!(m.bindings.get("b"), Some(ModelValue::Bool(true))));
        match m.bindings.get("s") {
            Some(ModelValue::Str(s)) => assert_eq!(s, "hi"),
            other => panic!("ожидалась строка, получили {:?}", other),
        }
    }

    #[test]
    fn model_value_negatives_and_unknown() {
        assert!(matches!(parse_model_value("(- 3)"), ModelValue::Int(-3)));
        assert!(matches!(parse_model_value("(_ bv5 8)"), ModelValue::Unknown));
    }

    #[test]
    fn translation_failure_yields_unknown() {
        // Непереводимая формула → check_sat обязан вернуть Unknown,
        // даже без cvc5 в окружении.
        let mut b = Cvc5Backend::new(2000);
        b.assert(Assertion {
            formula: SmtTerm::App("__no_such_op__".into(), vec![SmtTerm::IntLit(1)]),
            label: None,
        });
        assert!(matches!(b.check_sat(), SatResult::Unknown(_)));
    }

    // Интеграционный тест — только если cvc5 реально установлен.
    #[test]
    fn cvc5_integration_reflexive_if_available() {
        if !cvc5_available() {
            eprintln!("cvc5 недоступен — интеграционный тест пропущен");
            return;
        }
        use super::super::try_prove;
        let mut b = Cvc5Backend::new(5000);
        b.declare_var("x", SortRef::Int);
        let x = SmtTerm::Var("x".into());
        // x == x — тавтология, должна доказаться (try_prove → Unsat).
        let goal = SmtTerm::eq(x.clone(), x);
        assert!(matches!(try_prove(&mut b, goal), SatResult::Unsat(_)));
    }

    #[test]
    fn cvc5_integration_disproves_false_if_available() {
        if !cvc5_available() {
            return;
        }
        use super::super::try_prove;
        let mut b = Cvc5Backend::new(5000);
        // 100 == 42 — ложно, try_prove должен дать Sat (counterexample).
        let goal = SmtTerm::eq(SmtTerm::IntLit(100), SmtTerm::IntLit(42));
        assert!(matches!(try_prove(&mut b, goal), SatResult::Sat(_)));
    }
}
