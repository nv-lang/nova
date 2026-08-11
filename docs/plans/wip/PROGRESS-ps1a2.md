# PROGRESS — окно p-s1a2: дыра №289 в S1a (личный контроль владельца, P0)

worktree: `d:/Sources/nv-lang/nova-s1a2`, branch `ps1a2` от main `96960a5d5`.
Модель: Claude Sonnet 5 (claude-sonnet-5).

## Суть находки

S1a (`spawn_tainted_fields_of_module` + `spawn_tainted_method_params_of_module`,
`compiler-codegen/src/types/mod.rs`) считала «пятую точку» D441 §3(г)
(«замыкание как ПОЛЕ структуры») закрытой, но её пре-пасс:
1. сканировал ТОЛЬКО тела `Item::Fn` С receiver'ом (методы) — тесты и
   свободные функции не видел вообще;
2. распознавал ТОЛЬКО `@field()` (прямой `SelfAccess`-вызов) как форму
   «поле вызвано внутри границы» — `h.f()`, где `h` обычная локаль
   известного типа, мимо не проходил.

Скобочная форма `(h.f)()` из репро №289 синтаксически ИДЕНТИЧНА `h.f()` —
в грамматике нет узла `Paren` (скобки отбрасываются на парсинге), так что
реальная дыра была в СКОУПЕ пре-пасса, а не в форме записи.

## Фикс (компилятор, `compiler-codegen/src/types/mod.rs`)

1. **Генерализация пре-пасса `spawn_tainted_fields_of_module`** — теперь
   сканирует ВСЕ `Item::Fn` (методы И свободные функции) И `Item::Test`.
   Новая машина: `var_types` (имя локали → синтаксически известный
   `TypeRef`, из параметров/`let`-аннотаций/ctor-сигнатур) +
   `field_bindings` (имя → `(owner, field)`, генерализация старого
   однослотового `loop_var` в полную карту) + резолверы
   `resolve_owner_typeref`/`resolve_owner_type`/`resolve_field_read`,
   покрывающие self / локаль / вложенное поле / `Vec`-индекс.
2. **Форма 3 (аргумент вызова)** — новый пре-пасс
   `directly_called_param_positions_of_module`: для каждой свободной fn
   без receiver'а — какие её ПАРАМЕТРЫ-позиции вызываются где-либо в её
   ЖЕ теле (переиспользует существующий `capture_scan_expr`/`capture_scan_
   block`). На call-сайте внутри границы: если аргумент на такой позиции —
   field-read, тейнтуем `(owner, field)`.
3. **`resolve_field_owner_type`** (write-site рантайм-резолвер) — добавлен
   fallback: `Ident(name)`, не найденный в `state.scopes`, пробуется как
   ИМЯ ТИПА (`self.type_decls`) — статический receiver (`Holder.new(..)`).
4. **`ExprKind::Path` на call-сайте** (не только `Member`) — `Type.method`
   парсится КАК `Path(["Type","method"])`, НЕ `Member{obj,name}` (грамматика
   так устроена для static-receiver вызовов). Старый код проверял ТОЛЬКО
   `Member` — ни один `.new()`/`.of()`-ctor вызов в проекте не совпадал.
