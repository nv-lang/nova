#!/usr/bin/env bash
# Plan 70 Ф.2 — Internal lint guard: forbid NEW silent nova_int fallback
# patterns in compiler-codegen source.
#
# Rationale:
#   Plan 70 (см. docs/plans/70-no-silent-nova-int-fallback.md) elimin'ит
#   silent miscompilation от `type_ref_to_c(...).unwrap_or_else(|_| "nova_int")`
#   паттерна. Этот скрипт — CI gate против регресса: добавление нового сайта
#   silent fallback'а должно fail'ить lint.
#
# Existing (legitimate) sites:
#   - Cat B (intentional erasure): inline-документированы с rationale,
#     listed в docs/codegen-erasure-sites.md.
#   - Cat C/D (categorical mappings / method dispatch wildcards):
#     "_ => nova_int" в match по method-name на известный receiver type.
#   - Cat B (erased_type_ref_c / erase_unk wrappers).
#
# Все остальные silent fallback'и должны использовать:
#   - `err_no_int_fallback(context, cause)` + `?` для functions возвращающих Result
#   - `record_strict_error(context, cause)` для cascade-blocked sites
#
# Usage:
#   ./scripts/lint-no-silent-int-fallback.sh           # default scan
#   ./scripts/lint-no-silent-int-fallback.sh --strict  # fail on ANY match
#
# Exit:
#   0 — no new violations beyond baseline
#   1 — new violation detected (CI fail)

set -euo pipefail

# Baseline count of legitimate sites — must be UPDATED when adding new
# Cat B / Cat D sites with proper documentation. Bump baseline после
# adding inline-doc + entry в docs/codegen-erasure-sites.md.
BASELINE_TYPE_REF_TO_C_UNWRAP_OR=7   # type_ref_to_c(...).unwrap_or_else(|_| "nova_int")
                                     # Cat B: B1 (2720), B4 (5934), B5 (5948),
                                     # B6 (6149), B7 (6152), B8 (8311), B9 (8314).
                                     # B2/B3 (5846/5867) — Cat A2 wildcard pattern.
                                     # B10 (7051) — Cat A2 wildcard pattern.
BASELINE_WILDCARD_NOVA_INT=26        # _ => "nova_int" (Cat B/D legitimate)
                                     # Cat B: B2 (5846), B3 (5867), B10 (7051),
                                     # B11 (19493), B12 (~19437 wildcard variant),
                                     # B13 (18382). Plus ~18 Cat D dispatch wildcards.
                                     # Cat D: D1/D2 in sum_schema_registry.rs (Plan 62.A.bis,
                                     #   type_ref_to_c_minimal — schema-registration only).

PROJECT_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
CODEGEN_SRC="${PROJECT_ROOT}/compiler-codegen/src"

# Pattern 1: type_ref_to_c result with silent nova_int fallback
count_a1=$(grep -rE 'type_ref_to_c\([^)]*\)\.unwrap_or_else\(\|_\| "nova_int"' "$CODEGEN_SRC" | wc -l)

# Pattern 2: wildcard match arm with silent nova_int
count_a2=$(grep -rnE '_ => "nova_int"' "$CODEGEN_SRC" | wc -l)

echo "Plan 70 lint scan (against $CODEGEN_SRC):"
echo "  Cat A1 (type_ref_to_c silent fallback): $count_a1 (baseline $BASELINE_TYPE_REF_TO_C_UNWRAP_OR)"
echo "  Cat A2 (wildcard _ => nova_int):        $count_a2 (baseline $BASELINE_WILDCARD_NOVA_INT)"

exit_code=0

if [ "$count_a1" -gt "$BASELINE_TYPE_REF_TO_C_UNWRAP_OR" ]; then
    delta=$((count_a1 - BASELINE_TYPE_REF_TO_C_UNWRAP_OR))
    echo
    echo "ERROR: Cat A1 count ($count_a1) exceeds baseline ($BASELINE_TYPE_REF_TO_C_UNWRAP_OR) by $delta."
    echo "       New silent type_ref_to_c fallback site(s) introduced."
    echo
    echo "Use instead:"
    echo "  - For functions returning Result<_, String>:"
    echo "      .map_err(|e| self.err_no_int_fallback(\"context\", &e))?"
    echo "  - For cascade-blocked sites (no Result return):"
    echo "      .unwrap_or_else(|e| self.record_strict_error(\"context\", &e))"
    echo
    echo "Current violations:"
    grep -rnE 'type_ref_to_c\([^)]*\)\.unwrap_or_else\(\|_\| "nova_int"' "$CODEGEN_SRC"
    exit_code=1
fi

if [ "$count_a2" -gt "$BASELINE_WILDCARD_NOVA_INT" ]; then
    delta=$((count_a2 - BASELINE_WILDCARD_NOVA_INT))
    echo
    echo "ERROR: Cat A2 count ($count_a2) exceeds baseline ($BASELINE_WILDCARD_NOVA_INT) by $delta."
    echo "       New '_ => \"nova_int\"' wildcard introduced."
    echo
    echo "If intentional (Cat B erasure or Cat D dispatch fallback):"
    echo "  1. Add inline comment: // Plan 70 Cat B/D: <rationale>"
    echo "  2. Add entry to docs/codegen-erasure-sites.md"
    echo "  3. Bump BASELINE_WILDCARD_NOVA_INT in this script"
    echo
    echo "If silent fallback for unknown type:"
    echo "  Convert to explicit match arms or use record_strict_error."
    exit_code=1
fi

if [ "$exit_code" -eq 0 ]; then
    echo
    echo "OK — no new silent fallback violations."
fi

exit "$exit_code"
