# [M-freefn-named-default-arg-shift] — чекпоинт

Статус: фикс написан и собран (compiler-codegen + nova-cli release), прогоняю
верификацию (standalone-репро → core_test → d102/d372 regress).

## Корень (НЕ arg-shift в самом call'е)

Гипотеза интегратора («named-арг сдвигает аргументы в callnorm») не
подтвердилась буквально: сам вызов `int_fmt(v, buf, cap, spec)`,
синтезированный `callnorm::try_normalize_call` (compiler-codegen/src/callnorm.rs),
эмитится в ПРАВИЛЬНОМ param-order — позиции v/buf/cap/spec корректны.

Настоящий баг — в `compiler-codegen/src/codegen/emit_c.rs::emit_block_expr`
(~строка 42252, до фикса). Эта функция обслуживает КАЖДЫЙ `ExprKind::Block`,
включая Block, который `callnorm` синтезирует для default/named-арг
десахара (Plan 46 D102): `{ let __nova_arg_src0 = ...; let v = ...; let buf
= ...; ...; callee(v, buf, cap, spec) }`.

`var_types: HashMap<String,String>` — ПЛОСКАЯ, НЕ per-scope карта (уже
известный класс болезни — `[M-callnorm-free-fn-name-collision]` / 196.6,
`[M-parfor-capture-callee-name-collides-std-local]`). `emit_block_expr` уже
ЗНАЛА про эту болезнь (комментарий про "stale var_types leaked from a prior
function") и делала transient overlay+restore ТОЛЬКО для инференса типа
trailing-выражения (type-probe фаза, строки 42264-42286) — но РЕАЛЬНАЯ
эмиссия statements блока (строки 42293-42295, `self.emit_stmt(stmt)`)
вставляла типы block-локалов в ТУ ЖЕ карту и НИКОГДА их не восстанавливала
после закрытия `{ }`.

Когда параметр callee называется так же, как ВНЕШНЯЯ (по отношению к
Block) переменная другого типа — например `int_fmt(v int, buf *mut u8, cap
int, spec FmtSpec = ...)` и тестовая локальная `buf []u8` (`Vec[u8]`) —
синтезированный `let buf = ...` (тип `*mut u8`) ПЕРЕЗАПИСЫВАЛ
`var_types["buf"]` НАВСЕГДА (до конца функции), и последующие
name-based intrinsic-dispatch вызовы (`buf.ptr()` — D216/D410, ветвление
по `obj_ty.starts_with("Nova_Vec")` в emit_c.rs ~36556/36595) ошибочно
резолвились на `str`'s `.ptr()` вместо `Vec`'s → CC-FAIL "passing
'Nova_Vec____nova_byte *' to parameter of incompatible type 'nova_str'".

Это НЕ callnorm-специфичный баг (не связан конкретно с named-арг
резолвом на free vs static): любой `{ }`-Block-expr с локалом,
переименовывающим внешнюю переменную другого типа, тёк бы так же. Named+
default на free fn просто СОЗДАЁТ такой Block (через param.name-биндинги)
чаще прочих путей.

## Фикс

`emit_c.rs::emit_block_expr`: snapshot `var_types` СРАЗУ ПОСЛЕ регистрации
`tmp`'s типа (чтобы `tmp` пережил restore), restore ПОСЛЕ закрытия `{ }`
блока (симметрично паттерну `saved_var_types` уже используемому для
fn/test-body в этом же файле).

## §3 (одно окно)

Фикс — ОДНО изменение в ОДНОЙ функции (`emit_block_expr`), общее для ВСЕХ
Block-expr (не отдельная ветка под callnorm/named/free-fn). Никакого
спец-кейса под конкретный класс вызова.

## Дальше по гейту

- [ ] мини-репро (spec_tests/conformance/standalone/freefn_named_default_arg_shift.nv) RED→GREEN
- [ ] core_test.nv (std/src/runtime/fmt_buf/core_test.nv) PASS 1/0
- [ ] d102/d372 фикстуры не регрессировали
- [ ] standalone FAIL:0
- [ ] коммит + маркер закрыт
