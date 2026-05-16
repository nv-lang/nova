//! Plan 45 Ф.4: typed IR для рендеринга documentation.
//!
//! `DocTree` — корневая структура. Содержит один `DocModule` (для
//! single-file `nova doc`) или несколько (для `--workspace` режима в
//! Plan 45.A). MVP — один module.
//!
//! Дизайн:
//! - **Strings rendered as Nova source** для типов и signatures (см.
//!   D107 §«Signature shape»). Consumer'ы, которым нужна структура,
//!   могут парсить тем же parser'ом. Это keeps JSON output портабельным.
//! - **Stable IDs** для items: `<module_path>::<name>` для свободных
//!   функций, `<module_path>::<TypeName>.<method>` для методов.
//! - **Sorted, deterministic** order — renderer обязан сохранять.

use crate::ast::DocBlock;
use crate::diag::Span;

/// Корневая структура doc-tree.
#[derive(Debug, Clone)]
pub struct DocTree {
    /// Версия формата (D107 `format_version`). MVP = 1.
    pub format_version: u32,
    /// Список документированных модулей.
    pub modules: Vec<DocModule>,
    /// Plan 45 Ф.6: разрешённые intra-doc-links. Заполняется
    /// `resolve_intra_doc_links` pass'ом. Sorted by (from_id, text).
    pub links: Vec<DocLink>,
    /// Plan 45 Ф.7: doc-tests, извлечённые из ` ```nova ` blocks.
    pub doc_tests: Vec<DocTest>,
    /// Plan 45 Ф.22.3 / D107: абсолютный путь к корню документируемого
    /// исходника (file's parent dir для single-file, либо workspace
    /// root для `nova doc <dir>`). `None` если caller не задал —
    /// поле опускается в JSON output.
    pub source_root: Option<String>,
}

impl DocTree {
    pub fn new() -> Self {
        Self {
            format_version: 1,
            modules: Vec::new(),
            links: Vec::new(),
            doc_tests: Vec::new(),
            source_root: None,
        }
    }
}

/// Plan 45 Ф.6: разрешённая intra-doc ссылка. По D107 §«Links shape».
#[derive(Debug, Clone)]
pub struct DocLink {
    /// ID item'а, в чьём doc-text найдена ссылка. `None` — link из
    /// module-level doc'а.
    pub from_id: Option<String>,
    /// Текст ссылки как написан: `Range`, `mod.Range`, `Range.map`.
    pub text: String,
    /// ID цели, если разрешена. `None` — broken link.
    pub target_id: Option<String>,
}

/// Plan 45 Ф.7: один извлечённый doc-test. По D104 §«Doc-test modifiers».
#[derive(Debug, Clone)]
pub struct DocTest {
    /// Stable ID для теста: `<module>::doc_test_<N>`.
    pub id: String,
    /// ID item'а, в чьём doc'е найден тест. `None` — module-level.
    pub from_id: Option<String>,
    /// Порядковый номер в пределах DocTree (1-based).
    pub index: u32,
    /// Modifiers: no_run / ignore / compile_fail / should_panic /
    /// must_verify. Пустой = обычный test.
    pub modifiers: Vec<DocTestModifier>,
    /// Visible source — то что показывается в рендере (без hidden `# `).
    pub visible_source: String,
    /// Full source — то что компилируется (включая hidden `# ` boilerplate).
    pub full_source: String,
    /// Plan 45 Ф.23.7 / D106: `#doc(test_handlers = "path")` — handler-path
    /// для инжекции `with <handler>` в wrap_source. Наследуется от item.
    pub test_handlers: Option<String>,
}

/// Plan 45 Ф.7: doc-test modifier. По D104.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DocTestModifier {
    /// `no_run` — компилируется, не запускается.
    NoRun,
    /// `ignore` — не компилируется (только display).
    Ignore,
    /// `compile_fail` — ожидается compile-error.
    CompileFail,
    /// `should_panic` — ожидается runtime panic.
    ShouldPanic,
    /// `must_verify` — ожидается successful SMT verification (Plan 33).
    MustVerify,
}

impl DocTestModifier {
    pub fn as_str(self) -> &'static str {
        match self {
            DocTestModifier::NoRun => "no_run",
            DocTestModifier::Ignore => "ignore",
            DocTestModifier::CompileFail => "compile_fail",
            DocTestModifier::ShouldPanic => "should_panic",
            DocTestModifier::MustVerify => "must_verify",
        }
    }
}

impl Default for DocTree {
    fn default() -> Self {
        Self::new()
    }
}

