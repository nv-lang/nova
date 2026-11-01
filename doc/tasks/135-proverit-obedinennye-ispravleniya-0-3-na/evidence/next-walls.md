# Следующие стены объединённого сида

Ранжирование по первой диагностике входа, не по количеству каскадных сообщений; 118+19, финальный patched-scratch-driver-v2. Это наблюдаемые сообщения, не доказательство общего корня разных носителей. Пути в сообщениях нормализованы только для группировки.

## 18 входов: E_NOVAC_SUBSET :: novac did not parse this -- and it cannot tell whether the form is one it does not read yet or the text is malformed; this is the parser's fallback, narrowed form by form

- logs-118/L048.txt
- logs-118/L064.txt
- logs-118/L074.txt
- logs-118/L078.txt
- logs-118/L079.txt
- logs-118/L080.txt
- logs-118/L081.txt
- logs-118/L082.txt
- logs-118/L083.txt
- logs-118/L085.txt
- logs-118/L093.txt
- logs-118/L098.txt
- logs-118/L103.txt
- logs-118/L104.txt
- logs-packages/nova-polaris__examples_01-hello_src_main.txt
- logs-packages/nova-polaris__examples_06-static-site_src_main.txt
- logs-packages/nova-polaris__examples_12-https_src_main.txt
- logs-packages/nova-postgres__examples_session-check_src_main.txt

## 16 входов: E_NOVAC_SUBSET :: fn declares a return type but its body ends without a value

- logs-118/L086.txt
- logs-118/L087.txt
- logs-118/L088.txt
- logs-118/L089.txt
- logs-118/L090.txt
- logs-118/L091.txt
- logs-packages/nova-duckdb__examples_01-hello_hello_duckdb.txt
- logs-packages/nova-duckdb__examples_02-appender-history_appender_history.txt
- logs-packages/nova-duckdb__examples_03-with-tls_with_tls.txt
- logs-packages/nova-polaris__examples_02-routing_src_main.txt
- logs-packages/nova-polaris__examples_04-middleware_src_main.txt
- logs-packages/nova-polaris__examples_07-sse-stream_src_main.txt
- logs-packages/nova-polaris__examples_08-websocket-echo_src_main.txt
- logs-packages/nova-polaris__examples_09-graceful_src_main.txt
- logs-packages/nova-postgres__examples_live_src_main.txt
- logs-packages/nova-postgres__examples_smoke_src_main.txt

## 4 входов: E_NOVAC_SUBSET :: outside the subset: an effect attribute is read but not compiled yet

- logs-118/L076.txt
- logs-118/L077.txt
- logs-118/L084.txt
- logs-packages/nova-polaris__examples_05-auth_src_main.txt

## 4 входов: E_NOVAC_SUBSET :: outside the subset: this type declares several static methods of this name and arity, and novac does not narrow them by argument type yet (E2-b3)

- logs-118/L094.txt
- logs-118/L095.txt
- logs-118/L101.txt
- logs-118/L102.txt

## 4 входов: compile/link :: undefined symbol: nova_fn_main_impl

- logs-118/L072.txt
- logs-118/L109.txt
- logs-118/L110.txt
- logs-118/L118.txt

## 3 входов: E_NOVAC_SUBSET :: outside the subset: `#impl` of this protocol is derived only for `Display`, `Equal` and `Serialize` -- the derived methods of the others (`Deserialize`, D341; `Hash`, `Clone`, `Compare`, `Debug`) are not synthesized yet (E2-b3)

- logs-118/L100.txt
- logs-packages/nova-polaris__examples_03-json-api_src_main.txt
- logs-packages/nova-polaris__examples_10-mini-service_src_main.txt

## 3 входов: E_NOVAC_SUBSET :: outside the subset: effect `Counter` has no runtime pieces in the shell novac links into -- its operation table, thread slot and dispatchers are the shell's, carried for the std effects its probe uses; an effect a file declares is not emitted yet (E5, effect declarations)

- logs-118/L062.txt
- logs-118/L063.txt
- logs-118/L106.txt

## 3 входов: E_NOVAC_SUBSET :: outside the subset: this type form is not compiled yet (slices, tuples, fn types and qualifiers arrive with generics, E2-b)

- logs-118/L066.txt
- logs-118/L067.txt
- logs-118/L108.txt

