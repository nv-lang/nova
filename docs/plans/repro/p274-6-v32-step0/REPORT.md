ЗАМЕР | f1_tuple_bind | оракул: 1/2 | novac-exit=0 | novac: молчит
ЗАМЕР | f2_paren_ctor_bind | ORACLE-REFUSED: refutable pattern in `let`: `Point` is a variant-pattern | novac-exit=1 | novac: outside the subset: a declared local type is not compiled yet (E2-b3)
ЗАМЕР | f3_brace_full | оракул: 1/2 | novac-exit=1 | novac: syntax error: this is not a form of the language
ЗАМЕР | f4_brace_rest | оракул: 1 | novac-exit=1 | novac: syntax error: this is not a form of the language
ЗАМЕР | f5_brace_partial_no_rest | ORACLE-REFUSED: record-pattern binding lists 1 of 2 field(s) of `Point` without `..` | novac-exit=1 | novac: syntax error: this is not a form of the language
ЗАМЕР | f6_mut_tuple_bind | оракул: 11/2 | novac-exit=0 | novac: молчит
ЗАМЕР | f7_scrutinee_once | оракул: 99/1/2 | novac-exit=0 | novac: молчит
ИТОГО | проб 7 | оракул собрал: 5 | оракул отказал: 2 | novac молчит: 3 | novac отказал: 4
БИНАРИ | novac.exe: 2026-09-03 18:18 | nova.exe: 2026-09-03 16:55