/// Один модуль.
#[derive(Debug, Clone)]
pub struct DocModule {
    /// Dotted path: `["std", "collections", "range"]`.
    pub path: Vec<String>,
    /// Последний сегмент `path` (для удобства).
    pub name: String,
    /// `folder` (folder-module) или `file` (single-file).
    pub kind: ModuleKind,
    /// Peer-file paths (для folder-modules). Пусто для file.
    pub peers: Vec<String>,
    /// Summary — первое предложение из `doc` (см. `markdown::extract_summary`).
    pub summary: Option<String>,
    /// Полное markdown-тело документации (всё после summary).
    pub description: Option<String>,
    /// Plan 45 Ф.22.1 / D105: module-level deprecation.
    pub deprecation: Option<Deprecation>,
    /// Plan 45 Ф.22.1 / D105: module-level stability tier. Propagates
    /// на items без явного override (см. `propagate_stability` pass).
    pub stability: Option<Stability>,
    /// Plan 45 Ф.22.1 / D105: module-level `#hide_doc` — модуль
    /// exported, но скрыт из nova doc output.
    pub hide_doc: bool,
    /// Items этого модуля.
    pub items: Vec<DocItem>,
    /// Span первого токена модуля — для "View Source" links (D107).
    pub source_span: Span,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModuleKind {
    Folder,
    File,
}

/// Видимость item'а в JSON-выводе.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Visibility {
    Export,
    Private,
}

/// Один документированный item.
#[derive(Debug, Clone)]
pub struct DocItem {
    /// Stable ID, например `std.collections.range::Range` или
    /// `std.collections.range::Range.map`.
    pub id: String,
    /// `path` модуля (для multi-module DocTree).
    pub module_path: Vec<String>,
    /// Имя item'а (последний segment `id` после `::`).
    pub name: String,
    /// Видимость: `Export` если у item'а `is_export = true`, иначе
    /// `Private`. По дефолту `nova doc` рендерит только Export.
    pub visibility: Visibility,
    /// Summary — первое предложение из doc-content'а.
    pub summary: Option<String>,
    /// Plan 45 Ф.5 / D107: текст description'а ДО первого распознанного
    /// section heading'а. Полное markdown-тело item'а = `description` +
    /// все sections, склеенные обратно (consumer'у решать как
    /// рендерить).
    pub description: Option<String>,
    /// Plan 45 Ф.5 / D107: распарсенные стандартные секции
    /// (`# Examples` / `# Errors` / `# Panics` / `# Safety` / `# Effects`
    /// / `# Contracts` / `# Since` / `# See also` / `# Deprecated`).
    /// Ключи lowercase для канонического JSON output. `BTreeMap` для
    /// детерминистического порядка.
    pub sections: std::collections::BTreeMap<String, String>,
    /// Plan 45 Ф.3 / D105: structured deprecation marker. Заполняется
    /// `propagate_stability` pass'ом из `# Deprecated` section или
    /// `#deprecated` doc-attr.
    pub deprecation: Option<Deprecation>,
    /// Plan 45 Ф.3 / D105: structured stability marker. Заполняется
    /// `propagate_stability` pass'ом из `#stable` / `#unstable` /
    /// `#experimental` doc-attr или `# Since` section.
    pub stability: Option<Stability>,
    /// Plan 45 Ф.3 / D105: search-aliases из `#doc_alias("a", "b")`.
    pub aliases: Vec<String>,
    /// Plan 45 Ф.3 / D105: `#hide_doc` — item экспортирован, но не
    /// рендерится. `strip_private` фильтрует такие items.
    pub hide_doc: bool,
    /// Plan 45 Ф.3 / D105: `#doc(test_handlers = "path")` — handler-path
    /// для doc-test'ов. Сохраняется для будущего runner-integration.
    pub doc_test_handlers: Option<String>,
    /// Plan 45 Ф.23.3 / D63/D64: capability annotations (realtime, pure).
    pub capabilities: Capabilities,
    /// Kind-discriminator + специфичные поля.
    pub kind: ItemKind,
    /// Span декларации в исходнике — для "View Source".
    pub source_span: Span,
    /// Plan 45 Ф.23.11: peer-file attribution — имя файла (basename)
    /// откуда пришёл item (в folder-module режиме). None в single-file.
    pub peer_file: Option<String>,
    /// Plan 45 Ф.23.23: back-links — IDs items, которые ссылаются на этот
    /// item через intra-doc-link. Заполняется `resolve_intra_doc_links` pass'ом.
    pub linked_from: Vec<String>,
}

/// Plan 45 Ф.23.3 / D63/D64: capability annotations на item.
#[derive(Debug, Clone, Default)]
pub struct Capabilities {
    /// `@realtime` или `@realtime nogc` — функция гарантирует realtime-safe execution.
    pub realtime: bool,
    /// `@realtime nogc` — дополнительно без GC.
    pub realtime_nogc: bool,
    /// `@pure` / выведенная `Pure` — функция без side effects.
    pub pure_fn: bool,
    /// Module-level `#forbid X, Y` — запрещённые effects (из module attrs).
    pub forbid: Vec<String>,
}