## 2 входов: E_NOVAC_LANG :: `break` and `continue` stand only inside the body of a `for` or `while` loop -- there is no loop around this one

- logs-118/L096.txt
- logs-118/L097.txt

## 2 входов: E_NOVAC_LANG :: this cast is no conversion the language has: `() as int` -- the language names no such conversion, and a cast without a rule is an error (D54)

- logs-118/L068.txt
- logs-118/L071.txt

## 2 входов: E_NOVAC_SUBSET :: outside the subset: a handler literal is compiled only as a `with` binding's handler or a local binding's initializer (`ro h = effect E { ... }`) that captures no `mut` binding -- a handler value in any other position, and a held one whose `mut` capture would be an address into its frame, is not compiled yet (E4, boundary E.5 of plan 274.11)

- logs-118/L099.txt
- logs-packages/nova-polaris__examples_11-json-logs_src_main.txt

## 1 входов: E_NOVAC_LANG :: outside the subset: `defer` is not compiled yet -- running the body at scope exit, LIFO, on every way out (D90) is the effects/consume horizon's debt (E2)

- logs-118/L105.txt

## 1 входов: E_NOVAC_LANG :: this variant name is declared by more than one sum in scope -- qualify it as `Type.Variant` so the sum is named (D406; registry #964)

- logs-118/L069.txt

## 1 входов: E_NOVAC_SUBSET :: a bare `None` here has no type to take: it stands for an Option only where a return type says which one (`-> Option[int]`, as a tail or a `return` operand) -- a typed binding or a later assignment is not compiled yet (E2-b3)

- logs-118/L114.txt

## 1 входов: E_NOVAC_SUBSET :: in the instance of `to_uppercase`: outside the subset: `[]T.new(...)` takes no arguments here -- a capacity (or the raw-parts form) is not compiled yet; write `[]T.new()` (274.5 P3, registry 221.1 #1457)

- logs-118/L113.txt

## 1 входов: E_NOVAC_SUBSET :: outside the E1 subset: generic parameters are not compiled yet (planned for E2-b2)

- logs-118/L073.txt

## 1 входов: E_NOVAC_SUBSET :: outside the subset: a format specifier in string interpolation (`${expr:?}`, D229) is not compiled yet (274.7 B1b, the Display family) -- write the value plain and format it by hand

- logs-118/L115.txt

## 1 входов: E_NOVAC_SUBSET :: outside the subset: a spread `...xs` inside an array literal is not compiled yet (274.5 P5)

- logs-118/L008.txt

## 1 входов: E_NOVAC_SUBSET :: outside the subset: an array literal is compiled only as an initializer, a call argument, a constructor field, the body of a match arm, the right side of `=` or the tail of an `if`-value branch -- other positions are not compiled yet (E2, the basics horizon)

- logs-118/L075.txt

## 1 входов: E_NOVAC_SUBSET :: outside the subset: an extern nova fn declaration -- only the C ABI is compiled yet, by the declaration's literal name; the registry-spelled runtime symbol arrives with the shell wave (D282, E2-b2)

- logs-118/L117.txt

## 1 входов: E_NOVAC_SUBSET :: outside the subset: an interpolation slot of a non-primitive type without its own `@display(mut f Fmt)` (`Option[T]`, a record that relies on `#impl(Display)`, a generic instance) is not compiled yet (274.7 B1b) -- every primitive family and every plain type with `@display` is

- logs-118/L070.txt

## 1 входов: E_NOVAC_SUBSET :: outside the subset: effect `Audit` has no runtime pieces in the shell novac links into -- its operation table, thread slot and dispatchers are the shell's, carried for the std effects its probe uses; an effect a file declares is not emitted yet (E5, effect declarations)

- logs-118/L092.txt

## 1 входов: E_NOVAC_SUBSET :: outside the subset: the shell novac links into carries no `Result` instance for `NovaRes_nova_int_Nova_ParseErr_p` -- the interop surface is the probe program (novac/probe/shell_probe.nv, E2-b); novac emits no `Result` instance of its own yet (E2-b3)

- logs-118/L107.txt

## 1 входов: E_NOVAC_SUBSET :: unknown name: nothing with this name is bound at this point

- logs-118/L065.txt

## 1 входов: dependency :: remote-not-found

- logs-packages/nova-sdl__probes_thread-affine_src_main.txt
