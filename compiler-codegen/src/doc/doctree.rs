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
}

impl DocTree {
    pub fn new() -> Self {
        Self {
            format_version: 1,
            modules: Vec::new(),
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
    /// Полное markdown-тело документации.
    pub description: Option<String>,
    /// Kind-discriminator + специфичные поля.
    pub kind: ItemKind,
    /// Span декларации в исходнике — для "View Source".
    pub source_span: Span,
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
    },
    /// Protocol-декларация (D72).
    Protocol {
        methods: Vec<ProtocolMethodSig>,
    },
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
    pub effects: Vec<String>,
    /// `Fail[X]`-варианты, извлечённые из effect-row.
    pub raises: Vec<String>,
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

/// Helper для построения `summary` / `description` из `DocBlock`.
/// Использует `markdown::extract_summary` для разбиения первого
/// предложения и остального тела.
pub fn split_doc(doc: &Option<DocBlock>) -> (Option<String>, Option<String>) {
    match doc {
        None => (None, None),
        Some(b) => crate::doc::markdown::extract_summary(&b.content),
    }
}
