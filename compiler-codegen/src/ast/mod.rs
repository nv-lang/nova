//! Типы AST.
//!
//! Минималистичный набор: всё что нужно для bootstrap'а Nova-on-Nova.
//! Не все фичи парсятся в детальном виде — `comptime`, contracts,
//! attributes пропускаются на уровне парсера.

use crate::diag::Span;

/// Корневой узел — модуль (файл).
#[derive(Debug, Clone)]
pub struct Module {
    pub name: Vec<String>, // module a.b.c → ["a", "b", "c"]
    pub imports: Vec<Import>,
    pub items: Vec<Item>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct Import {
    pub path: Vec<String>,
    pub alias: Option<String>,
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
}

/// Функция: и свободная, и метод (через `receiver`).
#[derive(Debug, Clone)]
pub struct FnDecl {
    pub is_export: bool,
    pub name: String,
    /// Receiver — для методов через `@`. None у свободных функций.
    pub receiver: Option<Receiver>,
    pub generics: Vec<String>, // [T, U] параметры
    pub params: Vec<Param>,
    pub effects: Vec<TypeRef>, // эффекты между `)` и `->`
    pub return_type: Option<TypeRef>,
    pub body: FnBody,
    pub span: Span,
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
}

#[derive(Debug, Clone)]
pub enum FnBody {
    /// `=> expr`
    Expr(Expr),
    /// `{ stmts; ...; expr? }`
    Block(Block),
}

#[derive(Debug, Clone)]
pub struct TypeDecl {
    pub is_export: bool,
    pub name: String,
    pub generics: Vec<String>,
    pub kind: TypeDeclKind,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub enum TypeDeclKind {
    /// `type Name { fields }`
    Record(Vec<RecordField>),
    /// `type Name | A | B(int) | C { x int }` (D52)
    Sum(Vec<SumVariant>),
    /// `type Name effect { signatures }`
    Effect(Vec<EffectMethod>),
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
    pub generics: Vec<String>,
    pub params: Vec<Param>,
    pub effects: Vec<TypeRef>,
    pub return_type: Option<TypeRef>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct LetDecl {
    pub mutable: bool,
    pub pattern: Pattern,
    pub ty: Option<TypeRef>,
    pub value: Expr,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct ConstDecl {
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
}

#[derive(Debug, Clone)]
pub enum ExprKind {
    // Литералы
    IntLit(i64),
    FloatLit(f64),
    StrLit(String),
    BoolLit(bool),
    UnitLit,
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

    // Вызовы
    Call {
        func: Box<Expr>,
        args: Vec<Expr>,
        /// trailing-block: D43
        trailing_block: Option<TrailingBlock>,
    },
    /// `expr?` — пробрасывание Fail (D25/D65)
    Try(Box<Expr>),
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
    },
    /// `while let pattern = expr { ... }` — D34
    WhileLet {
        pattern: Pattern,
        scrutinee: Box<Expr>,
        body: Block,
    },
    Loop {
        body: Block,
    },

    // Функции и handlers
    /// `(a, b) => expr` — лямбда (D22, строго `=> expr`)
    Lambda {
        params: Vec<LambdaParam>,
        effects: Vec<TypeRef>,
        return_type: Option<TypeRef>,
        body: Box<Expr>,
    },
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
    /// Блок-выражение `{ stmts; expr }`
    Block(Block),
    /// `spawn body` — D50
    Spawn(Box<Expr>),
    /// `supervised { body }` — structured-concurrency scope (D50)
    Supervised(Block),
    /// `detach { body }` — fire-and-forget, global supervisor (D50).
    /// Requires `Detach` effect in the enclosing function's signature.
    Detach(Block),
    /// `cancel_scope { tok => body }` — D75 manual structured cancellation.
    /// Same as `supervised` but exposes a `CancelToken` binding so external
    /// code can call `tok.cancel()` to fail-fast all fibers in the scope.
    CancelScope {
        token_name: String,
        body: Block,
    },

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

#[derive(Debug, Clone)]
pub struct WithBinding {
    pub effect: TypeRef,
    pub handler: Expr,
    pub span: Span,
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
            | Pattern::Binding { span: s, .. } => *s,
        }
    }
}
