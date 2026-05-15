//! Типы AST.
//!
//! Минималистичный набор: всё что нужно для bootstrap'а Nova-on-Nova.
//! Plan 33.1: добавлены контракты (`Contract`, `VerifyMode`, `Purity`) —
//! AST-узлы готовы; парсер/typecheck/SMT расширения — в последующих
//! фазах [Plan 33.1](../../../docs/plans/33.1-contracts-core.md).

use crate::diag::Span;

/// Корневой узел — модуль (файл).
#[derive(Debug, Clone)]
pub struct Module {
    pub name: Vec<String>, // module a.b.c → ["a", "b", "c"]
    pub imports: Vec<Import>,
    pub items: Vec<Item>,
    /// Plan 42 Sub-plan 42.A: module-level attributes (`#forbid X, Y`).
    /// `#requires` отвергнут — нарушает AI-first explicit principle.
    pub attrs: Vec<ModuleAttr>,
    pub span: Span,
    /// Plan 42 Sub-plan 42.4 (шаг 1, 2026-05-14): per-peer attribution.
    ///
    /// Для **single-file** module — `peer_files = [PeerFile { ... }]`
    /// (1 elem) со всеми imports/items entry-файла.
    ///
    /// Для **folder-module** — N elem'ов, по одному на peer-файл
    /// (alphabetical order, как `module.items`). Каждый peer хранит свои
    /// `imports` (per-peer scope по правилу C) и `items_here` (items,
    /// объявленные именно в этом peer-файле).
    ///
    /// `Module.imports` и `Module.items` остаются flat для backward compat
    /// (codegen/interp/verify их читают как раньше). 42.4 шаги 2-3 учат
    /// type-checker использовать `peer_files` для per-peer name resolution.
    ///
    /// Имя `peer_files` (а не `files`) — чтобы не конфликтовать со
    /// смежным `diag::SourceFile` и явно отражать терминологию Plan 42 spec.
    pub peer_files: Vec<PeerFile>,
    /// Plan 45 / D104: inner doc-comment модуля (`//!`), если присутствует.
    /// Допустим только в начале файла (после `module X` и `import`-ов,
    /// до первого item'а). Объединяется с `#doc "..."` module-attr
    /// (D101) на этапе collector'а (Plan 45 Ф.4).
    pub doc: Option<DocBlock>,
}

/// Plan 42 Sub-plan 42.4 (шаг 1, 2026-05-14): per-peer source attribution.
///
/// Сохраняет imports каждого peer-файла отдельно, не merge'нутые в
/// flat `Module.imports`/`Module.items`. Используется type-checker'ом
/// для enforce'а правила C (peers share declarations namespace, но
/// **не imports**).
///
/// Для single-file module — один PeerFile со всеми entry-данными.
/// Для folder-module — N PeerFile, по одному на peer-файл.
///
/// `file_id` — будет заполнен в шаге 2 (FileRegistry activation +
/// span walker). Сейчас (шаг 1) остаётся `MAIN_FILE_ID` placeholder.
#[derive(Debug, Clone)]
pub struct PeerFile {
    /// Канонический путь к файлу (для diagnostics + identity).
    pub path: std::path::PathBuf,
    /// FileId — назначается в шаге 2. В шаге 1 = `MAIN_FILE_ID`.
    pub file_id: crate::diag::FileId,
    /// `import` statements именно этого peer-файла (per-peer scope).
    pub imports: Vec<Import>,
    /// Items, объявленные **здесь** (не pulled via imports' transitive
    /// resolve). Type-checker использует для построения per-peer
    /// declarations namespace (shared между peers одного module).
    pub items_here: Vec<Item>,
    /// Plan 42.15: имена items, ставших видимыми в этом peer'е через
    /// его **прямые** `import` statements (после rename + selective
    /// filter). Транзитивные imports (import чужого import'а) сюда НЕ
    /// попадают — это и есть Rule C strict isolation.
    ///
    /// NameResCtx использует это + items_here peers ЭТОГО module
    /// (shared decls) для построения per-peer visible scope.
    pub imported_item_names: std::collections::HashSet<String>,
    /// Plan 42.15: true — peer принадлежит **компилируемому** module
    /// (entry + его folder-module peers). false — peer импортированного
    /// модуля. NameResCtx собирает `shared_decls` ТОЛЬКО из
    /// `is_entry_module = true` peers — иначе items импортированных
    /// folder-modules «протекли» бы в shared namespace (нарушение Rule C).
    pub is_entry_module: bool,
}

/// Plan 45 / D104: блок doc-comment'ов, привязанный к item'у или
/// модулю. Это последовательность одного или нескольких подряд
/// идущих `///` (outer) либо `//!` (inner) комментариев, склеенных
/// лексером (см. `lexer::TokenKind::DocComment`).
///
/// `kind` — указывает, outer это (привязан к следующей декларации) или
/// inner (привязан к окружающему модулю). `content` — сырой текст,
/// markdown НЕ парсится на уровне AST; парсинг markdown +
/// section-extraction — отдельный pass (`doc::passes::derive_sections`).
#[derive(Debug, Clone, PartialEq)]
pub struct DocBlock {
    pub kind: crate::lexer::DocCommentKind,
    pub content: String,
    pub span: Span,
}

/// Plan 42 Sub-plan 42.A: module-level attribute.
#[derive(Debug, Clone)]
pub struct ModuleAttr {
    pub kind: ModuleAttrKind,
    /// Для `#forbid` — список эффектов. Для `#cfg` — пусто (predicate в kind).
    pub effects: Vec<String>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModuleAttrKind {
    /// `#forbid X, Y` — все functions module не могут использовать
    /// эти effects (compile error через capability check).
    Forbid,
    /// Plan 42.12 Ф.2: `#cfg(feature = "X")` или `#cfg(target_os = "Y")` —
    /// module/peer активен только при matching condition.
    /// Поскольку filename suffix (Ф.1) покрывает 90% target_os кейсов,
    /// `#cfg(target_os)` рекомендуется только для item-level (Ф.3).
    Cfg(CfgPredicate),
    /// Plan 42.11: `#doc "..."` — module-level documentation line.
    /// Multi-peer merging: все `#doc` строки из всех peers concat'аются
    /// в alphabetical filename order. AI-first signal: LLM видит
    /// module purpose прямо в peer-файле без CLI invoke.
    /// Consumer: Plan 45 (nova doc).
    Doc(String),
}

/// Plan 42.12 Ф.2 + Plan 42.14 Ф.1: cfg predicate.
/// Plan 42.14: добавлены `any/all/not` композиции (Rust-style).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CfgPredicate {
    /// `#cfg(feature = "X")` — active если feature `X` в `nova.toml [features]`
    /// AND enabled через `--features` CLI flag.
    Feature(String),
    /// `#cfg(target_os = "Y")` — active если current target matches.
    TargetOs(String),
    /// Plan 42.14 Ф.1: `#cfg(any(P1, P2, ...))` — active если хоть один.
    Any(Vec<CfgPredicate>),
    /// Plan 42.14 Ф.1: `#cfg(all(P1, P2, ...))` — active если все.
    All(Vec<CfgPredicate>),
    /// Plan 42.14 Ф.1: `#cfg(not(P))` — active если P inactive.
    Not(Box<CfgPredicate>),
}

