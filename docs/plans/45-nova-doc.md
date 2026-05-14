# Plan 45: `nova doc` — production-grade documentation tooling

> **Создан 2026-05-14**, переписан с чистого листа 2026-05-14
> (production-rewrite после первой draft-версии). **Ревизия 2026-05-15:**
> spec-deltas перенумерованы D100-D103 → **D104-D107** (D100-D103 заняты:
> D100 `_module.nv` peer, D101 `#doc` module-attr, D102 named params,
> D103 preemption); реконсиляция с уже отгруженными D100/D101; incremental
> cache промоутнут из «optional» в обязательную фазу; stdlib doc-pass
> выделен из Plan 45 как отдельный трек; закрыты разрывы между §19
> (заявления) и фазами (signature-aware search, page-size budget);
> контрактный IR уточнён (`MarkdownExpr` → `ContractExpr` из Plan 33).
>
> **Контекст:** в Plan 42 правило G отвергло `overview.nv` convention
> (manual signature copy → drift). Replacement = auto-tooling,
> source-of-truth = implementation. `nova doc` — обязательство,
> уже зафиксированное в [spec/decisions/04-effects.md D62 / §1267](../../spec/decisions/04-effects.md):
> «Public — это то, что попадает в `nova doc`. Эффекты — часть
> документации, не runtime-деталь.»
>
> **Бар:** не хуже rustdoc (типизированный язык с intra-doc links +
> doc-tests + stable JSON output). Лучше — там, где у Nova
> уникальные первоклассные конструкты (эффекты, контракты,
> capabilities, folder-modules, AI-first JSON).

---

## TL;DR — что мы делаем лучше Go/Rust/TS (простыми словами)

> Эта секция нужна для быстрого ответа на вопрос «зачем своё, чем
> rustdoc плох». Детальная таблица с issue-номерами — в §19.

**Главное в одном предложении:** Go/Rust/TS показывают **имена и
типы**; Nova показывает **что функция делает с миром, что от неё
требует и что гарантирует — и всё это проверено компилятором**, а не
написано в комментариях.

1. **Doc сразу показывает побочки.** Из `fn save(x)` в Go/Rust/TS не
   видно — ходит ли функция в сеть, пишет ли на диск, читает ли время.
   У нас в сигнатуре стоит `Net Db Fail[NotFound]` — LLM/человек
   понимает побочки за секунду без чтения тела.

2. **Doc показывает требования и гарантии — проверенные.** В Rust
   `// must be > 0` — комментарий, никем не проверяется. У нас
   `requires x > 0` рядом с сигнатурой, помечен **PROVEN** /
   **UNVERIFIED** / **TRUSTED**, SMT-проверен (Plan 33.1).

3. **Примеры реально запускаются.** Go: `Example_xxx` в отдельном
   файле, оторвано от функции. TS `@example`: просто текст. Rust:
   запускает, но медленно и без шаринга setup. У нас — примеры рядом
   с функцией, проверяются (включая контракты через `must_verify`),
   setup шарится через folder-peer `_doctest_setup.nv`.

4. **AI получает API одним JSON-запросом, schema стабильна.** godoc:
   JSON нет. TypeDoc: есть, но schema меняется. Rustdoc: только
   `+nightly`, unstable. У нас — `nova doc --format json`, schema v1
   зафиксирована, embedded JSON Schema, валидируется. Один tool-call
   → полный API.

5. **Ссылки `[Foo]` всегда работают.** rustdoc регулярно ломает
   cross-crate links. У нас резолвер тот же, что в type-checker'е —
   битый линк = ошибка в `nova check`, а не молчаливо пустая ссылка
   в HTML.

6. **`--diff` показывает реальные breaking changes.** Нигде нет из
   коробки. У нас встроено и видит не только смену сигнатуры, но и
   **добавление эффекта** (breaking), **усиление `requires`**
   (breaking!), **ослабление `ensures`** (breaking!) — категории,
   которые `cargo public-api` не видит, потому что у Rust этого
   в типах нет.

7. **Приватно по умолчанию.** Rust `pub` всё палит → нужны
   `#[doc(hidden)]` затычки. У нас `export` opt-in — что не
   экспортнул, того в doc нет. Случайных утечек API не бывает.

8. **Deprecation с дедлайном.** `#deprecated(since="0.4", until="1.0",
   note="...")` — CI ругается, если до 1.0 не удалил. Rust `since`
   есть, `until` нет; Go/TS — просто текст.

9. **HTML byte-identical между прогонами.** rustdoc/TypeDoc дают
   разный HTML каждый раз (timestamps, порядок). У нас — `SOURCE_DATE_
   EPOCH` + deterministic ordering, reproducibility test в acceptance.
   Diff в git показывает реальные изменения, а не шум.

10. **`--watch`, `--theme`, поиск — флаги, не экосистема.** В Rust
    для watch'а ставят `cargo install cargo-watch` (внешний tool). В
    TypeDoc темы — отдельные npm-пакеты, ломаются между версиями. У
    нас `nova doc --watch` и `--theme dark` — флаги той же команды.
    Одна программа, одна версия, одна точка обновления. Discoverable
    через `nova doc --help`, не через гугл.

**Формула:** «экосистема» = искать, ставить, версионировать, надеяться.
«Флаг» = просто написал `--watch`.

---

## 1. Цели и не-цели

### Цели

1. **Source-of-truth = implementation.** Doc генерируется из AST после
   полного `infer_effects` + `type_check` + `lint_module` (тот же pipeline
   что `nova check`/`nova build`). Никаких parallel parsers, sidecar
   manifests, manual sig copy.
2. **AI-first JSON output, stable schema.** Один tool call — полный
   API модуля: signature, effects, contracts, generics, bounds, doc,
   examples, attrs, location. Schema versioned, **forward-compatible
   внутри major version**.
3. **Полный rustdoc-paritet:**
   - intra-doc links (`[Type]`, `[mod.fn]`, `[Self.method]`) с
     резолвом и fail-loud в `--check` режиме;
   - doc-tests (компилируются и запускаются как тесты, модификаторы
     `no_run`/`ignore`/`compile_fail`/`should_panic`/`must_verify`);
   - markdown CommonMark + GFM (tables, footnotes, task-lists);
   - standalone HTML site с поиском и темой;
   - JSON output по стабильной schema;
   - source-links (file:line:column);
   - re-exports корректно (через `export import`);
   - stability + deprecation attrs.
4. **Nova-уникальное** (rustdoc этого не имеет, потому что в Rust этого
   нет в типах):
   - **Effect row** в signature рендерится structurally (`Net Db Fail[E]`)
     с линками к effect-декларациям;
   - **Contracts** (`requires`/`ensures`/`invariant`/`reads`/`modifies`/
     `decreases`) рендерятся отдельной секцией, статус верификации
     (`#must_verify` PROVEN / TIMEOUT / `#unverified` / `#trusted`)
     рядом с сигнатурой;
   - **Capability annotations** (`#forbid(Io)`, `#realtime nogc`,
     `#allow_transit(...)`) видны на item-level;
   - **Folder-modules** (Plan 42) — peers перечислены, items
     atributed к peer-файлу, internal vs export ясны;
   - **Handler/effect docs** — `type X effect { ... }` рендерит
     операции, `handler H for E { ... }` рендерит реализации;
   - **Protocol docs** — методы протокола + список impl'ов
     (структурных) на типах в workspace.
5. **Workspace-level guarantees.** `nova doc --workspace --check`
   обязан проходить на полном `std/` без warnings: все exports
   задокументированы, intra-links резолвятся, doc-tests проходят.
6. **CI-grade.** Exit codes 0/1/2/101 (как Plan 36). Deterministic
   output (sorted/canonical). Stable byte-for-byte между прогонами
   на одной revision.

### Не-цели (явные, не «later»)

- **Versioned doc archive / docs.rs equivalent** — это infrastructure,
  не tool. Когда появится package registry (Plan 03), отдельный план.
- **Live web playground** — out of scope. HTML output — static.
- **Internationalization** — strings hardcoded English. Если нужен
  RU/ZH, отдельный план с i18n pipeline.
- **Plugin system / custom themes** — KISS. Theme = CSS variables, OK
  переопределить через `--css <file>`. Никаких user-plugins в schema.
- **Документация на private items в публичном выводе** — по умолчанию
  только `export`. `--include-private` — debug, помечает items как
  `[internal]` и рендерит отдельной секцией.

---

## 2. Что меняет в языке (spec deltas)

Plan 45 вводит **четыре** spec-decisions: **D104, D105, D106, D107**
(следующий свободный блок после D103 — preemption).

### Реконсиляция с уже существующими D100/D101

D100-D103 **заняты** и Plan 45 их **не трогает**:

