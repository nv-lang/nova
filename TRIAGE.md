# TRIAGE: 76 кодов из MISSING_CODES.txt

Триаж кодов `E_*`/`W_*`, встречающихся в спеке, но отсутствующих в `.rs`-файлах
компилятора. Проверка: грепом по `spec/`, затем по `compiler-codegen/src`,
`nova-cli/src` (2026-08-07). Категории: A — реально беззубая норма, B —
самоописанная отложенность/ретракция, C — реализовано под другим именем,
D — код-заглушка/пример.

| Код | Категория | Обоснование (1 строка) | Если C — под каким именем |
|---|---|---|---|
| E_ADDR_OF_MUT_REQUIRES_MUT_BINDING | C | Переименован в Plan 118.6 (09-tooling.md:3177 «rename from»); семейство addr_of удалено, вызовы ловит E_ADDR_OF_REMOVED | E_ADDR_OF_REMOVED (const_fn_eval.rs:2473, types/mod.rs:32990) |
| E_AMP_CONST_BINDING | A | `&const_value` в списке кодов D216 (02-types.md:10509) без пометки отложенности; эквивалентной проверки в компиляторе не найдено (есть только E_AMP_LITERAL/E_AMP_RECORD_LITERAL) — требует ручной проверки | — |
| E_AT_RETURN_OUTSIDE_METHOD | C | Проверка `-> @` вне instance-метода есть в парсере (parser/mod.rs:3491), но эмитится голым сообщением без кодового префикса | (голое сообщение, нет кода) |
| E_AUTO_DERIVE_FIELD_LACKS_ | D | Обрубок в тексте спеки (02-types.md:16756) — полный код E_AUTO_DERIVE_FIELD_LACKS_PROTOCOL | — |
| E_BINDING_NOT_MUT | C | `mut @method` на ro-биндинге (02-types.md:4772) ловится под именем E_RECEIVER_BINDING_NOT_MUT (types/mod.rs:42606) | E_RECEIVER_BINDING_NOT_MUT |
| E_BOUND_MISSING | B | 02-types.md:13800 «Known limitation: checker does not validate field Debug bounds … produces CC-FAIL, not E_BOUND_MISSING» — честно помечено как невведённое | — |
| E_CAST_RAW_FN_TO_CLOSURE | A | `*fn → fn` cast вне unsafe (02-types.md:9870,10523); эквивалентной проверки не найдено (E_CLOSURE_HAS_ENV/E_CALLBACK_THROWS — обратное направление fn→*fn) — требует ручной проверки | — |
| E_CLEANUP_FORBIDDEN_OPERATION | C | Запрет spawn/parallel/supervised в cleanup-теле есть для defer (types/mod.rs:44302, голое сообщение «D159-spawn-in-defer»); покрытие тел @cleanup-методов требует ручной проверки | (голое сообщение, нет кода) |
| E_COALESCE_RETURN_ | D | Обрубок `E_COALESCE_RETURN_FALLBACK` через перенос строки (04-effects.md:5028) | — |
| E_CODE | D | Заглушка-плейсхолдер в тексте о префиксах диагностик (09-tooling.md:3134) | — |
| E_COERCE_AMBIGUOUS | B | 02-types.md:17491 «честная пометка: … не встречается нигде в текущем коде компилятора»; followup [M-coerce-r5-ambiguous-overload-unimplemented] | — |
| E_COERCE_GENERIC_UNSUPPORTED | B | RETRACTED (02-types.md:17698-17703), заменён на E_COERCE_GENERIC_PATTERN_UNSUPPORTED (есть в компиляторе) с другой семантикой | — |
| E_COMPARISON_BOOL_OPERAND | B | Только вариант (a) в spec/open-questions.md:7862 — предложение, не действующая норма | — |
| E_CONST_FN_GENERIC | C | V1-запрет generic-параметров снят (Ф.4 V2 generic const fn, 03-syntax.md:8004); остаточное ограничение T-reflection под именем E_CONST_FN_GENERIC_NEEDS_T_REFLECTION | E_CONST_FN_GENERIC_NEEDS_T_REFLECTION (const_fn_eval.rs:2079) |
| E_CONST_FN_MUT_BINDING | B | V1-запрет снят в V3 (Plan 114.4.4 Ф.3) — mut-биндинги в const fn разрешены (types/mod.rs:2875) | — |
