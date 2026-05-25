//! Токены лексера.

use crate::diag::Span;

/// Один токен — kind + span.
#[derive(Debug, Clone, PartialEq)]
pub struct Token {
    pub kind: TokenKind,
    pub span: Span,
}

impl Token {
    pub fn new(kind: TokenKind, span: Span) -> Self {
        Self { kind, span }
    }
}

/// Plan 45 / D104: kind doc-comment'а.
///
/// `Outer` (`///`) — внешний doc-comment, привязывается к следующей
/// декларации (fn / type / const / effect / handler / protocol).
/// `Inner` (`//!`) — внутренний, привязывается к окружающему модулю;
/// валиден только в начале файла (после `module X` и любых `import`,
/// до первой декларации).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DocCommentKind {
    Outer,
    Inner,
}

/// Виды токенов. Все ключевые слова — отдельные варианты, иначе
/// `Ident(string)`.
#[derive(Debug, Clone, PartialEq)]
pub enum TokenKind {
    // Литералы и идентификаторы
    Int(i64),
    Float(f64),
    Str(String),
    Backtick(String),
    Ident(String),
    /// Plan 45 / D104: doc-comment. `///` (Outer) — внешний, к следующей
    /// декларации; `//!` (Inner) — внутренний модуля.
    ///
    /// `content` — склеенный текст одной или нескольких подряд идущих
    /// doc-line того же kind'а. С каждой строки снят префикс `///`/`//!`
    /// и одна опциональная ведущая пробел-позиция; затем убран общий
    /// leading whitespace (по всем непустым строкам). Строки склеены
    /// через `\n`. Markdown на этом уровне НЕ парсится — это сырой
    /// текст, передаётся в parser и далее в Plan 45 doc-collector.
    DocComment {
        kind: DocCommentKind,
        content: String,
    },

    // Ключевые слова
    KwModule,
    KwImport,
    KwUse,
    KwExport,
    /// D82: external fn — runtime-implemented в nova_rt/*.h.
    /// Только в std.runtime.* whitelisted namespace.
    KwExternal,
    KwFn,
    KwType,
    KwProtocol,
    KwEffect,
    // Plan 97 Ф.3 (D142): `KwHandler` снят (clean break). Литерал
    // handler'а — `effect X { ... }`. См. parse_atom + parse_handler_lit.
    KwAlias,
    KwLet,
    KwConst,
    KwMut,
    /// Plan 73 (D131): `consume` — consuming receiver/param qualifier.
    KwConsume,
    KwReadonly,
    KwIf,
    KwElse,
    KwMatch,
    KwFor,
    KwWhile,
    KwLoop,
    KwIn,
    KwReturn,
    KwBreak,
    KwContinue,
    KwTest,
    KwTrue,
    KwFalse,
    KwWith,
    KwThrow,
    KwAs,
    KwIs,
    KwSpawn,
    KwSupervised,
    KwParallel,
    KwDetach,
    /// Plan 83.3 (D50): `blocking { body }` — увод leaf-блокирующей
    /// работы (FFI/syscall) в libuv threadpool, чтобы не пинить
    /// M:N-worker. Требует эффект `Blocking` в сигнатуре enclosing-fn.
    KwBlocking,
    /// Q-char-literals: 'a' / '\n' / '\u{1F600}' — Unicode codepoint
    Char(u32),
    KwInterrupt,
    KwForbid,
    KwRealtime,
    KwAnd,
    KwOr,
    KwNot,
    /// D90: scope-level cleanup statement. `defer body` запускается на
    /// любом exit (normal/return/throw/panic/interrupt).
    KwDefer,
    /// D90: error-only cleanup. `errdefer body` запускается только на
    /// throw/panic-exit, не на normal/return/interrupt.
    KwErrDefer,
    /// D160 Plan 100.4.3: success-only cleanup. `okdefer body` запускается
    /// ТОЛЬКО на success-path (normal end-of-scope или `return expr`);
    /// skipped при throw/panic/interrupt. Complement к `errdefer`.
    KwOkDefer,
    /// D94: multiplexed channel operation.
    KwSelect,
    /// Plan 33.5 Ф.4.1: `lemma` — proof term декларация (верифицируется SMT,
    /// не emit'ится в runtime). Семантика как у ghost fn с обязательным verify.
    KwLemma,
    /// Plan 33.5 Ф.4.1: `apply lemma_name(args)` — активировать lemma в текущем
    /// SMT-scope. Adds lemma.ensures как assertion; не emit'ится в runtime.
    KwApply,

    // Пунктуация
    LParen,
    RParen,
    LBracket,
    RBracket,
    LBrace,
    RBrace,
    Comma,
    Semicolon,
    Colon,
    At,
    /// `#` — префикс для function/type/module attributes
    /// (`#realtime`, `#pure`, `#must_verify` и т.д.).
    /// Plan 33.1: мигрировано с `@` на `#` чтобы разделить attribute-
    /// prefix от receiver-prefix `@` (см. D-NN attribute syntax).
    Hash,
    Dot,
    DotDot,
    DotDotEq,
    DotDotDot,
    Question,
    Question2,