- **D100** (`_module.nv` peer convention) — не связан с doc.
- **D101** (`#doc "..."` module-level attribute, отгружен Plan 42.11) —
  **остаётся**. Plan 45 строится поверх него:
  - `//!` (D104) — rich multi-line module-doc-comment — **сосуществует**
    с `#doc "..."` (D101). `nova doc` мерджит оба источника module-doc
    в детерминированном порядке: сначала все `#doc "..."` строки (в
    порядке появления, multi-peer — alphabetical filename), затем `//!`
    блок. `//!` — основная форма (markdown, multi-line, intra-links);
    `#doc "..."` — терсная атрибутная альтернатива для одной строки.
  - **Namespace `#doc`** используется обеими decisions, парсер
    различает по форме аргумента (D96): `#doc "строка"` — module-text
    (D101); `#doc(key = val)` / `#doc(flag)` — doc-tool директивы
    (D105). Это зафиксировано в D105 явно, чтобы не было tag-soup.
- **D102** (named params), **D103** (preemption) — не связаны.

### D104. Doc-comment syntax: `///` outer, `//!` inner

- `///` (тройной слэш) **до** declaration — attaches к следующему
  `fn` / `type` / `const` / nested `module` / `effect` / `handler` / `protocol`.
- `//!` (slash-bang) — module-inner doc. Допустим **только в начале
  файла**, до первой declaration (после `module X` keyword и
  `import` блока). Прикрепляется к module.
- `////` (4+ слэшей) — regular comment, NOT doc. Escape-hatch для
  разделителей.
- Multi-line: consecutive `///` (или `//!`) lines объединяются с
  preserved newlines, leading single space после `///`/`//!`
  стрипается (Rust convention).
- Markdown CommonMark + GFM (tables, footnotes, task-lists) — passthrough,
  парсится только в `nova doc`, не в компиляторе.
- **Несовместимо с regular `//`** — `//` остаётся regular
  comment, drop'ается лексером.
- **Сосуществование с D101 `#doc "..."`** — `//!` и `#doc "..."` оба
  валидны для module-doc; `nova doc` мерджит (см. «Реконсиляция»).
  `//!` рекомендуется для нового кода (richer), `#doc` не deprecated.

### D105. Doc attributes: `#deprecated`, `#since`, `#stable`, `#unstable`, `#experimental`, `#hide_doc`, `#doc_alias`, `#doc(inline|no_inline)`

Все используют D96 синтаксис (`#name` / `#name(args)`). **Namespace
`#doc`:** key-form (`#doc(inline)`, `#doc(test_handlers = "...")`,
`#doc(summary = "...")`) — это D105 doc-tool директивы; string-form
(`#doc "..."`) — module-text (D101). Парсер различает по аргументу
(parenthesized args vs string literal). Item-level top-level атрибуты
(`#deprecated`, `#since`, `#stable`, ...) — отдельные имена, не под
`#doc`.

- `#deprecated(since = "0.4.0", note = "use foo instead")` —
  triggers compile-time warning при использовании, рендерится
  в doc как deprecated banner.
- `#since("0.3.0")` — первая версия с этим item. Чисто
  doc-metadata, не влияет на компиляцию. Используется в
  `nova doc --since` фильтре.
- `#stable` / `#unstable` / `#experimental` — stability tier.
  Рендерится в doc badge. `--check` опционально enforces «public
  items не должны быть `#experimental`».
- `#hide_doc` — item парсится, type-check проходит, но
  исключается из `nova doc` output (даже без `--include-private`).
  Use case: internal-but-public helpers, которые not part of
  public API contract.
- `#doc_alias("v0_name", "json")` — search index aliases. LLM может
  искать «json» и найти `serialize`/`deserialize`.
- `#doc(inline)` / `#doc(no_inline)` — для re-exports (`export import
  X.Y as Z`). `inline` копирует doc target'а; `no_inline` — лишь
  ссылка. Default = `no_inline` для cross-module, `inline` для
  same-package.

### D106. Doc-test semantics

- Code-blocks в doc-comments с лангом `nova` (или без лангом =
  default `nova`) — **doc-tests**.
- Каждый блок становится отдельным тестом: `<module>::<item>::doc_<N>`
  (N = index блока в doc-comment'е item'а).
- Модификаторы после лангом (как в rustdoc):
  - `nova` — компилируется + запускается + exit 0 ожидается;
  - `nova,no_run` — компилируется но не запускается;
  - `nova,ignore` — парсится но skip;
  - `nova,compile_fail` — должен **не** скомпилироваться;
  - `nova,should_panic` — runtime panic ожидается;
  - `nova,must_verify` — должны пройти **SMT контракты** (Plan 33.1);
  - `text` / `bash` / `c` — не Nova, не запускаются.
- Wrapping: блок без `fn main()` оборачивается в `fn main() => { ... }`
  (Rust convention). Импорты блока merge'аются с импортами модуля.
- Handler-coverage: doc-test, который использует эффект без handler'а,
  fail'ится при запуске с `effect not handled`. Helper attribute
  `#doc(test_handlers = "std.testing.handlers")` на module-level
  auto-importit testing handlers в каждый doc-test.

### D107. JSON output schema v1

- Schema file: `nova-doc-schema-v1.json` (JSON Schema draft 2020-12).
- Versioning: `schema_version: 1`. Внутри v1 — только additive
  changes (новые optional fields). Breaking → v2, оба поддерживаются
  параллельно ≥1 release.
- Stability commitment: `nova doc --json-schema` emits the schema
  file. CI-tooling валидирует output против него.
- Полный contract (см. §6 «JSON schema»).

---

## 3. Архитектура

### Pipeline

```
.nv files
   ↓ (Plan 35 resolve_imports_inline)
Module + peer_files (FileId-attributed)
   ↓ (lexer: D104 // /// //!)
TokenStream с DocComment(span, content, kind=outer|inner)
   ↓ (parser: attach docs + attrs к items)
AST с Item { ..., doc: Option<DocBlock>, attrs: Vec<Attr> }
   ↓ (infer_effects + type_check + lint_module — full pipeline)
TypedAST (signatures полностью разрешены: effects, contracts, generics)
   ↓ (doc_collector в nova-codegen)
DocModel — typed IR для рендеринга
   ↓ (intra_link_resolver — обходит DocModel, резолвит [X])
DocModel + resolved_links
   ↓ (doc_test_extractor — выдёргивает code-blocks с lang=nova)
DocModel + doc_tests[]
   ↓ (опционально: doc_test_runner — компилирует+запускает через test_runner)
DocModel + test_results[]
   ↓ (renderer per format)
markdown | json | html | man
```

### Crate layout

```
compiler-codegen/src/
   lexer/
      token.rs          — TokenKind::DocComment { kind, content, span }
      mod.rs            — recognize /// //! //// и /// после // (precedence)
   ast/
      mod.rs            — добавляем generic Attr struct +
                          doc: Option<DocBlock> на Fn/Type/Const/Effect/Handler/Protocol/Module
   parser/
      mod.rs            — collect docs + attrs до declarations,
                          attach к next item
   doc/                  (NEW)
      mod.rs            — public API: build_doc_model, render_*
      model.rs          — DocModel structs
      collector.rs      — TypedAST → DocModel
      links.rs          — intra-doc link resolver
      sections.rs       — recognize # Effects / # Errors / # Panics /
                          # Safety / # Examples / # Contracts / # Since /
                          # See also / # Deprecated
      doctest.rs        — extract + harness
      markdown.rs       — md parsing (pulldown-cmark) + render
      render_md.rs      — DocModel → Markdown
      render_json.rs    — DocModel → JSON (schema v1)
      render_html.rs    — DocModel → static HTML site
      render_man.rs     — DocModel → groff man-page
      search_index.rs   — JSON search index для HTML
      diff.rs           — semantic API diff между двумя DocModel
      schema.rs         — embedded JSON Schema v1 (для --json-schema)
   bin/
      nova-codegen.rs   — subcommand `doc` (internal, для nova CLI)

nova-cli/src/
   cmd_doc.rs           — CLI surface, routing к nova-codegen
   main.rs              — register `nova doc`
```

### Зависимости (crates)

- `pulldown-cmark` (CommonMark + GFM) — markdown parsing. Pure-Rust,
  no_std-compatible, MIT-OR-Apache, актуально поддерживается.
- `serde` + `serde_json` (уже в проекте).
- `tinytemplate` или `handlebars` для HTML templates — **отвергнуто**:
  shipped HTML — embedded `format!` + minimal `Display`-templates,
  без deps. Снижение поверхности атаки + reproducibility.
- `notify` (для `--watch`) — опционально, feature-gated.
- `roff` — для man-page. Если crate выглядит слабым, пишем мини-emitter
  своими руками (~100 LOC).

Все deps **обязательно** через workspace-уровневые versions с
`Cargo.lock` под контролем. Сторонние библиотеки не патчим
([feedback_third_party_libs](../../memory/feedback_third_party_libs.md)).

---

## 4. DocModel (typed IR)