/// Plan 35 sub-plan 35.A (R26): селективный import — `import X.Y.{A, B as C}`.
#[derive(Debug, Clone)]
pub struct ImportItem {
    pub name: String,
    pub alias: Option<String>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct Import {
    pub path: Vec<String>,
    /// Plan 35 sub-plan 35.A (R26): селективный список `{A, B as C}`.
    /// `None` = весь модуль (legacy). `Some([])` — невалидно, парсер не
    /// эмитит. `Some([item, ...])` — селективный набор: видимы только
    /// перечисленные имена.
    pub items: Option<Vec<ImportItem>>,
    pub alias: Option<String>,
    /// Plan 35 sub-plan 35.A (R26): `export import X.{A}` — re-export.
    /// При false — обычный import.
    pub is_export: bool,
    pub span: Span,
}

/// Top-level декларация в модуле.
#[derive(Debug, Clone)]
pub enum Item {
    Fn(FnDecl),
    Type(TypeDecl),
    Let(LetDecl),
    Const(ConstDecl),
    Test(TestDecl),
    /// Plan 33.5 Ф.4.1: `lemma` — proven proof term.
    /// Тело SMT-верифицируется (unknown == fail), не emit'ится в runtime.
    /// Может применяться через `apply lemma_name(args)` в телах функций.
    Lemma(LemmaDecl),
}

/// Plan 33.5 Ф.4.1: декларация lemma.
///
/// `lemma name(params) requires P ensures Q { proof_body }`
///
/// - Тело верифицируется SMT (`verify_mode == MustVerify` по умолчанию).
/// - Не emit'ится в C (ghost, только для proof).
/// - `apply name(args)` в теле fn добавляет `ensures[args/params]` как
///   assertion в SMT-scope вызывающей fn (подстановкой аргументов).
#[derive(Debug, Clone)]
pub struct LemmaDecl {
    pub name: String,
    pub generics: Vec<GenericParam>,
    pub params: Vec<Param>,
    pub contracts: Vec<Contract>,
    pub body: FnBody,
    pub span: Span,
}

/// Plan 45 Ф.3 / D105: doc-атрибуты (`#deprecated(...)`, `#since(...)`,
/// `#stable`, `#unstable(feature=...)`, `#experimental(note=...)`,
/// `#hide_doc`, `#doc_alias("a","b")`, `#doc(summary=...)`,
/// `#doc(inline)` / `#doc(no_inline)`, `#doc(section="Name")`).
#[derive(Debug, Clone)]
pub enum DocAttr {
    /// `#deprecated(since = "X", note = "...", until = "Y"?)`.
    Deprecated {
        since: Option<String>,
        note: Option<String>,
        until: Option<String>,
    },
    /// `#since("X.Y")` или `#since(version = "X.Y")`.
    Since(String),
    /// `#stable` или `#stable(since = "X.Y")`.
    Stable { since: Option<String> },
    /// `#unstable(feature = "name")`.
    Unstable { feature: Option<String> },
    /// `#experimental(note = "...")`.
    Experimental { note: Option<String> },
    /// `#hide_doc` — exported, но скрыт из nova doc output.
    HideDoc,
    /// `#doc_alias("name", "name", ...)` — alternative names для search.
    DocAlias(Vec<String>),
    /// `#doc(inline)` — re-export рендерится inline (default same-package).
    DocInline,
    /// `#doc(no_inline)` — re-export рендерится только ссылкой.
    DocNoInline,
    /// `#doc(summary = "...")` — override первого-предложения summary.
    DocSummary(String),
    /// `#doc(section = "Name")` — custom section grouping.
    DocSection(String),
    /// `#doc(test_handlers = "path.to.handlers")`.
    DocTestHandlers(String),
}

/// Функция: и свободная, и метод (через `receiver`).
#[derive(Debug, Clone)]
pub struct FnDecl {
    /// Plan 45 / D104: doc-comment, прикреплённый парсером (один или
    /// несколько подряд идущих `///` непосредственно перед `fn`).
    /// `None` — у функции нет doc-comment'а.
    pub doc: Option<DocBlock>,
    /// Plan 45 Ф.3 / D105: doc-атрибуты, собранные парсером.
    pub doc_attrs: Vec<DocAttr>,
    pub is_export: bool,
    /// D82: external fn — реализована в nova_rt/*.h. Body отсутствует
    /// (FnBody::External). Только в std.runtime.* whitelisted.
    pub is_external: bool,
    pub name: String,
    /// Receiver — для методов через `@`. None у свободных функций.
    pub receiver: Option<Receiver>,
    /// Plan 15 (D72): `[T]` или `[T Hashable]` — имя + optional bound.
    pub generics: Vec<GenericParam>,
    pub params: Vec<Param>,
    pub effects: Vec<TypeRef>, // эффекты между `)` и `->`
    pub return_type: Option<TypeRef>,
    pub body: FnBody,
    pub span: Span,
    /// Plan 16 (D64 sugar §3697): `@realtime` атрибут перед `fn`.
    /// Эквивалентен оборачиванию body в `realtime { ... }`. Type-checker
    /// (CapabilityCtx) применяет realtime-проверки ко всему телу fn.
    pub realtime_attr: RealtimeAttr,
    /// Plan 33.1 (D24): контракты после сигнатуры, до тела.
    /// Пустой вектор у функций без контрактов (backward-compat).
    pub contracts: Vec<Contract>,
    /// Plan 33.2 (D24): `reads <expr>{, <expr>}*` — frame-read targets.
    /// Перечисляет l-values которые функция читает (handler-state + record
    /// fields). Используется SMT для frame-axiom («всё что вне reads
    /// не повлияет на ensures»).
    pub reads: Vec<FrameTarget>,
    /// Plan 33.2 (D24): `modifies <expr>{, <expr>}*` — frame-write targets.
    /// Перечисляет l-values которые функция МОЖЕТ изменить.
    /// Type-check проверяет body: assignments вне frame → error.
    /// SMT использует frame-axiom: всё вне modifies = old.
    pub modifies: Vec<FrameTarget>,
    /// Plan 33.2 Ф.7 (D24): `decreases <expr>` — termination measure
    /// для recursive функций. Well-founded measure; SMT-verify check'ает
    /// что для каждого рекурсивного вызова `f(args')` в теле `f(args)`:
    /// `decreases(args') < decreases(args)`.
    /// Без decreases для recursive fn — warning (ensures could be vacuously
    /// satisfied if fn diverges). В trivial-backend 33.2 — parsed, not enforced.
    pub decreases: Option<Expr>,
    /// Plan 33.1 (D24): режим верификации контрактов.
    /// `@must_verify` / `@unverified` / default.
    pub verify_mode: VerifyMode,
    /// Plan 33.1 (D24 §35): `@verify_timeout(ms)` — локальный override
    /// для SMT-таймаута. None = глобальный default (2000ms).
    pub verify_timeout_ms: Option<u32>,
    /// Plan 33.1 (D24): `@pure` — assertion что функция чистая.
    /// Реальная purity выводится в Ф.2 через SCC по call-graph;
    /// если выведенная не соответствует объявленной — compile error.
    pub purity: Purity,
}

/// Plan 33.1 (D24): один контракт-clause функции.
#[derive(Debug, Clone)]
pub struct Contract {
    pub kind: ContractKind,
    pub expr: Expr,
    pub span: Span,
}

/// Plan 33.2 (D24): frame-target — l-value, который функция читает
/// (`reads`) или пишет (`modifies`).
///
/// Поддерживаемые формы:
/// - `name` — целая variable / receiver (`self` / `acc`).
/// - `name.field` — отдельное поле record'а.
/// - `arr[i]` — конкретный array-элемент (33.2 partial).
/// - `arr[*]` — все элементы array.
#[derive(Debug, Clone)]
pub enum FrameTarget {
    /// Whole l-value: `acc`, `self`.
    Whole(Expr),
    /// Field of receiver: `acc.balance`, `self.field`.
    Field { receiver: Expr, field: String, span: Span },
    /// Specific array element: `xs[i]`.
    ArrayElem { array: Expr, index: Expr, span: Span },
    /// All elements: `xs[*]`.
    ArrayAll { array: Expr, span: Span },
}

impl FrameTarget {
    pub fn span(&self) -> Span {
        match self {
            FrameTarget::Whole(e) => e.span,
            FrameTarget::Field { span, .. }
            | FrameTarget::ArrayElem { span, .. }
            | FrameTarget::ArrayAll { span, .. } => *span,
        }
    }
}

/// Plan 33.1: вид контракта (`requires` vs `ensures`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContractKind {
    /// `requires <bool-expr>` — предусловие. Проверяется на входе.
    /// `result`/`old(...)` запрещены.
    Requires,
    /// `ensures <bool-expr>` — постусловие. Проверяется на выходе.
    /// Доступны `result`, `result.is_ok`/`.is_err`/`.value`/`.error`,
    /// `old(expr)` для значений до вызова.
    Ensures,
    /// D.1.5: `ensures_fail <bool-expr>` — постусловие для Fail-пути.
    /// `result` недоступен (fn не вернула нормально); `old(x)` доступен.
    /// SMT-верифицируется независимо; runtime-check не эмитируется (V1).
    EnsuresFail,
}

