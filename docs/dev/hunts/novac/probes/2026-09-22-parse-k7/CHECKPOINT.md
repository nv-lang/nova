# Hunt: novac / parse x K7 -- 2026-09-22

Binaries: oracle D:/Sources/nv-lang/nova/nova-cli/target/release/nova.exe (09-21 04:43);
novac D:/Sources/nv-lang/nova/novac/target/novac.exe (09-21 22:57) -- NEWER than every
novac/src/parse/*.nv (newest expr.nv 09-21 22:45), so the runs judge these sources.
Worktree parse/*.nv md5-identical to main tree's.

Run: cd D:/Sources/nv-lang/nova && sh <probe-dir>/cmd.sh D:/Sources/nv-lang/nova

| probe dir | oracle | novac |
|---|---|---|
| p1-qualified-alt            | PASS | UNNAMED FALLBACK at byte 97 (`.` of `Kind.B`) |
| p1c-control-qualified-single| PASS | green (no output) |
| p2-intlit-alt               | PASS | UNNAMED FALLBACK at byte 63 (`|`) |
| p2c-control-intlit-single   | PASS | green (no output) |
| p3-record-variant-alt       | PASS | named record-form refusal + UNNAMED FALLBACK at 141 |
| p3c-control-record-variant-single | PASS | named refusals only, NO fallback |
| p4-slice-of-pointer  []*int | PASS | UNNAMED FALLBACK at 24-36 + false "function without a declared return type" |
| p4c-control-slice-of-named [][]int | PASS | named type-universe refusal only |
| p4c2-control-bare-pointer *int | PASS | named "this type form is not compiled yet" only |
| p6-iflet-mut-binder         | PASS | 2x UNNAMED FALLBACK (55,62) + false "record constructor" (64) |
| p6c-control-iflet-ro-binder | PASS | green (no output) |
| p7-whilelet                 | PASS | UNNAMED FALLBACK (76) + false "record constructor" (78) |
| p7c-control-plain-while     | PASS | green (no output) |
| p8-iflet-two-binders        | PASS | UNNAMED FALLBACK (94) + false "record constructor" (96) |
| p8c-control-iflet-one-binder| PASS | NAMED refusal "`if <pattern> = ...` takes only `Some(name)` here" |
| p9-iflet-guard-and          | PASS | MISPARSE: "unknown name" at byte 65 = the `v` bound one token earlier |
| p10-iflet-tuple-pattern     | PASS | named tuple-type refusal + UNNAMED FALLBACK (64) + false "record constructor" (66) |

## Addresses

F1 match-arm alternative: novac/src/parse/expr.nv:516-530 vs head at :428-434, :476-479, :487-495
F2 slice of pointer:      novac/src/parse/type_ref.nv:67-74 (Star) vs :75 (EmptyBrackets loop)
F3 if/while pattern-bind: novac/src/parse/parse.nv:632-639 (lookahead), :808-819 (while), spec 03-syntax.md:1607-1625
