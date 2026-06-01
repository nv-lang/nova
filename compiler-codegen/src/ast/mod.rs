//! Типы AST.
//!
//! Минималистичный набор: всё что нужно для bootstrap'а Nova-on-Nova.
//! Plan 33.1: добавлены контракты (`Contract`, `VerifyMode`, `Purity`) —
//! AST-узлы готовы; парсер/typecheck/SMT расширения — в последующих
//! фазах [Plan 33.1](../../../docs/plans/33.1-contracts-core.md).

pub mod pretty;

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
    /// Plan 45 Ф.22.1 / D105: module-level doc-attrs
    /// (`#stable`/`#unstable`/`#experimental`/`#deprecated`/`#hide_doc`/etc.).
    /// Семантически: tier propagates на items module'а без явного override.
    pub doc_attrs: Vec<DocAttr>,
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
    /// Plan 81 Ф.1: объявленное имя модуля (из `module X.Y.Z`).
    /// Используется NameResCtx для группировки peers одного module:
    /// только files с одинаковым module_name делят declarations namespace.
    /// Защищает от ложного group-sharing когда два разных модуля живут
    /// в одной директории (e.g. nova_tests/plan81/lib.nv vs
    /// nova_tests/plan81/visibility_fn_rejected.nv — разные модули,
    /// разные module_name, но одна папка).
    pub module_name: Vec<String>,
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
    /// Plan 33.3 Ф.13: `#must_verify_module` — все функции модуля
    /// автоматически получают MustVerify. Любой unproven контракт → compile error.
    MustVerifyModule,
    /// Ф.3.4 (Plan 33.6): `#proof_budget(timeout_ms=N, vc_count_max=M)` —
    /// module-level бюджет верификации. Переопределяется per-fn `#verify_timeout`.
    ProofBudget { timeout_ms: Option<u32>, vc_count_max: Option<u32> },
    /// **Plan 107 D174:** `#no_prelude` before `module` declaration. Suppress'ит
    /// auto-import `std.prelude` (D26). Применение:
    ///   - real-time / embedded (prelude содержит GC-using код),
    ///   - bootstrap уровни (сам prelude и его sub-modules — auto-detected
    ///     через `is_prelude_self_module`, не требует opt-out),
    ///   - обучающие примеры где надо явно показать всё.
    /// Прежняя inline-форма `module X no_prelude` удалена (D174, Plan 107).
    /// Без `#no_prelude` — стандартный prelude auto-import (D26 default).
    /// Совместим с explicit `import std.prelude.core.{Option}` etc.
    NoPrelude,
    /// **Plan 107 D174:** `#prelude(core, runtime)` before `module` declaration.
    /// Auto-import только перечисленных sub-modules `std.prelude.<name>`
    /// вместо full facade. Валидные имена: `core`, `runtime`, `errors`,
    /// `collections`, `protocols`, `effects`. Имена валидируются на
    /// resolver-этапе (compiler error при опечатке).
    /// Пустой list `#prelude()` — compile error; используй `#no_prelude`.
    /// Прежняя inline-форма `module X partial_prelude(...)` удалена (D174).
    PartialPrelude(Vec<String>),
    /// **Plan 107 D174:** `#allow(shadow)` before `module` declaration.
    /// Suppresses `W_PRELUDE_SHADOW` warnings emitted by
    /// `lints::lint_prelude_shadow` for user-declarations that shadow
    /// prelude-imported names.
    /// Прежняя inline-форма `module X allow_prelude_shadow` удалена (D174).
    ///
    /// **Когда применять:**
    ///   - Local DSL слой, переопределяющий `Option`/`Result`/etc. с
    ///     осознанным intent'ом (e.g. embedded targets с non-GC types).
    ///   - Test fixtures, где user-decl эксплицитно тестирует shadowing.
    ///   - Bootstrap слои, не имеющие #no_prelude но желающие тихо
    ///     shadow'ить отдельные имена.
    ///
    /// Без `#allow(shadow)` (default) — shadowing → W_PRELUDE_SHADOW
    /// warning + user-declaration wins (compilation продолжается).
    /// С `#allow(shadow)` — то же поведение, но без warning'а.
    ///
    /// Item-level suppress (`#[allow(prelude_shadow)] type Foo`) — DEFERRED
    /// (требует generic attribute parser, который пока hardcoded на
    /// `TypeAttr` enum'ы; см. ast::TypeAttr).
    AllowPreludeShadow,
    /// **Plan 90.1 D141 amend:** `#allow(view_extend_detach)` before `module`.
    /// Suppresses `W_VIEW_EXTEND_DETACH` warnings emitted by
    /// `lints::lint_view_extend_detach` when a grow-method (append / insert /
    /// reserve) is called on a parent array after a slice-view
    /// of that parent was created in the same function scope.
    ///
    /// **When to apply:**
    ///   - Code where the view is intentionally discarded before the grow call.
    ///   - Test fixtures verifying extend/insert/reserve behaviour on arrays
    ///     that happen to have a prior view binding in scope.
    ///
    /// Default: grow-call after view-binding → W_VIEW_EXTEND_DETACH warning.
    /// With `#allow(view_extend_detach)`: same behaviour, no warning.
    AllowViewExtendDetach,
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