/// Plan 33.1 (D24 §49): режим верификации контрактов функции.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum VerifyMode {
    /// Без атрибута. SMT пытается доказать; недоказанное в release —
    /// compile error (R20), пока программист не пометит явно
    /// `@unverified` или не добавит hint'ов.
    #[default]
    Default,
    /// `@must_verify` — SMT обязан доказать. Unknown → compile error
    /// даже в debug.
    MustVerify,
    /// `@unverified` — отказ от SMT-доказательства заранее. В debug —
    /// runtime check; в release — контракт стирается (no-op).
    Unverified,
}

/// Plan 33.1: чистота функции (для использования в контрактах
/// через composition в 33.2 + ghost в 33.3).
///
/// Выводится в Ф.2 через SCC по call-graph (как `const fn` в Rust).
/// Атрибут `@pure` — assertion программиста; расхождение с выведенным —
/// compile error.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Purity {
    /// Чистая функция: нет effects в сигнатуре, все вызываемые —
    /// тоже pure. Можно использовать в контрактах других функций
    /// (composition — Plan 33.2).
    Pure,
    /// Effectful. Использование в контрактах запрещено.
    Effectful,
    /// Не определено (до запуска Ф.2 inference). Парсер выставляет
    /// это значение если `@pure` не указан явно.
    #[default]
    Unknown,
}

/// Plan 16: вид `@realtime` атрибута на функции (D64 §3697).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RealtimeAttr {
    /// Без атрибута.
    None,
    /// `@realtime` — body обёрнут в `realtime { ... }` контекст.
    Realtime,
    /// `@realtime nogc` — body обёрнут в `realtime nogc { ... }`.
    RealtimeNogc,
}