    // Операторы
    Plus,
    Minus,
    Star,
    Slash,
    Percent,
    Eq,
    PlusEq,
    MinusEq,
    StarEq,
    SlashEq,
    EqEq,
    BangEq,
    Lt,
    Le,
    Gt,
    Ge,
    Bang,
    AmpAmp,
    PipePipe,
    Pipe,
    /// `&` — bitwise and (D-operators)
    Amp,
    /// `^` — bitwise xor
    Caret,
    /// `<<` — left shift
    Shl,
    /// `>>` — right shift
    Shr,
    FatArrow,
    Arrow,
    /// Plan 33.1 (D24): `==>` — логическая импликация в контрактах.
    /// `A ==> B` ≡ `!A || B`. Правоассоциативный, приоритет ниже `||`.
    Implies,
    /// Plan 33.1 (D24): `<==>` — логическая эквивалентность в контрактах.
    /// `A <==> B` ≡ `A == B` для bool. Приоритет такой же как `==>`.
    Iff,

    // Структурные
    Newline,
    Eof,
}

impl TokenKind {
    /// Человеко-читаемое имя — для сообщений об ошибках парсера.
    pub fn name(&self) -> &'static str {
        match self {
            TokenKind::Int(_) => "int literal",
            TokenKind::Float(_) => "float literal",
            TokenKind::Str(_) => "string literal",
            TokenKind::Backtick(_) => "backtick string",
            TokenKind::Ident(_) => "identifier",
            TokenKind::DocComment { kind: DocCommentKind::Outer, .. } => "outer doc-comment `///`",
            TokenKind::DocComment { kind: DocCommentKind::Inner, .. } => "inner doc-comment `//!`",
            TokenKind::KwModule => "`module`",
            TokenKind::KwImport => "`import`",
            TokenKind::KwUse => "`use`",
            TokenKind::KwExport => "`export`",
            TokenKind::KwExternal => "`external`",
            TokenKind::KwFn => "`fn`",
            TokenKind::KwType => "`type`",
            TokenKind::KwProtocol => "`protocol`",
            TokenKind::KwEffect => "`effect`",
            TokenKind::KwAlias => "`alias`",
            TokenKind::KwLet => "`let`",
            TokenKind::KwConst => "`const`",
            TokenKind::KwMut => "`mut`",
            TokenKind::KwConsume => "`consume`",
            TokenKind::KwReadonly => "`readonly`",
            TokenKind::KwIf => "`if`",
            TokenKind::KwElse => "`else`",
            TokenKind::KwMatch => "`match`",
            TokenKind::KwFor => "`for`",
            TokenKind::KwWhile => "`while`",
            TokenKind::KwLoop => "`loop`",
            TokenKind::KwIn => "`in`",
            TokenKind::KwReturn => "`return`",
            TokenKind::KwBreak => "`break`",
            TokenKind::KwContinue => "`continue`",
            TokenKind::KwTest => "`test`",
            TokenKind::KwTrue => "`true`",
            TokenKind::KwFalse => "`false`",
            TokenKind::KwWith => "`with`",
            TokenKind::KwThrow => "`throw`",
            TokenKind::KwAs => "`as`",
            TokenKind::KwIs => "`is`",
            TokenKind::KwSpawn => "`spawn`",
            TokenKind::KwSupervised => "`supervised`",
            TokenKind::KwParallel => "`parallel`",
            TokenKind::KwDetach => "`detach`",
            TokenKind::KwBlocking => "`blocking`",
            TokenKind::Char(_) => "char literal",
            TokenKind::KwInterrupt => "`interrupt`",
            TokenKind::KwForbid => "`forbid`",
            TokenKind::KwRealtime => "`realtime`",
            TokenKind::KwAnd => "`and`",
            TokenKind::KwOr => "`or`",
            TokenKind::KwNot => "`not`",
            TokenKind::KwDefer => "`defer`",
            TokenKind::KwErrDefer => "`errdefer`",
            TokenKind::KwOkDefer => "`okdefer`",
            TokenKind::KwSelect => "`select`",
            TokenKind::KwLemma => "`lemma`",
            TokenKind::KwApply => "`apply`",
            TokenKind::LParen => "`(`",
            TokenKind::RParen => "`)`",
            TokenKind::LBracket => "`[`",
            TokenKind::RBracket => "`]`",
            TokenKind::LBrace => "`{`",
            TokenKind::RBrace => "`}`",
            TokenKind::Comma => "`,`",
            TokenKind::Semicolon => "`;`",
            TokenKind::Colon => "`:`",
            TokenKind::At => "`@`",
            TokenKind::Hash => "`#`",
            TokenKind::Dot => "`.`",
            TokenKind::DotDot => "`..`",
            TokenKind::DotDotEq => "`..=`",
            TokenKind::DotDotDot => "`...`",
            TokenKind::Question => "`?`",
            TokenKind::Question2 => "`??`",
            TokenKind::Plus => "`+`",
            TokenKind::Minus => "`-`",
            TokenKind::Star => "`*`",
            TokenKind::Slash => "`/`",
            TokenKind::Percent => "`%`",
            TokenKind::Eq => "`=`",
            TokenKind::PlusEq => "`+=`",
            TokenKind::MinusEq => "`-=`",
            TokenKind::StarEq => "`*=`",
            TokenKind::SlashEq => "`/=`",
            TokenKind::EqEq => "`==`",
            TokenKind::BangEq => "`!=`",
            TokenKind::Lt => "`<`",
            TokenKind::Le => "`<=`",
            TokenKind::Gt => "`>`",
            TokenKind::Ge => "`>=`",
            TokenKind::Bang => "`!`",
            TokenKind::AmpAmp => "`&&`",
            TokenKind::PipePipe => "`||`",
            TokenKind::Pipe => "`|`",
            TokenKind::Amp => "`&`",
            TokenKind::Caret => "`^`",
            TokenKind::Shl => "`<<`",
            TokenKind::Shr => "`>>`",
            TokenKind::FatArrow => "`=>`",
            TokenKind::Arrow => "`->`",
            TokenKind::Implies => "`==>`",
            TokenKind::Iff => "`<==>`",
            TokenKind::Newline => "newline",
            TokenKind::Eof => "end of file",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::lex;
    use super::*;

    fn kinds(src: &str) -> Vec<TokenKind> {
        let toks = lex(src).unwrap();
        toks.into_iter()
            .filter(|t| !matches!(t.kind, TokenKind::Newline | TokenKind::Eof))
            .map(|t| t.kind)
            .collect()
    }

    #[test]
    fn int_literals() {
        assert_eq!(kinds("42"), vec![TokenKind::Int(42)]);
        assert_eq!(kinds("0xFF"), vec![TokenKind::Int(0xFF)]);
        assert_eq!(kinds("0b1010"), vec![TokenKind::Int(0b1010)]);
        assert_eq!(kinds("0o755"), vec![TokenKind::Int(0o755)]);
        assert_eq!(kinds("1_000"), vec![TokenKind::Int(1000)]);
    }

    #[test]
    fn float_literals() {
        let TokenKind::Float(v) = &kinds("3.14")[0] else {
            panic!()
        };
        assert!((v - 3.14).abs() < 1e-9);
        let TokenKind::Float(v) = &kinds("1e3")[0] else {
            panic!()
        };
        assert!((v - 1000.0).abs() < 1e-9);
    }

    #[test]
    fn strings() {
        assert_eq!(kinds(r#""hello""#), vec![TokenKind::Str("hello".into())]);
        assert_eq!(
            kinds(r#""hi\nyou""#),
            vec![TokenKind::Str("hi\nyou".into())]
        );
    }

    #[test]
    fn backtick() {
        assert_eq!(
            kinds("`SELECT 1`"),
            vec![TokenKind::Backtick("SELECT 1".into())]
        );
    }

    #[test]
    fn keywords_vs_idents() {
        assert!(matches!(kinds("fn")[0], TokenKind::KwFn));
        let TokenKind::Ident(name) = &kinds("foo")[0] else {
            panic!()
        };
        assert_eq!(name, "foo");
    }

    #[test]
    fn operators() {
        let toks = kinds("a + b == c => d");
        assert_eq!(
            toks,
            vec![
                TokenKind::Ident("a".into()),
                TokenKind::Plus,
                TokenKind::Ident("b".into()),
                TokenKind::EqEq,
                TokenKind::Ident("c".into()),
                TokenKind::FatArrow,
                TokenKind::Ident("d".into()),
            ]
        );
    }

    #[test]
    fn dot_variants() {
        let toks = kinds("a.b 0..5 ...rest");
        assert_eq!(
            toks,
            vec![
                TokenKind::Ident("a".into()),
                TokenKind::Dot,
                TokenKind::Ident("b".into()),
                TokenKind::Int(0),
                TokenKind::DotDot,
                TokenKind::Int(5),
                TokenKind::DotDotDot,
                TokenKind::Ident("rest".into()),
            ]
        );
    }

    #[test]
    fn comments_skipped() {
        assert_eq!(
            kinds("a // line comment\n+ b"),
            vec![
                TokenKind::Ident("a".into()),
                TokenKind::Plus,
                TokenKind::Ident("b".into()),
            ]
        );
    }

    #[test]
    fn newline_preserved() {
        let toks = lex("a\nb").unwrap();
        // Newline должен сохраняться (D49 — разделитель statement'ов)
        let any_newline = toks.iter().any(|t| matches!(t.kind, TokenKind::Newline));
        assert!(any_newline);
    }
}
