//! Plan 45 Ф.25.4 — doc-test mutation testing for contracts.
//!
//! **Идея:** для каждой публичной функции с `requires`/`ensures`
//! генерируем "мутантов" (слегка изменённые контракты), и проверяем
//! что хотя бы один doc-test FAIL'ит. Если ВСЕ doc-tests проходят
//! с мутированным контрактом → контракт под-tested (mutant survived),
//! автор получает отчёт.
//!
//! Это формальная analogия mutation testing для кода (Stryker, mutmut, PIT),
//! но применённая к **спецификациям**, не реализации. Никто из rustdoc /
//! godoc / typedoc этого не делает — Nova-unique благодаря first-class
//! contracts в signature (Plan 45 Ф.23.1).
//!
//! **Mutation kinds (MVP):**
//! - `>` ↔ `>=` (boundary off-by-one)
//! - `<` ↔ `<=`
//! - `==` ↔ `!=` (negation)
//! - drop `requires` (test что requires вообще нужен)
//!
//! **Скоуп:** только в `requires`/`ensures` — `decreases`/`invariant`
//! пока не мутируем (нет clear mutation semantics).
//!
//! **Out-of-scope (Plan 45 Ф.25.5+):**
//! - Mutation для invariants (нужны loop-invariants test runner).
//! - Mutation для AST-level expressions (e.g., `x + 1` → `x - 1`).
//! - Parallelism — sequential для simplicity, parallel — отдельный sprint.

use super::doctree::*;

/// Plan 45 Ф.25.4 — отчёт по mutation testing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MutationReport {
    /// Все мутанты для всех fn с contracts.
    pub mutants: Vec<Mutant>,
    /// Total mutants generated.
    pub total: usize,
    /// Mutants which were killed (≥1 doc-test failed under the mutant).
    pub killed: usize,
    /// Mutants which survived (all doc-tests passed under the mutant).
    pub survived: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mutant {
    /// Item ID функции (e.g., "mymod::clamp").
    pub item_id: String,
    /// Какой kind contract мутирован (`requires`, `ensures`).
    pub contract_kind: String,
    /// Original expression (как в signature).
    pub original_expr: String,
    /// Mutated expression.
    pub mutated_expr: String,
    /// Mutation operator (`gt-to-ge`, `lt-to-le`, `eq-to-ne`, `drop-requires`).
    pub operator: String,
    /// Outcome: killed (test detected) или survived (test passed mutated).
    pub outcome: MutantOutcome,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MutantOutcome {
    /// Mutant поймали — ≥1 doc-test fail'ит.
    Killed,
    /// Mutant выжил — все doc-tests прошли. Под-tested contract.
    Survived,
    /// Не было doc-tests для этой функции — не можем оценить.
    NoTests,
}

impl MutantOutcome {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Killed => "killed",
            Self::Survived => "survived",
            Self::NoTests => "no-tests",
        }
    }
}

/// Plan 45 Ф.25.4 — text-based mutant evaluation.
///
/// Для каждого mutant'а проверяет: содержит ли хоть один doc-test'а для
/// родительской функции (`from_id == mutant.item_id`) assertion который бы
/// killing'нул мутанта.
///
/// **Heuristics (MVP):**
/// - Для `requires` мутации: если test содержит вызов fn с аргументами
///   ровно на boundary (`fn(0)` для `requires x >= 0`) — mutant killed.
/// - Для `ensures`: если test содержит assert после fn call который бы
///   not-hold под мутацией — killed.
///
/// Для production-grade нужен real test execution через `test_runner` —
/// это roadmap Ф.25.5 (требует source-rewrite + re-parse, expensive).
/// MVP подход даёт useful signal без exec cost.
pub fn evaluate_mutants_textual(tree: &DocTree, mutants: &mut Vec<Mutant>) {
    for mutant in mutants.iter_mut() {
        // Найти doc-tests, относящиеся к этой fn.
        let tests: Vec<&DocTest> = tree
            .doc_tests
            .iter()
            .filter(|t| t.from_id.as_deref() == Some(&mutant.item_id))
            .collect();
        if tests.is_empty() {
            mutant.outcome = MutantOutcome::NoTests;
            continue;
        }
        // Heuristic: если doc-test содержит assertion которая бы failed
        // под mutated предикатом (т.е. явно ссылается на boundary value),
        // считаем mutant killed.
        let killed = tests.iter().any(|t| test_kills_mutant(t, mutant));
        mutant.outcome = if killed { MutantOutcome::Killed } else { MutantOutcome::Survived };
    }
}