/// Receiver метода.
///
/// `fn TypeName @method() ...` — instance-метод (`@` доступ к receiver'у).
/// `fn TypeName.static_method() ...` — static-метод (точка).
#[derive(Debug, Clone)]
pub struct Receiver {
    pub type_name: String,
    pub generics: Vec<TypeRef>,    // Repo[T] — generics типа
    pub kind: ReceiverKind,
    pub mutable: bool,             // `fn Type mut @method`
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ReceiverKind {
    Instance, // @
    Static,   // .
}

#[derive(Debug, Clone)]
pub struct Param {
    pub name: String,
    pub ty: TypeRef,
    pub span: Span,
    /// Plan 14 Ф.6 (D69): `...name Type` — variadic-параметр. Только
    /// последний param может быть variadic; тип обязан быть `[]T`.
    /// Caller'ы могут передать N args (которые collected'ятся в []T)
    /// или `...arr` (spread в variadic position).
    pub is_variadic: bool,
    /// Plan 46 (D102): значение по умолчанию `fn f(x int = expr)`.
    /// Default-выражение вычисляется **на месте вызова** (не def-time),
    /// может ссылаться на предшествующие параметры и module-level const.
    /// Параметры с дефолтом идут строго после параметров без дефолта;
    /// variadic-параметр НЕ может иметь дефолт. `None` — обязательный.
    pub default: Option<Expr>,
}

/// Plan 15 (D72): generic-параметр с optional bound.
///
/// `[T]` — `GenericParam { name: "T", bound: None }`.
/// `[T Hashable]` — `GenericParam { name: "T", bound: Some(Hashable_TypeRef) }`.
///
/// Bound — это protocol-тип ([D72](spec/decisions/02-types.md#d72)).
/// Запрещены forward-references: имя в bound должно быть объявлено
/// раньше в том же `[...]` или в окружающем type-context.
#[derive(Debug, Clone)]
pub struct GenericParam {
    pub name: String,
    pub bound: Option<TypeRef>,
    /// Plan 19 C10 (D88): default-значение generic-параметра.
    /// Используется когда вызывающий код не указал T явно и
    /// inference из аргументов не дал результат. Для `[T = f64]`
    /// или `[T Bound = Default]`.
    pub default: Option<TypeRef>,
    pub span: Span,
}

impl GenericParam {
    /// Helper для legacy кода: если bound не нужен.
    pub fn unbounded(name: String, span: Span) -> Self {
        Self { name, bound: None, default: None, span }
    }
}

#[derive(Debug, Clone)]
pub enum FnBody {
    /// `=> expr`
    Expr(Expr),
    /// `{ stmts; ...; expr? }`
    Block(Block),
    /// D82: `external fn` — body отсутствует, реализация в nova_rt.
    External,
}

#[derive(Debug, Clone)]
pub struct TypeDecl {
    /// Plan 45 / D104: doc-comment перед `type`.
    pub doc: Option<DocBlock>,
    /// Plan 45 Ф.3 / D105: doc-атрибуты.
    pub doc_attrs: Vec<DocAttr>,
    pub is_export: bool,
    pub name: String,
    /// Plan 15 (D72): `[K Hashable, V]` — имена + optional bounds.
    pub generics: Vec<GenericParam>,
    pub kind: TypeDeclKind,
    pub span: Span,
    /// Plan 33.2 Ф.7 (D24): `invariant <expr>` clauses на record-типах.
    /// Проверяются runtime (debug): после record-литерала, после mut field
    /// assignments, на выходе mut-методов. SMT-verify в 33.3.
    /// Пустой вектор у типов без invariants (backward-compat).
    pub invariants: Vec<Contract>,
    /// Plan 33.3 Ф.9 (D24): `axiom <name>(binders) => <formula>` —
    /// global formulas про `pure_view` ops эффекта. Применимо только к
    /// `TypeDeclKind::Effect`; на других типах пустой Vec.
    /// При SMT verify добавляются как глобальные `assert` в любом
    /// scope где эффект импортирован.
    pub axioms: Vec<EffectAxiom>,
}

#[derive(Debug, Clone)]
pub enum TypeDeclKind {
    /// `type Name { fields }`
    Record(Vec<RecordField>),
    /// `type Name | A | B(int) | C { x int }` (D52)
    Sum(Vec<SumVariant>),
    /// `type Name effect { signatures }` — capability с runtime
    /// vtable + handler-dispatch. Использование в позиции эффекта
    /// `(...) Eff -> ...` или через handler stack.
    Effect(Vec<EffectMethod>),
    /// `type Name protocol { signatures }` — структурный контракт
    /// (D53). Compile-time проверка satisfaction; нет runtime vtable.
    /// Используется как:
    ///   - bound в generic params `[T Protocol]` (D72);
    ///   - тип значения (existential), `fn f(x Hashable) -> ...`.
    Protocol(Vec<EffectMethod>),
    /// `type NewType u64` — newtype (D52)
    Newtype(TypeRef),
    /// `type Name alias OtherType` (D52)
    Alias(TypeRef),
}

#[derive(Debug, Clone)]
pub struct RecordField {
    pub name: String,
    pub ty: TypeRef,
    pub readonly: bool,
    pub mutable: bool,
    /// D39 / Plan 11 Ф.9: `use name Type` (named embed) или
    /// `use _ Type` (anonymous embed). Auto-proxy методы генерируются
    /// codegen'ом; override-precedence: own > delegated.
    /// Для anonymous embed `name` устанавливается в синтетический
    /// `__embed_<TypeName>` (доступ только через auto-proxy, не через
    /// `@<name>.method`).
    pub is_embed: bool,
    /// Если true — embed был объявлен как `use _ Type` (без alias);
    /// `name` — синтетический. Используется для multi-anonymous detection.
    pub embed_anonymous: bool,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct SumVariant {
    pub name: String,
    pub kind: SumVariantKind,
    pub discriminant: Option<i64>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub enum SumVariantKind {
    Unit,
    /// `Some(T)` — позиционный variant с одним полем
    Tuple(Vec<TypeRef>),
    /// `Cons { head T, tail List[T] }` — record-variant
    Record(Vec<RecordField>),
}

#[derive(Debug, Clone)]
pub struct EffectMethod {
    pub name: String,
    /// Plan 15 (D72): generic-параметры на effect/protocol method.
    pub generics: Vec<GenericParam>,
    pub params: Vec<Param>,
    pub effects: Vec<TypeRef>,
    pub return_type: Option<TypeRef>,
    pub span: Span,
    /// Plan 33.3 Ф.9 (D24): kind операции — Operation (default,
    /// действие через handler) vs PureView (наблюдение состояния).
    /// PureView-методы:
    ///   - ВНУТРИ effect-блока объявляются как `pure_view name(args) -> R`.
    ///   - Не имеют side-effects, не вызываются как handler-actions.
    ///   - Используются в контрактах функций (если эффект в сигнатуре).
    ///   - SMT кодируются как uninterpreted functions (UF).
    ///   - Backward-compat: для protocol-методов всегда Operation.
    pub kind: EffectOpKind,
    /// Plan 33.5 Ф.5.1: контракты метода эффекта (requires/ensures).
    /// Используются в Ф.5.2 для верификации Liskov-подобия handler'ов.
    /// Пустые = нет spec-контрактов (handler принимается без проверки).
    pub contracts: Vec<Contract>,
}

/// Plan 33.3 Ф.9 (D24): Operation vs PureView для effect-метода.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EffectOpKind {
    /// Обычная operation — side-effecting, dispatch через handler.
    Operation,
    /// `pure_view` — read-only observation эффект-state'а. Не вызывается
    /// напрямую; используется в контрактах. SMT-side — uninterpreted
    /// function; semantics задаётся через `axiom <expr>`.
    PureView,
}

/// Plan 33.3 Ф.9 (D24): глобальная формула про pure_view-ops эффекта.
///
/// Пример:
/// ```nova
/// type Db effect {
///     SetBalance(id AccountId, x money)
///     pure_view balance(id AccountId) -> money
///     axiom non_negative(id) => balance(id) >= 0
///     axiom after_set(id, x) =>
///         post(SetBalance(id, x))(balance(id)) == x
/// }
/// ```
///
/// Plan 33.4 P1-5: вид binder'а в EffectAxiom.
/// Различает три семантически разных состояния вместо Option<TypeRef>:
/// - Untyped: `axiom foo(id)` — тип не аннотирован, выводится из usage
/// - Typed: `axiom foo(id int)` — конкретный тип
/// - Generic: `axiom foo[T](id T)` — ссылка на generic param по имени
#[derive(Debug, Clone)]
pub enum BinderType {
    /// Тип не аннотирован — inference из usage.
    Untyped,
    /// Конкретный аннотированный тип: `int`, `str`, named type и т.п.
    Typed(TypeRef),
    /// Ссылка на generic-параметр аксиомы: `axiom foo[T](id T)` → Generic("T").
    Generic(String),
}

/// Plan 33.4 P1-5: параметр (binder) аксиомы с типом.
#[derive(Debug, Clone)]
pub struct BinderDef {
    pub name: String,
    pub kind: BinderType,
    pub span: Span,
}

impl BinderDef {
    /// Если binder типизирован (не generic и не untyped), вернуть TypeRef.
    pub fn typed_ref(&self) -> Option<&TypeRef> {
        match &self.kind {
            BinderType::Typed(t) => Some(t),
            _ => None,
        }
    }