```rust
pub struct DocModel {
    pub schema_version: u32,            // 1
    pub nova_version: String,           // от compiler-codegen Cargo.toml
    pub generated_at: String,           // ISO-8601 UTC; deterministic
                                        // через SOURCE_DATE_EPOCH env
    pub workspace: Option<WorkspaceInfo>,
    pub modules: Vec<DocModule>,
    pub intra_doc_links: Vec<LinkRecord>, // для CI диагностики
    pub doc_tests: Vec<DocTest>,
}

pub struct DocModule {
    pub path: ModulePath,               // parent.X (D29 rev-3)
    pub kind: ModuleKind,               // SingleFile | Folder
    pub peers: Vec<PeerFile>,           // FileId + relpath + size
    pub summary: Option<MarkdownBlock>, // первое предложение module-doc
    pub description: Option<MarkdownBlock>, // полный //! блок
    pub items: Vec<DocItem>,
    pub attrs: ItemAttrs,
    pub source_span: Span,
}

pub enum DocItem {
    Fn(DocFn),
    Type(DocType),                      // record | sum | alias | newtype
    Protocol(DocProtocol),
    Effect(DocEffect),
    Handler(DocHandler),
    Const(DocConst),
    Module(DocModule),                  // nested
}

pub struct DocFn {
    pub name: String,
    pub visibility: Visibility,         // Export | Private
    pub signature: FnSignature {
        pub generics: Vec<GenericParam>,    // with bounds (Plan 15)
        pub receiver: Option<Receiver>,     // method?
        pub params: Vec<Param>,             // mut?, name, type
        pub return_type: TypeRef,
        pub effects: Vec<EffectRef>,        // resolved links to effects
        pub raises: Vec<TypeRef>,           // Fail[E] flattened
        pub contracts: Contracts {
            pub requires:   Vec<ContractExpr>,
            pub ensures:    Vec<ContractExpr>,
            pub reads:      Vec<ContractExpr>,
            pub modifies:   Vec<ContractExpr>,
            pub decreases:  Option<ContractExpr>,
            pub invariants: Vec<ContractExpr>,
            pub verify_status: VerifyStatus, // Proven | Timeout(ms) |
                                             // Unverified | Trusted |
                                             // RuntimeFallback
        },
    },
    // ContractExpr — НЕ markdown. Реальное контрактное выражение из
    // Plan 33 AST + детерминированный pretty-print для рендеринга.
    // pub struct ContractExpr {
    //     pub ast: ContractExprId,   // ссылка в Plan 33 contract-AST
    //     pub pretty: String,        // canonical pretty-print (deterministic)
    //     pub note: Option<MarkdownBlock>, // опц. прозаический комментарий автора
    // }
    pub doc: Option<DocBlock>,
    pub sections: StandardSections,     // см. ниже
    pub attrs: ItemAttrs,
    pub location: SourceLocation,       // file:line:col + span end
}

pub struct DocType {
    pub name: String,
    pub kind: TypeKind {                // Record | Sum | Alias | Newtype
        Record { fields: Vec<Field> },
        Sum { variants: Vec<Variant> },
        Alias { target: TypeRef },
        Newtype { inner: TypeRef },
    },
    pub generics: Vec<GenericParam>,
    pub invariant: Option<ContractExpr>, // record-level invariant (33.2)
    pub protocol_impls: Vec<ProtocolRef>, // структурно резолвится
    pub methods: Vec<DocFn>,            // методы на типе
    pub doc: Option<DocBlock>,
    pub sections: StandardSections,
    pub attrs: ItemAttrs,
    pub visibility: Visibility,
    pub location: SourceLocation,
}

pub struct DocProtocol {
    pub name: String,
    pub generics: Vec<GenericParam>,
    pub required_methods: Vec<MethodSig>,
    pub provided_methods: Vec<DocFn>,
    pub implementors: Vec<TypeRef>,     // структурные impls в workspace
    pub doc: Option<DocBlock>,
    pub sections: StandardSections,
    pub attrs: ItemAttrs,
    pub visibility: Visibility,
    pub location: SourceLocation,
}

pub struct DocEffect {
    pub name: String,
    pub generics: Vec<GenericParam>,
    pub operations: Vec<EffectOp>,      // signatures каждой op
    pub handlers: Vec<HandlerRef>,      // handlers в workspace
    pub doc: Option<DocBlock>,
    pub sections: StandardSections,
    pub attrs: ItemAttrs,
    pub visibility: Visibility,
    pub location: SourceLocation,
}

pub struct DocHandler {
    pub name: String,
    pub effect_ref: EffectRef,
    pub interrupt_type: TypeRef,        // D87 IRT
    pub implementations: Vec<EffectOpImpl>,
    pub doc: Option<DocBlock>,
    pub sections: StandardSections,
    pub attrs: ItemAttrs,
    pub visibility: Visibility,
    pub location: SourceLocation,
}

pub struct DocConst {
    pub name: String,
    pub ty: TypeRef,
    pub value: Option<String>,          // pretty-printed литерал
    pub doc: Option<DocBlock>,
    pub sections: StandardSections,
    pub attrs: ItemAttrs,
    pub visibility: Visibility,
    pub location: SourceLocation,
}

pub struct DocBlock {
    pub raw: String,                    // сырой markdown
    pub rendered_md: String,            // post-link-resolve
    pub summary: String,                // первое предложение
    pub spans: Vec<DocCommentSpan>,     // для error reporting
}

pub struct StandardSections {
    pub examples: Vec<DocTest>,         // # Examples
    pub errors:   Option<MarkdownBlock>, // # Errors / # Failures
    pub panics:   Option<MarkdownBlock>, // # Panics
    pub safety:   Option<MarkdownBlock>, // # Safety
    pub effects_note: Option<MarkdownBlock>, // # Effects (override)
    pub contracts_note: Option<MarkdownBlock>, // # Contracts (override)
    pub since:    Option<String>,
    pub see_also: Vec<LinkRef>,
    pub deprecated: Option<DeprecationNote>,
}

pub struct ItemAttrs {
    pub deprecated: Option<DeprecationNote>,
    pub since: Option<String>,
    pub stability: Stability,           // Stable | Unstable | Experimental
    pub hide_doc: bool,
    pub aliases: Vec<String>,
    pub forbid: Vec<EffectRef>,         // D63
    pub realtime: bool,                 // D64
    pub realtime_nogc: bool,
    pub pure: bool,                     // D-pure
    pub allow_transit: Vec<EffectRef>,
    pub must_verify: bool,
    pub unverified: Option<String>,     // reason
    pub verify_timeout_ms: Option<u32>,
    pub trusted: bool,                  // SMT trust boundary
    pub other: Vec<RawAttr>,            // future-proofing
}
```

**Auto-derived sections** (если автор не указал явно):

- `# Effects` — если effect row не empty, авто-секция из `effects[]`.
- `# Errors` — если `raises[]` не empty.
- `# Panics` — если в теле есть `panic!`/`assert!`/`unreachable!`
  (грубая static оценка, лучше чем ничего).
- `# Contracts` — если `requires`/`ensures` не empty.

Auto-derived помечается `(auto)` в HTML/MD и `auto: true` в JSON,
чтобы LLM/человек видели что это не написано автором.

---

## 5. Intra-doc links

### Синтаксис

- `[Name]` — поиск Name в порядке: текущий module → imports → prelude.
- `[mod.Name]` — fully-qualified, без поиска.
- `[Self.method]` — current type (внутри `impl`/method context).
- `[Name as alias]` — alias для display text (Rust shortcut).
- `[text](explicit_url)` — обычная markdown-ссылка, не резолвится.
- `<https://...>` / `[text](url)` для external — passthrough.

### Резолвинг

1. Пытаемся резолвить через тот же name-resolver, что type-checker.
2. Если резолвится — emit структурную ссылку в DocModel:
   ```
   LinkRecord { from: ItemId, to: ItemId, kind: Type|Fn|Const|...,
                resolved: true, anchor: "type.User" }
   ```
3. Если не резолвится — `resolved: false`, текст остаётся как
   `[Name]` в выводе. В `--check` режиме это **ошибка**.

### Rendering

- **Markdown:** `[Name](#type.User)` — anchor в same-page.
  Для cross-module — `[Name](../mod_path/index.md#type.User)`.
- **HTML:** `<a href="...">Name</a>` с tooltip = summary target'а.
- **JSON:** ссылки в `intra_doc_links[]` массиве; в текст-полях
  оставляем `[Name]` markup чтобы LLM мог интерпретировать.

---

## 6. JSON schema v1 (контракт)