/// Plan 45 Ф.3 / D105: deprecation marker.
#[derive(Debug, Clone)]
pub struct Deprecation {
    /// Свободный текст с объяснением + рекомендацией замены.
    pub note: String,
    /// Optional `since` version если упомянута в note или явно.
    pub since: Option<String>,
    /// Plan 45 Ф.23.6 / D105: `until` version — планируемое удаление.
    pub until: Option<String>,
}

/// Plan 45 Ф.3 / D105: stability tier.
#[derive(Debug, Clone)]
pub struct Stability {
    pub tier: StabilityTier,
    /// Версия, с которой действует tier (если известна).
    pub since: Option<String>,
    /// Plan 45 Ф.22.2 / D105: для `#unstable(feature = "name")` —
    /// имя feature-флага. Consumer (build-system) использует для
    /// gate'а use-сайтов. `None` для stable/experimental.
    pub feature: Option<String>,
    /// Plan 45 Ф.22.2 / D105: для `#experimental(note = "...")` —
    /// free-form объяснение, что может измениться. `None` для
    /// stable/unstable.
    pub note: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StabilityTier {
    /// `#stable` или derived из `# Since` (semver ≥ 1.0).
    Stable,
    /// `#unstable` — может измениться без notice.
    Unstable,
    /// `#experimental` — preview-quality, не для production.
    Experimental,
}

impl StabilityTier {
    pub fn as_str(self) -> &'static str {
        match self {
            StabilityTier::Stable => "stable",
            StabilityTier::Unstable => "unstable",
            StabilityTier::Experimental => "experimental",
        }
    }
}

/// Tagged union по D107 §«Item shape».
#[derive(Debug, Clone)]
pub enum ItemKind {
    /// Свободная функция или метод.
    Fn(Signature),
    /// Record/Sum/Protocol/Alias.
    Type(TypeDefinition),
    /// Константа.
    Const {
        ty: String,
        value: String,
    },
    /// Effect-декларация (D62).
    Effect {
        methods: Vec<EffectMethodSig>,
        /// Plan 45 Ф.22.4 / D107: axioms эффекта (по D24). Пустой Vec
        /// если нет axioms (большинство эффектов).
        axioms: Vec<EffectAxiomDoc>,
    },
    /// Protocol-декларация (D72).
    Protocol {
        methods: Vec<ProtocolMethodSig>,
    },
}

/// Plan 45 Ф.23.8 / D62: структурная запись effect'а в signature.
#[derive(Debug, Clone)]
pub struct EffectEntry {
    /// Rendered name (e.g. "Fs", "Fail[IoError]", "(E)" for row-var).
    pub name: String,
    /// Stable target_id если effect задокументирован как DocItem.
    /// `None` если row-var или неизвестный effect.
    pub target_id: Option<String>,
    /// Summary из DocItem если доступен (workspace mode). Иначе None.
    pub summary: Option<String>,
    /// true если это row-variable, а не конкретный effect.
    pub is_row_var: bool,
}

/// Plan 45 Ф.23.1 / D24/D106: один contract-clause функции в doc-output.
#[derive(Debug, Clone)]
pub struct ContractDoc {
    /// "requires" | "ensures" | "ensures_fail" | "decreases"
    pub kind: String,
    /// Выражение-контракт, рендерёное как Nova source.
    pub expr: String,
}

/// Plan 45 Ф.23.2 / D106: статус SMT-верификации функции.
#[derive(Debug, Clone)]
pub enum VerifyStatus {
    /// Верификация не запускалась (функция без контрактов или `@unverified`).
    NotAttempted,
    /// Все контракты доказаны SMT-solver'ом.
    Proven,
    /// Solver нашёл контрпример. Содержит краткое описание.
    HasCounterexample(String),
    /// Solver вышел по таймауту.
    Timeout,
}

impl VerifyStatus {
    pub fn as_str(&self) -> &str {
        match self {
            VerifyStatus::NotAttempted => "not_attempted",
            VerifyStatus::Proven => "proven",
            VerifyStatus::HasCounterexample(_) => "has_counterexample",
            VerifyStatus::Timeout => "timeout",
        }
    }
}