    /// True если binder ссылается на generic-параметр аксиомы.
    pub fn is_generic(&self) -> bool {
        matches!(&self.kind, BinderType::Generic(_))
    }
}

/// `binders` — параметры формулы (свободные переменные).
/// `formula` обязана быть `bool`-выражением; видны binders + все
/// pure_view ops эффекта.
#[derive(Debug, Clone)]
pub struct EffectAxiom {
    pub name: String,
    /// Generic-параметры: `axiom foo[T](id T) => ...` → `generics = [T]`.
    /// V1: парсинг + AST; SMT encoding generic axioms — V2.
    pub generics: Vec<GenericParam>,
    /// Plan 33.4 P1-5: параметры формулы с типами через BinderDef.
    /// Заменяет Vec<(String, Option<TypeRef>)>.
    pub binders: Vec<BinderDef>,
    pub formula: Expr,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct LetDecl {
    pub mutable: bool,
    pub pattern: Pattern,
    pub ty: Option<TypeRef>,
    pub value: Expr,
    pub span: Span,
    /// Plan 33.3 (D24): `ghost let` / `ghost var` — spec-only binding.
    /// Видим в `requires`/`ensures`/`invariant` и других `ghost`-stmts,
    /// но НЕ emit'ится в codegen (паритет с Dafny).
    /// Backward-compat: default false.
    pub is_ghost: bool,
}

#[derive(Debug, Clone)]
pub struct ConstDecl {
    /// Plan 45 / D104: doc-comment перед `const`.
    pub doc: Option<DocBlock>,
    /// Plan 45 Ф.3 / D105: doc-атрибуты.
    pub doc_attrs: Vec<DocAttr>,
    pub is_export: bool,
    pub name: String,
    pub ty: Option<TypeRef>,
    pub value: Expr,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct TestDecl {
    pub name: String,
    pub body: Block,
    pub span: Span,
}

/// Ссылка на тип. Для bootstrap'а — упрощённая структура.
#[derive(Debug, Clone)]
pub enum TypeRef {
    /// Простое имя или путь: `int`, `User`, `module.User`
    Named {
        path: Vec<String>,
        generics: Vec<TypeRef>,
        span: Span,
    },
    /// `[]T`
    Array(Box<TypeRef>, Span),
    /// `[N]T` фиксированный массив
    FixedArray(usize, Box<TypeRef>, Span),
    /// `(A, B, C)` кортеж
    Tuple(Vec<TypeRef>, Span),
    /// `fn(A, B) E1 E2 -> R` — функциональный тип. Эффекты опциональны.
    Func {
        params: Vec<TypeRef>,
        effects: Vec<TypeRef>,
        return_type: Option<Box<TypeRef>>,
        span: Span,
    },
    /// `()` unit
    Unit(Span),
}

impl TypeRef {
    pub fn span(&self) -> Span {
        match self {
            TypeRef::Named { span, .. }
            | TypeRef::Array(_, span)
            | TypeRef::FixedArray(_, _, span)
            | TypeRef::Tuple(_, span)
            | TypeRef::Func { span, .. }
            | TypeRef::Unit(span) => *span,
        }
    }
}

/// Блок: список statement'ов + опциональное финальное выражение.
#[derive(Debug, Clone)]
pub struct Block {
    pub stmts: Vec<Stmt>,
    pub trailing: Option<Box<Expr>>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub enum Stmt {
    Let(LetDecl),
    Expr(Expr),
    Assign {
        target: Expr,
        op: AssignOp,
        value: Expr,
        span: Span,
    },
    Return {
        value: Option<Expr>,
        span: Span,
    },
    Break(Span),
    Continue(Span),
    Throw {
        value: Expr,
        span: Span,
    },
    /// D90: `defer body` — выполнить body при любом exit из enclosing
    /// scope (normal/return/throw/panic/interrupt). НЕ на exit(N, msg).
    Defer {
        body: Expr,
        span: Span,
    },
    /// D90: `errdefer body` — выполнить body только при exit через
    /// throw/panic. На normal/return/interrupt НЕ выполняется.
    ErrDefer {
        body: Expr,
        span: Span,
    },
    /// Plan 33.2 Ф.8 (D24): `assert_static <bool>` — intermediate proof
    /// obligation. SMT обязан доказать в release; debug — runtime check.
    /// Доказанные → стираются. Недоказанные → compile error (как
    /// `#must_verify`).
    AssertStatic {
        expr: Expr,
        span: Span,
    },
    /// Plan 33.3 (D24): `assume <bool>` — escape hatch для FFI / external
    /// knowledge. SMT принимает как axiom (без proof). В debug — runtime
    /// check; в release — стирается. Warning категории `trust-introduced`
    /// вне `#trusted` функции.
    Assume {
        expr: Expr,
        span: Span,
    },
    /// Plan 33.5 Ф.4.1: `apply lemma_name(args)` — активировать lemma.
    /// В SMT-scope: добавляет `ensures[args/params]` как assertion.
    /// Не emit'ится в runtime (ghost statement).
    Apply {
        lemma: String,
        args: Vec<Expr>,
        span: Span,
    },
    /// Plan 33.5 Ф.4.2: `calc { expr; == expr; == expr; }` — structured
    /// equational reasoning. Ghost statement: each step asserts adjacency
    /// relation in SMT; erased in codegen.
    Calc {
        steps: Vec<CalcStep>,
        span: Span,
    },
}

/// Один шаг calc-доказательства: отношение + выражение.
/// Первый шаг (expr1) не имеет отношения; остальные — `== expr`, `<= expr`, etc.
#[derive(Debug, Clone)]
pub struct CalcStep {
    /// None для первого шага, Some(rel) для последующих.
    pub rel: Option<CalcRel>,
    pub expr: Expr,
    pub span: Span,
}

/// Отношение между шагами calc.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CalcRel {
    Eq,   // ==
    Le,   // <=
    Lt,   // <
    Ge,   // >=
    Gt,   // >
}

impl CalcRel {
    pub fn to_smt_op(self) -> &'static str {
        match self {
            CalcRel::Eq => "=",
            CalcRel::Le => "<=",
            CalcRel::Lt => "<",
            CalcRel::Ge => ">=",
            CalcRel::Gt => ">",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AssignOp {
    Assign,
    Add,
    Sub,
    Mul,
    Div,
}

#[derive(Debug, Clone)]
pub struct Expr {
    pub kind: ExprKind,
    pub span: Span,
}

impl Expr {
    pub fn new(kind: ExprKind, span: Span) -> Self {
        Self { kind, span }
    }

    /// D38 turbofish: type_args — explicit hint для monomorphization, в bootstrap
    /// прозрачно. Возвращает inner base, разворачивая вложенные TurboFish; если
    /// expr — не TurboFish, возвращает себя.
    pub fn unwrap_turbofish(&self) -> &Expr {
        let mut cur = self;
        while let ExprKind::TurboFish { base, .. } = &cur.kind {
            cur = base.as_ref();
        }
        cur
    }
}

/// Часть `"... ${expr} ..."` interpolated-строки (D44, Plan 17 Ф.4).
#[derive(Debug, Clone)]
pub enum InterpStrPart {
    /// Буквальная часть строки (literal-сегмент).
    Lit(String),
    /// Подвыражение `${expr}` — будет вычислено и приведено к str.
    Expr(Box<Expr>),
}

#[derive(Debug, Clone)]
pub enum ExprKind {
    // Литералы
    IntLit(i64),
    FloatLit(f64),
    StrLit(String),
    /// `"hello ${name}, age=${n}"` — D44 string interpolation.
    /// Codegen эмитит StringBuilder-цепочку: одна аллокация
    /// + per-fragment `@append` (без O(N²) от `+`).
    InterpolatedStr { parts: Vec<InterpStrPart> },
    BoolLit(bool),
    UnitLit,
    /// Q-char-literals: 'a' / '\n' / '\u{...}' — Unicode codepoint as u32.
    /// Тип char в bootstrap эмитируется как nova_int.
    CharLit(u32),
    /// `arr` или `[1, 2, ...rest, 4]` — D60
    ArrayLit(Vec<ArrayElem>),
    /// `{ field: value, ...spread, name }` — D17/D52/D60
    RecordLit {
        type_name: Option<Vec<String>>, // Some(["User"]) для `User { ... }`
        fields: Vec<RecordLitField>,
    },
    /// `(a, b, c)` кортеж
    TupleLit(Vec<Expr>),

    // Имена и пути
    /// `name`
    Ident(String),
    /// `Module.name` или `Type.method`
    Path(Vec<String>),
    /// `@field` — поле или метод receiver'а
    SelfAccess,

    // Доступ
    /// `obj.field` или `obj.0` (positional)
    Member {
        obj: Box<Expr>,
        name: String,
    },
    /// `arr[index]`
    Index {
        obj: Box<Expr>,
        index: Box<Expr>,
    },
    /// `Type[T1, T2]` или `func[T]` — generic-application (turbofish, D38).
    /// Семантически `base` сохраняется как есть; `type_args` — explicit hints
    /// для monomorphization (в bootstrap-codegen monomorphization идёт по
    /// receiver/call-site, поэтому TurboFish прозрачно делегирует в `base`).
    /// Появляется только если за `]` идёт `.` / `(` / `?` (postfix-continuation),
    /// иначе `[` парсится как Index.
    TurboFish {
        base: Box<Expr>,
        type_args: Vec<TypeRef>,
    },

    // Вызовы
    Call {
        func: Box<Expr>,
        /// Plan 14 Ф.6 (D69): `Vec<CallArg>` где `CallArg::Item(Expr)`
        /// для обычного аргумента и `CallArg::Spread(Expr)` для `...e`.
        /// Spread разрешён только в variadic-position (codegen check).
        args: Vec<CallArg>,
        /// trailing-конструкция (D43-rev, Plan 19): либо `{ block }`
        /// (без params, DSL), либо `fn(p) body` (с params), либо
        /// legacy `{ x => body }` до миграции (C13 удалит legacy).
        trailing: Option<Trailing>,
    },
    /// `expr?` — пробрасывание Fail (D25/D65)
    Try(Box<Expr>),
    /// `expr!!` — throw-стиль для Result/Option (D85, Plan 19 C7).
    ///
    /// На `Some(v)` / `Ok(v)`: разворачивает в `v`.
    /// На `None` / `Err(e)`: бросает через `Fail[E]` (для Option —
    /// `RuntimeNoneError`). Внешняя fn должна иметь `Fail[E]` в
    /// effect-row, иначе compile error.
    Bang(Box<Expr>),
    /// `expr ?? default` — coalesce
    Coalesce(Box<Expr>, Box<Expr>),
    /// `expr as Type`
    As(Box<Expr>, TypeRef),
    /// `expr is Type` — runtime type check (D54)
    Is(Box<Expr>, TypeRef),

    // Бинарные / унарные
    Binary {
        op: BinOp,
        left: Box<Expr>,
        right: Box<Expr>,
    },
    Unary {
        op: UnOp,
        operand: Box<Expr>,
    },

    // Control flow
    If {
        cond: Box<Expr>,
        then: Block,
        else_: Option<ElseBranch>,
    },
    /// `if let pattern = expr { ... }` — D34
    IfLet {
        pattern: Pattern,
        scrutinee: Box<Expr>,
        then: Block,
        else_: Option<ElseBranch>,
    },
    Match {
        scrutinee: Box<Expr>,
        arms: Vec<MatchArm>,
    },
    For {
        pattern: Pattern,
        iter: Box<Expr>,
        body: Block,
        /// Plan 33.4 D.0.3: loop invariants (SMT + runtime).
        invariants: Vec<Expr>,
        /// Plan 33.4 D.0.3: well-founded termination measure.
        decreases: Option<Box<Expr>>,
    },
    /// `parallel for x in iter { body }` — D14, fan-out body for each element.
    /// Desugars to `supervised { for x in iter { spawn { body } } }`.
    ParallelFor {
        pattern: Pattern,
        iter: Box<Expr>,
        body: Block,
    },
    While {
        cond: Box<Expr>,
        body: Block,
        /// Plan 33.4 D.0.3: loop invariants (SMT + runtime).
        invariants: Vec<Expr>,
        /// Plan 33.4 D.0.3: well-founded termination measure.
        decreases: Option<Box<Expr>>,
    },
    /// `while let pattern = expr { ... }` — D34
    WhileLet {
        pattern: Pattern,
        scrutinee: Box<Expr>,
        body: Block,
        /// Plan 33.4 D.0.3: loop invariants.
        invariants: Vec<Expr>,
        /// Plan 33.4 D.0.3: well-founded termination measure.
        decreases: Option<Box<Expr>>,
    },
    Loop {
        body: Block,
        /// Plan 33.4 D.0.3: loop invariants (SMT + runtime).
        invariants: Vec<Expr>,
        /// Plan 33.4 D.0.3: well-founded termination measure.
        decreases: Option<Box<Expr>>,
    },
    /// `select { Some(v) = rx.recv() => body, _ => default }` --- D94
    Select {
        arms: Vec<SelectArm>,
    },

    // Функции и handlers
    /// `(a, b) => expr` — лямбда (D22, строго `=> expr`).
    ///
    /// **DEPRECATED — Plan 19** заменяет на [`ExprKind::ClosureLight`]
    /// (untyped `|x| body`) и [`ExprKind::ClosureFull`] (typed
    /// `fn(...) ...`). Старый узел остаётся для backward-compat
    /// до завершения миграции (C11–C13 в Plan 19 retro).
    Lambda {
        params: Vec<LambdaParam>,
        effects: Vec<TypeRef>,
        return_type: Option<TypeRef>,
        body: Box<Expr>,
    },
    /// closure-light — `|x| body` (D22-rev, Plan 19).
    ///
    /// Untyped lightweight closure. Параметры — только имена (типы
    /// выводятся из контекста использования: HOF-arg, annotated let,
    /// return-position, first-use inference). Тело — bare expression
    /// или block.
    ///
    /// Эффекты в сигнатуре **не пишутся** — наследуются из ambient
    /// effect-set parent fn'а + активных with-блоков.
    ///
    /// `||` для no-arg, `|_|` для wildcard (D59 расширение).
    ClosureLight {
        params: Vec<ClosureLightParam>,
        body: ClosureBody,
    },
    /// closure-full — `fn(x int) Effects -> R body` (D22-rev, Plan 19).
    ///
    /// Анонимная типизированная fn-форма. Идентична named fn без имени
    /// (по спецификации D22-rev). Используется когда нужны типы
    /// параметров, return-type или эффекты — то, чего не может
    /// closure-light.
    ///
    /// Тело — `=> expr` (FnBody::Expr) или `{ block }` (FnBody::Block).
    /// `external` запрещён (только для named fn).
    ///
    /// Содержимое в `Box<FnSigBody>` чтобы избежать infinite-size в
    /// recursive типе `ExprKind` (FnSigBody содержит FnBody который
    /// содержит Expr).
    ClosureFull(Box<FnSigBody>),
    /// `with X = handler { ... }` — D11
    With {
        bindings: Vec<WithBinding>,
        body: Block,
    },
    /// Handler-литерал: `EffectName { op(p) => ... ; ... }`
    HandlerLit {
        effect_name: Vec<String>,
        methods: Vec<HandlerMethod>,
    },
    /// `interrupt v` — досрочное завершение всего with-блока (D61).
    /// Значение становится результатом всего with-блока.
    Interrupt(Option<Box<Expr>>),
    /// `forbid X1, X2 { body }` — capability sandbox (D63).
    /// В bootstrap-интерпретаторе runtime барьер не реализован,
    /// блок исполняется как обычный block-expression. Compile-time
    /// проверка type checker'а — задача production-компилятора.
    Forbid {
        effects: Vec<TypeRef>,
        body: Block,
    },
    /// `realtime { body }` или `realtime nogc { body }` — гарантия
    /// не-приостановки (D64). В bootstrap нет fiber-runtime'а с
    /// safepoint'ами, блок исполняется как обычный block-expression.
    Realtime {
        nogc: bool,
        body: Block,
    },
    /// `range expr (a..b)` — D58 (генерируется как обычный вызов `Range.exclusive`)
    Range {
        start: Box<Expr>,
        end: Box<Expr>,
        inclusive: bool,
    },
    /// D.1.3: универсальный квантор в контрактах.
    /// `forall x in lo..hi : P(x)`
    Forall {
        var: String,
        range: Box<Expr>,
        body: Box<Expr>,
    },
    /// D.1.3: экзистенциальный квантор в контрактах.
    /// `exists x in lo..hi : P(x)`
    Exists {
        var: String,
        range: Box<Expr>,
        body: Box<Expr>,
    },
    /// Блок-выражение `{ stmts; expr }`
    Block(Block),
    /// `spawn body` — D50
    Spawn(Box<Expr>),
    /// `supervised { body }` / `supervised(cancel: tok) { body }` —
    /// structured-concurrency scope (D50). `cancel` — опциональный
    /// именованный аргумент `cancel:` с выражением типа `CancelToken`
    /// (D75 revised, Plan 47): внешний код может вызвать `tok.cancel()`
    /// чтобы fail-fast все fiber'ы scope'а.
    Supervised {
        body: Block,
        cancel: Option<Box<Expr>>,
    },
    /// `detach { body }` — fire-and-forget, global supervisor (D50).
    /// Requires `Detach` effect in the enclosing function's signature.
    Detach(Block),
    /// `throw expr` в позиции expression (D25/D65). Обрабатывается как
    /// эффект `Fail.fail(msg)`, тип `Never`. В codegen эмитируется как
    /// `(Nova_Fail_fail(msg), zero<T>)` — comma-expression, dummy после
    /// fail() недостижим.
    Throw(Box<Expr>),

    // Внутреннее: backtick-tagged template — для bootstrap'а: tag-функция
    // вызывается с (parts: []str, args: []SqlValue/...) — но в bootstrap
    // мы не делаем split на parts/args. Просто обозначаем как литерал.
    /// `tag\`literal\``
    TaggedTemplate {
        tag: Box<Expr>,
        parts: Vec<String>,
        args: Vec<Expr>,
    },
}

#[derive(Debug, Clone)]
pub enum ArrayElem {
    /// Обычный элемент.
    Item(Expr),
    /// `...expr` spread (D60)
    Spread(Expr),
}

/// Plan 14 Ф.6 (D69): аргумент вызова. Зеркально к `ArrayElem`.
/// `Spread` разрешён только в variadic-position на call-site
/// (codegen в emit_call валидирует).
/// Plan 46 (D102): `Named` — именованный аргумент `name: expr`.
#[derive(Debug, Clone)]
pub enum CallArg {
    /// Обычный позиционный аргумент.
    Item(Expr),
    /// `...expr` — spread в variadic-position.
    Spread(Expr),
    /// Plan 46 (D102): `name: expr` — именованный аргумент.
    /// `name` — имя параметра callee (не выражение). Именованные
    /// аргументы переставимы; позиционный после именованного — error.
    Named { name: String, value: Expr },
}

impl CallArg {
    /// Достать выражение независимо от kind'а.
    pub fn expr(&self) -> &Expr {
        match self {
            CallArg::Item(e) | CallArg::Spread(e) => e,
            CallArg::Named { value, .. } => value,
        }
    }

    pub fn is_spread(&self) -> bool {
        matches!(self, CallArg::Spread(_))
    }

    /// Plan 46: имя именованного аргумента, если это `Named`.
    pub fn arg_name(&self) -> Option<&str> {
        match self {
            CallArg::Named { name, .. } => Some(name.as_str()),
            _ => None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct RecordLitField {
    pub name: String,
    /// None — shorthand `{ name }` (D52 field punning)
    pub value: Option<Expr>,
    /// Spread `...expr` — D60. Если `is_spread = true`, то `name` = ""
    /// и `value = Some(expr)`.
    pub is_spread: bool,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct LambdaParam {
    pub name: String,
    pub ty: Option<TypeRef>,
    pub span: Span,
}

/// Параметр closure-light (Plan 19, D22-rev).
///
/// В отличие от [`LambdaParam`] и [`Param`] — **тип не пишется**.
/// Closure-light всегда untyped; типы выводятся из контекста.
/// Wildcard `_` разрешён как имя (D59 расширение): означает «параметр
/// требуется по арности, но не используется в теле».
#[derive(Debug, Clone)]
pub struct ClosureLightParam {
    /// Имя параметра. `"_"` — wildcard.
    pub name: String,
    pub span: Span,
}

/// Тело closure-light (Plan 19, D22-rev).
///
/// Closure-light не использует `=>` в синтаксисе — после `|...|` сразу
/// идёт либо expression, либо block. AST разделяет два случая для
/// прозрачной поддержки `return`/`break`/`continue` в block-форме.
#[derive(Debug, Clone)]
pub enum ClosureBody {
    /// `|x| x + 1` — bare expression
    Expr(Box<Expr>),
    /// `|x| { stmts; expr }` — block-форма
    Block(Block),
}

impl ClosureBody {
    pub fn span(&self) -> Span {
        match self {
            ClosureBody::Expr(e) => e.span,
            ClosureBody::Block(b) => b.span,
        }
    }
}

#[derive(Debug, Clone)]
pub struct WithBinding {
    pub effect: TypeRef,
    pub handler: Expr,
    pub span: Span,
    /// Plan 33.3 Ф.9.6: верификационный статус handler'а для этого
    /// эффекта. Required когда body использует функции с контрактами,
    /// ссылающимися на pure_view данного эффекта (gate в types/mod.rs).
    /// По умолчанию — `Unverified`.
    pub verification: HandlerVerification,
}

/// Plan 33.3 Ф.9.6 (D24): верификация handler'а для конкретного эффекта.
///
/// - `Unverified` — default. Handler не проверен и не trusted; использовать
///   нельзя если body вызывает fn с pure_view-contract'ами этого эффекта.
/// - `Verify` — `#verify_handler`. Symbolic verification handler.action
///   body против axiom'ов эффекта (Ф.9.7). До Ф.9.7 — treated как
///   placeholder (Ф.9.6 принимает синтаксис, но не верифицирует).
/// - `Trusted` — `#trusted_handler`. Программист берёт ответственность,
///   что handler корректно реализует axioms эффекта. Без verification.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HandlerVerification {
    Unverified,
    Verify,
    Trusted,
}

#[derive(Debug, Clone)]
pub struct HandlerMethod {
    pub name: String,
    pub params: Vec<HandlerMethodParam>,
    pub body: HandlerMethodBody,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct HandlerMethodParam {
    pub name: String,
    pub ty: Option<TypeRef>, // обычно None — выводится из effect-сигнатуры (Q-handler-method-param-inference)
    pub span: Span,
}

#[derive(Debug, Clone)]
pub enum HandlerMethodBody {
    /// `op(p) => expr`
    Expr(Expr),
    /// `op(p) { stmts }` (без `=>`)
    Block(Block),
}

#[derive(Debug, Clone)]
pub struct TrailingBlock {
    pub params: Vec<LambdaParam>, // [] если без params
    pub body: Block,
    pub span: Span,
}

/// Trailing-конструкция при вызове функции (Plan 19, D43-rev).
///
/// После `f(args)` может идти:
/// - `{ block }` — без параметров (DSL-форма: `with_timeout`,
///   `retry`, `transaction`). См. [`Trailing::Block`].
/// - `fn(p) body` — с параметрами, идентично closure-full без
///   имени. См. [`Trailing::Fn`].
///
/// Старая форма `f(args) { x => body }` (с параметрами через `=>`
/// внутри `{...}`) после Plan 19 **отменена**. Во время dual-mode
/// (C2–C12) parser продолжает поддерживать через
/// [`Trailing::LegacyBlockWithParams`]; после миграции (C11/C12)
/// и удаления (C13) этот вариант исчезает.
#[derive(Debug, Clone)]
pub enum Trailing {
    /// `f(args) { block }` — DSL без params (D43-rev).
    /// Boxed чтобы избежать infinite-size в recursive типе ExprKind.
    Block(Box<Block>),
    /// `f(args) fn(p) Effects? -> R? body` — trailing closure-full
    /// (D43-rev). Boxed по той же причине.
    Fn(Box<FnSigBody>),
    /// **DEPRECATED, dual-mode only.** Старая форма
    /// `f(args) { x => body }` или `{ (a, b) => body }`. Парсер
    /// сохраняет её для backward-compat до C12 (миграция) и C13
    /// (удаление варианта).
    LegacyBlockWithParams(Box<TrailingBlock>),
}

impl Trailing {
    pub fn span(&self) -> Span {
        match self {
            Trailing::Block(b) => b.span,
            Trailing::Fn(f) => f.span,
            Trailing::LegacyBlockWithParams(tb) => tb.span,
        }
    }
}

/// Сигнатура+тело анонимной fn — общий стержень для
/// [`ExprKind::ClosureFull`] и [`Trailing::Fn`] (Plan 19).
///
/// Поля повторяют [`FnDecl`] минус `is_export`/`is_external`/`name`/
/// `receiver`/`generics`/`realtime_attr`. Generics на closure-full
/// в bootstrap не поддерживаются (rank-2 polymorphism — Q-открытый);
/// если потребуется — расширить структуру отдельно.
#[derive(Debug, Clone)]
pub struct FnSigBody {
    pub params: Vec<Param>,
    pub effects: Vec<TypeRef>,
    pub return_type: Option<TypeRef>,
    pub body: FnBody,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub enum ElseBranch {
    /// `else { ... }`
    Block(Block),
    /// `else if ...` — рекурсивно следующий `if`
    If(Box<Expr>),
}

#[derive(Debug, Clone)]
pub struct MatchArm {
    pub pattern: Pattern,
    pub guard: Option<Expr>,
    pub body: MatchArmBody,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub enum MatchArmBody {
    /// `pattern => expr`
    Expr(Expr),
    /// `pattern => { block }` — единственное исключение из D40 (D19)
    Block(Block),
}

/// One arm of a `select` expression --- D94.
#[derive(Debug, Clone)]
pub struct SelectArm {
    pub op: SelectOp,
    pub guard: Option<Expr>,
    pub body: Block,
    pub span: Span,
}

/// Channel operation in a select arm --- D94.
#[derive(Debug, Clone)]
pub enum SelectOp {
    Recv {
        binding: Option<String>,
        chan: Box<Expr>,
    },
    Send {
        chan: Box<Expr>,
        value: Box<Expr>,
    },
    Default,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    Eq,
    Neq,
    Lt,
    Le,
    Gt,
    Ge,
    And,
    Or,
    /// Plan 33.1 (D24): `==>` импликация. `A ==> B` ≡ `!A || B`.
    /// Используется только в контрактах (Ф.2 проверит, что Implies
    /// не появляется в обычном коде — это compile error).
    Implies,
    /// Plan 33.1 (D24): `<==>` эквивалентность. `A <==> B` ≡ `A == B` для bool.
    /// Используется только в контрактах.
    Iff,
    /// Bitwise (D-operators в spec/03-syntax.md). Применимы только к int.
    BitAnd,
    BitOr,
    BitXor,
    Shl,
    Shr,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum UnOp {
    Neg,
    Not,
}

/// Pattern для match / let / if-let.
#[derive(Debug, Clone)]
pub enum Pattern {
    /// `_`
    Wildcard(Span),
    /// `42`, `"hello"`, `true`
    Literal(Literal, Span),
    /// `name` — связывает (или enum unit-variant без скобок)
    Ident {
        name: String,
        span: Span,
    },
    /// `Variant`, `Variant(p1, p2)`, `Cons(h, ..)` — D59
    Variant {
        path: Vec<String>,
        kind: VariantPatternKind,
        span: Span,
    },
    /// `{ field: pat, name, .. }` — D17/D52
    Record {
        type_path: Option<Vec<String>>,
        fields: Vec<RecordPatternField>,
        rest: bool, // присутствует ли ..
        span: Span,
    },
    /// `[]`, `[a]`, `[head, ..rest]` — D59
    Array {
        elems: Vec<ArrayPatternElem>,
        span: Span,
    },
    /// `(a, b, c)`
    Tuple(Vec<Pattern>, Span),
    /// `pattern as binding` — TODO (не нужно в bootstrap)
    Binding {
        name: String,
        inner: Box<Pattern>,
        span: Span,
    },
    /// `p1 | p2 | p3` — alternation в match arm.
    /// Все варианты должны иметь одинаковый набор bindings (по spec
    /// pattern-match семантике); в bootstrap'е bindings из первого
    /// варианта используются в теле arm. Не вкладывается внутрь других
    /// patterns — alternation только на верхнем уровне match-arm.
    Or {
        alternatives: Vec<Pattern>,
        span: Span,
    },
}

#[derive(Debug, Clone)]
pub enum VariantPatternKind {
    /// `Variant`
    Unit,
    /// `Variant(pat1, pat2)` или `Variant(..)` или `Variant(pat, ..)`
    Tuple {
        patterns: Vec<Pattern>,
        rest: bool,
    },
}

#[derive(Debug, Clone)]
pub struct RecordPatternField {
    pub name: String,
    /// `field: pat` — Some(pat); `field` — None (shorthand)
    pub pattern: Option<Pattern>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub enum ArrayPatternElem {
    /// `[a, b]` — обычный pattern
    Item(Pattern),
    /// `..` — без bind
    Rest,
    /// `..rest` — slice-bind (D59)
    RestBind(String),
}

#[derive(Debug, Clone)]
pub enum Literal {
    Int(i64),
    Float(f64),
    Str(String),
    Bool(bool),
    Unit,
    /// Q-char-literals: 'a' codepoint as u32 (используется в pattern'ах, как `match c { 'a' => ... }`).
    Char(u32),
}

impl Pattern {
    pub fn span(&self) -> Span {
        match self {
            Pattern::Wildcard(s)
            | Pattern::Literal(_, s)
            | Pattern::Ident { span: s, .. }
            | Pattern::Variant { span: s, .. }
            | Pattern::Record { span: s, .. }
            | Pattern::Array { span: s, .. }
            | Pattern::Tuple(_, s)
            | Pattern::Binding { span: s, .. }
            | Pattern::Or { span: s, .. } => *s,
        }
    }
}
