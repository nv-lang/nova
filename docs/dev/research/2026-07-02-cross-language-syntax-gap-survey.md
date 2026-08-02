# Cross-language syntax-gap survey — что из Rust/Go/TS/Kotlin/Java/Zig/Swift стоит добавить в Nova

> Дата: 2026-07-02
> Метод: multi-agent workflow (31 агент, ~1.37M ток.) — baseline текущего синтаксиса Nova → параллельные каталоги 7 языков → дедуп кандидатов, которых в Nova нет → адверсариальная оценка совместимости с философией Nova (минимализм, эффекты, `type Name`, «сигнатура = полный контракт»).

## Главный вывод

**Ни одного `strong`-кандидата. 3 из 22 — `consider`, остальные 19 — `skip`.**

Синтаксическая поверхность Nova уже плотно закрывает то, что ценится в 7 языках: либо своим механизмом, либо через сознательный отказ по D-решениям. Ни одна из 22 конструкций не даёт **новой выразительной силы** — максимум эргономическая экономия.

Побочная находка: baseline-агент местами **устарел** — пометил `while-let` и top-level or-patterns как «отсутствуют/частично», хотя грепом подтверждено, что они **уже реализованы и оттестированы** (D34 `while Pat = expr`; `Pattern::Or` для `Red | Yellow =>`).

---

## Что стоит рассмотреть (3 × `consider`)

Все три — **не новые концепции**, а снятие частного ограничения с уже существующей грамматики. Без новых ключевых слов/сигилов, low/medium затраты.

### 1. Multi-bind if/while chain — `consider`, low (~1-2 дня)

Разрешить **более одного** `Pat = expr` в голове условия (сейчас D34 допускает один binder + `&&`-guard):

```nova
if Some(x) = get_x() && Some(y) = get_y(x) && x + y > 10 {
    use(x, y)
}
```

- Это ровно открытый вопрос **Q-if-let-chain-multi** (сами мейнтейнеры пометили LOW: guard уже покрывает ~90%, есть nested-`if` workaround).
- Разделитель — `&&`, **не** запятая (запятая занята под tuples/args/variants, D17).
- Семантика уже определена D34: left-to-right scope, binding'и видны дальше по цепочке и в теле, невидимы в `else`.
- AST-изменение: `IfLet/WhileLet` → унифицированный `conds: Vec<IfCond>`, где `IfCond = LetBind(pattern, scrutinee) | BoolExpr(expr)`.
- **Не** `strong`, потому что выигрыш над nested-`if` мал, а цепочка может разрастись (multi-bind + multi-guard head) — лёгкое давление на минимализм.

### 2. Labeled loops — `consider`, medium

Метка-идентификатор перед циклом + операнд у `break`/`continue`:

```nova
for outer in grid {
    for cell in outer {
        if bad(cell)  { break outer }      \ выход из внешнего цикла
        if skip(cell) { continue outer }   \ следующая итерация внешнего
    }
}
```

- **Genuinely absent** — не покрыто ничем: `interrupt` выходит из `with`-блока эффекта, `return`/`throw` — из всей функции, `mut`-флаг + post-loop = ровно тот boilerplate, что это убирает.
- Форма — **только identifier** (`for outer …` + `break outer`). `'outer` **невозможен лексически** (`'` — делимитер char-литерала → незакрытый char). `:`-форма (`outer:`) чужда грамматике (в Nova нет `:` в биндингах). Bare `break`/`continue` продолжают целить в ближайший цикл.
- Десугарится в существующий `loop { match _it.next() {…} }`. Value-carrying `break outer x` — **явно вне scope** (пересекается с block-as-expression и match).
- **Не** `strong`, потому что идиома Nova для «глубокий скан» — вынести во вспомогательную функцию с типизированным `return` (лучше читается для «LLM пишет / человек проверяет»). Метки рискуют поощрять вложенность вместо декомпозиции. Ниша — hot grid/matrix-сканы, где function boundary мешает `mut`-capture / нельзя `continue` внешний.

### 3. Nested or-patterns — `consider`, low

Разрешить `|` **внутри** варианта/позиции (сейчас только top-level в арме):

```nova
match x {
    Some(1 | 2 | 3) => "small"
    Some(n)         => "other"
    None            => "none"
}
match pair {
    (0 | 1, y) => y
    (x, _)     => x
}
```

- Top-level `|` **уже есть** (`Pattern::Or`) — брешь только во вложенной позиции (AST-комментарий: «не вкладывается внутрь других patterns»).
- Fix крошечный: hoist `|`-сбора из `parse_match` в `parse_pattern`, тот же `Pattern::Or`, тот же инвариант «все альтернативы биндят одинаковый набор имён».
- Убирает дублирование тела (`Some(1) => b, Some(2) => b`). Это **обобщение** (снятие top-level-only ограничения), а не новая фича → делает язык внутренне консистентнее.
- Граница: **только** вложенный `|`. Range-in-arm (`'a'..='z' =>`) агенты **отвергают** — полностью покрыт guard'ом (`c if c >= 'a' && c <= 'z'`).

