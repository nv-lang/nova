# PROGRESS — окно S1a: M:N-безопасность захватов ЦЕЛИКОМ (117+242§3+168)

worktree: `d:/Sources/nv-lang/nova-s1a`, branch `s1a` от main `7ded33e33`.
Модель: Claude Sonnet 5 (claude-sonnet-5).

## Ф.0 — дизайн-записка (2026-08-02)

### Первое чтение реестров

- `docs/plans/221.1-bug-sweep.md:120` (№117), `:145` (№168), `:331` (№242).
- `docs/plans/backlog-followups.md:4187` (`[M-router-handler-mut-capture-escape-soundness]`,
  §1/§2 закрыты волной p-soundness-pack 2026-08-01, §3 escaping — открыт,
  явно помечен как ядро этого окна).
- `docs/plans/238-fiber-memory-model.md` (план-источник D441), раздел
  "V2 — открытый остаток" перечисляет ровно тот же состав (1=242§3, 3=168).

### НАХОДКА: №168 уже закрыт кодом — реестр устарел

`git log` показывает волну A-V10 (2026-07-31, коммиты `73e5c0a47` "feat(A-V10,
D441 §5): закрыть №168 (precomputed-обработчики) и №167", `2ef6d0407`
спек-амендмент, `1c1460f15` merge) — **предок текущего main**
(`git merge-base --is-ancestor 1c1460f15 7ded33e33` → да). Код в
`compiler-codegen/src/types/mod.rs::check_handler_capture` уже содержит
ветку `ExprKind::Ident(name)`, резолвящую precomputed-handler через
`ScopeBinding.closure_free_vars` (комментарии "A-V10 (D441 §5 №168
closure)" — строки 27914-27954). Спека: `spec/decisions/06-concurrency.md`
§ "Amend D441 §5 — A-V10" (строка 8327) документирует закрытие ДОСЛОВНО,
включая транзитивную install-позицию (`spawn_tainted_params_of_fn`
подхватывает handler-параметр без нового пре-пасса). Фикстуры уже в
`spec_tests/conformance/`:
- `neg/handler_mut_capture_precomputed_neg.nv`
- `neg/handler_mut_capture_precomputed_install_transitive_neg.nv`
- `handler_mut_capture_precomputed_pos.nv`
- (сопутствующие №167: `neg/thread_affine_direct_in_fiber_neg.nv`,
  `neg/thread_affine_transitive_in_fiber_neg.nv`, `handler_thread_affine_pos.nv`)

Расследование истории строки реестра (`git log -p` по `docs/plans/
221.1-bug-sweep.md`): коммит `46d905177` "docs: A-V10 закрыт — №167/№168 в
реестре" на деле пометил ✅ ТОЛЬКО №167 (diff подтверждён); строка №168
осталась на состоянии 🔴 P1 OPEN — **документная недоделка того окна**
(владелец закрыл №167 галочкой, №168 — забыл), не код-гэп. Позже слияние
`4aa406677` (p-221-revision, 2026-08-0x) заявило «S1a-состав сверен:
№117/№242§3/№168», унаследовав устаревшую 🔴-строку без повторной проверки
кода.

**Действие Ф.1** (вместо реализации — она не нужна): верификация +
исправление реестра. См. ниже "Ф.1 — вердикт".

### Ядро (Ф.2): единый механизм для №117 + №242§3

**Существующая архитектура (D441 §2-§3, читать
`spec/decisions/06-concurrency.md:8068-8389` целиком) уже устроена как
список "точек пересечения границы файбера":**
1. `check_capture_boundary` — прямой захват в теле `spawn`/`detach`/
   `parallel for`/`blocking` (D415, было).
2. §3(а) `check_transitive_closure_arg` — замыкание передано ПАРАМЕТРОМ в
   fn, которая зовёт этот параметр внутри СВОЕГО spawn (пре-пасс
   `spawn_tainted_params_of_fn`, по всем `Item::Fn` модуля — методы это
   тоже `Item::Fn` с `receiver: Option<Receiver>`, отдельного `Item::Impl`
   в AST нет).
3. §3(б) — отправка в канал (`chan.send(closure)`), та же машина.
4. §3(в)/№168 `check_handler_capture` — замыкание установлено как
   `with`-обработчик вокруг spawn-содержащего тела (литерал ИЛИ
   precomputed-Ident через снэпшот).

