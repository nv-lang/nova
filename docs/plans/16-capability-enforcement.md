# План 16: Capability enforcement — `forbid` и `realtime` compile-time checks

**Статус:** ✅ **ЗАКРЫТ** (2026-05-10). Ф.1-Ф.9 реализованы;
nova_tests **97/97 PASS** (92 baseline + 6 positive forbid_realtime +
5 negative-capability).
**Дата создания:** 2026-05-08.
**Зависимости:** [D63](../../spec/decisions/04-effects.md#d63),
[D64](../../spec/decisions/04-effects.md#d64) уже описывают синтаксис.

> **Архитектурное уточнение vs изначальный план:** план 16 предлагал
> ставить enforcement в codegen (`emit_c.rs:EmitContext`), но Plan 15
> установил precedent для check'ов в type-checker'е (`types/mod.rs::BoundCtx`).
> Реализовано там же — `CapabilityCtx` рядом с `BoundCtx`, тот же
> visitor-pattern с push/pop state'ом. Codegen остался без изменений
> (Forbid/Realtime эмитятся как обычный block — runtime барьер
> отдельная задача).

---

## Проблема

Spec-то говорит «sandbox в типах, не в рантайме» (R5/R6 в
revolutionary.md), а codegen эмитит body как plain block без проверок:

```nova
// текущее поведение compiler'а:
fn run_user_script(code str) Fail -> Result =>
    forbid Net, Fs, Db {
        eval(code)               // ← Net.* call здесь = НЕ ловится
    }

fn realtime_audio(buf []f32) -> () =>
    realtime nogc {
        let arr = []int.new()    // ← managed alloc здесь = НЕ ловится
    }
```

`emit_c.rs:4139` (`forbid`) и `:4143` (`realtime`) — оба эмитят
содержимое как обычный block. Семантические гарантии **не
выполняются**. Это major spec-vs-impl drift, который может **скрыть
ошибки до production**.

---

## Что нужно

### Forbid (D63)

При `forbid X1, X2 { body }`:

1. На вход — список запрещённых эффектов `{X1, X2}`.
2. Внутри body — для каждого вызова функции `f(...)`: посмотреть
   эффекты в её сигнатуре. Если **прямые** эффекты пересекаются с
   запрещёнными — compile error на месте вызова:
   ```
   error E0144: function `http.get` requires effect `Net`,
     forbidden by enclosing `forbid Net` block
       at src/main.nv:42
     │
     │   forbid Net, Fs {
     │       http.get(url)        // ← вот тут
     │       ^^^^^^^^^^^^^
     ```
3. Транзитивные эффекты (по D62 — частичный contract) — **warning**,
   не error. С опцией `--strict-forbid` поднимать до error.
4. Forbid внутри forbid — union эффектов.

### Realtime (D64)

При `realtime { body }` или `realtime nogc { body }`:

1. Запретить **suspend-операции** — channel.recv (без `try_recv`),
   `Time.sleep`, `Net.*`, `Db.*`, `Fs.*` — всё что может
   приостановить fiber.
2. Для `realtime nogc` — также запретить **managed-heap аллокации**:
   `[]T.new()`, `[]T.with_capacity()`, `Type.new()` (если new-конструктор
   требует alloc), `str.from()` если конкатенация может alloc'ить.
3. Внутри realtime разрешён `region { ... }` — arena-allocations
   (D6).

### Сообщения

Структурированные ошибки по [R5.3](../../spec/revolutionary.md#r5-3) —
показать enclosing-scope, причину, патч.

---

## Фазы

### Ф.1 — Capability context в type-checker'е ✅ ЗАКРЫТ (2026-05-10)

**Файлы:** `compiler-codegen/src/types/mod.rs`.

Реализовано как `CapabilityCtx<'a>` рядом с `BoundCtx<'a>` (Plan 15
pattern). State через `CapState`:
```rust
struct CapState {
    forbidden_stack: Vec<HashSet<String>>,
    realtime_active: bool,
    realtime_nogc: bool,
    with_handler_stack: Vec<String>,
}
```

`forbidden_stack` — стек set'ов forbidden-эффектов от вложенных
`forbid` блоков. `union_forbidden()` берёт union всех уровней
(forbid внутри forbid → union, см. D63).

Walk_module → walk_fn_body → walk_block → walk_stmt → walk_expr с
mutable state. На входе/выходе из:
- `ExprKind::Forbid { effects, body }` — push/pop forbidden-set;
- `ExprKind::Realtime { nogc, body }` — set/restore realtime_active +
  nogc флагов;
- `ExprKind::With { bindings, body }` — push/pop with_handler_stack
  (для D63 forbid-handler-ban).

Также top-level fn с `RealtimeAttr::Realtime|RealtimeNogc`
оборачивает body в realtime-context (Ф.5 sugar).

**Объём:** ~150 строк (visitor + state mgmt).

### Ф.2 — Forbid intersection check ✅ ЗАКРЫТ (2026-05-10)

`check_capabilities_at` на каждом ExprKind::Call:

1. Извлекаем `path: Vec<String>` из func.kind:
   - `ExprKind::Path(parts)` → as-is.
   - `ExprKind::Member { obj: Path([..]), name }` → flat join (или
     special-case `[]T.method` через `Path(["__array", T])`).
   - `ExprKind::Member { obj: Ident, name }` → `[ident, name]`.
   - `ExprKind::Ident(n)` → `[n]`.
2. Для path длины 2: если head — registered effect-type → check
   forbid-intersection (D63) + suspend-effect (D64).
3. Для free-fn (path.len 1) или method (path.len 2 в method_table) —
   `check_callee_effects`: для каждого `eff` в `callee.effects`:
   - intersection с forbidden_union → R5.3 error «requires effect X,
     forbidden by enclosing forbid block».
   - realtime + suspend-effect → R5.3 error «cannot suspend in
     realtime».

**Объём:** ~80 строк.

### Ф.3 — Realtime suspend checks ✅ ЗАКРЫТ (2026-05-10)

`realtime_suspend_effect` whitelist: `Net | Fs | Db | Time |
Blocking`. Любой callee (effect-op или fn) с этими эффектами внутри
realtime — R5.3 error с hint'ом «use try_recv / non-blocking
alternative».

**Объём:** ~10 строк (set + check).

### Ф.4 — Realtime nogc alloc-fn enumeration ✅ ЗАКРЫТ (2026-05-10)

`nogc_blacklisted_call(path: &[String])`:
- `[]T.new` / `[]T.with_capacity` (T — element type).
- `StringBuilder | WriteBuffer | ReadBuffer.{new,with_capacity,from}`.
- `Channel.{new,with_capacity}`.
- `HashMap | Set | Vec | Deque | LinkedList | Lru | BloomFilter.{new,with_capacity}`.
- `str.from`.

При realtime_nogc + matching call → R5.3 error «cannot allocate
inside `realtime nogc` block (D64). Hint: use region {} for arena
allocations».

Не покрывается (явно в коде помечено как TODO):
- User-defined record-конструкторы `Foo.new()` (требовал бы effect-row
  inference чтобы flag'ать heap-alloc'ирующих).
- Транзитивные alloc'ы через chains pure-fn → alloc-fn.

**Объём:** ~30 строк (whitelist + check).

### Ф.5 — `@realtime` attribute (D64 sugar §3697) ✅ ЗАКРЫТ (2026-05-10)

AST: новый `RealtimeAttr { None | Realtime | RealtimeNogc }` enum;
`FnDecl.realtime_attr: RealtimeAttr` поле.

Parser: `parse_realtime_attr()` ловит `@realtime` / `@realtime nogc`
префикс перед `fn`. Использует `KwRealtime` keyword (lexer).

Type-checker: `check_module` оборачивает body fn'а с
`realtime_attr=Realtime|RealtimeNogc` в realtime-context (init state
с realtime_active=true и nogc=true для RealtimeNogc).

**Объём:** ~30 строк (AST + parser + check_module init).

### Ф.6 — Forbid-handler ban (D63 §3473) ✅ ЗАКРЫТ (2026-05-10)

При `ExprKind::With { bindings, body }`: для каждого binding'а берём
имя effect'а из `b.effect: TypeRef::Named { path, .. }` (last
segment). Если оно в `state.union_forbidden()` — R5.3 error «cannot
install handler for `X` inside `forbid X` block (D63): forbid is
impenetrable».

**Объём:** ~20 строк.

### Ф.7 — Negative-test infrastructure ✅ ЗАКРЫТ (2026-05-10)

`run_tests.ps1` — добавлен scan первых 30 строк .nv на маркер
`// EXPECT_COMPILE_ERROR <pattern>`. Если найден:
- Codegen ожидается с **non-zero** exit.
- Stderr должен содержать `<pattern>` (substring).
- Иначе — NEG-NO-ERROR / NEG-WRONG-MSG fail.

Файл с маркером не компилируется в .c → .exe и не запускается.

**Объём:** ~46 строк в run_tests.ps1.

### Ф.8 — Тесты ✅ ЗАКРЫТ (2026-05-10)

Расширен `nova_tests/effects/forbid_realtime.nv` (+41 строк):
- `forbid + pure_fn` — type-check OK.
- `@realtime fn rt_compute()` — body работает как realtime.
- `@realtime nogc fn rt_nogc_compute()` — pure body OK.

Новые `nova_tests/negative_capability/` (5 файлов, по 1 EXPECT_COMPILE_ERROR
маркеру каждый):
1. `forbid_effect_call.nv` — fn с effect внутри forbid.
2. `forbid_handler_ban.nv` — `with X = ...` внутри `forbid X`.
3. `realtime_suspend.nv` — Net.* в realtime.
4. `realtime_attr_suspend.nv` — `@realtime fn` использует Time.sleep.
5. `realtime_nogc_alloc.nv` — `[]int.new()` в realtime nogc.

Прогон: nova_tests **97/97 PASS**.

### Ф.9 — Spec implementation note ✅ ЗАКРЫТ (2026-05-10)

`spec/decisions/04-effects.md` D63: добавлен раздел «Реализация в
bootstrap (2026-05-09, Plan 16 Ф.1-Ф.6)» с описанием
`CapabilityCtx`, walk pattern, forbid-handler-ban механизм.

D62 «прямые vs транзитивные» — без правок (текущий permissive подход
консистентен с warning-уровнем спека). Полное transitive tracking —
отдельный план (требует effect-row inference).

---

## Что НЕ делаем

- **Async-effect blocking detection** — D62 говорит Async — ambient,
  а forbid Async запрещён. Не трогаем.
- **Closure capture эффектов** — если closure захватывает handler
  через `with`, эффект "уносится" в lambda. Полное отслеживание —
  отдельный план (нужен полноценный effect-row inference, что не
  bootstrap-scope).
- **Runtime sentinel-frame для transitive effects** — D63 упоминает,
  это runtime-mechanism для plug-in scenarios, не AOT. Не сейчас.

---

## Оценка (factual, после реализации)

- AST: ~15 строк (`RealtimeAttr` enum + `FnDecl.realtime_attr`).
- Parser: ~46 строк (`parse_realtime_attr` + integration в parse_item/fn).
- Type-checker (`CapabilityCtx`): ~492 строки (visitor + check + helpers).
- Test infra (`run_tests.ps1`): ~46 строк (EXPECT_COMPILE_ERROR scan).
- Tests: 1 extended + 5 new negative.
- Spec: ~19 строк (D63 implementation note).

**Итого: ~657 строк** core + ~80 строк tests + ~19 спека.

Реальный путь занял ~3 сессии: Plan 16 Ф.1-Ф.6 написан, AV
заблокировал build, recovery-script сохранил work как WIP commit
`be953d6`, после reboot fixed 3 corner case bug'а (`effect_name`,
`KwRealtime`, `Member<Path>` paths) и dochi sync'нут.

---

## Связь

- [Plan 14](14-stdlib-codegen-gaps.md) — параллельный, не зависит.
- [Plan 15](15-generic-bounds-enforcement.md) — параллельный.
- [spec/revolutionary.md → R6](../../spec/revolutionary.md) — capability
  security, главное обоснование плана.

---

## Ссылки

- [spec/decisions/04-effects.md → D63](../../spec/decisions/04-effects.md#d63) — forbid.
- [spec/decisions/04-effects.md → D64](../../spec/decisions/04-effects.md#d64) — realtime.
- [spec/decisions/04-effects.md → D62](../../spec/decisions/04-effects.md#d62) —
  прямые vs транзитивные эффекты.
- `compiler-codegen/src/codegen/emit_c.rs:4139` — текущий forbid emit.
- `compiler-codegen/src/codegen/emit_c.rs:4143` — текущий realtime emit.
