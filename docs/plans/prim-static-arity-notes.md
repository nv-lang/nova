# Checkpoint: [M-str-primitive-static-arity-overload]

Ветка: `p-prim-static-arity` (worktree `d:/Sources/nv-lang/nova-primstat`), от main
`cf4278b35` ("wrap_owned rename ОТКАЧЕН...").

## Репро (подтверждено эмпирически)

`std/src/runtime/string/core.nv`: rename `str.wrap_owned(buf *u8, len int)` →
`str.new(buf *u8, len int)` (декл ~186 + call-сайт в `into_str_unchecked` ~275),
рядом с `export fn str.new() -> Self => ""` (~37).

- ДО фикса: `nova test std/src/checksums` → CODEGEN-FAIL 6/6,
  `[E_UNKNOWN_STATIC_METHOD] str.new(...) — у примитива str нет статического
  метода new`.
- Без rename (baseline, `wrap_owned`): PASS 3/0 (+3 skip) — подтверждено.

## Root-cause (найден и подтверждён debug-трейсом, НЕ гипотеза)

Два независимых слоя, оба нужно понимать:

### Слой 1 — codegen (compiler-codegen/src/codegen/emit_c.rs, ~39114-39258)

Multi-overload static dispatch для `Type.method(args)` (Path-форма) уже
arity-AWARE (НЕ arity-слепой, вопреки формулировке маркера): собирает
`static_overloads` из `self.method_overloads[(recv_seg, method_name)]`
(оба overload'а — 0-арг и 2-арг — РЕГИСТРИРУЮТСЯ корректно, подтверждено
debug-принтом), затем при `len()>1` фильтрует строгим `==` по
`param_c_types` (Vec<String>) против `arg_types` (инференс
`infer_expr_c_type` на аргументах call-сайта).

Debug-трейс (env `NOVA_DEBUG_STATIC=1`, инструментация временно в файле —
см. "Незакоммиченное" ниже) на `std/src/checksums/fnv_test`:

```
key=("str","new") found=Some([
  CodegenView{param_c_types:[], ..., fn_span: Span{2014,2040,file 11}},        // 0-арг
  CodegenView{param_c_types:["const nova_byte*","nova_int"], ...,
              fn_span: Span{11083,11151,file 11}},                             // 2-арг
])
arg_types=["nova_byte*", "nova_int"]   // ⚠ БЕЗ "const"
resolved_callees.get(&call_id) = None  // ⚠ чекер НЕ резолвнул этот call
```

`arg_types[0] = "nova_byte*"` (БЕЗ `const`) vs декларированный
`param_c_types[0] = "const nova_byte*"` (С `const`) — тот же Nova-тип `*u8`
(ro pointee), но РАЗНАЯ C-сериализация (const-квалификатор). Строгий `==`
даёт 0 совпадений → codegen проваливается в legacy/primitive-guard fallback
→ `E_UNKNOWN_STATIC_METHOD`.

Т.е. маркер «арность-слепой» неточен: арность УЖЕ проверяется
(`param_c_types.len() == arg_types.len()`); слепота — к
Nova-типо-эквивалентным, но по-разному сериализованным C-строкам
(const pointee). Аргумент `buf` в `into_str_unchecked` приходит из
`ro buf = @ptr()` — ro-биндинг выбирает ro-overload `@ptr() -> *u8`, что и
даёт несовпадающую C-строку (см. NB-коммент в `core.nv` про
"arg-binding не сужает *mut→*ro при подборе оверлоада").

### Слой 2 — чекер (compiler-codegen/src/types/mod.rs, ~10922-10990)

`ExprKind::Path(parts) if parts.len()==2` (Type.method(args) resolution
в чекере, канал `resolved_callees`): есть explicit gate

```rust
let is_primitive_recv = matches!(parts[0].as_str(), "str"|"int"|"char"|"bool"|"f32"|"f64"|"u8"|...);
if is_primitive_recv {
    return;   // <-- ПОЛНОСТЬЮ пропускает resolved_callees для ЛЮБОГО примитива
}
match overloads {
    Some([single]) => single,
    Some(multi) => {
        // arity-aware compat-check + resolved_callees.insert() —
        // ТОЛЬКО для non-primitive (Vec/HashMap/user types)
        ...
    }
    None => return,
}
```

История гейта (комментарий Plan 91.8a.2 followup, 2026-05-29): защита от
ложных E7301 — `self.method_overloads()`/`self.sig.method_table` может
знать НЕ ВСЕ overload'ы примитива (внешние/derive-only живут в
`external_registry`, не в `method_table`), так что "single known overload"
arg-check ложно ругался (`str.from(char)` при известном чекеру только
`str.from(bool)`). Фикс тогда — ПОЛНОСТЬЮ выключить резолв для примитивов
(`return` до `match overloads`), включая multi-overload ветку — хотя риск
неполноты набора overload'ов относится в первую очередь к SINGLE-overload
case (чекер видит 1, но их реально ≥2), а НЕ к multi-overload arity+compat
case (если чекер уже видит ≥2 РЕАЛЬНЫХ, полностью объявленных в одном
модуле overload'а — ситуация ИДЕНТИЧНА non-primitive multi-overload сайту,
для которого этот же механизм уже используется в проде безопасно).

