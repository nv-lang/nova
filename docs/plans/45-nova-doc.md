# Plan 45: `nova doc` — production-grade documentation tooling

> **Создан 2026-05-14**, переписан с чистого листа 2026-05-14
> (production-rewrite после первой draft-версии).
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

Plan 45 вводит **четыре** spec-decisions (новые D-номера выбираем при
ревью; placeholder D100-D103):

### D100. Doc-comment syntax: `///` outer, `//!` inner

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

### D101. Doc attributes: `#deprecated`, `#since`, `#stable`, `#unstable`, `#experimental`, `#hide_doc`, `#doc_alias`, `#doc(inline|no_inline)`

Все используют D96 синтаксис (`#name` / `#name(args)`).

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

### D102. Doc-test semantics

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

### D103. JSON output schema v1

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
   ↓ (lexer: D100 // /// //!)
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
            pub requires: Vec<MarkdownExpr>,
            pub ensures:  Vec<MarkdownExpr>,
            pub reads:    Vec<Expr>,
            pub modifies: Vec<Expr>,
            pub decreases: Option<Expr>,
            pub invariants: Vec<MarkdownExpr>,
            pub verify_status: VerifyStatus, // Proven | Timeout(ms) |
                                             // Unverified | Trusted |
                                             // RuntimeFallback
        },
    },
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
    pub invariant: Option<MarkdownExpr>, // record-level invariant (33.2)
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

## 9. Doc-tests pipeline (D102)

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

### Ф.0 — Spec (D100-D103)

- Написать 4 D-decisions в `spec/decisions/03-syntax.md` (D100, D102)
  и `09-tooling.md` (D101, D103).
- В overview.md / revolutionary.md — упомянуть `nova doc` как
  shipping, не roadmap.
- Acceptance: spec reviewed, D-numbers assigned, history/evolution.md
  обновлён.
- Размер: ~600-900 LOC spec.

### Ф.1 — Lexer (D100)

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

### Ф.3 — Doc attributes (D101)

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

### Ф.9 — JSON renderer + schema (D103)

- `doc/render_json.rs` — serde-derived, stable field order
  (alphabetical внутри struct, deterministic).
- `doc/schema.rs` — embedded JSON Schema v1, emit'tится через
  `--json-schema`.
- Validator (опционально): nova doc validates own output (sanity).
- Tests: schema validates corpus output; round-trip
  (DocModel → JSON → parse → equal).
- Размер: ~400-600 LOC + schema файл ~500-1000 lines.

### Ф.10 — HTML renderer + search index

- `doc/render_html.rs` + embedded CSS/JS в `assets/doc-html/`.
- Theme via CSS variables + `prefers-color-scheme`.
- Search index emit alongside.
- Reproducible (SOURCE_DATE_EPOCH).
- Tests: smoke (HTML парсится валидным parser'ом), search index JSON
  валиден, no broken anchors.
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

### Ф.16 — Incremental cache (post-MVP, optional)

- On-disk cache (blake3-hashed DocModel per module).
- Invalidate: file mtime + content hash.
- `nova doc --no-cache` для CI bisect.
- **Note:** не блокер для прода. Прод-CI чаще запускает clean.
  Phase отдельно после fields'ового feedback.
- Размер: ~300-500 LOC.

### Ф.17 — CI integration

- `nova-cli` exit codes match Plan 36.
- GitHub Actions example workflow (`docs/ci/nova-doc-check.yml`).
- pre-commit hook example.
- Размер: ~50 LOC config + docs.

### Ф.18 — Stdlib pass

- Написать doc-comments для всех export items в `std/*`.
- Doc-tests для критических функций.
- `nova doc --workspace --check --doc-tests` обязан зелёным.
- Multi-session (стdlib ~50 модулей, по 5-10 за session).
- Acceptance gate для closing Plan 45.
- Размер: ~3000-5000 LOC doc-comments + tests.

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

- [ ] D100-D103 written, reviewed, merged в spec.
- [ ] `nova doc <module>` produces markdown для single-file + folder
      module.
- [ ] `nova doc --format json` validates against embedded schema v1.
- [ ] `nova doc --format html -o site/` produces standalone site
      (search работает, theme switches).
- [ ] `nova doc --check` exit 1 на missing doc / broken link /
      failing doc-test.
- [ ] `nova doc --diff baseline.json` detects all categories из §11.
- [ ] `nova doc --workspace --check --doc-tests` зелёный на полном
      `std/`.
- [ ] All export items в `std/` имеют doc-comments.
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
| Ф.16 cache (opt) | 300-500 | 100 |
| Ф.17 CI | 50 | 100 |
| Ф.18 stdlib pass | — | 3000-5000 |
| Ф.19 tests | — | 800-1200 |
| Ф.20 docs | — | 600-1000 |
| **Total** | **~6000-9000** | **~7000-11000** |

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
- D100-D103 (new) — Plan 45 spec deltas.

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
   в D102.

---

## 19. Связанные планы

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
