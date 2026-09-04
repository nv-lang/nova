# hunt sem x K4 - checkpoint (final)

Report was returned as agent text (harness forbids writing report files).

## Probes that DIVERGE (cited in the report)
- f2b_named_required_with_default  - oracle refuses, novac prints 1/5
- f2c_defaulted_positional         - oracle prints 1/2, novac refuses (D102)
- p_ctor_row_becomes_overload      - oracle refuses, novac prints 107
- i1_ctor_vs_fn_same_arity         - novac: D84 ambiguity (ctor row IS an overload)
- j1_foreign_paren_ctor (+fstd/)   - handed-over named tuple: wrong-cause refusal
- p_value_paren_shape_refused      - `value(...)`: oracle prints 1/2, novac refuses
- i2_value_paren_brace             - mirror: novac prints 1/2, oracle cannot emit
- p_paren_record_method_refused    - oracle prints 7, novac says "`value` record"
- k1_value_sum_flip / k2_value_sum_control - unused newtype flips sum placement
- p_forward_field_bad_c / p_forward_value_field_bad_c - NOT sem, NOT K4:
  forward-declared field type -> check clean, emitted C does not compile

## Probes that AGREE (where I walked and found nothing)
p00_baseline_named, p_tuple_default_ok, p_tuple_param_and_return_ok,
p_tuple_copy_semantics_ok, p_empty_paren_decl_ok, p_one_field_tuple_ok,
p_field_declared_before_ok, h1_named_arg_plain_fn, i3_dup_type_name

## Probes blocked by the subset (no carrier)
e1 (newtype over record), e2 (`type One(int)`), e3 (generics),
e4 (newtype over paren record), k3_sum_eq (`==` on sums),
j2_std_casraw_ctor + j3_variants (std/src/runtime/sync.nv never reaches defs)

## Tooling (self-contained, no absolute paths inside)
- novac-only.sh <repo-relative.nv>  - emit + link + RUN when the oracle refuses
- grab-oracle-c.sh <file> <out.c>   - capture the ORACLE's own generated C
Both need the smoke argv cache: run the smoke once on p00_baseline_named first.