`resolved=None` в debug-трейсе выше — прямое подтверждение: канал вообще
не сработал для этого call-сайта, codegen предоставлен сам себе.

## Прецедент (прочитан, не изобретён заново)

Plan 200 п.5 ([M-vec-new-static-arity-overload]) чинил ТОТ ЖЕ класс для
GENERIC типов, но на СЛОЕ callnorm.rs (`pick_static_params`/
`Sigs::static_methods`) — это pass ДЕФОЛТ-АРГ backfill (переписывает
call с опущенными default'ами в позиционный вид), НЕ финальный codegen
dispatch. `callnorm.rs`'s `static_methods: HashMap<(String,String),
Vec<Vec<Param>>>` УЖЕ собирает примитивы наравне с generic-типами (ключ —
просто `(recv.type_name, f.name)`, никакого primitive-gate там нет) —
т.е. callnorm.rs НЕ является источником этого бага; он относится к другому
слою (default-arg backfill), не к финальному C-дизамбигуации.

Родственный, уже рабочий прецедент ИМЕННО для финального codegen-dispatch —
`resolved_callees`/`fn_span`-канал, используемый в EMIT_C.RS в других
местах (НЕ для static-Path, а для instance-method / consume-detection):
- `call_consume_arg_idxs` (~emit_c.rs:26297-26299):
  `self.resolved_callees.get(&e.id).and_then(|sp| sigs.iter().find(|s| s.fn_span == Some(*sp))).or_else(|| sigs.last())`
- facade instance-dispatch (~emit_c.rs:37847-37855): аналогичный
  `fn_span`-match с фильтром на `fn_ret_by_span.contains_key`.

ПРОВЕРЕНО (debug): для str.new этот канал ПУСТ (чекер его не наполняет —
Слой 2 выше), так что «просто прочитать канал в emit_c.rs» НЕ решит —
сначала чекер должен НАЧАТЬ его наполнять для примитивных Path-static
вызовов (тем же способом, каким уже наполняет для non-primitive).

## План фикса (следующий шаг, ещё НЕ применён к коду)

1. **types/mod.rs (~10940-10990)**: сузить `is_primitive_recv { return; }`
   так, чтобы он бил ТОЛЬКО по `Some([single])`-ветке (сохраняя историческую
   защиту Plan 91.8a.2 от ложных single-overload false-positives), но
   ПРОПУСКАЛ примитивы через `Some(multi) => {...}` arity+compat-check
   (ту же логику, что уже безопасно работает для non-primitive) —
   ТОЛЬКО когда `overloads` содержит ≥2 ЗАРЕГИСТРИРОВАННЫХ в
   `self.sig.method_table` кандидата (что и есть наш случай: оба
   объявления `str.new` живут в одном модуле, чекеру ОБА известны).
   Это заполнит `resolved_callees[call_id]` для `str.new(buf, n)`.
2. **emit_c.rs (~39114-39258, ветка Path-form static dispatch)**: добавить
   channel-first lookup ПЕРЕД строгим C-type-string match — мирроринг
   `call_consume_arg_idxs`/facade-паттерна:
   ```rust
   let chosen = if static_overloads.len() == 1 {
       Some(static_overloads[0].clone())
   } else {
       self.resolved_callees.get(&call_id)
           .and_then(|sp| static_overloads.iter().find(|s| s.fn_span == Some(*sp)))
           .cloned()
           .or_else(|| /* существующий arity+string-type strict match, unchanged */)
   };
   ```
   Single-overload путь — БЕЗ изменений (byte-identical). Multi-overload
   без channel-хита (напр. call_id unset/synthesized) — падает на прежний
   string-match (unchanged для всех остальных существующих сайтов).
3. Вернуть rename `wrap_owned` → `str.new(buf, len)` (декл + вызов в
   `into_str_unchecked`), снять NB-комментарии-блокеры.
4. Верификация: red→green `nova test std/src/checksums`
   (CODEGEN-FAIL 6/6 → PASS 3/0), δ0 на `std/src/collections/vec` +
   `std/src/crypto`, standalone-фикстура на `unsafe { bytes.into_str_unchecked() }`
   + существующий `str.new()`-0-арг тест (d372) остаётся зелёным.
5. Закрыть маркер [M-str-primitive-static-arity-overload] в
   backlog-followups.md + лог в docs/simplifications.md.

## Незакоммиченное на момент чекпоинта (это WIP-коммит, будет доработано)

- `std/src/runtime/string/core.nv` — временный repro-rename
  `wrap_owned`→`new` (декл + call-сайт) — красный ДО фикса, ожидаемо.
- `compiler-codegen/src/codegen/emit_c.rs` — временная debug-инструментация
  (`NOVA_DEBUG_STATIC` env-gated `eprintln!`, 3 места ~39114/39122/39241) —
  используется ТОЛЬКО для эмпирической разведки выше; будет УДАЛЕНА перед
  финальным коммитом фикса (не часть фикса, не относится к диффу).

## Хэши

- Base (main, HEAD в момент старта): `cf4278b35`.
- Release-бинарь собран дважды в ходе разведки (debug eprintln итерации);
  оба раза `cargo build --release` в `nova-cli/` чистый (только
  pre-existing warnings).