/// Plan 84: якорь импорта — абсолютный (от корня пакета) или относительный.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImportAnchor {
    /// Абсолютный — путь от корня пакета (bare `import a.b.c`). Default.
    Package,
    /// Относительный (Plan 84) — `./` (`up == 0`, директория импортирующего
    /// файла) либо `../`×n (`up == n`, n уровней вверх). Резолвится
    /// относительно директории файла, строго в пределах своего пакета.
    Relative { up: u32 },
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
    /// Plan 45 Ф.24.11: doc-attrs on import/re-export.
    /// Currently: DocInline, DocNoInline (controls inline rendering in nova doc).
    pub doc_attrs: Vec<DocAttr>,
    /// Plan 84: относительный/абсолютный якорь резолва пути импорта.
    pub anchor: ImportAnchor,
}

/// Top-level декларация в модуле.
#[derive(Debug, Clone)]
pub enum Item {
    Fn(FnDecl),
    Type(TypeDecl),
    Let(LetDecl),
    Const(ConstDecl),
    Test(TestDecl),
    /// Plan 57: `bench "name" { ... measure { ... } ... }` —
    /// benchmark declaration. Только discoverable под `nova bench`,
    /// игнорируется в `nova test`/`nova build`. Body содержит setup
    /// + один `measure { ... }` блок.
    Bench(BenchDecl),
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
    /// Plan 77 (D132): `-> @` — метод возвращает сам receiver (fluent).
    /// `return_type` при этом = `Self` (тип результата — receiver-тип);
    /// флаг добавляет гарантию «возвращается именно receiver». Валидно
    /// только для instance-метода (parser enforce'ит).
    pub returns_receiver: bool,
    pub body: FnBody,
    pub span: Span,
    /// Plan 16 (D64 sugar §3697) / Plan 113: `#realtime` / `#realtime nogc` атрибут перед `fn`.
    /// Fn-level callee guarantee — body can only call other `#realtime` fns/primitives.
    pub realtime_attr: RealtimeAttr,
    /// Plan 113 (D172): `#blocking` attribute перед `fn`.
    /// Runtime threadpool offload — callers wrap fn in uv_queue_work, fiber parks.
    pub blocking_attr: bool,
    /// Plan 110.7.3.a (D188 §FFI): `#cancel_safe` attribute на `external fn`.
    /// Attests что C-side function is cancel-safe — can be invoked from inside
    /// ConsumeScope on_exit body (under cancel-shield). Without this attribute,
    /// the cancel-unsafe lint (W_FFI_CANCEL_UNSAFE) fires at the call site
    /// when the call appears inside an on_exit body. Default `false`.
    pub cancel_safe_attr: bool,
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
    /// Plan 33.3 Ф.13: `#trusted` external fn — контракты становятся axioms
    /// (без SMT-доказательства). Допустим только для `external fn`.
    pub is_trusted: bool,
    /// Plan 33.9 Ф.1 (D24 §35): `#opaque` — body не раскрывается в SMT
    /// scope (treated как UF). Требует `#pure`, conflict с `#verify`.
    /// Реализовано: parser/AST/lints (V1); Z3 axiomatic encoding — V2.
    pub is_opaque: bool,
    /// Plan 33.9 Ф.3: `#fuel(n)` — controlled unfolding depth для opaque
    /// recursive fns. None = default 0 (без unfold). Z3 emits chain of
    /// N axioms (V2 — TrivialBackend ignores).
    pub fuel: Option<u32>,
    /// Plan 33.7: `#nooverflow` — для каждой BitVec-арифметической операции
    /// в теле fn генерируется overflow VC. Если доказательство неудачно →
    /// compile error. Без атрибута: wrap-around семантика (2's complement).
    pub no_overflow: bool,
    /// Plan 103.6 / Plan 113: sync interaction class — parsed from #realtime/#parks/#wakes.
    /// None = no annotation (conservative: treated as Parks in realtime context).
    /// Stored in ExternalDecl for O(1) lookup during emit_call.
    pub sync_class: Option<SyncClass>,
    /// Plan 100.5 (D163): `needs <Cap1>, <Cap2>` clause on `external fn`.
    /// Declares which capabilities the FFI function requires. Empty = no
    /// needs clause declared (used by type-checker to emit D163-missing-cap
    /// when the function carries consume-obligations).
    pub needs_caps: Vec<String>,
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

/// Plan 103.6 / Plan 113: sync interaction class for external fn declarations.
///
/// Parsed from `#realtime` / `#parks` / `#wakes` attributes in sync.nv.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyncClass {
    /// Leaf method — no fiber park/wake. Callee guarantee: body is realtime-safe.
    /// Annotation: `#realtime` (Plan 113 rename from `#realtime_safe`).
    Realtime,
    /// May park the calling fiber. Forbidden in #realtime fns and blocking{}.
    /// Annotation: `#parks`.
    Parks,
    /// Wakes other fibers (no self-park). Forbidden in #realtime context
    /// (scheduler interaction); allowed in blocking{} as leaf operation.
    /// Annotation: `#wakes`.
    Wakes,
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
    /// Plan 73 (D131): `fn Type consume @method` — consuming receiver.
    /// После вызова такого метода переменная-источник логически
    /// инвалидируется; use-after-consume → compile error. Взаимно-
    /// исключающий с `mutable` (parser enforce'ит).
    pub consume: bool,
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
    /// Plan 73 (D131): `consume name Type` — consuming параметр.
    /// После передачи аргумента в такой параметр переменная-источник
    /// логически инвалидируется; use-after-consume → compile error.
    pub consume: bool,
    /// Plan 108.1 (D176 amend): `mut name Type` — параметр позволяет
    /// вызов mut-методов / index-assignment в callee.  Default = false
    /// (read-only param).  Конфликтует с `consume` и `readonly` —
    /// parser-level error.
    pub is_mut: bool,
}