5. **Bare-brace `RecordLit` (`{ f }`, `type_name: None`)** — идиоматичная
   форма ctor-тела ПОВСЕМЕСТНО в корпусе. И write-check
   (`walk_expr`'s `RecordLit`), и taint-scan (`field_write_param_scan_expr`)
   получили fallback: если `type_name` отсутствует — owner = enclosing
   receiver-type (`self_ty`/`state.current_receiver_type`).
6. **D52-shorthand `{ f }`** (`RecordLitField.value: None`) — ОБЯЗАТЕЛЬНАЯ
   форма записи, когда имя поля совпадает с источником (explicit `{f: f}`
   — hard compile error). Оба write-check сайта раньше молча пропускали
   `value: None` целиком — синтезирован `Ident(f.name)`-стенд-ин.

Находки (2), (4), (5), (6) — НЕ гипотетические edge-кейсы: это ЕДИНСТВЕННЫЙ
легальный способ написать `.new()`-конструктор в Nova (см. `docs/dev/
nv-coding-style.md` конвенция ctor'ов + grep любого `.new()` в проекте).
Без всех шести фиксов вместе scoped repro №289 не ловился (проверено
итеративно, eprintln-инструментированный дебаг в процессе, снят перед
финальным коммитом).

## Формы 1–5 из брифа — все РАБОЧИЕ дыры (не сужение периметра)

| # | форма | статус |
|---|---|---|
| 1 | `(h.f)()` (репро владельца) | ловится |
| 2 | `let g = h.f; g()` | ловится |
| 3 | `fn call_it(g) => g()`; `spawn { call_it(h.f) }` | ловится |
| 4 | `ro v = Vec[Holder].of(h); (v[0].f)()` | ловится |
| 5 | `type Outer { inner Holder }`; `(o.inner.f)()` | ловится |

Фикстуры: `spec_tests/conformance/neg/field_sink_call_{paren,let_bound,
param_pass,vec_index,nested_field}_neg.nv` (EXPECT_COMPILE_ERROR
E_CONCURRENT_MUT_CAPTURE) + `spec_tests/conformance/field_sink_call_pos.nv`
(#share/AtomicInt-твины всех пяти форм + локальная map-лямбда без ухода в
файбер — легально, ложняков нет).

## Реальная находка в polaris (не std) — почищена той же волной

Расширенный пре-пасс поймал ДЕЙСТВИТЕЛЬНО существовавшую (с самого начала
Plan 222.16) гонку в `nova-polaris/src/background_test.nv` +
`doc_samples_test.nv`: `bg.add(|| { order.push(1) })` — `order` (`mut
[]int`) уезжал в файбер `@drain()`'s `spawn` тем же классом, что и №289
(через `for t in @tasks { ro task = t; spawn { task() } }` —
переименование `t`→`task` пробивало старый однослотовый `loop_var`).
Починено ТЕМ ЖЕ приёмом, что `log.nv`'s `capture_log` уже применяет для
`logs` (баннер "Race fix", 2026-08-01): новый `#share`-типа `OrderLog`
(Mutex-guarded Vec[int], `background_test.nv`) + `AtomicBool` для голых
bool-флагов (`responded`/`ran_before_response`/`ran_after_response`/`ran`).
`doc_samples_test.nv` импортирует `OrderLog` из `polaris`. Полный список
починенных строк: `background_test.nv:26,27,28,46,64,66,83,85,101,103,
124,137`, `doc_samples_test.nv:667,668`.

## Гейты (дословно)

- `cargo build --release` (nova-cli) — чисто, только pre-existing warnings.
- `cargo check --lib` (compiler-codegen) — чисто.
- `nova check std/src` — `PASS: 147  FAIL: 26  WARN: 60` — канон **147/26/60** без сдвига.
- `nova-polaris ./nova.sh test src --strict-effects` — `PASS: 37  FAIL: 0  SKIP: 18` — канон **37/0/18**, ПОСЛЕ фикса `background_test.nv`/`doc_samples_test.nv` (до фикса: `PASS: 0  FAIL: 37  SKIP: 18`, все FAIL — каскад одного реального нарушения).
- `bash scripts/guards/arch-ratchet.sh` — `arch-ratchet ok: lines=64505 <= 64505`, `arch-ratchet ok: infer=348 <= 348` — на потолке, emit_c не наращен.
- `nova lint` на 6 новых spec_tests-файлах — `0 finding(s)` (после правки `counter = counter + 1` → `counter += 1`, W_NON_COMPOUND_ASSIGN).
- `nova lint` на `background_test.nv`/`doc_samples_test.nv` — 0 новых находок (8 pre-existing в doc_samples_test.nv, вне тронутых строк).
- Мега-CU (`spec_tests/conformance`) и флагман — НЕ гонял (интегратор при приёмке, по конвенции окна).

## Изменённые файлы

- `compiler-codegen/src/types/mod.rs` — компилятор-фикс (см. выше).
- `spec_tests/conformance/neg/field_sink_call_{paren,let_bound,param_pass,vec_index,nested_field}_neg.nv` — новые.
- `spec_tests/conformance/field_sink_call_pos.nv` — новый.
- `nova-polaris/src/background_test.nv` — добавлен `OrderLog`, `order []int`→`OrderLog`, bool-флаги → `AtomicBool` (ДРУГОЙ репозиторий, отдельное слияние).
- `nova-polaris/src/doc_samples_test.nv` — импорт `OrderLog`, тот же фикс на одном тесте (ДРУГОЙ репозиторий).

Ветка НЕ вливалась (по правилам окна) — сдаётся интегратору.