fn test_kills_mutant(test: &DocTest, mutant: &Mutant) -> bool {
    let src = &test.visible_source;
    // Очень consistive heuristics для MVP:
    // 1. `drop-requires` mutant — killed если test содержит вызов с invalid
    //    arg который бы trigger panic из original requires. Без AST анализа —
    //    эвристика: если test содержит `should_panic` или явный negative case.
    if mutant.operator == "drop-requires" {
        return test.modifiers.iter().any(|m| matches!(m, DocTestModifier::ShouldPanic | DocTestModifier::MustVerify));
    }
    // 2. Boundary mutations (`ge-to-gt`, `gt-to-ge` etc.) — killed если test
    //    содержит assert с явным boundary value из original_expr.
    //    Простейшая heuristic: оба expressions упомянуты в test source.
    let orig_tokens = extract_tokens(&mutant.original_expr);
    let mut_tokens = extract_tokens(&mutant.mutated_expr);
    // Если в тесте присутствуют значения boundary-критичные (literals из expr)
    // — считаем killed.
    let has_boundary_literal = orig_tokens.iter().any(|t| {
        t.chars().next().map(|c| c.is_ascii_digit()).unwrap_or(false) && src.contains(t.as_str())
    }) || mut_tokens.iter().any(|t| {
        t.chars().next().map(|c| c.is_ascii_digit()).unwrap_or(false) && src.contains(t.as_str())
    });
    // И что test ВЫЗЫВАЕТ родительскую функцию (последний segment item_id).
    let fn_name = mutant.item_id.rsplit("::").next().unwrap_or(&mutant.item_id);
    let fn_name = fn_name.rsplit('.').next().unwrap_or(fn_name);
    let calls_fn = src.contains(&format!("{}(", fn_name));
    has_boundary_literal && calls_fn
}

fn extract_tokens(expr: &str) -> Vec<String> {
    expr.split(|c: char| !c.is_ascii_alphanumeric() && c != '_')
        .filter(|s| !s.is_empty())
        .map(String::from)
        .collect()
}

/// Plan 45 Ф.25.4 — full mutation report (textual heuristic).
pub fn run_mutation_analysis(tree: &DocTree) -> MutationReport {
    let mut mutants = generate_mutants(tree);
    evaluate_mutants_textual(tree, &mut mutants);
    let total = mutants.len();
    let killed = mutants.iter().filter(|m| m.outcome == MutantOutcome::Killed).count();
    let survived = mutants.iter().filter(|m| m.outcome == MutantOutcome::Survived).count();
    MutationReport { mutants, total, killed, survived }
}

/// Plan 45 Ф.28.2 — REAL-EXEC mutation analysis.
///
/// **Что делает:**
/// 1. Generates mutants как обычно (operator swap on contract.expr).
/// 2. Для каждого мутанта — substitute original_expr на mutated_expr **в source**.
/// 3. Re-parses модуль с мутированным source.
/// 4. Прогоняет doc-tests модуля через `test_runner::run_doc_tests_with_source`.
/// 5. Если ANY doc-test fail'ит — mutant **killed** (good — test catches mutation).
/// 6. Если ALL doc-tests pass'ят — mutant **survived** (contract under-tested).
///
/// **Difference от textual evaluator:**
/// - Textual (Ф.25.4): heuristic — содержит ли test boundary literal + calls fn.
/// - Real-exec (Ф.28.2): actual test execution с mutated source. True positive guarantee.
///
/// **Tradeoff:** Real-exec ~100ms per mutant per test (parse + type-check + compile + run).
/// Для функции с 5 contracts × ~5 mutants × ~3 doc-tests = 75 runs ~= 7.5s.
/// Textual ~1ms. Use real-exec в CI, textual в `--watch`/IDE.
///
/// **Caveats:**
/// - Text replacement не precise: если `x > 0` встречается в нескольких contracts
///   на одной fn, мутируется первое. Для multi-contract fn — sequential mutation
///   возможно даёт ложные positives.
/// - Source rewrite naive: используется `replace(original, mutated)` без AST.
///   Если original_expr встречается в comment / string literal — false mutation.
pub fn run_mutation_analysis_executed(
    tree: &DocTree,
    source: &str,
) -> MutationReport {
    let mut mutants = generate_mutants(tree);
    evaluate_mutants_executed(tree, source, &mut mutants);
    let total = mutants.len();
    let killed = mutants.iter().filter(|m| m.outcome == MutantOutcome::Killed).count();
    let survived = mutants.iter().filter(|m| m.outcome == MutantOutcome::Survived).count();
    MutationReport { mutants, total, killed, survived }
}