/// Plan 15 (D72): generic-параметр с optional bound.
///
/// `[T]` — `GenericParam { name: "T", bounds: [] }`.
/// `[T Hashable]` — `GenericParam { name: "T", bounds: [Hashable_TypeRef] }`.
///
/// Plan 101.3 (D145 Ред. 5): multi-bound `[T A + B + C]` —
/// `bounds: [A, B, C]`. Type T должен удовлетворить ВСЕМ bound'ам
/// (conjunction / intersection). Семантически equivalent протоколу
/// `protocol { use A  use B  use C }` (см. Plan 101.4), но без
/// дополнительной декларации.
///
/// Bound — это protocol-тип ([D72](spec/decisions/02-types.md#d72)).
/// Запрещены forward-references: имя в bound должно быть объявлено
/// раньше в том же `[...]` или в окружающем type-context.
#[derive(Debug, Clone)]
pub struct GenericParam {
    pub name: String,
    /// Plan 101.3: список bound'ов (conjunction). Пустой = unbounded.
    /// Один = `[T Bound]` (D72 legacy). Несколько = `[T A + B]`.
    pub bounds: Vec<TypeRef>,
    /// Plan 19 C10 (D88): default-значение generic-параметра.
    /// Используется когда вызывающий код не указал T явно и
    /// inference из аргументов не дал результат. Для `[T = f64]`
    /// или `[T Bound = Default]`.
    pub default: Option<TypeRef>,
    pub span: Span,
    /// Plan 100.2 (D156): `[T consume]` — strict-mode consume bound.
    /// Внутри тела функции с этим bound'ом, T-typed значения трактуются
    /// как consume-obligations (must be consumed before scope-exit).
    /// Backward-compat: без bound — silent-ignore (D133 default).
    pub consume_bound: bool,
}

impl GenericParam {
    /// Helper для legacy кода: если bound не нужен.
    pub fn unbounded(name: String, span: Span) -> Self {
        Self { name, bounds: Vec::new(), default: None, span, consume_bound: false }
    }

