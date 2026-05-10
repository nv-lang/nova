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
    KwHandler,
    KwAlias,
    KwLet,
    KwConst,
    KwMut,
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
    KwCancelScope,
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
            TokenKind::KwModule => "`module`",
            TokenKind::KwImport => "`import`",
            TokenKind::KwUse => "`use`",
            TokenKind::KwExport => "`export`",
            TokenKind::KwExternal => "`external`",
            TokenKind::KwFn => "`fn`",
            TokenKind::KwType => "`type`",
            TokenKind::KwProtocol => "`protocol`",
            TokenKind::KwEffect => "`effect`",
            TokenKind::KwHandler => "`handler`",
            TokenKind::KwAlias => "`alias`",
            TokenKind::KwLet => "`let`",
            TokenKind::KwConst => "`const`",
            TokenKind::KwMut => "`mut`",
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
            TokenKind::KwCancelScope => "`cancel_scope`",
            TokenKind::Char(_) => "char literal",
            TokenKind::KwInterrupt => "`interrupt`",
            TokenKind::KwForbid => "`forbid`",
            TokenKind::KwRealtime => "`realtime`",
            TokenKind::KwAnd => "`and`",
            TokenKind::KwOr => "`or`",
            TokenKind::KwNot => "`not`",
            TokenKind::KwDefer => "`defer`",
            TokenKind::KwErrDefer => "`errdefer`",
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