/// Plan 45 Ф.29.4 — workspace-mode mutation analysis (real-exec).
///
/// Аналог `run_mutation_analysis_executed` но для multi-module workspace.
/// Принимает map `module_path → source` (одна запись per файл) —
/// для каждого мутанта substitute mutated_expr в соответствующем file source.
///
/// **Key difference от single-file:**
/// - `sources_by_module_path: BTreeMap<String, String>` вместо single &str.
/// - Per-mutant lookup правильного source через `mutant.item_id.split("::").next()` →
///   module_path → source.
/// - Run doc-tests с мутированным source конкретного file.
pub fn run_mutation_analysis_executed_workspace(
    tree: &DocTree,
    sources_by_module_path: &std::collections::BTreeMap<String, String>,
) -> MutationReport {
    let mut mutants = generate_mutants(tree);
    evaluate_mutants_executed_workspace(tree, sources_by_module_path, &mut mutants);
    let total = mutants.len();
    let killed = mutants.iter().filter(|m| m.outcome == MutantOutcome::Killed).count();
    let survived = mutants.iter().filter(|m| m.outcome == MutantOutcome::Survived).count();
    MutationReport { mutants, total, killed, survived }
}

fn evaluate_mutants_executed_workspace(
    tree: &DocTree,
    sources_by_module_path: &std::collections::BTreeMap<String, String>,
    mutants: &mut Vec<Mutant>,
) {
    // Найти doc-tests, group'нуть по item_id.
    let mut tests_by_item: std::collections::BTreeMap<String, Vec<&DocTest>> =
        std::collections::BTreeMap::new();
    for dt in &tree.doc_tests {
        if let Some(from) = &dt.from_id {
            tests_by_item.entry(from.clone()).or_default().push(dt);
        }
    }

    for mutant in mutants.iter_mut() {
        let tests = match tests_by_item.get(&mutant.item_id) {
            Some(t) if !t.is_empty() => t,
            _ => { mutant.outcome = MutantOutcome::NoTests; continue; }
        };

        // Extract module_path из item_id: "mod.path::fn_name" → "mod.path".
        let module_path = match mutant.item_id.split("::").next() {
            Some(p) => p,
            None => { mutant.outcome = MutantOutcome::NoTests; continue; }
        };
        let source = match sources_by_module_path.get(module_path) {
            Some(s) => s.as_str(),
            None => { mutant.outcome = MutantOutcome::NoTests; continue; }
        };

        let mutated_source = source.replacen(&mutant.original_expr, &mutant.mutated_expr, 1);
        if mutated_source == *source {
            mutant.outcome = MutantOutcome::NoTests;
            continue;
        }

        let test_refs: Vec<crate::doc::doctree::DocTest> = tests.iter().map(|t| (*t).clone()).collect();
        let summary = crate::doc::test_runner::run_doc_tests_with_source(
            &test_refs,
            Some(&mutated_source),
        );

        let has_failure = summary.results.iter().any(|r|
            matches!(r.outcome, crate::doc::test_runner::DocTestOutcome::Failed(_))
        );
        mutant.outcome = if has_failure {
            MutantOutcome::Killed
        } else {
            MutantOutcome::Survived
        };
    }
}