    /// Plan 101.3 helper: первый bound (legacy single-bound API).
    /// Используется для backward-compat с кодом, который пока работает
    /// только с первым bound (codegen mono-dispatch, doc render и пр.).
    /// Когда multi-bound enforcement распространится, callers переедут
    /// на `bounds()` итерацию.
    pub fn first_bound(&self) -> Option<&TypeRef> {
        self.bounds.first()
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

/// Plan 52 Ф.1: атрибут-маркер на декларации типа (`#from_fields`).
///
/// Маркер `#from_fields` помечает str-keyed map-тип, в который анонимный
/// record-литерал `{field: v}` коэрсится через D55 map-coercion (имена
/// полей становятся строковыми ключами). Bootstrap honored только для
/// `collections.hashmap.HashMap` — type-checker дополнительно сверяет
/// canonical identity, чтобы shadowing локальным `HashMap` не ломал
/// правило. Точка расширения (`OrderedMap`, `BTreeMap`) — позже.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TypeAttr {
    /// `#from_fields` — str-keyed map-тип для D55 map-coercion (D108 / Plan 52).
    FromFields,
    /// Plan 52 Ф.23: `#from_pairs` — тип desugar'а map-литерала `[k: v]`.
    /// Если expected type помечен `#from_pairs`, десугаринг вызывает
    /// `ExpectedType.with_capacity(n)` + `insert_new(k, v)` per pair
    /// вместо хардкода `HashMap`. Тип должен предоставить методы
    /// `static with_capacity(int) -> Self` и `mut insert_new(K, V)`.
    /// Stdlib HashMap имеет оба attribute. User-типы получают
    /// расширяемость без модификации компилятора.
    FromPairs,
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
    /// Plan 114.4.1 (D200): associated constants — `const NAME T = expr`
    /// внутри `type X { ... }`. НЕ в instance layout; accessible через
    /// namespace `Type.NAME`. Пустой вектор у типов без assoc consts.
    pub assoc_consts: Vec<AssocConst>,
    /// Plan 52 Ф.1: атрибуты-маркеры перед `type` (`#from_fields`).
    /// Пустой вектор у типов без атрибутов (backward-compat).
    pub attrs: Vec<TypeAttr>,
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
    /// Plan 100.1 (D133 / D1): `type X consume { ... }` — type-level
    /// must-be-consumed marker. Каждый instance такого типа обязан
    /// быть consumed до scope-exit (через consume-method, return,
    /// consume-param, record-field-move, или defer).
    /// Backward-compat: default false.
    pub consume: bool,
    /// Plan 91.9 (D186): `#impl(P1 + P2 + ...)` annotation list.
    /// Names of protocols the type explicitly opts into. Verification:
    /// compiler checks T provides every method of each P (via explicit
    /// methods OR synthesizable default body, per Plan 91.8a.2).
    /// Gating: bare-call protocol-method synthesis (`u.greet()` without
    /// `[T Greetable]` bound or `let g Greetable = u` coercion) ONLY
    /// fires когда the type is opted-in via `#impl`. Without `#impl`,
    /// bare call к default-body-synthesized method gives E7320 (no method).
    /// Empty Vec — default (no opt-in).
    pub impl_protocols: Vec<String>,
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
    ///
    /// Plan 101.4 (D145 Ред. 5): protocol composition через `use P` items
    /// в теле — `type ReadWriter protocol { use Reader  use Writer }`.
    /// `embeds` — список TypeRef'ов (всегда Named-форм с указателем на
    /// другой `TypeDeclKind::Protocol`). Type-checker flatten'ит embeds:
    /// все методы embedded protocol'а считаются методами outer'а
    /// (через `flatten_protocol_methods`). При duplicate signatures —
    /// ошибка `[E_PROTOCOL_EMBED_DUPLICATE]`; при non-protocol target —
    /// `[E_PROTOCOL_EMBED_NOT_PROTOCOL]`; при cycle — `[E_PROTOCOL_EMBED_CYCLE]`.
    Protocol {
        methods: Vec<EffectMethod>,
        /// Embedded protocols (`use Reader`, `use Writer`). Empty для
        /// плоских (не-composed) protocol'ов — backward-compat.
        embeds: Vec<TypeRef>,
    },
    /// `type NewType u64` — newtype (D52)
    Newtype(TypeRef),
    /// `type Name alias OtherType` (D52)
    Alias(TypeRef),
    /// Plan 120 (D215): `type Name(field1 T1, field2 T2)` — named tuple.
    /// Stack-allocated value type identical to positional tuple (D123)
    /// but fields accessed by name (`.x`, `.y`) instead of position (`.0`).
    NamedTuple(Vec<NamedTupleField>),
    /// Plan 62.D.bis (D126): `external type X [Generics]` — opaque
    /// type known to compiler by name, реализация в runtime
    /// (`nova_rt/<x>.h`/.c). Без body, без variants/fields. Restricted
    /// to `std.runtime.*` / `std.prelude.*` модулям (whitelist в
    /// types/mod.rs::check_module параллельно `external fn` per D82).
    /// Codegen эмитит ссылку как `Nova_<Name>*` (pointer); struct
    /// определение живёт в runtime header.
    Opaque,
}

/// Plan 114.4.1 (D200): associated constant — `const NAME T = expr` inside
/// `type X { ... }` body. НЕ в instance layout (zero storage); accessible
/// через namespace `Type.NAME`. Codegen emit'ит как top-level
/// `static const T Type_NAME = literal;` в .rodata.
#[derive(Debug, Clone)]
pub struct AssocConst {
    pub name: String,
    pub ty: Option<TypeRef>,
    pub value: Expr,
    pub span: Span,
    /// `export const FOO …` — public cross-module access.
    pub is_export: bool,
}

/// Plan 120 (D215): field in a named tuple type declaration.
#[derive(Debug, Clone)]
pub struct NamedTupleField {
    pub name: String,
    pub ty: TypeRef,
    pub span: Span,
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
    /// Plan 100.1 (D133 / D4): `consume field T` — field хранит
    /// consume-typed значение. Type-decl с таким полем ДОЛЖЕН быть
    /// сам объявлен `consume` (compile error D133-type-marker-missing
    /// иначе). Field-type должен сам быть consume-типом (W
    /// D133-marker-on-non-consume иначе).
    /// Backward-compat: default false.
    pub consume: bool,
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
    /// Plan 97 (D58 amend, `Q-static-method-protocol` resolved): метод
    /// объявлен как **статический** в protocol-теле через leading-точку
    /// (`.method(...)`); реализация ожидается через D35 `fn Type.method`.
    /// Для bare-имён (`method(...)`) — `false` (instance, backwards-compat).
    /// Для effect-методов всегда `false` (у эффектов нет static — это
    /// handler-actions через runtime stack). Hard-enforcement static↔
    /// instance mismatch — followup (см. plan97/Ф.0.4).
    pub is_static: bool,
    /// Plan 91.8a (D183): default body для protocol method. `None` =
    /// abstract method (тип-implementer ОБЯЗАН реализовать). `Some(body)` =
    /// default — implementer может override; если не задал явно, codegen
    /// синтезирует function из default body (substituting Self → impl-type).
    /// Для effect-методов всегда None (default bodies применимы только к
    /// protocols).
    pub default_body: Option<Block>,
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
    /// Plan 100.1 (D133 / D9): `consume tx = expr` — explicit binding
    /// для Live-linear ownership. Обязателен для consume-типов
    /// (compile error D133-consume-needs-keyword иначе). Без него
    /// `let tx = …` для consume-rvalue = parse-accepted, type-check
    /// rejects.
    /// Backward-compat: default false.
    pub consume: bool,
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

/// Plan 57: benchmark declaration.
///
/// Three forms:
///   1. Plain:           `bench "name" { setup; measure { body } teardown }`
///   2. Parameterized:   `bench "name" (n in [v1, v2, ...]) { setup; measure { body } teardown }`
///   3. Grouped (57.B.5): `bench "name" { group "g1" { case "c1" { ... } case "c2" { ... } } ... }`
///
/// Fields:
/// - `setup`, `measure_body`, `teardown` — для плоских benches (формы 1, 2).
/// - `params` — sweep param для form 2 (Plan 57.B.3).
/// - `groups` — для form 3 (Plan 57.B.5). Если непустой, поля setup/
///   measure_body/teardown игнорируются; каждый case даёт отдельную
///   entry с name `<bench>/<group>/<case>`.
#[derive(Debug, Clone)]
pub struct BenchDecl {
    pub name: String,
    pub setup: Vec<Stmt>,
    pub measure_body: Block,
    pub teardown: Vec<Stmt>,
    /// Plan 57.B.3: parameter sweep — `(name in [v1, v2, ...])`.
    pub params: Option<BenchParams>,
    /// Plan 57.B.5: sub-benchmarks via `group "..." { case "..." { ... } }`.
    pub groups: Vec<BenchGroup>,
    pub span: Span,
}

/// Plan 57.B.3: parameterized sweep — `bench "name" (n in [10, 100, 1000]) { ... }`.
#[derive(Debug, Clone)]
pub struct BenchParams {
    pub var_name: String,
    pub values: Vec<i64>,
    pub span: Span,
}

/// Plan 57.B.5: bench group — collection of cases sharing a logical context.
#[derive(Debug, Clone)]
pub struct BenchGroup {
    pub name: String,
    pub cases: Vec<BenchCase>,
    pub span: Span,
}

/// Plan 57.B.5: single case within a bench group.
#[derive(Debug, Clone)]
pub struct BenchCase {
    pub name: String,
    pub setup: Vec<Stmt>,
    pub measure_body: Block,
    pub teardown: Vec<Stmt>,
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
    /// Plan 97 Ф.2 (D53 §628, D142): анонимный protocol-тип в позиции
    /// типа — `protocol { method-sig* }`. Используется в:
    /// - параметре (`fn f(x protocol { close() -> () })`),
    /// - возвращаемом типе,
    /// - generic-bound (`fn min[T protocol { @lt(...) -> bool }]`).
    /// Body — переиспользует парсер named-protocol-тела
    /// (`parse_effect_methods`); `EffectMethod.is_static` поддерживается.
    Protocol {
        methods: Vec<EffectMethod>,
        span: Span,
    },
    /// `()` unit
    Unit(Span),
    /// `readonly T` — compile-time immutability modifier (D176, Plan 108).
    /// Zero runtime overhead: only compile-time check. Forbids mut-methods
    /// and index writes. `T → readonly T` coerce allowed; reverse forbidden.
    Readonly(Box<TypeRef>, Span),
}

impl TypeRef {
    pub fn span(&self) -> Span {
        match self {
            TypeRef::Named { span, .. }
            | TypeRef::Array(_, span)
            | TypeRef::FixedArray(_, _, span)
            | TypeRef::Tuple(_, span)
            | TypeRef::Func { span, .. }
            | TypeRef::Protocol { span, .. }
            | TypeRef::Unit(span)
            | TypeRef::Readonly(_, span) => *span,
        }
    }