/// Сигнатура функции/метода (D107 §«Signature shape»).
///
/// Типы рендерятся как Nova source (`String`), не как структурные AST.
#[derive(Debug, Clone)]
pub struct Signature {
    /// Receiver: `None` для свободных функций;
    /// `Some(...)` для instance/static-методов.
    pub receiver: Option<Receiver>,
    /// Generic-параметры.
    pub generics: Vec<GenericParam>,
    /// Параметры функции.
    pub params: Vec<Param>,
    /// Return type, рендерёный как Nova source.
    pub return_type: String,
    /// Effect-row: список effect-имён (alphabetical для детерминизма).
    /// Plan 45 Ф.23.8: structured entries с target_id для cross-linking.
    pub effects: Vec<EffectEntry>,
    /// `Fail[X]`-варианты, извлечённые из effect-row.
    pub raises: Vec<String>,
    /// Plan 45 Ф.23.1 / D24/D106: контракты функции
    /// (requires/ensures/ensures_fail/decreases).
    pub contracts: Vec<ContractDoc>,
    /// Plan 45 Ф.23.2 / D106: статус SMT-верификации.
    pub verify_status: VerifyStatus,
}

#[derive(Debug, Clone)]
pub struct Receiver {
    pub type_name: String,
    pub kind: ReceiverKind,
    pub mutable: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReceiverKind {
    Instance,
    Static,
}

#[derive(Debug, Clone)]
pub struct GenericParam {
    pub name: String,
    pub bound: Option<String>,
    pub default: Option<String>,
}

#[derive(Debug, Clone)]
pub struct Param {
    pub name: String,
    /// Тип, рендерёный как Nova source.
    pub ty: String,
    /// Default value (выражение, рендерёное как Nova source) — `None`
    /// если параметр обязателен.
    pub default: Option<String>,
    /// `true` если параметр — variadic (D69).
    pub variadic: bool,
    /// Plan 50 (ревизия D102): параметр с дефолтом передаётся только
    /// по имени. `true` ⇔ `default.is_some()`.
    pub keyword_only: bool,
}

/// Определение типа.
#[derive(Debug, Clone)]
pub enum TypeDefinition {
    Record(Vec<RecordField>),
    Sum(Vec<SumVariant>),
    /// `type Alias = T` (D85 type-aliases).
    Alias(String),
    /// Plan 45 Ф.23.10 / D107: `type Email = newtype str` — newtype wrapper.
    Newtype {
        /// Внутренний тип, рендерёный как Nova source.
        inner: String,
    },
}

#[derive(Debug, Clone)]
pub struct RecordField {
    pub name: String,
    /// Тип, рендерёный как Nova source.
    pub ty: String,
    pub mutable: bool,
}

#[derive(Debug, Clone)]
pub struct SumVariant {
    pub name: String,
    pub payload: VariantPayload,
}

#[derive(Debug, Clone)]
pub enum VariantPayload {
    Unit,
    Tuple(Vec<String>),
    Record(Vec<RecordField>),
}

/// Plan 45 Ф.22.4 / D107: axiom effect'а — formula-constraint на
/// `pure_view` операции (D24). `formula` рендерится как Nova source.
#[derive(Debug, Clone)]
pub struct EffectAxiomDoc {
    pub name: String,
    /// Строка-формула, рендерёная как Nova source (best-effort).
    pub formula: String,
}

#[derive(Debug, Clone)]
pub struct EffectMethodSig {
    pub name: String,
    pub params: Vec<Param>,
    pub return_type: String,
}

#[derive(Debug, Clone)]
pub struct ProtocolMethodSig {
    pub name: String,
    pub params: Vec<Param>,
    pub return_type: String,
}

impl DocItem {
    /// Возвращает doc-content (если есть), извлекая raw markdown из
    /// summary + description. Используется renderer'ами при рендеринге
    /// без разбиения.
    pub fn doc_text(&self) -> Option<String> {
        match (&self.summary, &self.description) {
            (Some(s), Some(d)) => Some(format!("{}\n\n{}", s, d)),
            (Some(s), None) => Some(s.clone()),
            (None, Some(d)) => Some(d.clone()),
            (None, None) => None,
        }
    }
}

/// Plan 45 Ф.5: разбить `DocBlock.content` на (summary, description,
/// sections). Используется collector'ом для всех item'ов.
///
/// - **summary** — первое предложение (см. `markdown::extract_summary`).
/// - **description** — текст после summary, до первого распознанного
///   section heading'а.
/// - **sections** — стандартные секции (по D107 §«Item shape»):
///   examples / errors / panics / safety / effects / contracts /
///   since / see also / deprecated.
pub fn split_doc(
    doc: &Option<DocBlock>,
) -> (Option<String>, Option<String>, std::collections::BTreeMap<String, String>) {
    match doc {
        None => (None, None, std::collections::BTreeMap::new()),
        Some(b) => {
            let (summary, body) = crate::doc::markdown::extract_summary(&b.content);
            // Из `body` извлекаем секции.
            let parsed = match &body {
                Some(text) => crate::doc::markdown::split_sections(text),
                None => crate::doc::markdown::ParsedBody::default(),
            };
            (summary, parsed.intro, parsed.sections)
        }
    }
}