fn evaluate_mutants_executed(tree: &DocTree, source: &str, mutants: &mut Vec<Mutant>) {
    // Найти doc-tests один раз, group'нуть по item_id.
    let mut tests_by_item: std::collections::BTreeMap<String, Vec<&DocTest>> =
        std::collections::BTreeMap::new();
    for dt in &tree.doc_tests {
        if let Some(from) = &dt.from_id {
            tests_by_item.entry(from.clone()).or_default().push(dt);
        }
    }

    for mutant in mutants.iter_mut() {
        let tests = match tests_by_item.get(&mutant.item_id) {
            Some(t) if !t.is_empty() => t,
            _ => { mutant.outcome = MutantOutcome::NoTests; continue; }
        };

        // Substitute original_expr → mutated_expr в source.
        // Используем replacen(_, 1) для safety: если original_expr встречается
        // несколько раз, мутируем только первое.
        let mutated_source = source.replacen(&mutant.original_expr, &mutant.mutated_expr, 1);
        if mutated_source == *source {
            // No-op replacement (original_expr не найден в source) — skip.
            // Это возможно если render_expr добавил parens вокруг expr, а source
            // имеет original без parens. Honest result: no-tests.
            mutant.outcome = MutantOutcome::NoTests;
            continue;
        }

        // Прогоняем все tests этого item с мутированным source.
        let test_refs: Vec<crate::doc::doctree::DocTest> = tests.iter().map(|t| (*t).clone()).collect();
        let summary = crate::doc::test_runner::run_doc_tests_with_source(
            &test_refs,
            Some(&mutated_source),
        );

        // Mutant killed если хоть один тест fail'ит под мутацией.
        // Skipped tests (compile_fail / ignore) — не counted в kill/survive.
        let has_failure = summary.results.iter().any(|r|
            matches!(r.outcome, crate::doc::test_runner::DocTestOutcome::Failed(_))
        );
        mutant.outcome = if has_failure {
            MutantOutcome::Killed
        } else {
            MutantOutcome::Survived
        };
    }
}

/// Plan 45 Ф.25.4 — генерирует мутантов из contracts в DocTree.
///
/// Возвращает Vec<Mutant> с outcome=NoTests изначально; caller вызывает
/// `evaluate_mutants` чтобы прогнать тесты и обновить outcome.
pub fn generate_mutants(tree: &DocTree) -> Vec<Mutant> {
    let mut out = Vec::new();
    for m in &tree.modules {
        for it in &m.items {
            if let ItemKind::Fn(sig) = &it.kind {
                for contract in &sig.contracts {
                    let kind = contract.kind.as_str();
                    if kind != "requires" && kind != "ensures" {
                        continue;
                    }
                    for (op_name, mutated) in mutate_expression(&contract.expr) {
                        out.push(Mutant {
                            item_id: it.id.clone(),
                            contract_kind: kind.to_string(),
                            original_expr: contract.expr.clone(),
                            mutated_expr: mutated,
                            operator: op_name.to_string(),
                            outcome: MutantOutcome::NoTests,
                        });
                    }
                    // Drop mutants — заменяют predicate на `true` (vacuously satisfied).
                    // Если test всё ещё проходит — contract является vacuous; killed
                    // если test fail'ит (e.g., contract пропускает valid input в test).
                    match kind {
                        "requires" => {
                            out.push(Mutant {
                                item_id: it.id.clone(),
                                contract_kind: kind.to_string(),
                                original_expr: contract.expr.clone(),
                                mutated_expr: "true".to_string(),
                                operator: "drop-requires".to_string(),
                                outcome: MutantOutcome::NoTests,
                            });
                        }
                        "ensures" => {
                            // Plan 45 Ф.29.3: drop-ensures mutator. Заменяет
                            // postcondition на `true` — если test не имеет
                            // post-call assertion, mutant survives (under-tested).
                            // Если есть assert(result == ...) — killed (test catches
                            // что postcondition реально нужно).
                            out.push(Mutant {
                                item_id: it.id.clone(),
                                contract_kind: kind.to_string(),
                                original_expr: contract.expr.clone(),
                                mutated_expr: "true".to_string(),
                                operator: "drop-ensures".to_string(),
                                outcome: MutantOutcome::NoTests,
                            });
                        }
                        _ => {}
                    }
                }
            }
        }
    }
    // Deterministic order: by (item_id, contract_kind, operator, original_expr).
    out.sort_by(|a, b| {
        a.item_id.cmp(&b.item_id)
            .then(a.contract_kind.cmp(&b.contract_kind))
            .then(a.operator.cmp(&b.operator))
            .then(a.original_expr.cmp(&b.original_expr))
    });
    out
}