---

## Что отброшено (19 × `skip`) — сгруппировано по причине

### A. Уже реализовано (baseline устарел)
| Конструкция | Реальность в Nova |
|---|---|
| `while-let` | **D34** `while Some(x) = pop()` (ExprKind::WhileLet, тесты, «most idiomatic drain») |
| top-level or-patterns | **`Pattern::Or`** (`Red \| Yellow =>`, `0 \| 1 \| 2 =>`) |

### B. Полностью покрыто существующим механизмом
| Конструкция | Чем покрыто |
|---|---|
| let-else / guard-let / orelse-return | `x ?? return` (binds-and-escapes Some/Ok) + divergent `match`; Plan 106 сознательно вынес let-else за scope |
| break-with-value | block-as-expression (last-expr) + `.find()/.position()/.fold()` → `Option[T]` |
| @-binding (as-pattern) | guard-арм `Some(p) if …`; плюс `@` занят под receiver, `as` — под каст |
| matches! | `expr is Variant` + D34 `if Pat = e && guard` в выражении; `is Variant(binding)` уже **явно отвергнут** (03-syntax:3377) |
| key paths `\.name` | замыкания `\|x\| x.field` + `sort_by_key` берёт проекции |
| method references `Type::m` | замыкания; `::` ломает single-dot D35 |
| scope functions (let/apply/also/with) | spread `{…base, f: v}` + `Option.map` + `??`; только `also` — мелкая брешь → библиотечный `@tap` |
| const/value generics | `[N]T` (D27) + `const fn`-моно (Plan 114.4.4.5) |
| struct tags (reflection) | **Plan 180** serde-derive — то же compile-time, без runtime-рефлексии |
| autoclosure | `\|\| expr` уже есть; `&&`/`\|\|`/`??` short-circuit нативно (D46) |
| lazy/delegated properties | `Lazy[T]`/`OnceCell`/`Once` (Plan 103.5 / D171) |
| computed properties | zero-arg `@`-методы с обязательными скобками |

### C. Отвергнуто на уровне философии («сигнатура = полный контракт», no hidden control flow)
| Конструкция | Конфликт |
|---|---|
| computed properties (no-parens getter) | **D14/D117**: скобки обязательны = сигнал «тут вычисление, возможно O(n)»; bare `.len` = hard error E_SIZE_ACCESSOR_FIELD |
| property observers (willSet/didSet) | скрытый control-flow на `x = v`; идиома — `use` + `mut @method` с эффектами в сигнатуре |
| delegated properties / property wrappers | скрытые get/set; `by` + `@Wrapper` конфликтуют с грамматикой; всё покрыто Lazy/contracts/effects/`use` |
| result builders (`@resultBuilder`) | невидимая переписка `if`/`for` в блоке в build-хуки — антитеза «human reviews»; DSL-часть покрыта D43 trailing-block + `-> @` + D48 tagged templates |
| declaration-site variance (`out`/`in`) | требует subtyping инстанциаций, которого Nova **намеренно не имеет** (Q-anonymous-union, D39, D55) |
| recover (panic→value) | **D13**: panic ловит только runtime на границе фибера; уже в rejected.md; наблюдение — reserved `Panic` handler + ScopeOutcome |
| template-literal pattern types | второй refinement-механизм рядом с contracts+newtypes; backtick занят под D48; `@comptime`-валидация уже намечена |
| bit-precise widths (u3/u7) + packed structs | нет C-типа под u3; layout-гарантии противоречат structural-модели без repr; покрыто u8+операторы+BitVec-contracts+newtype-аксессоры |

---

## Наблюдения (мета)

1. **Минимализм держится.** 19/22 отклонены не «лениво», а с конкретным existing-механизмом или D-решением. Nova не имеет свободного «feature-creep budget» в этих зонах.
2. **Многие чужие фичи — компенсация чужих дыр.** Kotlin scope-functions существуют, потому что в Kotlin нет record-spread; Go `recover` — потому что нет sum-type-ошибок/эффектов/супервизии; Swift autoclosure — потому что нет Z3-elided контрактов. У Nova эти дыры закрыты иначе → компенсаторы не нужны.
3. **Единственный реальный apple-to-pick** — 3 обобщения существующей грамматики (multi-bind chain, labeled loops, nested or-patterns), все низкорисковые, ни одно не вводит новую концепцию.

## Следующий шаг (не сделано)
- Multi-bind chain уже трекается как **Q-if-let-chain-multi**. Labeled loops + nested or-patterns — кандидаты в `spec/open-questions.md`, если решим закрывать.