    /// Returns the inner type if this is `readonly T`, otherwise returns `self`.
    pub fn strip_readonly(&self) -> &TypeRef {
        match self {
            TypeRef::Readonly(inner, _) => inner.strip_readonly(),
            other => other,
        }
    }

    pub fn is_readonly(&self) -> bool {
        matches!(self, TypeRef::Readonly(..))
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
    /// Plan 114.4 Ф.2: scope-local `const N = expr` — strict constexpr
    /// binding inside function body / block. Codegen inline'ит literal
    /// value на use-sites (zero allocation, zero binding overhead).
    /// Visible до end-of-enclosing-block. Constexpr-eligibility — same как
    /// module-level const (check_const_constexpr).
    Const(ConstDecl),
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
    /// D160 Plan 100.4.3: `okdefer body` — complement к errdefer.
    /// Выполнить body ТОЛЬКО при success-path (normal end-of-scope или
    /// `return expr`); skipped при throw/panic/interrupt. Симметризует
    /// defer-family: errdefer=error-only, okdefer=success-only.
    OkDefer {
        body: Expr,
        span: Span,
    },
    /// D160 Plan 100.4.3: `defer |result_binding| body` — reason-aware
    /// форма defer. Тело выполняется на ВСЕХ exit-paths (как `defer`),
    /// но с доступом к exit-reason через `result_binding`.
    /// `result_binding` — имя переменной (паттерн). Её тип: DeferResult[T, E]
    /// где T = return type, E = error type функции.
    DeferWithResult {
        result_binding: String,
        body: Expr,
        span: Span,
    },
    /// Plan 110 (D188): `consume IDENT (':' TYPE)? '=' EXPR '{' BODY '}'`
    /// — scope-block с автоматическим вызовом `Consumable.on_exit` при
    /// выходе из BODY (success/throw/panic/cancel).
    ///
    /// Parser detect block-form через lookahead `{` после init EXPR
    /// (с disabled `no_trailing_block` чтобы не путать с trailing-block
    /// call syntax).
    ///
    /// `binding`: single identifier (D188 disallows destructure для scope-
    /// block).
    /// `type_annot`: optional type annotation (parallel с `LetDecl`).
    /// `init`: expression — должна resolve к типу implementing
    /// `Consumable[E]` (D196 init type constraints; D188 R1 partial-
    /// construction safety).
    /// `body`: scope body — sequence of statements + optional trailing
    /// expression.
    ///
    /// Codegen pipeline:
    /// - Plan 110.1.4: basic desugaring (sync, no shield/timeout).
    /// - Plan 110.2: cancel-shield + 3-level timeout resolution.
    /// - Plan 110.1.7: D194 hot-path elision для `Consumable[never]`.
    ///
    /// См. spec/decisions/03-syntax.md D188.
    ConsumeScope {
        binding: String,
        type_annot: Option<TypeRef>,
        init: Expr,
        body: Block,
        span: Span,
    },
    /// Plan 33.2 Ф.8 (D24): `assert_static <bool>` — intermediate proof
    /// obligation. В debug — runtime check; в release — стирается.
    ///
    /// Plan 33.8 Ф.6.3: V1 — `assert_static` НЕ верифицируется SMT.
    /// Корректная compile-time проверка требует flow-sensitive
    /// верификации (нужно знать состояние именно в точке assert'а, а
    /// модель `verify_fn` flow-insensitive — та же причина, что у
    /// `assume`). Полная интеграция отложена (V2). В V1 `assert_static`
    /// действует как обычный runtime-assert; lint `assert-static-unverified`
    /// (lints.rs) предупреждает, чтобы не было ложной уверенности.
    AssertStatic {
        expr: Expr,
        span: Span,
    },
    /// Plan 33.3 (D24): `assume <bool>` — escape hatch для FFI / external
    /// knowledge. В debug — runtime check; в release — стирается.
    ///
    /// Plan 33.8 Ф.3.1: вне `#trusted` функции эмитится lint-warning
    /// `trust-introduced` (lints.rs `check_assume_trust`).
    ///
    /// V1: `assume` НЕ интегрирован в SMT-scope верификатора (формула не
    /// ассертится как аксиома). Корректная интеграция требует
    /// flow-sensitive верификации (assume на позиции N ограничивает только
    /// VC после N) — текущая модель verify_fn не flow-sensitive. Полная
    /// интеграция отложена (Plan 33.8 Ф.3.2 → V2). Текущее поведение
    /// soundness-safe: assume просто не влияет на доказательства.
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
    /// Plan 33.9 Ф.2: `reveal name` — раскрывает opaque fn body в SMT
    /// scope текущей fn body. Ghost statement: emit'ит axiom
    /// `forall args. name(args) == body` в SMT scope; erased в codegen.
    /// V1: parser/AST/lints (без real Z3 axiom emission — V2 task).
    Reveal {
        name: String,
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
    /// **Plan 115 D214:** `null ptr` two-token literal. Bitwise zero opaque
    /// pointer. V1 ограничение: только `null ptr` (другие типы → parser
    /// emit'ит `E_NULL_LITERAL_REQUIRES_PTR`). Plan 118 future расширит до
    /// `null *T` family.
    NullPtrLit,
    /// `arr` или `[1, 2, ...rest, 4]` — D60
    ArrayLit(Vec<ArrayElem>),
    /// `[k1: v1, k2: v2]` — map-литерал (D108, Plan 52). Конструирует
    /// `HashMap[K, V]`. Ключи и значения — выражения; порядок вычисления
    /// нормативный: `k1, v1, k2, v2, ...` (слева направо, ключ перед
    /// значением). Десугарится в codegen/interp в `with_capacity`+`@insert`
    /// block-expression. Пустой `[]` остаётся `ArrayLit(vec![])` —
    /// разрешается по ожидаемому типу на type-check.
    ///
    /// Plan 52 Ф.7 production-fix: `inferred_key`/`inferred_value` —
    /// типы K/V, выведенные type-checker'ом (MapLitCtx::annotate_module)
    /// после inference. Десугаринг использует их для генерации turbofish
    /// `HashMap[K, V].with_capacity(n)` — без turbofish мономорфизация
    /// инстанциирует `HashMap[void*, void*]` → runtime segfault на
    /// generic-метод-резолюции. Парсер заполняет `None`, type-checker —
    /// `Some(_)` если K/V определены однозначно.
    MapLit {
        /// Plan 55 Ф.* (followup): mixed pairs + spreads. Раньше было
        /// `Vec<(Expr, Expr)>` (только пары); теперь spreads `...m`
        /// перемежаются с парами `k: v` в любом порядке.
        elems: Vec<MapElem>,
        inferred_key: Option<TypeRef>,
        inferred_value: Option<TypeRef>,
        /// Plan 52 Ф.23: имя target-типа для desugar. Если type-checker
        /// определил что expected type помечен `#from_pairs` —
        /// записывает сюда полный path (e.g. `["HashMap"]` для stdlib,
        /// `["MyMap"]` для user-типа). Десугаринг использует это вместо
        /// хардкода `HashMap`. `None` → fallback на `HashMap` (legacy).
        inferred_target_type: Option<Vec<String>>,
    },
    /// `{ field: value, ...spread, name }` — D17/D52/D60
    RecordLit {
        type_name: Option<Vec<String>>, // Some(["User"]) для `User { ... }`
        fields: Vec<RecordLitField>,
        /// Plan 52 Ф.10 production-fix: D55 map-coercion маркер. Когда
        /// type-checker (`MapLitAnnotator`) обнаруживает, что
        /// анонимный `{field: v}` стоит в позиции, ожидающей тип с
        /// `#from_fields` (= `HashMap[str, V]`) — записывает сюда
        /// `Some(V)`. Десугаринг тогда превращает узел в
        /// `HashMap[str, V].with_capacity(n) + insert("field", v)`
        /// block-expression (mirror MapLit-desugar). Без этого flag'а
        /// codegen пытался бы построить record-struct из полей →
        /// «no member named 'debug' in 'struct Nova_HashMap'».
        /// `None` для обычного record-литерала или type_name = Some.
        inferred_map_v: Option<TypeRef>,
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
        /// Plan 87: явная аннотация типа элемента — `for x TYPE in iter`.
        /// `None` — тип элемента выводится (поведение до Plan 87).
        elem_type: Option<TypeRef>,
        /// Plan 33.4 D.0.3: loop invariants (SMT + runtime).
        invariants: Vec<Expr>,
        /// Plan 33.4 D.0.3: well-founded termination measure.
        decreases: Option<Box<Expr>>,
        /// Plan 100.2 (D156): `for consume x in iter` — consume-iteration mode.
        /// Each loop variable is a consume-obligation (must be consumed in body).
        /// The iter expression itself is marked Consumed after the loop.
        /// Default false = view-mode (iter stays Live, loop var = view borrow).
        iter_consume: bool,
    },
    /// `parallel for x in iter { body }` — D14, fan-out body for each element.
    /// Desugars to `supervised { for x in iter { spawn { body } } }`.
    ParallelFor {
        pattern: Pattern,
        iter: Box<Expr>,
        body: Block,
        /// Plan 87: явная аннотация типа элемента — `parallel for x TYPE in iter`.
        elem_type: Option<TypeRef>,
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
    /// Handler-литерал: `effect EffectName { op(p) => ... ; ... }`
    /// (Plan 97 Ф.3: keyword `handler` → `effect`).
    HandlerLit {
        effect_name: Vec<String>,
        methods: Vec<HandlerMethod>,
    },
    /// Plan 97 Ф.4 (D142): protocol-литерал в expression-position —
    /// value, реализующий контракт named-protocol'а. Записывается как
    /// `protocol ProtoName { method-impl* }`. Семантика — closure-bundle
    /// (D22 capture-rules, managed heap D6) для one-off реализаций
    /// (capability-split factory pattern). **Instance-only**: static
    /// методы не могут быть в литерале (static — `Type.method` D35,
    /// у литерала нет «своего типа»).
    ProtocolLit {
        proto_name: Vec<String>,
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
    /// Range-expression — D58 (closed-form `a..b`/`a..=b`) + D144 (open-ended
    /// `a..`/`..b`/`..`, Plan 96 Ф.2).
    ///
    /// `start` / `end` — `Option`: `None` означает «без явной границы»
    /// (требуется bounded context — slice-index, D144). Closed-form (оба
    /// `Some`) допустимы везде: for-loop, materialize, slice. Open-ended
    /// (любой `None`) — **только** в slice-position `arr[range]`; вне
    /// slice-context type-checker эмитит ошибку (D144).
    Range {
        start: Option<Box<Expr>>,
        end: Option<Box<Expr>>,
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
    /// `blocking { body }` — Plan 83.3 (D50): leaf-блокирующая работа
    /// (FFI/syscall) уводится в libuv threadpool, fiber паркуется,
    /// M:N-worker свободен. Requires `Blocking` effect in the enclosing
    /// function's signature; запрещён внутри `realtime { }` (D64).
    Blocking(Block),
    /// `throw expr` в позиции expression (D25/D65). Обрабатывается как
    /// эффект `Fail.fail(msg)`, тип `never`. В codegen эмитируется как
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

/// Plan 55 followup (D108-spread): элемент map-литерала.
/// `[k: v]` — пара ключ-значение. `[...m]` — spread другой map.
/// Mixed: `[...defaults, "x": 1, ...overrides]` — последовательно
/// applies в порядке (later overrides earlier для duplicate keys).
#[derive(Debug, Clone)]
pub enum MapElem {
    /// Обычная пара `k: v`.
    Pair(Expr, Expr),
    /// `...expr` — spread другой map того же типа. expr должен
    /// иметь тип совместимый с inferred K, V литерала.
    Spread(Expr),
}

impl MapElem {
    /// Возвращает clone'd Vec<(Expr, Expr)> только pairs — для type-check /
    /// lint callers которые работают только с известными парами k:v.
    /// Spreads пропускаются (их type-check / lints — отдельной passes).
    pub fn cloned_pairs(elems: &[MapElem]) -> Vec<(Expr, Expr)> {
        elems.iter().filter_map(|e| match e {
            MapElem::Pair(k, v) => Some((k.clone(), v.clone())),
            MapElem::Spread(_) => None,
        }).collect()
    }

    /// True если есть хотя бы один Spread элемент.
    pub fn has_spread(elems: &[MapElem]) -> bool {
        elems.iter().any(|e| matches!(e, MapElem::Spread(_)))
    }
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
    /// D52 §2 `@field`-shorthand marker: parser desugar'ит `{ @name }`
    /// в AST `RecordLitField { name, value: Some(Member { obj: SelfAccess,
    /// name }) }` — identical к explicit `{ name: @name }`. Flag различает
    /// shorthand vs explicit, чтобы D52 §2 enforcement не false-positive'ил
    /// на parser-generated `@field` shorthand.
    pub at_shorthand: bool,
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
    /// `name` — связывает (или enum unit-variant без скобок).
    /// Plan 108.3 (D36 amend): `is_mut` — `mut name` форма (per-name
    /// в pattern: `let (mut a, b) = ...`; для loop-var `for mut x in ...`).
    Ident {
        name: String,
        span: Span,
        is_mut: bool,
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