```jsonc
{
  "$schema": "https://nova-lang.org/schemas/doc/v1.json",
  "schema_version": 1,
  "nova_version": "0.5.0",
  "generated_at": "2026-05-14T00:00:00Z",
  "workspace": {
    "root": "/abs/path",
    "members": ["std", "examples", "nova_tests"]
  },
  "modules": [
    {
      "path": "std.encoding.hex",
      "kind": "folder",                 // or "single_file"
      "peers": [
        { "file": "std/encoding/hex/encode.nv", "lines": 124 },
        { "file": "std/encoding/hex/decode.nv", "lines": 198 }
      ],
      "summary": "Hex encoding and decoding.",
      "description_md": "...",
      "attrs": { "stability": "stable", "since": "0.4.0" },
      "items": [
        {
          "kind": "fn",
          "name": "encode",
          "visibility": "export",
          "signature": {
            "generics": [],
            "receiver": null,
            "params": [
              { "name": "bytes", "mut": false,
                "type": { "name": "[u8]", "id": "core.slice<u8>" } }
            ],
            "return_type": { "name": "str", "id": "core.str" },
            "effects": [],
            "raises": [],
            "contracts": {
              "requires": [],
              "ensures":  ["result.len() == bytes.len() * 2"],
              "reads":    [],
              "modifies": [],
              "decreases": null,
              "invariants": [],
              "verify_status": "proven"
            }
          },
          "doc_md": "Encodes bytes as lowercase hex.",
          "summary": "Encodes bytes as lowercase hex.",
          "sections": {
            "examples": [
              { "code": "let s = encode(b\"\\xff\")\nassert(s == \"ff\")",
                "lang": "nova", "modifiers": [], "auto": false }
            ],
            "errors": null,
            "panics": null,
            "safety": null,
            "since": "0.4.0",
            "see_also": [{ "to": "std.encoding.hex.decode" }],
            "deprecated": null
          },
          "attrs": {
            "stability": "stable",
            "must_verify": true,
            "pure": true,
            "aliases": ["hex_encode", "to_hex"]
          },
          "location": {
            "file": "std/encoding/hex/encode.nv",
            "line": 42, "column": 1,
            "end_line": 58, "end_column": 2
          }
        }
        // ... DocType, DocConst, DocProtocol, DocEffect, DocHandler
      ]
    }
  ],
  "intra_doc_links": [
    { "from": "std.encoding.hex.encode",
      "to":   "std.encoding.hex.decode",
      "kind": "fn", "resolved": true }
  ],
  "doc_tests": [
    { "id": "std.encoding.hex.encode::doc_0",
      "module": "std.encoding.hex",
      "item": "encode",
      "lang": "nova", "modifiers": [],
      "code": "...", "status": "passed", "duration_ms": 12 }
  ]
}
```

**Stability rules:**

- v1 — additive only. Новые optional fields разрешены, но
  существующие fields не меняют тип / семантику.
- Удаление field, переименование, смена типа → v2. Оба
  поддерживаются ≥1 release с deprecation warning.
- `nova doc --json-schema` emits embedded JSON Schema; CI
  валидирует output против него.

---

## 7. CLI surface

Соблюдает D95 (positional path, recurse default):

```
nova doc [<target>...] [flags]

target:
  (none)                       — walks parents до nova.toml, default
                                 = doc на текущем модуле/workspace
  <module-path>                — admin, std.encoding.hex, parent.X
  <file-path>                  — ./admin/users.nv
  <dir-path>                   — ./admin/   (folder-module)
  --workspace                  — все members в nova.toml

flags:
  --format <fmt>               markdown|json|html|man
                               default: markdown для stdout,
                                        html для -o <dir>/,
                                        json для -o <file>.json
  --output, -o <path>          файл или директория
  --include-private            non-exported items, помеченные [internal]
  --check                      lint mode (см. §8)
  --doc-tests                  компилировать+запускать doc-tests
                               default: on для --check, off для render
  --no-doc-tests               disable doc-tests даже в --check
  --diff <baseline.json>       semantic API diff
  --since <version>            фильтр: только items с #since >= version
  --json-schema                emit JSON Schema v1, exit 0
  --search-index               emit search index alongside HTML
                               (default: on для html, off остальное)
  --theme light|dark|auto      HTML theme (default: auto через
                               prefers-color-scheme)
  --css <file>                 override theme CSS
  --color auto|always|never    terminal color (D95)
  --watch                      re-render on file change
  --deterministic              sort items alphabetically
                               (default: source order)
  --jobs, -j N                 параллелизм для doc-tests
                               (default: num_cpus)
  --offline                    запретить любые network fetches
                               (no-op в bootstrap, future-proof)

exit codes (Plan 36 / D95):
  0    success
  1    diagnostic (broken link / failing doc-test / missing doc в check)
  2    usage error (bad flag, path not found)
  101  panic (tool bug)
```

---

## 8. `--check` режим (CI gate)

`nova doc --check` — **обязательный CI gate для stdlib**. Семантика:

| Lint | Default | Можно отключить |
|---|---|---|
| Missing doc на export item | error | `#doc(allow_missing)` per-item |
| Broken intra-doc link | error | — (исправь или удали) |
| Failing doc-test | error | `nova,ignore` модификатор |
| Doc-test compile-fail (без `compile_fail`) | error | `nova,no_run` |
| Doc-test panic (без `should_panic`) | error | — |
| `#experimental` в `export` item | warn | `--allow-experimental-exports` |
| Section name typo (e.g. `# Example` без `s`) | warn | — |
| Code block без lang tag | warn | поставить `text` или `nova` |
| Markdown syntax error | warn | — |
| Effect/contract несогласован с # Effects/# Contracts текстом | warn | — |

`--check` exit code 1 на любую error, exit 0 на warnings.
`--check --deny-warnings` промоутит warnings в errors.

---

## 9. Doc-tests pipeline (D106)

1. **Extract:** в `doc/doctest.rs` обходим DocModel, собираем
   `(item_id, block_idx, lang, modifiers, code)`.
2. **Synthesize:** каждый block → temp `.nv` файл вида:
   ```
   module nova_tests._doctest.<sha8>
   import <module> as _doc_target
   <module's imports inlined>
   [optional: import std.testing.handlers]
   fn main() => {
     <code, оборачивается в block если нет fn main>
   }
   ```
3. **Build + run:** через существующий `test_runner` (Plan 24).
   Reuse parallel job pool + EXPECT-маркеры:
   - `compile_fail` → `EXPECT_COMPILE_ERROR`
   - `should_panic` → `EXPECT_RUNTIME_PANIC`
   - `must_verify` → дополнительный gate `nova check --strict-contracts`
4. **Cleanup:** temp dir удаляется после прогона, если не `--keep-temp`.
5. **Reporting:** аналогично `nova test`, summary в stderr.

**Handlers convention:** doc-test, использующий эффект `E` без `with`-
блока, **fail**'ится. Helper: `#doc(test_handlers = "X.Y.Z")` на
module-level auto-impotrит handlers в каждый doc-test модуля.
Convention для stdlib: `std/testing/handlers.nv` содержит mock-handlers
для всех std эффектов (Io/Net/Db/Fs/Time/Random/Log/Trace).

---

## 10. HTML output

Standalone static site. Что включено:

- **Layout:** sticky sidebar (TOC по модулям), main content,
  optional right-panel (item details / source).
- **Search:** client-side, indexed `search-index.json`. JavaScript
  ~150 LOC, no frameworks. Search by name + alias + summary text.
- **Theme:** light/dark/auto через CSS variables + `prefers-color-
  scheme`. `--theme` override.
- **Source links:** каждое item имеет `[src]` link в file:line.
  Если `--source-html` enabled — рендерим .nv в highlighted HTML
  alongside (rustdoc-style).
- **Footer:** «generated by nova doc v0.5.0 — schema v1».
- **Reproducibility:** SOURCE_DATE_EPOCH respected → byte-identical
  output на одной revision.

Embedded assets (CSS/JS) — в `compiler-codegen/assets/doc-html/` —
сompiled-in через `include_str!`. Никаких runtime template engines.

---

## 11. Diff mode

`nova doc --diff baseline.json` — semantic API diff. Полезно для
release notes и breaking-change detection.

Categories:

| Кат. | Что | Severity |
|---|---|---|
| **Removed** | export item был, теперь нет | breaking |
| **Added** | new export item | minor |
| **SigChange** | parameter type/count/order, return | breaking |
| **EffectAdd** | в signature добавлен эффект (не было) | breaking |
| **EffectRemove** | эффект убран | minor |
| **ContractTighten** | requires добавлен/усилен | breaking |
| **ContractLoosen** | requires ослаблен | minor |
| **ContractStrengthen** | ensures усилен | minor (better) |
| **ContractWeaken** | ensures ослаблен | breaking |
| **Deprecated** | item получил `#deprecated` | minor |
| **Undeprecated** | `#deprecated` снят | minor |
| **StabilityChange** | tier изменён | depends |

Output: markdown table (default), JSON (`--format json`), exit 1
если найден breaking change и `--deny-breaking` (для CI release-gate).

---

## 12. Phases

### Ф.0 — Spec (D104-D107)

- Написать 4 D-decisions: **D104** (doc-comment syntax) в
  `spec/decisions/03-syntax.md`; **D105** (doc attributes), **D106**
  (doc-test semantics), **D107** (JSON schema v1) в `09-tooling.md`.
