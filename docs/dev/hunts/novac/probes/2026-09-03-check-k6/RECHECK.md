СВЕРКА | p_interp_index_mixed | оракул: first=7 last=9 done | novac-check-exit=0 | novac сказал: молчит | доп: smoke stdout разходится
СВЕРКА | p_namedarg_unknown_name | оракул: ORACLE-REFUSED | novac-check-exit=0 | novac сказал: молчит | доп: emit-exit=0 молчит
СВЕРКА | p_namedarg_call | оракул: 30 | novac-check-exit=0 | novac сказал: молчит | доп: emit-exit=2
СВЕРКА | p_order_named | оракул: a b 3 | novac-check-exit=0 | novac сказал: молчит | доп: -
СВЕРКА | p_order_positional | оракул: b a 3 | novac-check-exit=1 | novac сказал: outside the E1 subset: two arguments that both call something | доп: -
СВЕРКА | p_index_arg | оракул: 8 | novac-check-exit=1 | novac сказал: this call omits `n`, a parameter with no default value | доп: -
СВЕРКА | p_index_methodarg | оракул: 14 | novac-check-exit=1 | novac сказал: outside the subset: this type has no such method | доп: -
СВЕРКА | p_index_tail | оракул: 7 | novac-check-exit=1 | novac сказал: fn declares a return type but its body ends without a value | доп: -
ИТОГО | проб 8 | оракул собрал: 7 | novac check молчит: 4 | novac отказал: 4
БИНАРИ | novac.exe mtime: 2026-09-03 18:18 | nova.exe mtime: 2026-09-03 16:55