/// Возвращает список (operator_name, mutated_expr) для данного contract.expr.
///
/// MVP: text-based replacement. Не парсит AST — для production grade
/// нужно через parser, но MVP покрывает 80% случаев.
fn mutate_expression(expr: &str) -> Vec<(&'static str, String)> {
    let mut mutants = Vec::new();
    // Список взаимных замен — каждая создаёт отдельный mutant.
    let swaps: &[(&str, &str, &str)] = &[
        // `>` (но не `>=`) → `>=`
        (">=", ">", "ge-to-gt"),    // ослабление: >= → >
        ("<=", "<", "le-to-lt"),
        ("==", "!=", "eq-to-ne"),
        ("!=", "==", "ne-to-eq"),
    ];
    // Сначала double-character ops (`>=`, `<=`, `==`, `!=`) — иначе `>` mutator
    // съест `>=`.
    for (from, to, op) in swaps {
        if expr.contains(from) {
            mutants.push((*op, replace_first(expr, from, to)));
        }
    }
    // Затем single-character ops, но избегая double-char patterns.
    if contains_single_op(expr, '>', "=") {
        mutants.push(("gt-to-ge", replace_single_gt_lt(expr, '>', ">=")));
    }
    if contains_single_op(expr, '<', "=") {
        mutants.push(("lt-to-le", replace_single_gt_lt(expr, '<', "<=")));
    }
    mutants
}

fn replace_first(s: &str, from: &str, to: &str) -> String {
    if let Some(idx) = s.find(from) {
        let mut out = String::with_capacity(s.len());
        out.push_str(&s[..idx]);
        out.push_str(to);
        out.push_str(&s[idx + from.len()..]);
        out
    } else {
        s.to_string()
    }
}

/// Проверка: содержит ли `s` символ `ch` НЕ как часть double-character op.
/// Например `contains_single_op("x > 0", '>', "=")` = true,
/// `contains_single_op("x >= 0", '>', "=")` = false (т.к. `>=`).
fn contains_single_op(s: &str, ch: char, exclude_next: &str) -> bool {
    let bytes = s.as_bytes();
    for (i, &b) in bytes.iter().enumerate() {
        if b as char == ch {
            let next = bytes.get(i + 1).copied().unwrap_or(0);
            if !exclude_next.as_bytes().contains(&next) {
                return true;
            }
        }
    }
    false
}

/// Заменяет первое вхождение `ch` (не как часть double-op) на `replacement`.
fn replace_single_gt_lt(s: &str, ch: char, replacement: &str) -> String {
    let bytes = s.as_bytes();
    for (i, &b) in bytes.iter().enumerate() {
        if b as char == ch {
            let next = bytes.get(i + 1).copied().unwrap_or(0);
            if next != b'=' {
                let mut out = String::with_capacity(s.len() + 1);
                out.push_str(&s[..i]);
                out.push_str(replacement);
                out.push_str(&s[i + 1..]);
                return out;
            }
        }
    }
    s.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mutate_simple_gt() {
        let muts = mutate_expression("x > 0");
        // Only single-char `>` → `>=`.
        assert!(muts.iter().any(|(op, e)| *op == "gt-to-ge" && e == "x >= 0"));
    }

    #[test]
    fn mutate_ge_to_gt() {
        let muts = mutate_expression("x >= 0");
        assert!(muts.iter().any(|(op, e)| *op == "ge-to-gt" && e == "x > 0"));
        // Не должно генерить gt-to-ge для уже-`>=`.
        assert!(!muts.iter().any(|(op, _)| *op == "gt-to-ge"));
    }

    #[test]
    fn mutate_eq() {
        let muts = mutate_expression("a == b");
        assert!(muts.iter().any(|(op, e)| *op == "eq-to-ne" && e == "a != b"));
    }

    #[test]
    fn mutate_combined() {
        // `x > 0 && y == 5` — should generate `gt-to-ge` AND `eq-to-ne`.
        let muts = mutate_expression("x > 0 && y == 5");
        assert!(muts.iter().any(|(op, _)| *op == "gt-to-ge"));
        assert!(muts.iter().any(|(op, _)| *op == "eq-to-ne"));
    }

    #[test]
    fn mutate_no_operators() {
        // Pure literal — no mutants.
        let muts = mutate_expression("true");
        assert!(muts.is_empty());
    }

    #[test]
    fn outcome_as_str() {
        assert_eq!(MutantOutcome::Killed.as_str(), "killed");
        assert_eq!(MutantOutcome::Survived.as_str(), "survived");
        assert_eq!(MutantOutcome::NoTests.as_str(), "no-tests");
    }
}