- **D105 обязан cross-ref'ить существующий D101** (`#doc "..."`
  module-attr, 07-modules.md) и зафиксировать namespace-разделение
  `#doc "строка"` vs `#doc(key=val)`. **D104 обязан зафиксировать
  сосуществование `//!` с D101**.
- Перед стартом — верифицировать, что D104-D107 свободны (на момент
  ревизии плана 2026-05-15 max = D103). При коллизии — взять следующий
  свободный блок и обновить плана-внутренние ссылки.
- В overview.md / revolutionary.md — упомянуть `nova doc` как
  shipping, не roadmap.
- Acceptance: spec reviewed, D104-D107 не коллидируют, history/evolution.md
  обновлён, D101/D100 не изменены (только cross-ref'ятся).
- Размер: ~600-900 LOC spec.

### Ф.1 — Lexer (D104)

- `TokenKind::DocComment { kind: Outer | Inner, content: String,
  span: Span }`.
- Recognize `///` vs `////` (4+) vs `//`. `//!` только в начале файла.
- Tests: lexer-corpus с doc + non-doc + edge cases (CRLF, BOM,
  trailing spaces).
- Размер: ~80-120 LOC.

### Ф.2 — Parser: attrs generalization + doc attachment

- Generic `Vec<Attr>` инфраструктура (rename existing realtime_attr +
  must_verify_attr в pieces of generic Attr). Keep backward-compat
  для known attrs (recognize → typed enum).
- Pending-docs/pending-attrs accumulator перед каждой declaration.
- Attach к `Fn` / `Type` / `Const` / `Module` / `Effect` / `Handler` /
  `Protocol`. Поле `doc: Option<DocBlock>` + `attrs: Vec<Attr>`.
- Inner doc `//!` — на module после `module X` + imports.
- Tests: positive + negative (`//!` не в начале, `///` без следующего
  item — warning).
- Размер: ~250-400 LOC + AST changes (~50 LOC).

### Ф.3 — Doc attributes (D105)

- Расширить attribute recognizer: `#deprecated`, `#since`, `#stable`,
  `#unstable`, `#experimental`, `#hide_doc`, `#doc_alias`,
  `#doc(inline|no_inline)`.
- `#deprecated` triggers diagnostics (lint) на use-sites.
- Tests: каждый атрибут positive + negative use.
- Размер: ~200-300 LOC.

### Ф.4 — DocModel + collector

- `compiler-codegen/src/doc/model.rs` + `collector.rs`.
- TypedAST → DocModel walker. Полная сигнатура (effects + contracts +
  generics + bounds + receiver).
- Visibility filter (default = export only, `--include-private`
  переключает).
- Tests: collector emits expected DocModel на golden corpus (5-7
  модулей).
- Размер: ~600-900 LOC.

### Ф.5 — Markdown parsing + sections

- `pulldown-cmark` integration в `doc/markdown.rs`.
- Recognize standard sections (`# Effects`, `# Errors`, ...) → split
  в `StandardSections`. Body — оставшийся markdown.
- Summary extraction (первое предложение через простой regex/parser).
- Auto-derive missing sections из signature.
- Tests: разные комбинации секций, edge cases (empty body, only
  sections, sections в неправильном порядке).
- Размер: ~300-500 LOC.

### Ф.6 — Intra-doc link resolver

- `doc/links.rs`. Reuse name-resolver из type-checker (shared utility).
- Three resolution modes: in-module, imported, prelude.
- Cross-module: через известные members workspace.
- Output: `LinkRecord` per ссылку (resolved + target ItemId или
  unresolved + text).
- Tests: positive (single-module / cross-module / prelude),
  negative (broken link surfaces в `--check`).
- Размер: ~250-400 LOC.

### Ф.7 — Doc-test extractor + runner

- `doc/doctest.rs`: extract code blocks, parse modifiers.
- Synthesize `.nv` файлы во временный каталог.
- Hook в `test_runner.rs`: doc-tests inj'ектируются как обычные тесты
  с display-name `<module>::<item>::doc_<N>`.
- EXPECT-маркер mapping (`compile_fail` → EXPECT_COMPILE_ERROR, etc.).
- Tests: каждый модификатор + handler scenario.
- Размер: ~400-600 LOC.

### Ф.8 — Markdown renderer

- `doc/render_md.rs`. CommonMark output.
- Signature rendering: типы как inline-code, effects подсвечены, contracts
  отдельным subsection.
- Tests: golden file comparison на тех же 5-7 модулях.
- Размер: ~300-500 LOC.

### Ф.9 — JSON renderer + schema (D107)

- `doc/render_json.rs` — serde-derived, stable field order
  (alphabetical внутри struct, deterministic).
- `doc/schema.rs` — embedded JSON Schema v1, emit'tится через
  `--json-schema`.
- Validator (опционально): nova doc validates own output (sanity).
- Tests: schema validates corpus output; round-trip
  (DocModel → JSON → parse → equal).
- Размер: ~400-600 LOC + schema файл ~500-1000 lines.

### Ф.10 — HTML renderer + signature-aware search index

- `doc/render_html.rs` + embedded CSS/JS в `assets/doc-html/`.
- Theme via CSS variables + `prefers-color-scheme`.
- **Signature-aware search index** (не name-only — это и есть «лучше
  rustdoc», §19.2). Индексируемые поля на каждый item: `name`,
  `aliases` (`#doc_alias`), `summary`, **`effects[]`**, **`raises[]`**,
  **`contract_status`** (proven/unverified/...), `kind`, `stability`.
  Запросы вида «functions raising NotFound», «realtime functions»,
  «unverified contracts» — резолвятся клиентским индексом. Формат
  индекса — часть schema-контракта (детерминированный, versioned).
- **Page-size budget.** Embedded assets shared (один CSS, один JS на
  весь сайт, не per-page), no framework, no per-page inline JS.
  Acceptance — budget-тест в Ф.19: средний размер item-страницы
  ≤ заданного порога (фиксируется в Ф.10, напр. 50 KB без assets).
- Source links `[src]` → file:line; `--source-html` рендерит .nv
  highlighted alongside.
- Reproducible (SOURCE_DATE_EPOCH, deterministic ordering, embedded
  assets — byte-identical между прогонами).
- Tests: smoke (HTML парсится валидным parser'ом), search index JSON
  валиден против своей схемы, no broken anchors, page-size budget,
  signature-search возвращает ожидаемые items на golden corpus.
- Размер: ~800-1200 LOC + ~500 LOC assets.

### Ф.11 — Man-page renderer

- `doc/render_man.rs` → groff output.
- Sections: NAME, SYNOPSIS (signature), DESCRIPTION, EFFECTS,
  ERRORS, EXAMPLES, SEE ALSO.
- Tests: smoke (groff parses), один golden модуль.
- Размер: ~200-300 LOC.

### Ф.12 — CLI subcommand

- `nova-cli/src/cmd_doc.rs` — все флаги из §7.
- `nova-codegen doc` internal subcommand — actual implementation.
- Workspace discovery (`nova.toml` walk-up, как `nova check`).
- Tests: каждый флаг smoke-test.
- Размер: ~400-600 LOC.

### Ф.13 — Diff mode

- `doc/diff.rs` — semantic API diff (§11).
- Output: markdown table или JSON.
- Tests: каждая category change, breaking detection, exit codes.
- Размер: ~300-500 LOC.

### Ф.14 — Check mode

- Integrate Ф.6 (broken links) + Ф.7 (doc-tests) + missing-doc lint.
- Exit codes per §8 table.
- `--deny-warnings` promotion.
- Tests: каждый lint positive + negative.
- Размер: ~200-300 LOC.

### Ф.15 — Watch mode

- `notify` crate (feature-gated, default-on).
- Debounce 200ms, re-resolve only changed module + dependents.
- Tests: smoke (touch file → re-render fires once).
- Размер: ~150-250 LOC.

### Ф.16 — Incremental cache (обязательная фаза)

Не «optional» — это load-bearing для «быстрее rustdoc» (§19.2 первая
жалоба: rustdoc doc-tests медленные, CI bottleneck). Production-grade
doc-tooling обязан не пересчитывать неизменённое.

- On-disk cache: blake3-hashed **DocModel per module** + **doc-test
  results per test** (id + input-hash → pass/fail/duration).
- Invalidate: content hash входных `.nv` (не mtime — mtime ненадёжен
  в CI/git checkout) + hash зависимостей (изменился импортируемый
  модуль → инвалидируется зависимый DocModel) + nova_version +
  schema_version.
- Cache key включает флаги, влияющие на вывод (`--include-private`,
  `--theme`, ...), чтобы не отдать stale-вывод под другими флагами.
- `nova doc --no-cache` — форсировать clean (CI bisect, отладка).
- Cache-директория — `<workspace>/.nova/doc-cache/`, в `.gitignore`.
- Tests: touch неизменяющий файл → cache hit; изменить тело функции →
  только её DocModel + зависимые инвалидируются; изменить импорт →
  каскад; doc-test с неизменным input → результат из кэша.
- Размер: ~350-550 LOC.

### Ф.17 — CI integration

- `nova-cli` exit codes match Plan 36.
- GitHub Actions example workflow (`docs/ci/nova-doc-check.yml`).
- pre-commit hook example.
- Размер: ~50 LOC config + docs.

### Ф.18 — Stdlib doc-pass — отдельный трек, не блокер закрытия Plan 45

Документирование всего `std/` (~50 модулей, ~3000-5000 LOC
doc-comments) — это **не часть Plan 45**. Plan 45 — это **инструмент**;
наполнение stdlib документацией — отдельный долгоиграющий трек,
координируемый с Plan 18 (stdlib roadmap). Бандлить 3-5k LOC
doc-writing в Plan 45 = делать его неуказуемо-закрываемым.

В рамках Plan 45 (как доказательство, что инструмент работает на
реальном коде):

- Полностью задокументировать **репрезентативный набор** — 3-5 std
  модулей разных видов: folder-module, single-file, модуль с
  эффектами, модуль с контрактами, модуль с protocol/handler.
- На этом наборе `nova doc --workspace --check --doc-tests` обязан
  быть зелёным — это acceptance-gate Plan 45.
- Полный проход по `std/` — трекать отдельным планом (или как часть
  Plan 18), Plan 45 от него не зависит.
- Размер (в рамках Plan 45): ~400-700 LOC doc-comments + doc-tests на
  репрезентативном наборе.

### Ф.19 — Tests + golden files

- `nova_tests/doc/` — minimum 20 тестов:
  - `doc_basic.nv` (single-file fn + type + const)
  - `doc_folder_module.nv` (3-peer aggregate)
  - `doc_intra_links.nv` (positive resolve)
  - `doc_intra_links_broken.nv` (EXPECT_COMPILE_ERROR в --check)
  - `doc_doctest_pass.nv` / `_fail.nv` / `_no_run.nv` /
    `_compile_fail.nv` / `_should_panic.nv` / `_must_verify.nv`
  - `doc_effects_section.nv` (auto-derive + override)
  - `doc_contracts_section.nv`
  - `doc_deprecated.nv` (attr triggers lint)
  - `doc_hide_doc.nv` (item hidden)
  - `doc_aliases.nv` (search index includes)
  - `doc_re_exports.nv` (`export import` paths)
  - `doc_protocol_impls.nv` (auto-list implementors)
  - `doc_effect_handlers.nv` (auto-list handlers)
  - `doc_json_schema_valid.nv` (output validates against schema)
  - `doc_diff_breaking.nv` (semantic diff catches sig change)
  - `doc_diff_minor.nv` (semantic diff catches additions)
  - `doc_reproducible.nv` (две прогона → byte-identical)
  - `doc_workspace_check_clean.nv`
- Golden files в `nova_tests/doc/golden/`.
- Размер: ~800-1200 LOC tests + golden files.

### Ф.20 — Docs

- Update `docs/promts/read-project.md`: упомянуть `nova doc` workflow.
- `docs/user-guide/doc-writing.md` (new): guide для авторов
  (как писать doc-comments, sections, links, doc-tests).
- `docs/schema-v1.md` — schema reference для AI tooling.
- README.md: tooling section, упомянуть `nova doc`.
- Размер: ~600-1000 LOC docs.

---

## 13. Acceptance criteria (closing plan)

- [ ] D104-D107 written, reviewed, merged в spec; D100/D101 не
      изменены, только cross-ref'нуты; namespace `#doc` разделён.
- [ ] `nova doc <module>` produces markdown для single-file + folder
      module.
- [ ] `nova doc --format json` validates against embedded schema v1.
- [ ] `nova doc --format html -o site/` produces standalone site
      (signature-aware search работает — запрос «raises NotFound»
      возвращает ожидаемое; theme switches).
- [ ] HTML page-size budget соблюдён (Ф.10 порог).
- [ ] `nova doc --check` exit 1 на missing doc / broken link /
      failing doc-test.
- [ ] `nova doc --diff baseline.json` detects all categories из §11.
- [ ] `nova doc --workspace --check --doc-tests` зелёный на
      **репрезентативном наборе** std-модулей (Ф.18) — proof, что
      инструмент работает на реальном коде. Полный проход по `std/` —
      отдельный трек, не блокер.
- [ ] Incremental cache: повторный `nova doc` без изменений — cache
      hit; точечное изменение инвалидирует только затронутое + зависимое.
- [ ] `nova_tests/doc/*` — 20+ тестов, все PASS.
- [ ] HTML output reproducible byte-identical с SOURCE_DATE_EPOCH.
- [ ] JSON output deterministic (sorted keys, stable item order).
- [ ] Exit codes per Plan 36 / D95 (0/1/2/101).
- [ ] No regression в `nova test` (271+ PASS).
- [ ] `nova doc --json-schema` emits valid JSON Schema 2020-12.

---

## 14. Size estimate

| Phase | LOC (impl) | LOC (tests/docs) |
|---|---|---|
| Ф.0 spec | 600-900 | — |
| Ф.1 lexer | 80-120 | 100-150 |
| Ф.2 parser/AST | 300-450 | 200-300 |
| Ф.3 attrs | 200-300 | 150-200 |
| Ф.4 DocModel + collector | 600-900 | 200-300 |
| Ф.5 markdown+sections | 300-500 | 150-200 |
| Ф.6 link resolver | 250-400 | 150-200 |
| Ф.7 doc-tests | 400-600 | 200-300 |
| Ф.8 render md | 300-500 | 100-150 |
| Ф.9 render json + schema | 400-600 | 500-1000 (schema) |
| Ф.10 render html + search | 800-1200 | 500 (assets) |
| Ф.11 render man | 200-300 | 50 |
| Ф.12 CLI | 400-600 | 100-150 |
| Ф.13 diff | 300-500 | 100-150 |
| Ф.14 check | 200-300 | 100-150 |
| Ф.15 watch | 150-250 | 50 |
| Ф.16 cache (обязательная) | 350-550 | 150-200 |
| Ф.17 CI | 50 | 100 |
| Ф.18 stdlib doc-pass (репрезентативный набор) | — | 400-700 |
| Ф.19 tests | — | 800-1200 |
| Ф.20 docs | — | 600-1000 |
| **Total** | **~6000-9000** | **~4500-6800** |

> Полный stdlib doc-pass (~3000-5000 LOC doc-comments на ~50 модулей) —
> **вне** этих оценок: отдельный трек, координируется с Plan 18.

Сравнительно: rustdoc ≈ 30k LOC Rust (без markdown crate). Naш план
скромнее за счёт reuse compiler infrastructure (parser, resolver,
test_runner) и embedded HTML без template engine.

**Календарь:** ~8-12 фокус-сессий, не линейно (Ф.4 + Ф.10 — heaviest).
MVP-cut (Ф.0-Ф.9 + Ф.12 + Ф.14 + Ф.19) — 5-7 сессий до точки
«можно reliable генерировать markdown+json для std».

---

## 15. Risks / Trade-offs

- **AST refactor для generic `Vec<Attr>`.** Существующий
  `realtime_attr: RealtimeAttr` rename + миграция call-sites. Risk:
  silent regression в Plan 16 #realtime gating. Mitigation:
  переименовываем через `cargo check --all-targets` + полный `nova test`
  на каждом коммите.
- **pulldown-cmark dep.** Pure Rust, well-maintained, MIT/Apache.
  Альтернатива (написать свой CommonMark) — overkill. Pin major
  version в `Cargo.toml`.
- **Doc-test scope explosion.** Stdlib doc-test'ы могут стать
  существенным временем CI. Mitigation: `--no-doc-tests` для fast-loop,
  doc-tests в `--check --doc-tests` для full CI.
- **Schema v1 lock-in.** Любое breaking change в JSON → v2 + ≥1
  release параллельной поддержки. Mitigation: rigorous review Ф.9
  перед merge.
- **HTML reproducibility.** SOURCE_DATE_EPOCH, deterministic ordering,
  embedded assets — необходимы для CI byte-identical. Mitigation:
  reproducibility test в Ф.19.
- **Markdown injection.** Doc-comment с `# heading` может shadow'ить
  output headings. Mitigation: rebase user headings на +2 уровня
  (`# Foo` пользователя → `### Foo` в output).
- **Cross-module type-resolution для intra-doc links.** Performance:
  при `--workspace` index всех items в `O(items)` map'у; resolve в
  `O(1)`. Acceptable.
- **Breaking change `@realtime` → `#realtime` legacy.** Already
  closed by D96; planner relevant только если кто-то ещё откладывает
  миграцию.

---

## 16. Dependencies / position в roadmap

**Blocking:**
- ✅ Plan 35 (cross-file resolve) — нужен для intra-doc links cross-module.
- ✅ Plan 42 MVP (folder-modules) — нужен для multi-peer aggregation.
- ✅ Plan 36 MVP (CLI hardening) — нужен для exit codes / path conv.
- ✅ Plan 24 (test_runner subcommands) — reuse для doc-tests.
- ✅ Plan 33.1 V1 (contracts) — нужен для contract rendering + must_verify.
- ✅ D96 (attributes) — нужен для doc-attrs.
- ✅ Plan 42.11 / D101 (`#doc "..."` module-attr) — отгружен; D104 `//!`
  и D105 doc-attrs строятся поверх, не конфликтуют (см. §2).

**Non-blocking but improves:**
- Plan 42.4 (per-file imports scope) — без него folder-module
  doc-tests merge'аются impostors. С 42.4 attribution per-peer
  becomes exact.
- Plan 18 (stdlib roadmap) — `nova doc --workspace --check` особенно
  ценен когда std/ становится толстой.

**Future post-Plan-45:**
- Versioned docs / package registry — Plan 03.
- docs.rs equivalent web-host — отдельная infra.
- LSP integration: hover-doc через `nova doc --format json --target
  <item>` — отдельный план.

---

## 17. Связь со spec

- D5 (visibility) — `export` filter.
- D29 rev-3 (module declarations) — `parent.X` paths.
- D62 (effects) — § 1267 «public → nova doc» — обязательство закрыто.
- D63 (#forbid) — атрибут рендерится.
- D64 (#realtime) — атрибут рендерится.
- D78 (workspace) — `nova.toml` discovery.
- D89 (EXPECT-маркеры) — reuse в doc-test runner.
- D95 (CLI conventions) — `nova doc` follows.
- D96 (attribute syntax) — reuse для doc-attrs.
- D100 (`_module.nv` peer) — **существующий**, не изменяется; peers
  могут содержать `#doc`.
- D101 (`#doc "..."` module-attr, Plan 42.11) — **существующий**, не
  изменяется; D104 `//!` сосуществует с ним, D105 делит namespace `#doc`.
- D104-D107 (new) — Plan 45 spec deltas (doc-comment syntax, doc
  attributes, doc-test semantics, JSON schema v1).

---

## 18. Outstanding open questions

1. **Re-export inline semantics.** Default `#doc(inline)` vs
   `no_inline` для same-package vs cross-package — нужно валидировать
   на реальных stdlib re-exports.
2. **Generic instantiation rendering.** Показывать ли «typical»
   monomorphizations (e.g. `Vec[int]` примеры на странице `Vec[T]`)?
   **Решение:** нет, LLM/IDE могут синтезировать. Bloat'ит HTML.
3. **Comptime + type-level Nova features.** Когда `comptime` будет
   расширен — doc должен показывать compile-time computations? Решить
   когда `comptime` API stabilizes.
4. **Effect handler «matrix» view.** Для эффекта `Net` показывать
   таблицу всех handler'ов в workspace + их особенности (prod/test/
   mock)? Полезно, но дополнительная Ф. Оставляем post-MVP.
5. **Privacy boundary для doc-tests.** Doc-test для `export fn` —
   может ли test обращаться к private items того же модуля? **Решение:**
   да, doc-test имеет module-private access (как Rust). Документируем
   в D106.

---

## 19. Что жалуются в Go / Rust / TS и как Nova делает лучше

Сводка реальных жалоб community на godoc/pkg.go.dev, rustdoc, TypeDoc —
и как Plan 45 их закрывает. Не теоретические гипотезы: каждый пункт из
обсуждений на rust-lang/rust, golang/go issues, TypeStrong/typedoc
issues, Reddit r/rust, Hacker News.

### 19.1. godoc / pkg.go.dev

| Жалоба | Источник | Как Nova решает |
|---|---|---|
| Нет intra-doc links — `Foo` в комментарии не кликабельно | golang/go #34866, #41497 | Ф.6: `[Foo]` / `[mod.Foo]` / `[Self.method]` с резолвом через name-resolver, fail-loud в `--check` |
| Нет структурных секций (Errors/Panics/Examples) — только конвенции | dev.to discussions | Ф.5: распознаваемые `# Effects` / `# Errors` / `# Panics` / `# Safety` / `# Contracts` / `# Since` / `# See also` / `# Deprecated`, auto-derive из signature |
| Doc-tests слабые — `ExampleFoo` в отдельном test-файле, оторвано от документируемой функции | golang/go #16851 | D106 + Ф.7: code-blocks **внутри** doc-comment'а, рядом с кодом; модификаторы `no_run`/`ignore`/`compile_fail`/`should_panic`/`must_verify` |
| Нет машино-читаемого deprecation — только `Deprecated:` в прозе | golang/go #54546 | D105: `#deprecated(since=..., note=...)` + parse-time warning + render-time banner; в JSON — структурно |
| Нет JSON output — godoc/pkg.go.dev только HTML | golang/go #21342 | D107 + Ф.9: stable JSON schema v1 как первоклассный output, embedded JSON Schema для валидации |
| Generics в documentation поверхностные, late arrival | golang/go #54393 | Ф.4: `GenericParam` с bounds (Plan 15) — структурно в DocModel |
| Subpackages discovery awkward — clicking through tree | пользовательские отзывы pkg.go.dev | Ф.10: sidebar + поиск + folder-module aggregation (Plan 42 peers видны сразу) |
| Нет semantic diff / changelog generation | golang/go #51599 | Ф.13: `--diff baseline.json` с 9 категориями (Removed/SigChange/EffectAdd/ContractTighten/...) |
| Темы / dark mode — pkg.go.dev добавил недавно, godoc local не имеет | golang/go #29991 | Ф.10: light/dark/auto через CSS variables + `prefers-color-scheme`, `--css` override |
| Нет offline reproducibility — pkg.go.dev hosted | многочисленные | Ф.10 + Ф.17: HTML reproducible через `SOURCE_DATE_EPOCH`, deterministic ordering, embedded assets, `--offline` flag |

### 19.2. rustdoc

| Жалоба | Источник | Как Nova решает |
|---|---|---|
| Doc-tests **медленные** — каждый = отдельная компиляция, CI bottleneck | rust-lang/rust #45599, #67295 | Ф.7: reuse Plan 24 parallel test_runner + `--jobs N`; Ф.16: incremental cache (blake3 hash inputs) skip unchanged tests |
| Doc-tests не имеют IDE support — текст внутри строк | rust-lang/rust #50912 | D106: doc-tests synthesizes реальные `.nv` файлы во временный каталог, LSP видит как обычные test modules |
| `[Foo]` не резолвится — confusing errors | rust-lang/rust #74481, #83864 | Ф.6: error message указывает file:line, conflicting candidates, candidate suggestion (LLM-friendly) |
| HTML output **гигантский** — много CSS/JS на каждой странице | rust-lang/rust #76526 | Ф.10: embedded assets shared, no template engine, no per-page JS framework; smoke test проверяет page-size budget |
| Search ограничен — name-only, signature search скрыт | rust-lang/rust #51030 | Ф.10 + `#doc_alias`: search index включает name + aliases + effect-row + raises + contract status; LLM может искать «functions raising NotFound» |
| `#[doc(hidden)]` — **opt-out**, exports leak by default | rust-lang/rust #54574 | D5: `export` — opt-in. По умолчанию **private**. Никаких leak'ов. |
| JSON output (`rustdoc --output-format json`) — **только nightly**, schema нестабильна | rust-lang/rust #76578 | D107: stable schema v1 на **stable** компиляторе, embedded JSON Schema, additive-only changes в v1 |
| Нет нативного API diff — `cargo public-api` третий-сторонний | rust-lang/rust #58197 | Ф.13: built-in `--diff` с severity categories + `--deny-breaking` exit code для release-gate |
| Inter-crate links ненадёжны (`--extern-html-root-url`) | rust-lang/rust #56935 | Ф.6: workspace-scoped index в `O(items)` map'у, cross-module = `O(1)` resolve, no external URL configuration |
| Doc-tests не могут share setup — `# use foo::*` hack | rust-lang/rust #67918 | D106: `#doc(test_handlers = "...")` module-level + folder-module peer `_doctest_setup.nv` (Plan 42 даёт нам peers, никаких hidden hacks) |
| HTML reproducibility issues — версии rustdoc emit разный HTML | rust-lang/rust #88791 | Ф.10 + Ф.19 acceptance: byte-identical с `SOURCE_DATE_EPOCH`, reproducibility test в corpus |
| Нет native man-page output | rust-lang/rust #21178 | Ф.11: groff output из того же DocModel |
| Module-level summary обрезается на первом предложении — fragile | rust-lang/rust #74481 | Ф.5: explicit `summary` extraction правило документировано в D104; `#doc(summary = "...")` override |
| `pub use` re-export confusion — `#[doc(inline)]` per-item, забывают | rust-lang/rust #50847 | D105: `#doc(inline)` default для same-package, `no_inline` для cross-package — sensible defaults, override только когда нужно |
| Нет watch mode — community уходит в `cargo watch` | rust-lang/rust #44095 | Ф.15: `--watch` built-in, debounce 200ms, re-resolve only changed module + dependents |
| Нет "stability index" — нельзя увидеть весь `unstable`/`experimental` API сразу | rust-lang/rust #43466 | D105: `#stable`/`#unstable`/`#experimental` атрибут; `--since` filter; в JSON структурно |
| Нет deprecation timeline — `since=` есть, `until=` нет | rust-lang/rust #58622 | D105: `#deprecated(since = "0.4", until = "1.0", note = "...")` — `until` ставит deadline для удаления, CI gate `--deny-overdue-deprecations` |
| Не могут включить произвольные markdown файлы | rust-lang/rust #44732 | `#doc(include = "CHANGELOG.md")` атрибут (рассматривается в Ф.3 advanced; LP open question) |
| Нет hide-test-from-output-but-run — `# ` prefix ugly | rust-lang/rust #67918 | D106: doc-test code не показывается в HTML если префиксован `# ` (Rust convention принят, но docs explicit) или `#doc(test_only = "...")` блок |

### 19.3. TypeDoc / TSDoc

| Жалоба | Источник | Как Nova решает |
|---|---|---|
| Медленный на больших проектах — TS compiler API bottleneck | TypeStrong/typedoc #2155, #1944 | Reuse compiler-codegen — single-pass parse + type-check; Ф.16 incremental cache |
| HTML output dated — выглядит как jsdoc 2010 | TypeStrong/typedoc #1844, #2231 | Ф.10: modern layout, sticky TOC, client-side search, theme variables; comparison points = rustdoc layout |
| Multiple competing tag conventions (JSDoc vs TSDoc) — fragmented | microsoft/tsdoc #4 | D104 + D105: **один** конвент (`///` + `# Section` + `#attr`), spec'нут, не дополняется ad-hoc |
| `@example` parsing inconsistent — иногда требует lang hint | TypeStrong/typedoc #1672 | D106: lang tag формализован (default = `nova` если опущен), модификаторы документированы |
| Нет doc-test runner — `@example` не выполняется | microsoft/tsdoc #211 | D106 + Ф.7: doc-tests реально компилируются и запускаются через test_runner |
| Plugin ecosystem fragmented — темы ломаются между версиями | TypeStrong/typedoc #2089 | Никаких user-plugins. Theme = CSS variables + `--css` override. Schema lock-in через D107. |
| Нет breaking-change detection | TypeStrong/typedoc #1789 | Ф.13: built-in diff |
| `@deprecated` не connect к migration paths | TypeStrong/typedoc #1916 | D105: `#deprecated(note = "use foo instead — [foo.bar]")` — поддержка intra-doc link в note |
| `{@link Foo}` fragile, sometimes doesn't resolve | TypeStrong/typedoc #2138 | Ф.6: `[Foo]` резолвится через type-checker name-resolver, не через regex match |
| Module/namespace/file boundaries confusing | TypeStrong/typedoc #1830 | D29 rev-3: `parent.X` — один формат; folder-module = explicit; namespace concept не вводим |
| Нет JSON schema для `--json` output — schema меняется | TypeStrong/typedoc #1942 | D107: embedded JSON Schema v1, stability commitment |
| Inheritance docs incomplete | TypeStrong/typedoc #1827 | Ф.4: protocols structural, implementors auto-listed; effects + handlers — explicit relationship rendered |
| Нет проверки что `@example` компилируется | TypeStrong/typedoc #2090 | D106: compile_fail модификатор делает «не должен компилироваться» first-class; default = «должен» |
| Нет "internal but exported" mark — `@internal` recognized inconsistently | TypeStrong/typedoc #1843 | D105: `#hide_doc` — item exported (real export), но скрыт из docs. Bright line, не tag-soup. |
| AI consumption — must scrape HTML or wrangle unstable `--json` | community feedback | D107 + AI-first design: stable JSON schema v1, all signatures structural, ready для LLM tool use |

### 19.4. Общие проблемы всех трёх — где Nova **уникально** лучше

Эти жалобы общие для godoc + rustdoc + TypeDoc; Nova решает их за счёт
конструктов, которых **нет** в Go/Rust/TS в типах:

1. **Effect rows в signature.**
   - Go/Rust/TS: эффекты невидимы (`func f() {}` ничего не говорит про
     I/O, time, randomness).
   - Nova: `fn f() Net Db Fail[E] -> T` — DocFn.signature.effects[]
     рендерится как набор линкованных badge'ев. LLM понимает побочные
     эффекты функции из doc без чтения тела.

2. **Контракты как часть API.**
   - Go/Rust/TS: «pre/postconditions» — комментарии в прозе, никем
     не проверяемые.
   - Nova: `requires` / `ensures` / `invariant` — статически
     верифицируются (Plan 33.1), рендерятся отдельной секцией с
     verify_status (PROVEN / TIMEOUT / UNVERIFIED / TRUSTED).
     `must_verify` doc-test модификатор — Nova-уникальный.

3. **Capabilities в типах.**
   - Go/Rust/TS: `#[forbid_unsafe]` etc. — lint-config, не часть API.
   - Nova: `#forbid(Io)` / `#realtime nogc` / `#allow_transit(...)` —
     атрибуты item-level, видны в doc как capability badges. Real-time
     audit становится `nova doc --filter realtime`.

4. **Protocol implementors / Effect handlers — auto-listed.**
   - Rust: `impl Trait for Type` — частично работает, cross-crate
     ненадёжен.
   - Nova: structural protocols + workspace-scoped index → 100%
     reliable auto-listing implementors + handlers.

5. **Folder-modules с peer attribution.**
   - Go: один файл = один package; `internal/` для приватных — но
     прятать deeper.
   - Rust: `mod.rs` + tree — конфузит начинающих, peers неестественны.
   - TS: namespace/module/file — три способа сделать одно.
   - Nova: D29 rev-3 + Plan 42 — folder = module, peers explicit, doc
     показывает «defined in users.nv:42» с per-peer attribution.

6. **AI-first JSON schema с первого дня.**
   - Все три: JSON output либо отсутствует (godoc), либо nightly +
     unstable (rustdoc), либо schema-less (TypeDoc).
   - Nova: D107 — stable schema v1 на release builds, embedded JSON
     Schema, validated в Ф.19 corpus tests. LLM tool-call может
     consume output напрямую.

7. **Semantic API diff включая контракты и эффекты.**
   - `cargo public-api` (Rust third-party) — sig changes only.
   - Nova: Ф.13 — sig + effects + raises + contracts (tighten/loosen
     pre/post) + stability + deprecation. **Contract tightening =
     breaking change** detected automatically.

8. **Doc-tests с handler injection.**
   - Rust: `# use foo::*;` hidden lines.
   - Nova: `#doc(test_handlers = "std.testing.handlers")` module-level,
     + folder-module peer `_doctest_setup.nv` — clean abstraction,
     leveraging Plan 42.

9. **Reproducibility byte-identical как acceptance criterion.**
   - Все три: best-effort, регулярные регрессии.
   - Nova: Ф.19 включает reproducibility test (две прогона →
     diff = empty). CI gate в Ф.17.

10. **Stability tiers + deprecation timeline.**
    - Rust: `#[deprecated(since, note)]` — нет `until`.
    - Go/TS: free-text convention.
    - Nova: D105 + Ф.14 — `#deprecated(since, until, note)` +
      `--deny-overdue-deprecations` CI gate.

11. **`--since <version>` filter** для changelog generation.
    - Никто из трёх не имеет.
    - Nova: Ф.12 — built-in, использует `#since` attr из D105.

12. **`--check` как обязательный CI gate с полным spectrum lint'ов.**
    - Rust: `cargo doc` warnings — best-effort.
    - Go: pkg.go.dev hosted, нет local `--check`.
    - TS: TypeDoc errors mode частичный.
    - Nova: §8 таблица — 10 lint категорий, exit 1 на errors, full
      stdlib должен проходить.

---

## 20. Связанные планы

- [Plan 42 — folder-modules](42-folder-modules.md): был sub-plan 42.2,
  выделен 2026-05-14.
- [Plan 18 — stdlib roadmap](18-stdlib-roadmap.md): Ф.18 stdlib pass
  координируется с Plan 18 priorities.
- [Plan 33.1 — contracts core](33.1-contracts-core.md): contract
  rendering + `must_verify` doc-test modifier.
- [Plan 36 — CLI hardening](36-cli-production-hardening.md): exit
  codes + path conventions + JSON output.
- [Plan 28 — nova CLI](28-nova-cli.md): infrastructure для subcommand.
- [Plan 03 — package ecosystem](03-package-ecosystem-roadmap.md): future
  versioned doc hosting.
