# Discussion log — nova-lang

Записи о принятых решениях, отвергнутых альтернативах и пути к текущему состоянию.
Дополняет нормативные `spec/decisions/history/` (которые хранят *что* решили),
а не дублирует — здесь хранится *как* пришли к решению.

---

## Этап 1. Plan 33.3 Ф.11-Ф.14: completion Advanced contract verifier (2026-05-16)

**Запрос:** продолжение с Ф.11 (IEEE 754 FP + strings) после предыдущей сессии.

### Ключевые решения и открытия

**Ф.11: FP sort pointer stability.**
Первая попытка — кэшировать `f32_sort`/`f64_sort` в `Z3Backend` struct (как `rne`).
Падение с ACCESS_VIOLATION при использовании. Причина: `Z3_mk_fpa_sort_32` возвращает
`Z3_sort = *mut c_void`, который Z3 может GC'd если нет reference. Решение: вызывать
fresh при каждом `sort_for()` — Z3 hash-cons'ит (возвращает тот же логический sort),
но новый call держит ref. Альтернатива: добавить `Z3_inc_ref` на сорт — отвергнута
как менее очевидная и более хрупкая.

**Ф.11: var_sorts всегда пустой.**
`EncodeCtx.var_sorts` инициализировался `HashMap::new()` во всех 3 точках создания.
`is_fp_term(Var("x"))` для `fn foo(x f64)` возвращал false → Z3 получал int sort.
Решение: заполнять из `fd.params.iter().map(|p| (p.name, type_to_sort(&p.ty)))`.
Альтернатива: выводить sort из типов AST напрямую в encoder — отвергнута (сложнее,
требует type-check pass перед verify).

**Ф.11: NaN semantics.**
Контракт `ensures result >= 0.0` для `fn abs_f64(x f64)` получал counterexample
с NaN (NaN >= 0.0 = false в SMT). Это корректное поведение IEEE 754. Решение:
документировать как by-design, писать контракты которые NaN избегают (literals,
simple relations). Альтернатива: добавить `requires !fp.is_nan(x)` автоматически —
отвергнута (скрывает поведение от программиста).

**Ф.12: Хэш стабильность.**
std::hash (SipHash) нестабилен между Rust-запусками. FNV-1a реализован вручную
(без внешних крейтов) — стабилен between sessions, детерминирован.
Альтернатива: SHA256 — overkill для cache key.

**Ф.13: #trusted синтаксис.**
Проблема: `#trusted external fn` не парсится — `contract_attrs.is_empty()` возвращало
true (не учитывало is_trusted), guard 803-810 ловил `external` как неожиданный токен.
Решение 1: `is_empty()` не включает `is_trusted`. Решение 2: guard допускает
`external` как следующий токен после contract attrs. Оба применены.
Ограничение: `external fn` разрешён только в `std.runtime.*` — acceptance test
убрал external часть, тестирует только #must_verify_module.

**Ф.13: nova contracts CLI.**
`nova contracts list/verify/suggest/counterexample` — вопрос был: делать subcommand
или флаги к `nova check`. Выбран subcommand: чище, не мешает `check` flow,
легче расширять. `serde_json` добавлен как dep в nova-cli (раньше не было).

**Ф.14: let в контрактах/телах.**
Encoder Plan 33.1 не поддерживает `let` binding в телах верифицируемых функций
(straight-line code only). 4 теста из 20 упали с "encoder cannot represent".
Решение: переписать без let (inline expressions). По плану: let support — 33.5.
Это подтвердило ограничение MVP и не потребовало изменений компилятора.

### Что осталось не зафиксированным (открытые вопросы)

- Parallel verification (rayon) — Plan 33.5.
- Z3↔CVC5 cross-check — Plan 33.5 (CVC5 bindings нет).
- Set/Map SMT теории — Plan 33.5.
- `let` в телах верифицируемых функций — Plan 33.5 (encoder extension).
- Trigger pattern аннотации в SmtTerm IR — V2 Plan 33.5.