Все четыре точки используют ОДНУ функцию-ядро `flag_boundary_captures`
(строка 27652): получает `free: HashSet<String>` (свободные переменные
проверяемого выражения) + текст "почему это граница" → резолвит каждое имя
против `state.scopes`, применяет share/linear-классификацию
(`protocols::share_check::mut_alias_failure`), эмитит `E_CONCURRENT_MUT_
CAPTURE`/`E_HANDLER_MUT_CAPTURE_IN_FIBER`/`E_LINEAR_CAPTURE_IN_FIBER`.

**D441 §5 уже НАЗЫВАЕТ этот класс честной границей дословно** (строка
8301): "Класс «замыкание как ПОЛЕ структуры» (`BackgroundTasks.tasks
[]fn()->()`, ...) — НЕ проверяется: запись значения в поле записи/
коллекции и последующее извлечение — на 2+ хопа глубже call-site-механизма
§3(а). Структурный риск, не подтверждённое нарушение — остаётся риском, не
забытым багом." **Это буквально №117 + материальная часть №242§3.**

**Решение по критерию владельца (rustc-эталон, БЕЗ нового синтаксиса):**
не заводить Send-подобную аннотацию типа `fn` (языковое решение,
отклонённое уже в D441 §2 как YAGNI для v1 — "Send/Migrate-маркер НЕ
заводить"). Вместо этого — **пятая точка пересечения (г)**, СИММЕТРИЧНАЯ
уже существующим (а)/(б)/(в), той же природы "escaping-классификация точек
сохранения + проверка захватов в этих точках" (второй вариант дизайна из
брифа, прямо разрешённый без аннотации):

- **Пре-пасс `spawn_tainted_fields_of_module`** (по всем `Item::Fn` с
  `receiver: Some(r)` — методам): внутри тела метода, ВНУТРИ границы
  `spawn`/`detach`/`parallel for`/`blocking` (та же `block_contains_
  fiber_boundary`-детекция) ищем:
  - прямой вызов `@field()`/`self.field()` (поле fn-типа) →
    `(r.type_name, field)` в тэйнт-множество;
  - `for x in @field { ... x(...) ... }` / `for x in self.field { ... }`
    (поле-контейнер `[]fn`/`Vec[fn]`) с вызовом переменной цикла внутри
    ТОГО ЖЕ тела границы → `(r.type_name, field)` в тэйнт-множество.
  Пре-пасс строится ОДИН раз в `CapabilityCtx::build`, ДО проверки любого
  write-сайта (тот же тайминг, что `spawn_tainted_params`).
- **Гейт на WRITE-сайтах** тэйнтованного поля (симметрично §3(а)'s
  call-сайту): `flag_boundary_captures` вызывается на выражении,
  записываемом в тэйнтованное поле, когда это выражение резолвится к
  замыканию (литерал ИЛИ `ScopeBinding.closure_free_vars`-снэпшот, тот же
  путь, что (а)/(в)) — формы записи V1:
  1. `@field.push(v)` / `self.field.push(v)` / `.append(v)`/`.add(v)`/
     `.insert(v)` — Call с Member-цепочкой, чья база резолвится в
     тэйнтованное поле текущего/известного receiver-типа;
  2. `@field = v` / `self.field = v` / `obj.field = v` (`Stmt::Assign`,
     Member-target);
  3. `RecordLit { type_name, fields: [.., field: v, ..] }` (конструктор).
  Тип receiver'а expr-базы поля резолвится ТОЛЬКО когда он статически
  известен (`@`/`SelfAccess` → текущий `receiver.type_name` метода,
  протаскивается новым полем `CapState.current_receiver_type_name`; голый
  `Ident` receiver — через `ScopeBinding.ty`, если аннотирован/выведен
  синтаксически). Неизвестный receiver-тип → консервативный no-op (та же
  честная политика under-approximation, что везде в D441 §3/§5) — НЕ
  ложный срабатыватель, задокументированная граница.

**Почему это не даёт ложняков на map/filter/fold** (жёсткое требование
брифа): map/filter/fold-лямбды НИКОГДА не пишутся в тэйнтованное поле —
они передаются ПАРАМЕТРОМ, вызываемым СИНХРОННО внутри тела HOF (тот же
файбер, тот же вызов), не сохраняются ни в структуру, ни в канал, ни в
with-обработчик. Пре-пасс §3(а) их тоже не помечает (параметр вызывается
НЕ внутри spawn/detach/parallel-for/blocking тела HOF-функции — обычный
`for`/рекурсия). Значит цепочка (г) их тоже не тронет: нет пути "значение
попало в тэйнтованное поле".

**#share-исключение (D415) сохраняется по построению**: `flag_boundary_
captures` уже пропускает share-типы (тот же `mut_alias_failure`); поле
типа `Mutex[[]fn()->()]`/`AtomicRef`-обёртка и т.п. не потребует новой
логики — write-check вызывает ТУ ЖЕ функцию.

**Честно вне периметра V1 (документировать как честную границу §5, не
скрывать):**
- 2+ хопа передачи поля дальше (та же граница, что уже честно
  задокументирована для §3(а)/(в) в D441 §5 — принципиальный лимит
  снэпшот-машины, не новый для этого окна);
  receiver неизвестного типа (bare generic/unresolved) — консервативный
  no-op;
- вызов поля через МЕТОД чужого типа, не через `@`/известный
  bare-`Ident` (напр. `getStorage().tasks.push(f)`) — база не резолвится
  статически, no-op;
- поле, тэйнтованное ЧЕРЕЗ вызов другого метода того же типа (метод А
  вызывает метод Б, Б спавнит поле) — пре-пасс V1 смотрит ТОЛЬКО на
  прямой spawn-boundary внутри ОДНОГО метода; транзитивность между
  методами одного типа НЕ прослеживается (симметрично тому, что
  `spawn_tainted_params_of_fn` тоже не транзитивен через цепочку вызовов
  разных fn — только "именует параметр как тэйнтованный, если он вызван
  ГДЕ-ТО в теле", а не "если он передан дальше и там вызван"). Если замер
  на std/polaris/examples вскроет живой сайт этого класса — эскалация,
  не молчаливый пропуск.

**№242§3 (escaping вообще, не только через поле типа)**: та же машина
покрывает материальную часть — Router-регистрация (`router.get(path,
handler)` эквивалентно `@routes.push((path, handler))` или
`@routes.insert(path, handler)` — тот же write-сайт-класс (1)). Общая
"escaping ⇔ записано в тэйнтованное поле" классификация И ЕСТЬ ответ на
"как отличить убегающее от локального" — локальное = never written to
storage, escaping = written to a field later read+called inside a fiber
boundary. Возврат замыкания как РЕЗУЛЬТАТА функции (`return closure`) —
подкласс `RecordLit`/поля не покрывает; честно вне периметра V1 (в реестр
как остаточная честная граница, симметрично уже принятым).

### Design-условие "СТОП" — не сработало

Механизм НЕ требует нового пользовательского синтаксиса/аннотации типа fn
— целиком укладывается в существующую архитектуру "точки пересечения
границы" (то же расширение, каким Ф.1/Ф.3 плана 238 уже были для
(а)/(б)/(в)). Решение владельца по Send-маркеру (D441 §2, YAGNI) не
пересматривается. Продолжаю в Ф.2 (реализация), НЕ останавливаюсь.

## Ф.1 — вердикт: №168 уже закрыт (A-V10), реестр исправлен

Пересобрал `nova-cli` в свежем worktree (`cargo build --release`, чисто,
2м34с). Прогнал через свежий `nova check`:
- `spec_tests/conformance/neg/handler_mut_capture_precomputed_neg.nv` →
  FAIL с `E_HANDLER_MUT_CAPTURE_IN_FIBER` (верный код, верное объяснение
  "PRECOMPUTED `with`-handler").
- `spec_tests/conformance/neg/handler_mut_capture_precomputed_install_transitive_neg.nv`
  → FAIL с `E_CONCURRENT_MUT_CAPTURE` (транзитивная install-позиция).
- `spec_tests/conformance/handler_mut_capture_precomputed_pos.nv` → ok.

Вердикт: №168 закрыт кодом ПОЛНОСТЬЮ, только реестр отставал (документная
недоделка A-V10, не код-гэп). Исправлены три файла (коммит `d152659be`):
`docs/plans/221.1-bug-sweep.md:145`, `docs/plans/221-release-v0-1.md:24`,
`docs/plans/238-fiber-memory-model.md` (заголовок + V2 п.2/3).

**Реализация Ф.1 не нужна** — переход сразу к Ф.2.

## Ф.2 — ядро (№117 + №242§3): РЕАЛИЗОВАНО

### Механизм (единый, расширяет D441 §3 существующими "точками пересечения")

Новая пятая точка (г) — "closure-as-field": замыкание, записанное в
spawn-тэйнтованное ПОЛЕ структуры/контейнера, вместо прямого захвата.
Реализация — `compiler-codegen/src/types/mod.rs`, всё в чекер-канале
(`CapabilityCtx`), emit_c НЕ тронут:

1. **`spawn_tainted_fields_of_module`** (пре-пасс, вычисляется в `build()`
   ОДИН раз, тот же тайминг, что `spawn_tainted_params`/`thread_affine_
   closure`) — по всем методам (`Item::Fn` с `receiver: Some(_)`, у Nova
   нет отдельного `Item::Impl`) ищет `(TypeName, field)`-пары, чьё значение
   ВЫЗЫВАЕТСЯ внутри `spawn`/`detach`/`parallel for`/`blocking`-тела.
   Две формы: (i) прямой `@field()` внутри границы; (ii) `for`/`parallel
   for`-петля по прямому `@field`-чтению, где ТЕЛО петли зовёт свою
   pattern-переменную внутри вложенной границы (`BackgroundTasks.drain()`
   форма) — И симметричный `let`-вариант (`ro x = @field` → `x()` внутри
   границы дальше в том же блоке, добавлено ПОСЛЕ находки codegen-гэпа,
   см. ниже) — обе формы делят один и тот же `loop_var`-трекинг
   (`field_taint_block/_stmt/_expr`, borrowed `&str` без аллокаций).
2. **`spawn_tainted_method_params_of_module`** (второй пре-пасс, зависит от
   первого, тот же файл) — по всем методам ищет, какие ИЗ СОБСТВЕННЫХ
   параметров метод ЗАПИСЫВАЕТ в уже-тэйнтованное поле (`fn X mut @add(f
   fn()->()) { @tasks.push(f) }` → параметр `f` тэйнтован для
   `(X, "add")`). Один хоп, НЕ fixed-point (честная граница, см. ниже).
3. **Гейт на write-сайтах** (три формы, везде вызывает ОДНУ и ту же
   `flag_boundary_captures` — тот же движок, что (а)/(б)/(в)):
   - `.push`/`.append`/`.add`/`.insert`/`.push_back`/`.push_front`/`.set`
     на тэйнтованном поле (в `ExprKind::Call`-ветке `walk_expr`);
   - прямое присваивание `@field = v` / `obj.field = v` (`Stmt::Assign`);
   - конструктор `Type { field: v, .. }` (`ExprKind::RecordLit`).
   Резолюция `v` — литерал ИЛИ `ScopeBinding.closure_free_vars`-снэпшот,
   ИДЕНТИЧНО (а)/(в). Владелец поля резолвится через НОВОЕ поле
   `CapState.current_receiver_type` (`@field`/`SelfAccess`) ИЛИ
   `ScopeBinding.ty` (bare `Ident`-ресивер) — новый хелпер
   `resolve_field_owner_type`.
4. **Метод-call-сайт для (2)** — новая ветка в `ExprKind::Call` (аналог уже
   существующей для free-fn `Ident`-вызовов): `obj.method(arg)` резолвит
   `obj`'s тип, смотрит `spawn_tainted_method_params[(type,method)]`, матчит
   `self.sig.method_overloads` по арности (D84-конвенция), гоняет `arg`
   через ту же резолюцию (`check_field_write_via_method_param`).

Диагностика — существующий `E_CONCURRENT_MUT_CAPTURE` (текст объясняет
двух-хоповую цепочку: параметр → запись в поле → вызов в границе где-то ещё).

### №242§3 (escaping вообще)

Router-регистрация (`router.get(path, handler)` ≡ `@routes.push((path,
handler))`) покрывается ТЕМ ЖЕ механизмом (класс (1) write-сайтов). Ответ на
"как отличить убегающее от локального": escaping ⇔ значение ЗАПИСАНО в
тэйнтованное поле; локальное (map/filter) ⇔ никогда не пишется в
storage — течёт прямо в синхронный вызов. Возврат замыкания как РЕЗУЛЬТАТА
функции — честно ВНЕ периметра V1 (не встречено в фикстурах брифа, не
измерено как живой сайт).

### НАХОДКА (codegen, НЕ чекер, НЕ трогать в этом окне)

При верификации pos-фикстуры мега-CU (`a_q3_println_debug_record` —
известный label-misattribution артефакт единого combined-CU entry, см.
221.1-bug-sweep.md §"Перепроверка 2026-07-29") давал CC-FAIL с текстом,
упоминающим МОЁ новое поле `on_error` — расследовано изолированным `nova
build` минимального репро: **`@field()` — прямой вызов bare fn-типизиро-
ванного ПОЛЯ receiver'а внутри `spawn{}`-тела метода — CC-FAIL `undeclared
identifier 'nova_self'`** (emit_c не захватывает `nova_self` в контекст
спавненного closure для этого пути). Родственный: `Vec[fn()->()]`-поле,
вызываемое через `for t in @tasks { spawn { t() } }` — линкер-ошибка
`undefined symbol: nova_fn_t` (родня уже известного
`[M-vec-iter-fn-newtype-next-option-mismatch]`). ОБА — pre-existing emit_c
гэпы, впервые вскрытые этим окном (видимо, шаблон "self-field вызван внутри
spawn метода" ранее не встречался в корпусе). Зарегистрирован
`[M-spawn-self-field-call-nova-self-undeclared]` в backlog-followups.md;
НЕ фикшу (channel-first, легаси emit_c не наращивать). Обходной путь
(`ro handler = @field; spawn { handler() }`) — добавлен КАК ПРОДУКТИВНОЕ
расширение пре-пасса (см. п.1 выше, `let`-форма), не просто костыль:
подтверждено, что ЭТО и есть идиоматичный/уже-проверенный-корпусом способ
звать fn-значение внутри spawn. Все живые NEG/POS-фикстуры переписаны на
эту форму; BackgroundTasks-neg сознательно ОСТАВЛЕН на прямой `for`-форме
(codegen туда не доходит — negative-тест, безопасно, и это ДОСЛОВНАЯ форма
из бага владельца).

### Гейты Ф.2

- `cargo build`/`cargo build --release` (compiler-codegen + nova-cli) —
  чисто, без новых warning'ов на новых символах.
- 3 новые фикстуры: `spec_tests/conformance/neg/
  field_sink_mut_capture_bare_field_neg.nv` (EXPECT_COMPILE_ERROR
  E_CONCURRENT_MUT_CAPTURE, двух-хоповая цепочка через bare fn-поле),
  `neg/field_sink_mut_capture_background_tasks_neg.nv` (BackgroundTasks
  дословно, Vec[fn]-поле), `field_sink_mut_capture_pos.nv` (3 test-блока:
  #share-захват AtomicInt легален, map/filter НЕ ложнякует, Mutex-
  SharedLog #share-vouch легален) — все зелёные (`nova check` индивидуально
  + `--filter a_q3`/эквивалент mega-CU root-entry: PASS:1 включая все 3
  assert'а pos-теста).
- Существующие A-V10-фикстуры (№168) — не тронуты, по-прежнему PASS/FAIL
  как задокументировано в Ф.1.
- `a_q3_println_debug_record` (известный label-misattribution CU-entry) —
  ЗЕЛЁНЫЙ на модифицированном чекауте (было временно CC-FAIL из-за
  ПЕРВОЙ версии pos-фикстуры, до `let`-переписывания — см. НАХОДКУ выше).

### Честные границы V1 (зафиксировать в спек-амендменте Ф.4)

- Один хоп для метод-параметра (2-hop: write→call); параметр, переданный
  ДАЛЬШЕ в третий метод — не отслеживается (тот же класс лимита, что уже
  принят для §3(а)/(в)).
- `loop_var`/`let`-трекинг — ОДИН активный биндинг на блок (не граф
  датафлоу); переприсвоение/несколько разных `let` для одного поля в
  разных ветках может не подхватиться идеально консервативно (недо-
  апроксимация, не ложный+).
- Ресивер неизвестного статического типа (generic/unresolved) — no-op.
- Возврат замыкания как значения функции — вне периметра.
- НЕ транзитивно между МЕТОДАМИ одного типа (поле, тэйнтованное вызовом
  ЧЕРЕЗ другой метод того же типа, — не подхватывается; симметрично тому,
  что `spawn_tainted_params_of_fn` тоже не транзитивен через цепочку вызовов).

## Ф.3 — std/polaris/examples --strict-effects

- `nova check std/src` — **147/26/60** (байт-в-байт канон, БЕЗ сдвига).
- `nova-polaris` `nova test src --strict-effects` (env на главный репо,
  `NOVA_STD_PATH` на nova-s1a) — **PASS: 37 FAIL: 0 SKIP: 18** (байт-в-байт
  канон).
- `examples/flagship/aggregator --strict-effects` — CC-FAIL
  `no field or method 'read_bytes' on type TlsStream`. Расследовано:
  метод РЕАЛЬНО существует (`nova-tls/src/stream.nv:293`,
  `export fn TlsStream mut @read_bytes(max int) Net -> Result[[]u8,
  TlsError]`) — значит дело не в API-дрейфе, а в версионном pin/lock
  (`examples/nova.lock.toml`, УЖЕ был модифицирован в рабочем дереве ДО
  начала этого окна — см. git status в начале сессии). На ЧИСТОМ main та
  же команда даёт `built:` — но с пометкой `build cache hit — reusing
  generated C`, т.е. маскируется старым кэшем, а не реально резолвит
  свежо. **Вывод: предсуществующий cross-repo lock-дрейф, НЕ вызван
  этим окном** (я не трогал net/tls/lock-механику). Вне периметра S1a —
  не исправлено, задокументировано (доложить владельцу/интегратору).

## Ф.4 — спек-амендмент

`spec/decisions/06-concurrency.md`: новый раздел «Amend D441 §5 — S1a:
№117 + №242§3 закрыты — §3 получила точку (г)» (в конце файла, после
Amend D441 §5 — A-V10); статус-строка D441 дополнена; §3/§5-текст правлен
на месте (bullet «Класс «замыкание как ПОЛЕ структуры»» теперь ссылается
на новый раздел вместо «НЕ проверяется»).

## Ф.5 — реестры

- `docs/plans/221.1-bug-sweep.md`: №117 ✅ ЗАКРЫТО, №242 ✅ ЗАКРЫТО (§1/§2/
  §3 все закрыты), №168 ✅ (реестровая недоделка A-V10 исправлена).
- `docs/plans/backlog-followups.md`: строка
  `[M-router-handler-mut-capture-escape-soundness]` УБРАНА (все §1-§3
  закрыты, per lifecycle-конвенция); новая находка
  `[M-spawn-self-field-call-nova-self-undeclared]` ДОБАВЛЕНА (codegen,
  НЕ фикшена).
- `docs/plans/221-release-v0-1.md`: строка S1a обновлена на «✅ ЗАКРЫТО
  ЦЕЛИКОМ».
- `docs/plans/238-fiber-memory-model.md`: V2 п.1 отмечен закрытым, статус-
  строка плана и «Закрытое ядро» обновлены.
- `docs/dev/simplifications.md`: новая запись «Окно S1a (2026-08-02)» —
  полная сводка (состав/механизм/находка/гейты/реестры).

## Гейты окна (дословно, финал)

- `cargo build` + `cargo build --release` (compiler-codegen, nova-cli) —
  чисто, без новых warning на новых символах.
- Все фикстуры (а-е): (а)+(б) в одной паре (bare-field + BackgroundTasks
  дословно) neg — ЗЕЛЁНЫЕ (корректно ОТКЛОНЕНЫ, верный E-код); (в) №168 —
  уже существующие A-V10-фикстуры, перепроверены зелёными; (г)+(д)+(е)
  pos — ЗЕЛЁНЫЕ (все 3 test-блока проходят assert'ы, включая полный
  build+run через mega-CU-эквивалентный `--filter a_q3` прогон — не
  только `nova check`).
- `nova check std/src` — 147/26/60 (канон, БЕЗ сдвига).
- `nova-polaris test src --strict-effects` — 37/0/18 (канон, БЕЗ сдвига).
- `examples/flagship/aggregator --strict-effects` — красный по
  ПРЕДСУЩЕСТВУЮЩЕЙ, НЕ-от-этого-окна причине (cross-repo lock-дрейф
  TlsStream, см. Ф.3) — задокументировано, не в периметре.
- ratchet: `scripts/guards/arch-ratchet.sh` — `lines=64389 <= 64389`
  (БЕЗ роста, emit_c не тронут), `infer=348 <= 349`.
- `nova lint` на 3 новых фикстурах — 1 finding (`W_NON_COMPOUND_ASSIGN`
  на `count = count + 1` в neg-фикстуре) — ТОЧНОЕ совпадение с уже
  принятым прецедентом (`handler_mut_capture_precomputed_neg.nv` даёт
  ИДЕНТИЧНЫЙ finding на идентичном паттерне) — не фикшено, соответствует
  конвенции этого класса фикстур.
- мега-CU/флагман (общий) — за интегратором (не гонялся целиком, часть
  a_q3-cross-check проверена вручную и зелёная, см. Ф.2/Ф.3).

## Итог

Все три составляющие S1a (№117, №242§3, №168) закрыты одним окном без
языкового решения владельца (СТОП не потребовался). Реестры/спека/лог
приведены в консистентное состояние. Ветка `s1a`, worktree
`d:/Sources/nv-lang/nova-s1a` — НЕ смёржена в main (личный контроль
владельца, интегратор/владелец забирает).
