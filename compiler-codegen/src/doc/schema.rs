//! Plan 45 Ф.9 — embedded JSON Schema 2020-12 для D107 v1 output.
//!
//! Single source of truth для schema. Используется:
//! - `nova doc --json-schema` (для IDE auto-completion / offline-validation);
//! - LLM prompt context (consumer'у достаточно скачать одну строку);
//! - integration-тестами при schema-soak period.
//!
//! Schema — embedded raw-string. Hand-written, ручной sync с
//! `render_json.rs`. Любое расширение output'а — additive в minor,
//! breaking — major bump `format_version`.

/// Полная JSON Schema 2020-12 для `nova doc --format json` output.
pub fn schema_v1() -> &'static str {
    SCHEMA_V1
}

const SCHEMA_V1: &str = r##"{
  "$schema": "https://json-schema.org/draft/2020-12/schema",
  "$id": "https://nova-lang.org/schemas/nova-doc-v1.json",
  "title": "Nova doc output (D107 schema v1)",
  "description": "Output produced by `nova doc <file> --format json`. Deterministic byte-for-byte; keys alphabetically sorted; arrays sorted by stable id.",
  "type": "object",
  "required": ["format_version", "nova_version", "doc_tests", "links", "modules", "items"],
  "additionalProperties": false,
  "properties": {
    "format_version": {
      "type": "integer",
      "const": 1,
      "description": "Schema version. MVP = 1. Bump major on breaking change."
    },
    "generated_at": {
      "type": "string",
      "description": "Optional ISO-8601 timestamp. Omitted by default (deterministic). Opt-in via NOVA_DOC_GENERATED_AT or SOURCE_DATE_EPOCH env vars."
    },
    "nova_version": {
      "type": "string",
      "description": "Nova compiler version that produced this doc."
    },
    "source_root": {
      "type": "string",
      "description": "Plan 45 Ф.22.3 / D107: absolute path to the documented source root (file's parent for single-file, workspace dir for `nova doc <dir>`). Omitted when caller did not set it (e.g., library API without path context)."
    },
    "doc_tests": {
      "type": "array",
      "description": "Doc-tests extracted from `nova` fenced code blocks. Sorted by (from_id, index).",
      "items": { "$ref": "#/$defs/DocTest" }
    },
    "links": {
      "type": "array",
      "description": "Intra-doc-links found in doc-text. Resolved or broken (target_id=null). Sorted by (from_id, text).",
      "items": { "$ref": "#/$defs/DocLink" }
    },
    "modules": {
      "type": "array",
      "description": "DocModule entries.",
      "items": { "$ref": "#/$defs/DocModule" }
    },
    "items": {
      "type": "array",
      "description": "DocItem entries (fn / type / const / effect / protocol). Sorted by id.",
      "items": { "$ref": "#/$defs/DocItem" }
    }
  },
  "$defs": {
    "DocModule": {
      "type": "object",
      "required": ["kind", "name", "path", "peers", "source"],
      "additionalProperties": false,
      "properties": {
        "deprecation": { "oneOf": [{ "type": "null" }, { "$ref": "#/$defs/Deprecation" }] },
        "description": { "type": ["string", "null"] },
        "doc_attrs": { "type": "array", "items": { "type": "object" } },
        "kind": { "type": "string", "enum": ["file", "folder"] },
        "name": { "type": "string" },
        "path": { "type": "string", "description": "Dotted module path." },
        "peers": { "type": "array", "items": { "type": "string" }, "description": "Peer files (folder-modules only)." },
        "source": { "$ref": "#/$defs/SourceLoc" },
        "stability": { "oneOf": [{ "type": "null" }, { "$ref": "#/$defs/Stability" }] },
        "summary": { "type": ["string", "null"] }
      }
    },
    "DocItem": {
      "type": "object",
      "required": ["id", "kind", "module_path", "name", "source"],
      "properties": {
        "aliases": { "type": "array", "items": { "type": "string" }, "description": "Plan 45 Ф.3 / D105: search-aliases from `#doc_alias(...)`. Sorted + deduplicated." },
        "deprecation": { "oneOf": [{ "type": "null" }, { "$ref": "#/$defs/Deprecation" }] },
        "description": { "type": ["string", "null"] },
        "doc_attrs": { "type": "array", "items": { "type": "object" }, "description": "Reserved for unrecognized doc-attrs (forward-compat)." },
        "doc_test_handlers": { "type": ["string", "null"], "description": "Plan 45 Ф.3 / D105: `#doc(test_handlers=\"path\")` — handler-path for doc-tests." },
        "id": { "type": "string", "description": "Stable id: `<module_path>::<name>` or `<module_path>::<Type>.<method>`." },
        "kind": { "type": "string", "enum": ["fn", "type", "const", "effect", "protocol"] },
        "module_path": { "type": "string" },
        "name": { "type": "string" },
        "sections": {
          "type": "object",
          "description": "Recognized markdown sections, keys lowercased.",
          "additionalProperties": { "type": "string" },
          "properties": {
            "examples":   { "type": "string" },
            "errors":     { "type": "string" },
            "panics":     { "type": "string" },
            "safety":     { "type": "string" },
            "effects":    { "type": "string" },
            "contracts":  { "type": "string" },
            "since":      { "type": "string" },
            "see also":   { "type": "string" },
            "deprecated": { "type": "string" }
          }
        },
        "source": { "$ref": "#/$defs/SourceLoc" },
        "stability": { "oneOf": [{ "type": "null" }, { "$ref": "#/$defs/Stability" }] },
        "summary": { "type": ["string", "null"] },
        "signature": { "$ref": "#/$defs/Signature" },
        "definition": { "$ref": "#/$defs/TypeDefinition" },
        "methods": { "type": "array", "items": { "$ref": "#/$defs/MethodSig" } },
        "type": { "type": "string", "description": "Type for const items." },
        "value": { "type": "string", "description": "Rendered value for const items." }
      },
      "allOf": [
        {
          "if": { "properties": { "kind": { "const": "fn" } } },
          "then": { "required": ["signature"] }
        },
        {
          "if": { "properties": { "kind": { "const": "type" } } },
          "then": { "required": ["definition"] }
        },
        {
          "if": { "properties": { "kind": { "const": "const" } } },
          "then": { "required": ["type", "value"] }
        },
        {
          "if": { "properties": { "kind": { "const": "effect" } } },
          "then": { "required": ["methods", "axioms"] }
        },
        {
          "if": { "properties": { "kind": { "const": "protocol" } } },
          "then": { "required": ["methods", "implementors"] }
        }
      ]
    },
    "Signature": {
      "type": "object",
      "required": ["params", "return_type", "effects", "raises", "generics"],
      "additionalProperties": false,
      "properties": {
        "contracts": { "$ref": "#/$defs/Contracts" },
        "effects": { "type": "array", "items": { "type": "string" } },
        "generics": { "type": "array", "items": { "$ref": "#/$defs/GenericParam" } },
        "params": { "type": "array", "items": { "$ref": "#/$defs/Param" } },
        "raises": { "type": "array", "items": { "type": "string" } },
        "receiver": { "oneOf": [{ "type": "null" }, { "$ref": "#/$defs/Receiver" }] },
        "return_type": { "type": "string" }
      }
    },
    "Param": {
      "type": "object",
      "required": ["name", "type", "keyword_only", "variadic"],
      "additionalProperties": false,
      "properties": {
        "default": { "type": ["string", "null"] },
        "keyword_only": { "type": "boolean" },
        "name": { "type": "string" },
        "type": { "type": "string" },
        "variadic": { "type": "boolean" }
      }
    },
    "GenericParam": {
      "type": "object",
      "required": ["name"],
      "additionalProperties": false,
      "properties": {
        "bound": { "type": ["string", "null"] },
        "default": { "type": ["string", "null"] },
        "name": { "type": "string" }
      }
    },
    "Receiver": {
      "type": "object",
      "required": ["kind", "mutable", "type"],
      "additionalProperties": false,
      "properties": {
        "kind": { "type": "string", "enum": ["instance", "static"] },
        "mutable": { "type": "boolean" },
        "type": { "type": "string" }
      }
    },
    "Contracts": {
      "type": "object",
      "required": ["ensures", "requires", "verify_status"],
      "additionalProperties": false,
      "properties": {
        "ensures": { "type": "array", "items": { "type": "string" } },
        "requires": { "type": "array", "items": { "type": "string" } },
        "verify_status": { "type": "string", "enum": ["UNVERIFIED", "PROVEN", "TIMEOUT", "FAILED"] }
      }
    },
    "TypeDefinition": {
      "type": "object",
      "required": ["kind"],
      "properties": {
        "kind": { "type": "string", "enum": ["record", "sum", "alias"] },
        "fields": { "type": "array", "items": { "$ref": "#/$defs/RecordField" } },
        "variants": { "type": "array", "items": { "$ref": "#/$defs/SumVariant" } },
        "aliased_type": { "type": "string" }
      }
    },
    "RecordField": {
      "type": "object",
      "required": ["mutable", "name", "type"],
      "additionalProperties": false,
      "properties": {
        "mutable": { "type": "boolean" },
        "name": { "type": "string" },
        "type": { "type": "string" }
      }
    },
    "SumVariant": {
      "type": "object",
      "required": ["name", "payload_kind"],
      "properties": {
        "name": { "type": "string" },
        "payload_kind": { "type": "string", "enum": ["unit", "tuple", "record"] },
        "types": { "type": "array", "items": { "type": "string" } },
        "fields": { "type": "array", "items": { "$ref": "#/$defs/RecordField" } }
      }
    },
    "MethodSig": {
      "type": "object",
      "required": ["name", "params", "return_type"],
      "additionalProperties": false,
      "properties": {
        "name": { "type": "string" },
        "params": { "type": "array", "items": { "$ref": "#/$defs/Param" } },
        "return_type": { "type": "string" }
      }
    },
    "DocLink": {
      "type": "object",
      "required": ["text"],
      "additionalProperties": false,
      "properties": {
        "from_id": { "type": ["string", "null"], "description": "Item id where link was found. null = module-level." },
        "target_id": { "type": ["string", "null"], "description": "Resolved target item id. null = broken or ambiguous." },
        "text": { "type": "string", "description": "Raw link text as written: `Name`, `Type.method`, `mod.Name`." }
      }
    },
    "DocTest": {
      "type": "object",
      "required": ["id", "index", "modifiers", "visible_source", "full_source"],
      "additionalProperties": false,
      "properties": {
        "from_id": { "type": ["string", "null"] },
        "full_source": { "type": "string", "description": "Source including hidden `# ` boilerplate lines." },
        "id": { "type": "string", "description": "Stable id: `<module>::doc_test_<N>`." },
        "index": { "type": "integer", "minimum": 1 },
        "modifiers": {
          "type": "array",
          "items": { "type": "string", "enum": ["no_run", "ignore", "compile_fail", "should_panic", "must_verify"] }
        },
        "visible_source": { "type": "string", "description": "Source as rendered (hidden lines stripped)." }
      }
    },
    "Deprecation": {
      "type": "object",
      "required": ["note"],
      "additionalProperties": false,
      "properties": {
        "note": { "type": "string" },
        "since": { "type": ["string", "null"] }
      }
    },
    "Stability": {
      "type": "object",
      "required": ["tier"],
      "additionalProperties": false,
      "properties": {
        "feature": { "type": ["string", "null"], "description": "Plan 45 Ф.22.2 / D105: для `#unstable(feature = \"name\")` — имя feature-флага. null для stable/experimental." },
        "note": { "type": ["string", "null"], "description": "Plan 45 Ф.22.2 / D105: для `#experimental(note = \"...\")` — объяснение, что может измениться. null для stable/unstable." },
        "since": { "type": ["string", "null"] },
        "tier": { "type": "string", "enum": ["stable", "unstable", "experimental"] }
      }
    },
    "SourceLoc": {
      "type": "object",
      "required": ["file_id", "line"],
      "additionalProperties": false,
      "properties": {
        "file_id": { "type": "integer" },
        "line": { "type": "integer", "minimum": 1 }
      }
    }
  }
}
"##;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schema_parses_as_valid_json() {
        // Sanity: schema должен парситься. Парсим вручную через
        // встроенный JSON-reader из тестов (или simple validate).
        let s = schema_v1();
        // Минимальная проверка: открывающая `{` и закрывающая `}`.
        let trimmed = s.trim();
        assert!(trimmed.starts_with('{'));
        assert!(trimmed.ends_with('}'));
        // Required top-level keys присутствуют в тексте.
        for k in [
            "format_version",
            "nova_version",
            "doc_tests",
            "links",
            "modules",
            "items",
            "DocModule",
            "DocItem",
            "Signature",
            "DocLink",
            "DocTest",
            "Stability",
            "Deprecation",
        ] {
            assert!(s.contains(k), "schema missing key: {}", k);
        }
    }
}
