//! Recursive-descent parser для Nova.
//!
//! Один большой модуль: `Parser` — состояние с указателем на токены,
//! методы для каждого нетерминала. Никаких внешних парсер-комбинаторов:
//! минимум зависимостей в bootstrap'е.

use crate::ast::*;
use crate::diag::{Diagnostic, Span};
use crate::lexer::{Token, TokenKind};

pub struct Parser {
    tokens: Vec<Token>,
    pos: usize,
    /// Когда true — `Ident { ... }` не парсится как record-литерал
    /// (используется в head-позициях `if`/`while`/`match`-scrutinee
    /// и `for`-итераторах, чтобы `{` следующего блока не съедался).
    no_struct_lit: bool,
    /// Оригинальный текст для обратной выборки (используется в
    /// `.n.m`-positional-tuple-access, где Float-токен нужно
    /// расщепить обратно в две части).
    src: String,
}

impl Parser {
    pub fn new(tokens: Vec<Token>) -> Self {
        Self::with_src(tokens, String::new())
    }

    pub fn with_src(tokens: Vec<Token>, src: String) -> Self {
        Self {
            tokens,
            pos: 0,
            no_struct_lit: false,
            src,
        }
    }

    fn src_substring(&self, span: Span) -> String {
        if self.src.is_empty() {
            // Парсер был создан без src — fallback к синтезу из Float
            return String::new();
        }
        self.src
            .get(span.start..span.end)
            .map(|s| s.to_string())
            .unwrap_or_default()
    }

    fn with_no_struct_lit<R>(
        &mut self,
        f: impl FnOnce(&mut Self) -> Result<R, Diagnostic>,
    ) -> Result<R, Diagnostic> {
        let saved = self.no_struct_lit;
        self.no_struct_lit = true;
        let result = f(self);
        self.no_struct_lit = saved;
        result
    }

    /// Точка входа: парсит модуль (файл целиком).
    pub fn parse_module(&mut self) -> Result<Module, Diagnostic> {
        self.skip_newlines();
        let start = self.peek().span;

        // module keyword.path
        let module_name = if self.eat(&TokenKind::KwModule).is_some() {
            let path = self.parse_dotted_path()?;
            self.expect_newline_or_eof()?;
            path
        } else {
            Vec::new()
        };

        let mut imports = Vec::new();
        let mut items = Vec::new();
        loop {
            self.skip_newlines();
            if matches!(self.peek().kind, TokenKind::Eof) {
                break;
            }
            if matches!(self.peek().kind, TokenKind::KwImport | TokenKind::KwUse) {
                imports.push(self.parse_import()?);
                continue;
            }
            items.push(self.parse_item()?);
        }
        let span = start.merge(self.peek().span);
        Ok(Module {
            name: module_name,
            imports,
            items,
            span,
        })
    }

    // ─── helpers ─────────────────────────────────────────────────────────

    fn peek(&self) -> &Token {
        &self.tokens[self.pos]
    }

    fn peek_at(&self, offset: usize) -> &Token {
        if self.pos + offset >= self.tokens.len() {
            self.tokens.last().unwrap()
        } else {
            &self.tokens[self.pos + offset]
        }
    }

    fn bump(&mut self) -> Token {
        let t = self.tokens[self.pos].clone();
        if self.pos + 1 < self.tokens.len() {
            self.pos += 1;
        }
        t
    }

    fn eat(&mut self, kind: &TokenKind) -> Option<Token> {
        if std::mem::discriminant(&self.peek().kind) == std::mem::discriminant(kind) {
            Some(self.bump())
        } else {
            None
        }
    }

    fn expect(&mut self, kind: &TokenKind) -> Result<Token, Diagnostic> {
        if std::mem::discriminant(&self.peek().kind) == std::mem::discriminant(kind) {
            Ok(self.bump())
        } else {
            let span = self.peek().span;
            let actual = self.peek().kind.name();
            Err(Diagnostic::new(
                format!("expected {}, got {}", kind.name(), actual),
                span,
            ))
        }
    }

    fn skip_newlines(&mut self) {
        while matches!(self.peek().kind, TokenKind::Newline | TokenKind::Semicolon) {
            self.bump();
        }
    }

    fn at_newline(&self) -> bool {
        matches!(
            self.peek().kind,
            TokenKind::Newline | TokenKind::Semicolon | TokenKind::Eof
        )
    }

    fn expect_newline_or_eof(&mut self) -> Result<(), Diagnostic> {
        match self.peek().kind {
            TokenKind::Newline | TokenKind::Semicolon => {
                self.bump();
                Ok(())
            }
            TokenKind::Eof => Ok(()),
            _ => {
                let span = self.peek().span;
                Err(Diagnostic::new(
                    format!(
                        "expected newline or end of input, got {}",
                        self.peek().kind.name()
                    ),
                    span,
                ))
            }
        }
    }

    fn parse_ident(&mut self) -> Result<(String, Span), Diagnostic> {
        let span = self.peek().span;
        match &self.peek().kind {
            TokenKind::Ident(s) => {
                let name = s.clone();
                self.bump();
                Ok((name, span))
            }
            other => Err(Diagnostic::new(
                format!("expected identifier, got {}", other.name()),
                span,
            )),
        }
    }

    fn parse_dotted_path(&mut self) -> Result<Vec<String>, Diagnostic> {
        let mut parts = Vec::new();
        let (first, _) = self.parse_ident()?;
        parts.push(first);
        while self.eat(&TokenKind::Dot).is_some() {
            let (next, _) = self.parse_ident()?;
            parts.push(next);
        }
        Ok(parts)
    }

    // ─── imports ─────────────────────────────────────────────────────────

    fn parse_import(&mut self) -> Result<Import, Diagnostic> {
        let start = self.peek().span;
        // Принимаем как `import`, так и `use` — оба парсятся идентично:
        // `use` будет использоваться для embedding (D39), но в bootstrap
        // мы не различаем.
        self.bump();
        let path = self.parse_dotted_path()?;
        let alias = if matches!(self.peek().kind, TokenKind::KwAs) {
            self.bump();
            let (name, _) = self.parse_ident()?;
            Some(name)
        } else {
            None
        };
        self.expect_newline_or_eof()?;
        let span = start.merge(self.tokens[self.pos.saturating_sub(1)].span);
        Ok(Import { path, alias, span })
    }

    // ─── top-level items ─────────────────────────────────────────────────

    fn parse_item(&mut self) -> Result<Item, Diagnostic> {
        let is_export = self.eat(&TokenKind::KwExport).is_some();
        match self.peek().kind {
            TokenKind::KwFn => Ok(Item::Fn(self.parse_fn(is_export)?)),
            TokenKind::KwType => Ok(Item::Type(self.parse_type_decl(is_export)?)),
            TokenKind::KwLet => Ok(Item::Let(self.parse_let_decl()?)),
            TokenKind::KwConst => Ok(Item::Const(self.parse_const_decl(is_export)?)),
            TokenKind::KwTest if !is_export => Ok(Item::Test(self.parse_test_decl()?)),
            _ => {
                let span = self.peek().span;
                Err(Diagnostic::new(
                    format!(
                        "expected fn / type / let / const / test, got {}",
                        self.peek().kind.name()
                    ),
                    span,
                ))
            }
        }
    }

    // ─── fn ──────────────────────────────────────────────────────────────

    fn parse_fn(&mut self, is_export: bool) -> Result<FnDecl, Diagnostic> {
        let start = self.peek().span;
        self.expect(&TokenKind::KwFn)?;

        // Сначала парсим первый идентификатор. Это либо имя fn, либо
        // имя receiver-типа.
        let (first_ident, first_span) = self.parse_ident()?;

        // Если за ним `[`, `<`, `mut`, `@` или `.` — это receiver.
        let mut generics_first: Vec<TypeRef> = Vec::new();
        if matches!(self.peek().kind, TokenKind::LBracket) {
            // `Type[T] @method` или `funcName[T](...)` — посмотрим что дальше после `]`.
            // В bootstrap'е делаем простое разрешение: парсим generics, потом смотрим.
            generics_first = self.parse_type_args()?;
        }

        let receiver: Option<Receiver>;
        let name: String;
        let mut fn_generics: Vec<String> = Vec::new();
        let receiver_mut: bool;

        if matches!(self.peek().kind, TokenKind::KwMut)
            && matches!(
                self.peek_at(1).kind,
                TokenKind::At | TokenKind::Dot
            )
        {
            // `Type mut @method` или `Type mut .method` (на всякий случай)
            self.bump(); // mut
            receiver_mut = true;
            let kind = if matches!(self.peek().kind, TokenKind::At) {
                self.bump();
                ReceiverKind::Instance
            } else {
                self.bump();
                ReceiverKind::Static
            };
            receiver = Some(Receiver {
                type_name: first_ident.clone(),
                generics: generics_first,
                kind,
                mutable: receiver_mut,
                span: first_span,
            });
            let (n, _) = self.parse_ident()?;
            name = n;
        } else if matches!(self.peek().kind, TokenKind::At) {
            self.bump();
            receiver = Some(Receiver {
                type_name: first_ident.clone(),
                generics: generics_first,
                kind: ReceiverKind::Instance,
                mutable: false,
                span: first_span,
            });
            let (n, _) = self.parse_ident()?;
            name = n;
        } else if matches!(self.peek().kind, TokenKind::Dot) {
            self.bump();
            receiver = Some(Receiver {
                type_name: first_ident.clone(),
                generics: generics_first,
                kind: ReceiverKind::Static,
                mutable: false,
                span: first_span,
            });
            let (n, _) = self.parse_ident()?;
            name = n;
        } else {
            // Свободная функция: `fn name[T](...)`. В этом случае `generics_first`
            // — это generics функции, а `first_ident` — имя.
            receiver = None;
            name = first_ident;
            // generics_first собран как `Vec<TypeRef>`, но для свободной функции
            // нам нужны имена. Преобразуем (требуем чтобы каждый был Named без generics).
            for g in generics_first {
                if let TypeRef::Named { path, generics, .. } = g {
                    if generics.is_empty() && path.len() == 1 {
                        fn_generics.push(path.into_iter().next().unwrap());
                    } else {
                        return Err(Diagnostic::new(
                            "function generic parameter must be a simple identifier",
                            start,
                        ));
                    }
                } else {
                    return Err(Diagnostic::new(
                        "function generic parameter must be a simple identifier",
                        start,
                    ));
                }
            }
        }

        // Если у метода есть свои generics (D42 model B): `fn Repo[T] @bulk_load[K](...)`
        if receiver.is_some() && matches!(self.peek().kind, TokenKind::LBracket) {
            let method_generics_refs = self.parse_type_args()?;
            for g in method_generics_refs {
                if let TypeRef::Named { path, generics, .. } = g {
                    if generics.is_empty() && path.len() == 1 {
                        fn_generics.push(path.into_iter().next().unwrap());
                    } else {
                        return Err(Diagnostic::new(
                            "method generic parameter must be a simple identifier",
                            start,
                        ));
                    }
                } else {
                    return Err(Diagnostic::new(
                        "method generic parameter must be a simple identifier",
                        start,
                    ));
                }
            }
        }

        // (params)
        self.expect(&TokenKind::LParen)?;
        let mut params = Vec::new();
        while !matches!(self.peek().kind, TokenKind::RParen) {
            params.push(self.parse_param()?);
            if !matches!(self.peek().kind, TokenKind::RParen) {
                self.expect(&TokenKind::Comma)?;
                self.skip_newlines();
            }
        }
        self.expect(&TokenKind::RParen)?;

        // Effects: до `->` или до тела
        let effects = self.parse_effects_until_arrow_or_body()?;
        let return_type = if self.eat(&TokenKind::Arrow).is_some() {
            Some(self.parse_type()?)
        } else {
            None
        };

        // Тело: `=> expr` или `{ block }`
        let body = self.parse_fn_body()?;
        let end_span = match &body {
            FnBody::Expr(e) => e.span,
            FnBody::Block(b) => b.span,
        };
        Ok(FnDecl {
            is_export,
            name,
            receiver,
            generics: fn_generics,
            params,
            effects,
            return_type,
            body,
            span: start.merge(end_span),
        })
    }

    fn parse_param(&mut self) -> Result<Param, Diagnostic> {
        let (name, name_span) = self.parse_ident()?;
        let ty = self.parse_type()?;
        Ok(Param {
            name,
            ty: ty.clone(),
            span: name_span.merge(ty.span()),
        })
    }

    /// Парсит список эффектов между `)` и (`->` | `{` | `=>`).
    /// Эффект — TypeRef (обычно Named, но может быть с generics: Throws[E]).
    fn parse_effects_until_arrow_or_body(&mut self) -> Result<Vec<TypeRef>, Diagnostic> {
        let mut effects = Vec::new();
        loop {
            match self.peek().kind {
                TokenKind::Arrow | TokenKind::FatArrow | TokenKind::LBrace => break,
                TokenKind::Ident(_) => {
                    effects.push(self.parse_type()?);
                }
                _ => break,
            }
        }
        Ok(effects)
    }

    fn parse_fn_body(&mut self) -> Result<FnBody, Diagnostic> {
        match self.peek().kind {
            TokenKind::FatArrow => {
                self.bump();
                self.skip_newlines();
                let expr = self.parse_expr()?;
                Ok(FnBody::Expr(expr))
            }
            TokenKind::LBrace => Ok(FnBody::Block(self.parse_block()?)),
            _ => {
                let span = self.peek().span;
                Err(Diagnostic::new(
                    format!(
                        "expected `=>` or `{{` for function body, got {}",
                        self.peek().kind.name()
                    ),
                    span,
                ))
            }
        }
    }

    // ─── type declarations ───────────────────────────────────────────────

    fn parse_type_decl(&mut self, is_export: bool) -> Result<TypeDecl, Diagnostic> {
        let start = self.peek().span;
        self.expect(&TokenKind::KwType)?;
        let (name, _) = self.parse_ident()?;

        // generics: Repo[T, U]
        let mut generics: Vec<String> = Vec::new();
        if self.eat(&TokenKind::LBracket).is_some() {
            loop {
                let (n, _) = self.parse_ident()?;
                generics.push(n);
                if self.eat(&TokenKind::Comma).is_none() {
                    break;
                }
                self.skip_newlines();
            }
            self.expect(&TokenKind::RBracket)?;
        }

        // Тело типа может идти на следующей строке для multi-line sum'ов
        // и protocol'ов. Skip newlines перед body.
        self.skip_newlines();

        // Тело: `protocol { ... }` | `effect { ... }` | `alias TYPE` |
        // `{ fields }` | `| variant | variant` | `TYPE` (newtype) |
        // начинается с `|` для sum.
        //
        // protocol и effect семантически разные (D61), но в bootstrap-
        // интерпретаторе хранятся одинаково — оба дают TypeDeclKind::Protocol.
        // Различение на уровне type checker'а — задача будущего компилятора.
        let kind = match self.peek().kind {
            TokenKind::KwProtocol | TokenKind::KwEffect => {
                self.bump();
                self.expect(&TokenKind::LBrace)?;
                let methods = self.parse_protocol_methods()?;
                self.expect(&TokenKind::RBrace)?;
                TypeDeclKind::Protocol(methods)
            }
            TokenKind::KwAlias => {
                self.bump();
                let ty = self.parse_type()?;
                TypeDeclKind::Alias(ty)
            }
            TokenKind::LBrace => {
                self.bump();
                let fields = self.parse_record_fields()?;
                self.expect(&TokenKind::RBrace)?;
                TypeDeclKind::Record(fields)
            }
            TokenKind::Pipe => {
                let variants = self.parse_sum_variants()?;
                TypeDeclKind::Sum(variants)
            }
            // type Name OtherType — newtype
            _ => {
                let ty = self.parse_type()?;
                TypeDeclKind::Newtype(ty)
            }
        };
        // Sum-варианты сами съедают newlines в конце; для других форм
        // ожидаем разделитель.
        if !matches!(kind, TypeDeclKind::Sum(_)) {
            self.expect_newline_or_eof().ok();
        }
        let span = start.merge(self.tokens[self.pos.saturating_sub(1)].span);
        Ok(TypeDecl {
            is_export,
            name,
            generics,
            kind,
            span,
        })
    }

    fn parse_record_fields(&mut self) -> Result<Vec<RecordField>, Diagnostic> {
        let mut fields = Vec::new();
        self.skip_newlines();
        while !matches!(self.peek().kind, TokenKind::RBrace) {
            let mut readonly = false;
            let mut mutable = false;
            if self.eat(&TokenKind::KwReadonly).is_some() {
                readonly = true;
            } else if self.eat(&TokenKind::KwMut).is_some() {
                mutable = true;
            }
            let (name, name_span) = self.parse_ident()?;
            let ty = self.parse_type()?;
            fields.push(RecordField {
                name,
                ty: ty.clone(),
                readonly,
                mutable,
                span: name_span.merge(ty.span()),
            });
            // запятая или newline
            if self.eat(&TokenKind::Comma).is_some() {
                self.skip_newlines();
            } else {
                self.skip_newlines();
            }
        }
        Ok(fields)
    }

    fn parse_sum_variants(&mut self) -> Result<Vec<SumVariant>, Diagnostic> {
        let mut variants = Vec::new();
        self.skip_newlines();
        while matches!(self.peek().kind, TokenKind::Pipe) {
            self.bump(); // |
            let (name, name_span) = self.parse_ident()?;
            let kind = match self.peek().kind {
                TokenKind::LParen => {
                    self.bump();
                    let mut tys = Vec::new();
                    while !matches!(self.peek().kind, TokenKind::RParen) {
                        tys.push(self.parse_type()?);
                        if self.eat(&TokenKind::Comma).is_none() {
                            break;
                        }
                        self.skip_newlines();
                    }
                    self.expect(&TokenKind::RParen)?;
                    SumVariantKind::Tuple(tys)
                }
                TokenKind::LBrace => {
                    self.bump();
                    let fields = self.parse_record_fields()?;
                    self.expect(&TokenKind::RBrace)?;
                    SumVariantKind::Record(fields)
                }
                _ => SumVariantKind::Unit,
            };
            // Discriminant `= N`
            let discriminant = if self.eat(&TokenKind::Eq).is_some() {
                if let TokenKind::Int(n) = self.peek().kind {
                    self.bump();
                    Some(n)
                } else {
                    return Err(Diagnostic::new(
                        "expected integer discriminant",
                        self.peek().span,
                    ));
                }
            } else {
                None
            };
            let end = self.tokens[self.pos.saturating_sub(1)].span;
            variants.push(SumVariant {
                name,
                kind,
                discriminant,
                span: name_span.merge(end),
            });
            self.skip_newlines();
        }
        Ok(variants)
    }

    fn parse_protocol_methods(&mut self) -> Result<Vec<ProtocolMethod>, Diagnostic> {
        let mut methods = Vec::new();
        self.skip_newlines();
        while !matches!(self.peek().kind, TokenKind::RBrace) {
            let (name, name_span) = self.parse_ident()?;
            // generics
            let mut generics: Vec<String> = Vec::new();
            if self.eat(&TokenKind::LBracket).is_some() {
                loop {
                    let (n, _) = self.parse_ident()?;
                    generics.push(n);
                    if self.eat(&TokenKind::Comma).is_none() {
                        break;
                    }
                }
                self.expect(&TokenKind::RBracket)?;
            }
            self.expect(&TokenKind::LParen)?;
            let mut params = Vec::new();
            while !matches!(self.peek().kind, TokenKind::RParen) {
                params.push(self.parse_param()?);
                if self.eat(&TokenKind::Comma).is_none() {
                    break;
                }
                self.skip_newlines();
            }
            self.expect(&TokenKind::RParen)?;
            let effects = self.parse_effects_until_arrow_or_body()?;
            let return_type = if self.eat(&TokenKind::Arrow).is_some() {
                Some(self.parse_type()?)
            } else {
                None
            };
            let end = self.tokens[self.pos.saturating_sub(1)].span;
            methods.push(ProtocolMethod {
                name,
                generics,
                params,
                effects,
                return_type,
                span: name_span.merge(end),
            });
            self.skip_newlines();
        }
        Ok(methods)
    }

    // ─── let / const / test ──────────────────────────────────────────────

    fn parse_let_decl(&mut self) -> Result<LetDecl, Diagnostic> {
        let start = self.peek().span;
        self.expect(&TokenKind::KwLet)?;
        let mutable = self.eat(&TokenKind::KwMut).is_some();
        let pattern = self.parse_pattern()?;
        let ty = if !matches!(self.peek().kind, TokenKind::Eq) {
            Some(self.parse_type()?)
        } else {
            None
        };
        self.expect(&TokenKind::Eq)?;
        let value = self.parse_expr()?;
        let span = start.merge(value.span);
        self.expect_newline_or_eof().ok();
        Ok(LetDecl {
            mutable,
            pattern,
            ty,
            value,
            span,
        })
    }

    fn parse_const_decl(&mut self, is_export: bool) -> Result<ConstDecl, Diagnostic> {
        let start = self.peek().span;
        self.expect(&TokenKind::KwConst)?;
        let (name, _) = self.parse_ident()?;
        let ty = if !matches!(self.peek().kind, TokenKind::Eq) {
            Some(self.parse_type()?)
        } else {
            None
        };
        self.expect(&TokenKind::Eq)?;
        let value = self.parse_expr()?;
        let value_span = value.span;
        self.expect_newline_or_eof().ok();
        Ok(ConstDecl {
            is_export,
            name,
            ty,
            value,
            span: start.merge(value_span),
        })
    }

    fn parse_test_decl(&mut self) -> Result<TestDecl, Diagnostic> {
        let start = self.peek().span;
        self.expect(&TokenKind::KwTest)?;
        let name = match &self.peek().kind {
            TokenKind::Str(s) => {
                let n = s.clone();
                self.bump();
                n
            }
            _ => {
                return Err(Diagnostic::new(
                    "expected test name as string literal",
                    self.peek().span,
                ))
            }
        };
        let body = self.parse_block()?;
        let body_span = body.span;
        Ok(TestDecl {
            name,
            body,
            span: start.merge(body_span),
        })
    }

    // ─── types ───────────────────────────────────────────────────────────

    fn parse_type(&mut self) -> Result<TypeRef, Diagnostic> {
        let start = self.peek().span;
        match self.peek().kind {
            TokenKind::LBracket => {
                self.bump();
                // []T или [N]T
                if let TokenKind::Int(n) = self.peek().kind {
                    self.bump();
                    self.expect(&TokenKind::RBracket)?;
                    let inner = self.parse_type()?;
                    let span = start.merge(inner.span());
                    Ok(TypeRef::FixedArray(n as usize, Box::new(inner), span))
                } else {
                    self.expect(&TokenKind::RBracket)?;
                    let inner = self.parse_type()?;
                    let span = start.merge(inner.span());
                    Ok(TypeRef::Array(Box::new(inner), span))
                }
            }
            TokenKind::LParen => {
                self.bump();
                if matches!(self.peek().kind, TokenKind::RParen) {
                    let end = self.bump().span;
                    return Ok(TypeRef::Unit(start.merge(end)));
                }
                let mut tys = vec![self.parse_type()?];
                while self.eat(&TokenKind::Comma).is_some() {
                    tys.push(self.parse_type()?);
                }
                let end = self.expect(&TokenKind::RParen)?.span;
                if tys.len() == 1 {
                    Ok(tys.into_iter().next().unwrap())
                } else {
                    Ok(TypeRef::Tuple(tys, start.merge(end)))
                }
            }
            TokenKind::KwFn => {
                // fn(A, B) E1 E2 -> R
                self.bump();
                self.expect(&TokenKind::LParen)?;
                let mut params = Vec::new();
                while !matches!(self.peek().kind, TokenKind::RParen) {
                    params.push(self.parse_type()?);
                    if self.eat(&TokenKind::Comma).is_none() {
                        break;
                    }
                }
                self.expect(&TokenKind::RParen)?;
                let effects = self.parse_effects_until_arrow_or_body()?;
                let return_type = if self.eat(&TokenKind::Arrow).is_some() {
                    Some(Box::new(self.parse_type()?))
                } else {
                    None
                };
                let end = self.tokens[self.pos.saturating_sub(1)].span;
                Ok(TypeRef::Func {
                    params,
                    effects,
                    return_type,
                    span: start.merge(end),
                })
            }
            TokenKind::Ident(_) => {
                let mut path = vec![self.parse_ident()?.0];
                while matches!(self.peek().kind, TokenKind::Dot)
                    && matches!(self.peek_at(1).kind, TokenKind::Ident(_))
                {
                    self.bump();
                    path.push(self.parse_ident()?.0);
                }
                let generics = if matches!(self.peek().kind, TokenKind::LBracket) {
                    self.parse_type_args()?
                } else {
                    Vec::new()
                };
                let end = self.tokens[self.pos.saturating_sub(1)].span;
                Ok(TypeRef::Named {
                    path,
                    generics,
                    span: start.merge(end),
                })
            }
            _ => Err(Diagnostic::new(
                format!("expected type, got {}", self.peek().kind.name()),
                start,
            )),
        }
    }

    fn parse_type_args(&mut self) -> Result<Vec<TypeRef>, Diagnostic> {
        self.expect(&TokenKind::LBracket)?;
        let mut args = Vec::new();
        while !matches!(self.peek().kind, TokenKind::RBracket) {
            args.push(self.parse_type()?);
            if self.eat(&TokenKind::Comma).is_none() {
                break;
            }
        }
        self.expect(&TokenKind::RBracket)?;
        Ok(args)
    }

    // ─── expressions ─────────────────────────────────────────────────────

    pub fn parse_expr(&mut self) -> Result<Expr, Diagnostic> {
        self.parse_or()
    }

    fn parse_or(&mut self) -> Result<Expr, Diagnostic> {
        let mut left = self.parse_and()?;
        while matches!(self.peek().kind, TokenKind::PipePipe | TokenKind::KwOr) {
            self.bump();
            self.skip_newlines();
            let right = self.parse_and()?;
            let span = left.span.merge(right.span);
            left = Expr::new(
                ExprKind::Binary {
                    op: BinOp::Or,
                    left: Box::new(left),
                    right: Box::new(right),
                },
                span,
            );
        }
        Ok(left)
    }

    fn parse_and(&mut self) -> Result<Expr, Diagnostic> {
        let mut left = self.parse_eq()?;
        while matches!(self.peek().kind, TokenKind::AmpAmp | TokenKind::KwAnd) {
            self.bump();
            self.skip_newlines();
            let right = self.parse_eq()?;
            let span = left.span.merge(right.span);
            left = Expr::new(
                ExprKind::Binary {
                    op: BinOp::And,
                    left: Box::new(left),
                    right: Box::new(right),
                },
                span,
            );
        }
        Ok(left)
    }

    fn parse_eq(&mut self) -> Result<Expr, Diagnostic> {
        let mut left = self.parse_cmp()?;
        loop {
            let op = match self.peek().kind {
                TokenKind::EqEq => BinOp::Eq,
                TokenKind::BangEq => BinOp::Neq,
                _ => break,
            };
            self.bump();
            self.skip_newlines();
            let right = self.parse_cmp()?;
            let span = left.span.merge(right.span);
            left = Expr::new(
                ExprKind::Binary {
                    op,
                    left: Box::new(left),
                    right: Box::new(right),
                },
                span,
            );
        }
        Ok(left)
    }

    fn parse_cmp(&mut self) -> Result<Expr, Diagnostic> {
        let mut left = self.parse_range()?;
        loop {
            let op = match self.peek().kind {
                TokenKind::Lt => BinOp::Lt,
                TokenKind::Le => BinOp::Le,
                TokenKind::Gt => BinOp::Gt,
                TokenKind::Ge => BinOp::Ge,
                _ => break,
            };
            self.bump();
            self.skip_newlines();
            let right = self.parse_range()?;
            let span = left.span.merge(right.span);
            left = Expr::new(
                ExprKind::Binary {
                    op,
                    left: Box::new(left),
                    right: Box::new(right),
                },
                span,
            );
        }
        Ok(left)
    }

    fn parse_range(&mut self) -> Result<Expr, Diagnostic> {
        let left = self.parse_add()?;
        let inclusive = match self.peek().kind {
            TokenKind::DotDot => false,
            TokenKind::DotDotEq => true,
            _ => return Ok(left),
        };
        self.bump();
        let right = self.parse_add()?;
        let span = left.span.merge(right.span);
        Ok(Expr::new(
            ExprKind::Range {
                start: Box::new(left),
                end: Box::new(right),
                inclusive,
            },
            span,
        ))
    }

    fn parse_add(&mut self) -> Result<Expr, Diagnostic> {
        let mut left = self.parse_mul()?;
        loop {
            let op = match self.peek().kind {
                TokenKind::Plus => BinOp::Add,
                TokenKind::Minus => BinOp::Sub,
                _ => break,
            };
            self.bump();
            self.skip_newlines();
            let right = self.parse_mul()?;
            let span = left.span.merge(right.span);
            left = Expr::new(
                ExprKind::Binary {
                    op,
                    left: Box::new(left),
                    right: Box::new(right),
                },
                span,
            );
        }
        Ok(left)
    }

    fn parse_mul(&mut self) -> Result<Expr, Diagnostic> {
        let mut left = self.parse_unary()?;
        loop {
            let op = match self.peek().kind {
                TokenKind::Star => BinOp::Mul,
                TokenKind::Slash => BinOp::Div,
                TokenKind::Percent => BinOp::Mod,
                _ => break,
            };
            self.bump();
            self.skip_newlines();
            let right = self.parse_unary()?;
            let span = left.span.merge(right.span);
            left = Expr::new(
                ExprKind::Binary {
                    op,
                    left: Box::new(left),
                    right: Box::new(right),
                },
                span,
            );
        }
        Ok(left)
    }

    fn parse_unary(&mut self) -> Result<Expr, Diagnostic> {
        let start = self.peek().span;
        match self.peek().kind {
            TokenKind::Minus => {
                self.bump();
                let operand = self.parse_unary()?;
                let span = start.merge(operand.span);
                Ok(Expr::new(
                    ExprKind::Unary {
                        op: UnOp::Neg,
                        operand: Box::new(operand),
                    },
                    span,
                ))
            }
            TokenKind::Bang | TokenKind::KwNot => {
                self.bump();
                let operand = self.parse_unary()?;
                let span = start.merge(operand.span);
                Ok(Expr::new(
                    ExprKind::Unary {
                        op: UnOp::Not,
                        operand: Box::new(operand),
                    },
                    span,
                ))
            }
            _ => self.parse_postfix(),
        }
    }

    fn parse_postfix(&mut self) -> Result<Expr, Diagnostic> {
        let mut expr = self.parse_primary()?;
        loop {
            match self.peek().kind {
                TokenKind::Dot => {
                    self.bump();
                    // .field или .0 (positional). Float-токен `0.0` после
                    // `.` появляется когда лексер увидел `<expr>.0.0` и
                    // съел `.0.0` как `Dot Float(0.0)` — расщепляем
                    // `n.m` обратно в два positional-доступа.
                    let (name, name_span) = match self.peek().kind.clone() {
                        TokenKind::Int(n) => {
                            let sp = self.peek().span;
                            self.bump();
                            (format!("{}", n), sp)
                        }
                        TokenKind::Float(f) => {
                            // `n.m` — два positional access'а подряд.
                            let sp = self.peek().span;
                            let raw = self.tokens[self.pos].span;
                            let text = self.src_substring(raw);
                            let (first, second) = if !text.is_empty() {
                                let parts: Vec<&str> = text.splitn(2, '.').collect();
                                if parts.len() != 2 {
                                    return Err(Diagnostic::new(
                                        "malformed positional access",
                                        sp,
                                    ));
                                }
                                (parts[0].to_string(), parts[1].to_string())
                            } else {
                                // Fallback: восстанавливаем по значению
                                // (только для целых частей вроде 0.0, 1.2).
                                let s = format!("{}", f);
                                let parts: Vec<&str> = s.splitn(2, '.').collect();
                                if parts.len() != 2 {
                                    return Err(Diagnostic::new(
                                        "malformed positional access",
                                        sp,
                                    ));
                                }
                                (parts[0].to_string(), parts[1].to_string())
                            };
                            self.bump();
                            // Применяем первый Member, потом второй.
                            let mid = Expr::new(
                                ExprKind::Member {
                                    obj: Box::new(expr),
                                    name: first,
                                },
                                sp,
                            );
                            expr = Expr::new(
                                ExprKind::Member {
                                    obj: Box::new(mid),
                                    name: second,
                                },
                                sp,
                            );
                            continue;
                        }
                        TokenKind::Ident(_) => self.parse_ident()?,
                        _ => {
                            return Err(Diagnostic::new(
                                "expected field name or index after `.`",
                                self.peek().span,
                            ));
                        }
                    };
                    let span = expr.span.merge(name_span);
                    expr = Expr::new(
                        ExprKind::Member {
                            obj: Box::new(expr),
                            name,
                        },
                        span,
                    );
                }
                TokenKind::LBracket => {
                    self.bump();
                    let index = self.parse_expr()?;
                    let end = self.expect(&TokenKind::RBracket)?.span;
                    expr = Expr::new(
                        ExprKind::Index {
                            obj: Box::new(expr.clone()),
                            index: Box::new(index),
                        },
                        expr.span.merge(end),
                    );
                }
                TokenKind::LParen => {
                    self.bump();
                    let mut args = Vec::new();
                    self.skip_newlines();
                    while !matches!(self.peek().kind, TokenKind::RParen) {
                        args.push(self.parse_expr()?);
                        if self.eat(&TokenKind::Comma).is_some() {
                            self.skip_newlines();
                        } else {
                            break;
                        }
                    }
                    let end = self.expect(&TokenKind::RParen)?.span;
                    // trailing block?
                    let trailing_block = if matches!(self.peek().kind, TokenKind::LBrace) {
                        Some(self.parse_trailing_block()?)
                    } else {
                        None
                    };
                    let span = expr.span.merge(end);
                    expr = Expr::new(
                        ExprKind::Call {
                            func: Box::new(expr),
                            args,
                            trailing_block,
                        },
                        span,
                    );
                }
                TokenKind::Backtick(_) => {
                    // tagged template: `expr` `template` — но в bootstrap
                    // мы упрощаем: backtick за идентификатором → Call(func, [Str(template)])
                    let TokenKind::Backtick(tpl) = self.bump().kind else {
                        unreachable!()
                    };
                    let span = expr.span;
                    expr = Expr::new(
                        ExprKind::TaggedTemplate {
                            tag: Box::new(expr),
                            parts: vec![tpl],
                            args: Vec::new(),
                        },
                        span,
                    );
                }
                TokenKind::Question => {
                    self.bump();
                    let span = expr.span;
                    expr = Expr::new(ExprKind::Try(Box::new(expr)), span);
                }
                TokenKind::Question2 => {
                    self.bump();
                    let right = self.parse_unary()?;
                    let span = expr.span.merge(right.span);
                    expr = Expr::new(
                        ExprKind::Coalesce(Box::new(expr), Box::new(right)),
                        span,
                    );
                }
                TokenKind::KwAs => {
                    self.bump();
                    let ty = self.parse_type()?;
                    let span = expr.span.merge(ty.span());
                    expr = Expr::new(ExprKind::As(Box::new(expr), ty), span);
                }
                TokenKind::KwIs => {
                    self.bump();
                    let ty = self.parse_type()?;
                    let span = expr.span.merge(ty.span());
                    expr = Expr::new(ExprKind::Is(Box::new(expr), ty), span);
                }
                _ => break,
            }
        }
        Ok(expr)
    }

    fn parse_primary(&mut self) -> Result<Expr, Diagnostic> {
        let start = self.peek().span;
        match self.peek().kind.clone() {
            TokenKind::Int(n) => {
                self.bump();
                Ok(Expr::new(ExprKind::IntLit(n), start))
            }
            TokenKind::Float(n) => {
                self.bump();
                Ok(Expr::new(ExprKind::FloatLit(n), start))
            }
            TokenKind::Str(s) => {
                self.bump();
                Ok(Expr::new(ExprKind::StrLit(s), start))
            }
            TokenKind::KwTrue => {
                self.bump();
                Ok(Expr::new(ExprKind::BoolLit(true), start))
            }
            TokenKind::KwFalse => {
                self.bump();
                Ok(Expr::new(ExprKind::BoolLit(false), start))
            }
            TokenKind::Backtick(s) => {
                self.bump();
                // bare backtick без tag-функции = строка.
                Ok(Expr::new(ExprKind::StrLit(s), start))
            }
            TokenKind::At => {
                self.bump();
                // @ или @field
                if matches!(self.peek().kind, TokenKind::Ident(_)) {
                    let (name, name_span) = self.parse_ident()?;
                    let self_expr = Expr::new(ExprKind::SelfAccess, start);
                    Ok(Expr::new(
                        ExprKind::Member {
                            obj: Box::new(self_expr),
                            name,
                        },
                        start.merge(name_span),
                    ))
                } else {
                    Ok(Expr::new(ExprKind::SelfAccess, start))
                }
            }
            TokenKind::Ident(_) => {
                // Простой идентификатор. Dot-цепочки `.field`/`.method`
                // обрабатываются в parse_postfix как Member access — это
                // позволяет `p.x` работать когда `p` — обычная переменная,
                // а не qualifier пути.
                //
                // Type/Module qualifier (PascalCase + dot) превращается в
                // Path только когда первый токен — заглавная буква И за ним
                // `.IDENT`, чтобы поддержать `Type.method` / `Module.fn`.
                // Дальше member-доступ всё равно работает через Dot →
                // Member на postfix-стадии.
                let (first, first_span) = self.parse_ident()?;
                let starts_uppercase = first
                    .chars()
                    .next()
                    .map(|c| c.is_ascii_uppercase())
                    .unwrap_or(false);
                let mut path = vec![first.clone()];
                if starts_uppercase {
                    while matches!(self.peek().kind, TokenKind::Dot)
                        && matches!(self.peek_at(1).kind, TokenKind::Ident(_))
                    {
                        // Заглядываем: если следующий ident — тоже PascalCase,
                        // продолжаем path; иначе оставляем для Member access.
                        let TokenKind::Ident(next_name) = &self.peek_at(1).kind else {
                            break;
                        };
                        let next_upper = next_name
                            .chars()
                            .next()
                            .map(|c| c.is_ascii_uppercase())
                            .unwrap_or(false);
                        // Продолжаем только если оба — PascalCase (Module.SubModule).
                        // Type.method (метод с lowercase) останавливаем здесь,
                        // чтобы получить Path[Type] и Member через postfix —
                        // но это сломало бы static-method вызовы. Вместо этого:
                        // съедаем dot всегда после PascalCase, пока следующее —
                        // identifier. Проблема — для Type.method.x сначала
                        // соберём Path[Type, method], потом .x как Member.
                        let _ = next_upper;
                        self.bump();
                        path.push(self.parse_ident()?.0);
                    }
                }
                // Если за path идёт `{`, и **это валидно как record-литерал**:
                if matches!(self.peek().kind, TokenKind::LBrace) && self.looks_like_record_lit() {
                    return self.parse_record_lit_after_path(path, first_span);
                }
                if path.len() == 1 {
                    Ok(Expr::new(
                        ExprKind::Ident(path.into_iter().next().unwrap()),
                        first_span.merge(self.tokens[self.pos.saturating_sub(1)].span),
                    ))
                } else {
                    Ok(Expr::new(
                        ExprKind::Path(path),
                        first_span.merge(self.tokens[self.pos.saturating_sub(1)].span),
                    ))
                }
            }
            TokenKind::LBrace => {
                // Запись или блок? В Nova record-литерал без типа — тоже
                // валиден (D55 coercion). Различаем: { name : ... } -> record,
                // { name, name: ... } -> record (D52 punning), иначе блок.
                if self.looks_like_record_lit() {
                    self.parse_record_lit_after_path(Vec::new(), start)
                } else {
                    let block = self.parse_block()?;
                    let span = block.span;
                    Ok(Expr::new(ExprKind::Block(block), span))
                }
            }
            TokenKind::LBracket => self.parse_array_lit(),
            TokenKind::LParen => {
                self.bump();
                self.skip_newlines();
                if matches!(self.peek().kind, TokenKind::RParen) {
                    let end = self.bump().span;
                    return Ok(Expr::new(ExprKind::UnitLit, start.merge(end)));
                }
                // Lambda? `(p1, p2) => expr` или `(p) =>` или `(p Type) =>`
                // Сложно отличить от группировки/кортежа без lookahead.
                // Стратегия: попробуем как expr, и если за `)` идёт `=>` — переводим в лямбду.
                // Для bootstrap'а используем прямой lookahead: если все элементы — простые
                // `ident` или `ident type` и за `)` идёт `=>` или `Effects -> Type =>`,
                // парсим как лямбду. Иначе — кортеж/группа.
                let saved_pos = self.pos;
                if let Some(lambda) = self.try_parse_lambda(start)? {
                    return Ok(lambda);
                }
                self.pos = saved_pos;
                let first = self.parse_expr()?;
                if self.eat(&TokenKind::Comma).is_some() {
                    let mut elems = vec![first];
                    self.skip_newlines();
                    while !matches!(self.peek().kind, TokenKind::RParen) {
                        elems.push(self.parse_expr()?);
                        if self.eat(&TokenKind::Comma).is_none() {
                            break;
                        }
                        self.skip_newlines();
                    }
                    let end = self.expect(&TokenKind::RParen)?.span;
                    Ok(Expr::new(ExprKind::TupleLit(elems), start.merge(end)))
                } else {
                    self.expect(&TokenKind::RParen)?;
                    Ok(first)
                }
            }
            TokenKind::KwIf => self.parse_if(),
            TokenKind::KwMatch => self.parse_match(),
            TokenKind::KwFor => self.parse_for(),
            TokenKind::KwWhile => self.parse_while(),
            TokenKind::KwLoop => self.parse_loop(),
            TokenKind::KwWith => self.parse_with(),
            TokenKind::KwInterrupt => self.parse_interrupt_expr(),
            TokenKind::KwSpawn => self.parse_spawn(),
            TokenKind::KwHandler => self.parse_handler_lit(),
            TokenKind::KwForbid => self.parse_forbid(),
            TokenKind::KwRealtime => self.parse_realtime(),
            other => Err(Diagnostic::new(
                format!("unexpected {} in expression", other.name()),
                start,
            )),
        }
    }

    /// Эвристика: `{` перед нами — это начало record-литерала?
    /// Смотрим первый «значимый» токен внутри: `Ident :` или `...` или `}`.
    fn looks_like_record_lit(&self) -> bool {
        if self.no_struct_lit {
            return false;
        }
        // Skip newlines внутри.
        let mut i = self.pos + 1;
        while i < self.tokens.len()
            && matches!(self.tokens[i].kind, TokenKind::Newline | TokenKind::Semicolon)
        {
            i += 1;
        }
        if i >= self.tokens.len() {
            return false;
        }
        if matches!(self.tokens[i].kind, TokenKind::RBrace) {
            return true;
        }
        if matches!(self.tokens[i].kind, TokenKind::DotDotDot | TokenKind::At) {
            return true;
        }
        if matches!(self.tokens[i].kind, TokenKind::Ident(_)) {
            // smart: `Ident :` → record. `Ident ,` → punning. `Ident }` → punning.
            let next = self.tokens.get(i + 1);
            return matches!(
                next.map(|t| &t.kind),
                Some(TokenKind::Colon | TokenKind::Comma | TokenKind::RBrace)
            );
        }
        false
    }

    fn parse_record_lit_after_path(
        &mut self,
        path: Vec<String>,
        start: Span,
    ) -> Result<Expr, Diagnostic> {
        self.expect(&TokenKind::LBrace)?;
        let mut fields = Vec::new();
        self.skip_newlines();
        while !matches!(self.peek().kind, TokenKind::RBrace) {
            if self.eat(&TokenKind::DotDotDot).is_some() {
                let v = self.parse_expr()?;
                fields.push(RecordLitField {
                    name: String::new(),
                    value: Some(v.clone()),
                    is_spread: true,
                    span: v.span,
                });
            } else {
                // field-shorthand `name` или `@name`, или `name: expr`
                if matches!(self.peek().kind, TokenKind::At) {
                    let at_span = self.bump().span;
                    let (name, name_span) = self.parse_ident()?;
                    let value = Expr::new(
                        ExprKind::Member {
                            obj: Box::new(Expr::new(ExprKind::SelfAccess, at_span)),
                            name: name.clone(),
                        },
                        at_span.merge(name_span),
                    );
                    fields.push(RecordLitField {
                        name,
                        value: Some(value),
                        is_spread: false,
                        span: at_span.merge(name_span),
                    });
                } else {
                    let (name, name_span) = self.parse_ident()?;
                    if self.eat(&TokenKind::Colon).is_some() {
                        let v = self.parse_expr()?;
                        let span = name_span.merge(v.span);
                        fields.push(RecordLitField {
                            name,
                            value: Some(v),
                            is_spread: false,
                            span,
                        });
                    } else {
                        // shorthand
                        fields.push(RecordLitField {
                            name,
                            value: None,
                            is_spread: false,
                            span: name_span,
                        });
                    }
                }
            }
            if self.eat(&TokenKind::Comma).is_some() {
                self.skip_newlines();
            } else {
                self.skip_newlines();
            }
        }
        let end = self.expect(&TokenKind::RBrace)?.span;
        Ok(Expr::new(
            ExprKind::RecordLit {
                type_name: if path.is_empty() { None } else { Some(path) },
                fields,
            },
            start.merge(end),
        ))
    }

    fn parse_array_lit(&mut self) -> Result<Expr, Diagnostic> {
        let start = self.expect(&TokenKind::LBracket)?.span;
        let mut elems = Vec::new();
        self.skip_newlines();
        while !matches!(self.peek().kind, TokenKind::RBracket) {
            if self.eat(&TokenKind::DotDotDot).is_some() {
                let v = self.parse_expr()?;
                elems.push(ArrayElem::Spread(v));
            } else {
                let v = self.parse_expr()?;
                elems.push(ArrayElem::Item(v));
            }
            if self.eat(&TokenKind::Comma).is_some() {
                self.skip_newlines();
            } else {
                self.skip_newlines();
            }
        }
        let end = self.expect(&TokenKind::RBracket)?.span;
        Ok(Expr::new(ExprKind::ArrayLit(elems), start.merge(end)))
    }

    fn try_parse_lambda(&mut self, start: Span) -> Result<Option<Expr>, Diagnostic> {
        // Уже съели `(`. Пытаемся распарсить параметры лямбды.
        // Ограничиваемся простыми случаями: `(a, b) => expr` или `(a Ty, b Ty) => expr`.
        let mut params = Vec::new();
        let saved = self.pos;
        loop {
            if matches!(self.peek().kind, TokenKind::RParen) {
                break;
            }
            if !matches!(self.peek().kind, TokenKind::Ident(_)) {
                self.pos = saved;
                return Ok(None);
            }
            let (name, name_span) = self.parse_ident()?;
            // Опц. тип
            let ty = if !matches!(
                self.peek().kind,
                TokenKind::Comma | TokenKind::RParen
            ) {
                // Может быть тип. Попробуем.
                let attempt = self.pos;
                match self.parse_type() {
                    Ok(t) => Some(t),
                    Err(_) => {
                        self.pos = attempt;
                        None
                    }
                }
            } else {
                None
            };
            params.push(LambdaParam {
                name,
                ty,
                span: name_span,
            });
            if self.eat(&TokenKind::Comma).is_none() {
                break;
            }
            self.skip_newlines();
        }
        if !matches!(self.peek().kind, TokenKind::RParen) {
            self.pos = saved;
            return Ok(None);
        }
        self.bump();
        // Опционально: эффекты + `-> Type`
        let effects = self.parse_effects_until_arrow_or_body()?;
        let return_type = if self.eat(&TokenKind::Arrow).is_some() {
            Some(self.parse_type()?)
        } else {
            None
        };
        // Должна быть FatArrow
        if !matches!(self.peek().kind, TokenKind::FatArrow) {
            self.pos = saved;
            return Ok(None);
        }
        self.bump();
        self.skip_newlines();
        let body = self.parse_expr()?;
        let span = start.merge(body.span);
        Ok(Some(Expr::new(
            ExprKind::Lambda {
                params,
                effects,
                return_type,
                body: Box::new(body),
            },
            span,
        )))
    }

    fn parse_if(&mut self) -> Result<Expr, Diagnostic> {
        let start = self.expect(&TokenKind::KwIf)?.span;
        // `if let pattern = expr { ... }` — D34
        if matches!(self.peek().kind, TokenKind::KwLet) {
            self.bump();
            let pattern = self.parse_pattern()?;
            self.expect(&TokenKind::Eq)?;
            let scrutinee = self.with_no_struct_lit(|p| p.parse_expr())?;
            let then = self.parse_block()?;
            let else_ = self.parse_optional_else()?;
            let end = then.span;
            return Ok(Expr::new(
                ExprKind::IfLet {
                    pattern,
                    scrutinee: Box::new(scrutinee),
                    then,
                    else_,
                },
                start.merge(end),
            ));
        }
        let cond = self.with_no_struct_lit(|p| p.parse_expr())?;
        let then = self.parse_block()?;
        let else_ = self.parse_optional_else()?;
        let end = then.span;
        Ok(Expr::new(
            ExprKind::If {
                cond: Box::new(cond),
                then,
                else_,
            },
            start.merge(end),
        ))
    }

    fn parse_optional_else(&mut self) -> Result<Option<ElseBranch>, Diagnostic> {
        // newlines не должны мешать `else`, но обычно `}` else на той же строке
        let saved = self.pos;
        self.skip_newlines();
        if !matches!(self.peek().kind, TokenKind::KwElse) {
            self.pos = saved;
            return Ok(None);
        }
        self.bump();
        if matches!(self.peek().kind, TokenKind::KwIf) {
            let inner = self.parse_if()?;
            Ok(Some(ElseBranch::If(Box::new(inner))))
        } else {
            let block = self.parse_block()?;
            Ok(Some(ElseBranch::Block(block)))
        }
    }

    fn parse_match(&mut self) -> Result<Expr, Diagnostic> {
        let start = self.expect(&TokenKind::KwMatch)?.span;
        let scrutinee = self.with_no_struct_lit(|p| p.parse_expr())?;
        self.expect(&TokenKind::LBrace)?;
        let mut arms = Vec::new();
        self.skip_newlines();
        while !matches!(self.peek().kind, TokenKind::RBrace) {
            let pattern = self.parse_pattern()?;
            let guard = if matches!(self.peek().kind, TokenKind::KwIf) {
                self.bump();
                Some(self.parse_expr()?)
            } else {
                None
            };
            self.expect(&TokenKind::FatArrow)?;
            self.skip_newlines();
            // body: либо `{ block }` (D19 исключение), либо expr.
            let body = if matches!(self.peek().kind, TokenKind::LBrace) {
                let saved = self.pos;
                if self.looks_like_record_lit() {
                    // record-литерал — выражение
                    self.pos = saved;
                    MatchArmBody::Expr(self.parse_expr()?)
                } else {
                    MatchArmBody::Block(self.parse_block()?)
                }
            } else {
                MatchArmBody::Expr(self.parse_expr()?)
            };
            let span = pattern.span().merge(match &body {
                MatchArmBody::Expr(e) => e.span,
                MatchArmBody::Block(b) => b.span,
            });
            arms.push(MatchArm {
                pattern,
                guard,
                body,
                span,
            });
            self.skip_newlines();
        }
        let end = self.expect(&TokenKind::RBrace)?.span;
        Ok(Expr::new(
            ExprKind::Match {
                scrutinee: Box::new(scrutinee),
                arms,
            },
            start.merge(end),
        ))
    }

    fn parse_for(&mut self) -> Result<Expr, Diagnostic> {
        let start = self.expect(&TokenKind::KwFor)?.span;
        let pattern = self.parse_pattern()?;
        self.expect(&TokenKind::KwIn)?;
        let iter = self.with_no_struct_lit(|p| p.parse_expr())?;
        let body = self.parse_block()?;
        let end = body.span;
        Ok(Expr::new(
            ExprKind::For {
                pattern,
                iter: Box::new(iter),
                body,
            },
            start.merge(end),
        ))
    }

    fn parse_while(&mut self) -> Result<Expr, Diagnostic> {
        let start = self.expect(&TokenKind::KwWhile)?.span;
        if matches!(self.peek().kind, TokenKind::KwLet) {
            self.bump();
            let pattern = self.parse_pattern()?;
            self.expect(&TokenKind::Eq)?;
            let scrutinee = self.with_no_struct_lit(|p| p.parse_expr())?;
            let body = self.parse_block()?;
            let end = body.span;
            return Ok(Expr::new(
                ExprKind::WhileLet {
                    pattern,
                    scrutinee: Box::new(scrutinee),
                    body,
                },
                start.merge(end),
            ));
        }
        let cond = self.with_no_struct_lit(|p| p.parse_expr())?;
        let body = self.parse_block()?;
        let end = body.span;
        Ok(Expr::new(
            ExprKind::While {
                cond: Box::new(cond),
                body,
            },
            start.merge(end),
        ))
    }

    fn parse_loop(&mut self) -> Result<Expr, Diagnostic> {
        let start = self.expect(&TokenKind::KwLoop)?.span;
        let body = self.parse_block()?;
        let end = body.span;
        Ok(Expr::new(ExprKind::Loop { body }, start.merge(end)))
    }

    fn parse_with(&mut self) -> Result<Expr, Diagnostic> {
        let start = self.expect(&TokenKind::KwWith)?.span;
        let mut bindings = Vec::new();
        loop {
            let effect = self.parse_type()?;
            self.expect(&TokenKind::Eq)?;
            // handler — выражение. handler-литерал (`EffName { op() => ... }`)
            // приходит как `Path` + record-like, мы обрабатываем как expr
            // (эвристика record_lit может ошибиться; для надёжности парсим
            // handler-литерал отдельно если за typeref идёт `{` с
            // методами, не полями).
            let handler = self.parse_expr_or_handler_lit()?;
            let span = effect.span().merge(handler.span);
            bindings.push(WithBinding {
                effect,
                handler,
                span,
            });
            if self.eat(&TokenKind::Comma).is_none() {
                break;
            }
            self.skip_newlines();
        }
        let body = self.parse_block()?;
        let end = body.span;
        Ok(Expr::new(
            ExprKind::With { bindings, body },
            start.merge(end),
        ))
    }

    /// Парсит выражение, специально проверяя на handler-литерал `EffName { op... }`.
    /// Эвристика: после path `{` и первый элемент — `Ident (` (= операция),
    /// а не `Ident :` (= record).
    fn parse_expr_or_handler_lit(&mut self) -> Result<Expr, Diagnostic> {
        // Попробуем определить handler-литерал: ident + `{` + `Ident (` внутри.
        if matches!(self.peek().kind, TokenKind::Ident(_)) {
            let saved = self.pos;
            // Парсим path
            let mut path = vec![self.parse_ident()?.0];
            while matches!(self.peek().kind, TokenKind::Dot)
                && matches!(self.peek_at(1).kind, TokenKind::Ident(_))
            {
                self.bump();
                path.push(self.parse_ident()?.0);
            }
            if matches!(self.peek().kind, TokenKind::LBrace) {
                // Заглядываем внутрь: первый значимый — Ident `(`?
                let mut i = self.pos + 1;
                while i < self.tokens.len()
                    && matches!(
                        self.tokens[i].kind,
                        TokenKind::Newline | TokenKind::Semicolon
                    )
                {
                    i += 1;
                }
                if matches!(self.tokens.get(i).map(|t| &t.kind), Some(TokenKind::Ident(_)))
                    && matches!(
                        self.tokens.get(i + 1).map(|t| &t.kind),
                        Some(TokenKind::LParen)
                    )
                {
                    // handler-литерал
                    let start = self.tokens[saved].span;
                    self.bump(); // {
                    let mut methods = Vec::new();
                    self.skip_newlines();
                    while !matches!(self.peek().kind, TokenKind::RBrace) {
                        let (mname, mspan) = self.parse_ident()?;
                        self.expect(&TokenKind::LParen)?;
                        let mut params = Vec::new();
                        while !matches!(self.peek().kind, TokenKind::RParen) {
                            let (pname, pspan) = self.parse_ident()?;
                            // опциональный тип параметра
                            let pty = if !matches!(
                                self.peek().kind,
                                TokenKind::Comma | TokenKind::RParen
                            ) {
                                let attempt = self.pos;
                                match self.parse_type() {
                                    Ok(t) => Some(t),
                                    Err(_) => {
                                        self.pos = attempt;
                                        None
                                    }
                                }
                            } else {
                                None
                            };
                            params.push(HandlerMethodParam {
                                name: pname,
                                ty: pty,
                                span: pspan,
                            });
                            if self.eat(&TokenKind::Comma).is_none() {
                                break;
                            }
                        }
                        self.expect(&TokenKind::RParen)?;
                        let body = match self.peek().kind {
                            TokenKind::FatArrow => {
                                self.bump();
                                self.skip_newlines();
                                HandlerMethodBody::Expr(self.parse_expr()?)
                            }
                            TokenKind::LBrace => HandlerMethodBody::Block(self.parse_block()?),
                            _ => {
                                return Err(Diagnostic::new(
                                    "expected `=>` or `{` for handler-method body",
                                    self.peek().span,
                                ));
                            }
                        };
                        let end = match &body {
                            HandlerMethodBody::Expr(e) => e.span,
                            HandlerMethodBody::Block(b) => b.span,
                        };
                        methods.push(HandlerMethod {
                            name: mname,
                            params,
                            body,
                            span: mspan.merge(end),
                        });
                        self.skip_newlines();
                    }
                    let end = self.expect(&TokenKind::RBrace)?.span;
                    return Ok(Expr::new(
                        ExprKind::HandlerLit {
                            effect_name: path,
                            methods,
                        },
                        start.merge(end),
                    ));
                }
            }
            // Не handler-литерал — откатываемся и парсим как обычное выражение.
            self.pos = saved;
        }
        self.parse_expr()
    }

    fn parse_interrupt_expr(&mut self) -> Result<Expr, Diagnostic> {
        let start = self.expect(&TokenKind::KwInterrupt)?.span;
        let value = if self.at_newline() || matches!(self.peek().kind, TokenKind::RBrace) {
            None
        } else {
            Some(Box::new(self.parse_expr()?))
        };
        let end = match &value {
            Some(e) => e.span,
            None => start,
        };
        Ok(Expr::new(
            ExprKind::Interrupt(value),
            start.merge(end),
        ))
    }

    /// `handler EffectName { ops }` — keyword-форма handler-литерала (D61).
    /// Реиспользует существующую логику парсинга handler-method'ов.
    fn parse_handler_lit(&mut self) -> Result<Expr, Diagnostic> {
        let start = self.expect(&TokenKind::KwHandler)?.span;
        // Имя эффекта — dotted path, опционально с generic-параметрами.
        let mut path = vec![self.parse_ident()?.0];
        while matches!(self.peek().kind, TokenKind::Dot)
            && matches!(self.peek_at(1).kind, TokenKind::Ident(_))
        {
            self.bump();
            path.push(self.parse_ident()?.0);
        }
        // Опциональные generic-параметры эффекта (Fail[Error], Throws[E], etc.)
        // — парсим, но в bootstrap не используем (хранится в name path).
        if matches!(self.peek().kind, TokenKind::LBracket) {
            // Пропускаем generic-аргументы целиком
            let _ = self.parse_type_args()?;
        }
        self.expect(&TokenKind::LBrace)?;
        self.skip_newlines();
        let methods = self.parse_handler_methods()?;
        let end = self.expect(&TokenKind::RBrace)?.span;
        Ok(Expr::new(
            ExprKind::HandlerLit {
                effect_name: path,
                methods,
            },
            start.merge(end),
        ))
    }

    /// Извлечённая логика парсинга handler-method'ов.
    /// Используется и в parse_handler_lit, и в parse_expr_or_handler_lit
    /// (старая эвристика для обратной совместимости).
    fn parse_handler_methods(&mut self) -> Result<Vec<HandlerMethod>, Diagnostic> {
        let mut methods = Vec::new();
        while !matches!(self.peek().kind, TokenKind::RBrace) {
            let (mname, mspan) = self.parse_ident()?;
            self.expect(&TokenKind::LParen)?;
            let mut params = Vec::new();
            while !matches!(self.peek().kind, TokenKind::RParen) {
                let (pname, pspan) = self.parse_ident()?;
                let pty = if !matches!(
                    self.peek().kind,
                    TokenKind::Comma | TokenKind::RParen
                ) {
                    let attempt = self.pos;
                    match self.parse_type() {
                        Ok(t) => Some(t),
                        Err(_) => {
                            self.pos = attempt;
                            None
                        }
                    }
                } else {
                    None
                };
                params.push(HandlerMethodParam {
                    name: pname,
                    ty: pty,
                    span: pspan,
                });
                if self.eat(&TokenKind::Comma).is_none() {
                    break;
                }
            }
            self.expect(&TokenKind::RParen)?;
            let body = match self.peek().kind {
                TokenKind::FatArrow => {
                    self.bump();
                    self.skip_newlines();
                    HandlerMethodBody::Expr(self.parse_expr()?)
                }
                TokenKind::LBrace => HandlerMethodBody::Block(self.parse_block()?),
                _ => {
                    return Err(Diagnostic::new(
                        "expected `=>` or `{` for handler-method body",
                        self.peek().span,
                    ));
                }
            };
            let end = match &body {
                HandlerMethodBody::Expr(e) => e.span,
                HandlerMethodBody::Block(b) => b.span,
            };
            methods.push(HandlerMethod {
                name: mname,
                params,
                body,
                span: mspan.merge(end),
            });
            self.skip_newlines();
        }
        Ok(methods)
    }

    fn parse_spawn(&mut self) -> Result<Expr, Diagnostic> {
        let start = self.expect(&TokenKind::KwSpawn)?.span;
        let body = self.parse_expr()?;
        let span = start.merge(body.span);
        Ok(Expr::new(ExprKind::Spawn(Box::new(body)), span))
    }

    /// `forbid X1, X2, ... { body }` — capability sandbox (D63).
    fn parse_forbid(&mut self) -> Result<Expr, Diagnostic> {
        let start = self.expect(&TokenKind::KwForbid)?.span;
        // Список эффектов через запятую до открывающего `{`.
        let mut effects = Vec::new();
        loop {
            effects.push(self.parse_type()?);
            if self.eat(&TokenKind::Comma).is_none() {
                break;
            }
            self.skip_newlines();
        }
        let body = self.parse_block()?;
        let end = body.span;
        Ok(Expr::new(
            ExprKind::Forbid { effects, body },
            start.merge(end),
        ))
    }

    /// `realtime [nogc] { body }` — гарантия не-приостановки (D64).
    fn parse_realtime(&mut self) -> Result<Expr, Diagnostic> {
        let start = self.expect(&TokenKind::KwRealtime)?.span;
        // Опциональный модификатор `nogc`.
        let nogc = if let TokenKind::Ident(name) = &self.peek().kind {
            if name == "nogc" {
                self.bump();
                true
            } else {
                false
            }
        } else {
            false
        };
        let body = self.parse_block()?;
        let end = body.span;
        Ok(Expr::new(
            ExprKind::Realtime { nogc, body },
            start.merge(end),
        ))
    }

    // ─── block & stmts ───────────────────────────────────────────────────

    fn parse_block(&mut self) -> Result<Block, Diagnostic> {
        let start = self.expect(&TokenKind::LBrace)?.span;
        let mut stmts = Vec::new();
        let mut trailing: Option<Box<Expr>> = None;
        self.skip_newlines();
        while !matches!(self.peek().kind, TokenKind::RBrace) {
            // Пытаемся определить statement vs expression.
            let stmt_or_expr = self.parse_stmt_or_expr()?;
            match stmt_or_expr {
                StmtOrExpr::Stmt(s) => {
                    stmts.push(s);
                    self.skip_newlines();
                }
                StmtOrExpr::Expr(e) => {
                    self.skip_newlines();
                    if matches!(self.peek().kind, TokenKind::RBrace) {
                        trailing = Some(Box::new(e));
                    } else {
                        stmts.push(Stmt::Expr(e));
                    }
                }
            }
        }
        let end = self.expect(&TokenKind::RBrace)?.span;
        Ok(Block {
            stmts,
            trailing,
            span: start.merge(end),
        })
    }

    fn parse_stmt_or_expr(&mut self) -> Result<StmtOrExpr, Diagnostic> {
        let start = self.peek().span;
        match self.peek().kind {
            TokenKind::KwLet => {
                let l = self.parse_let_decl()?;
                Ok(StmtOrExpr::Stmt(Stmt::Let(l)))
            }
            TokenKind::KwReturn => {
                self.bump();
                let value = if self.at_newline() || matches!(self.peek().kind, TokenKind::RBrace)
                {
                    None
                } else {
                    Some(self.parse_expr()?)
                };
                let end = match &value {
                    Some(e) => e.span,
                    None => start,
                };
                Ok(StmtOrExpr::Stmt(Stmt::Return {
                    value,
                    span: start.merge(end),
                }))
            }
            TokenKind::KwBreak => {
                self.bump();
                Ok(StmtOrExpr::Stmt(Stmt::Break(start)))
            }
            TokenKind::KwContinue => {
                self.bump();
                Ok(StmtOrExpr::Stmt(Stmt::Continue(start)))
            }
            TokenKind::KwThrow => {
                self.bump();
                let value = self.parse_expr()?;
                let span = start.merge(value.span);
                Ok(StmtOrExpr::Stmt(Stmt::Throw { value, span }))
            }
            _ => {
                let expr = self.parse_expr()?;
                // Assignment?
                let op = match self.peek().kind {
                    TokenKind::Eq => Some(AssignOp::Assign),
                    TokenKind::PlusEq => Some(AssignOp::Add),
                    TokenKind::MinusEq => Some(AssignOp::Sub),
                    TokenKind::StarEq => Some(AssignOp::Mul),
                    TokenKind::SlashEq => Some(AssignOp::Div),
                    _ => None,
                };
                if let Some(op) = op {
                    self.bump();
                    self.skip_newlines();
                    let value = self.parse_expr()?;
                    let span = expr.span.merge(value.span);
                    return Ok(StmtOrExpr::Stmt(Stmt::Assign {
                        target: expr,
                        op,
                        value,
                        span,
                    }));
                }
                Ok(StmtOrExpr::Expr(expr))
            }
        }
    }

    fn parse_trailing_block(&mut self) -> Result<TrailingBlock, Diagnostic> {
        let start = self.peek().span;
        // `{` уже проверен снаружи. Парсим: optional `params =>`, потом block-body.
        self.expect(&TokenKind::LBrace)?;
        let mut params = Vec::new();
        // Эвристика для `params =>`: ищем в первом стрим без вложенных `{` —
        // если есть `=>` до первого `{` или statement-токена.
        let saved = self.pos;
        if self.try_parse_trailing_params(&mut params).is_err() {
            params.clear();
            self.pos = saved;
        }
        // Теперь парсим как block, но `}` финал.
        let mut stmts = Vec::new();
        let mut trailing: Option<Box<Expr>> = None;
        self.skip_newlines();
        while !matches!(self.peek().kind, TokenKind::RBrace) {
            match self.parse_stmt_or_expr()? {
                StmtOrExpr::Stmt(s) => {
                    stmts.push(s);
                    self.skip_newlines();
                }
                StmtOrExpr::Expr(e) => {
                    self.skip_newlines();
                    if matches!(self.peek().kind, TokenKind::RBrace) {
                        trailing = Some(Box::new(e));
                    } else {
                        stmts.push(Stmt::Expr(e));
                    }
                }
            }
        }
        let end = self.expect(&TokenKind::RBrace)?.span;
        Ok(TrailingBlock {
            params,
            body: Block {
                stmts,
                trailing,
                span: start.merge(end),
            },
            span: start.merge(end),
        })
    }

    fn try_parse_trailing_params(
        &mut self,
        out: &mut Vec<LambdaParam>,
    ) -> Result<(), Diagnostic> {
        // Простая попытка: `name =>`, `(a, b) =>`. Если не получается — Err.
        if matches!(self.peek().kind, TokenKind::LParen) {
            self.bump();
            while !matches!(self.peek().kind, TokenKind::RParen) {
                let (n, sp) = self.parse_ident()?;
                out.push(LambdaParam {
                    name: n,
                    ty: None,
                    span: sp,
                });
                if self.eat(&TokenKind::Comma).is_none() {
                    break;
                }
                self.skip_newlines();
            }
            self.expect(&TokenKind::RParen)?;
            self.expect(&TokenKind::FatArrow)?;
            self.skip_newlines();
            return Ok(());
        }
        if matches!(self.peek().kind, TokenKind::Ident(_))
            && matches!(self.peek_at(1).kind, TokenKind::FatArrow)
        {
            let (n, sp) = self.parse_ident()?;
            out.push(LambdaParam {
                name: n,
                ty: None,
                span: sp,
            });
            self.expect(&TokenKind::FatArrow)?;
            self.skip_newlines();
            return Ok(());
        }
        Err(Diagnostic::new("no trailing-block params", self.peek().span))
    }

    // ─── patterns ────────────────────────────────────────────────────────

    fn parse_pattern(&mut self) -> Result<Pattern, Diagnostic> {
        let start = self.peek().span;
        match self.peek().kind.clone() {
            TokenKind::Ident(s) if s == "_" => {
                self.bump();
                Ok(Pattern::Wildcard(start))
            }
            TokenKind::Int(n) => {
                self.bump();
                Ok(Pattern::Literal(Literal::Int(n), start))
            }
            TokenKind::Float(f) => {
                self.bump();
                Ok(Pattern::Literal(Literal::Float(f), start))
            }
            TokenKind::Str(s) => {
                self.bump();
                Ok(Pattern::Literal(Literal::Str(s), start))
            }
            TokenKind::KwTrue => {
                self.bump();
                Ok(Pattern::Literal(Literal::Bool(true), start))
            }
            TokenKind::KwFalse => {
                self.bump();
                Ok(Pattern::Literal(Literal::Bool(false), start))
            }
            TokenKind::Minus => {
                // отрицательный числовой литерал
                self.bump();
                match self.peek().kind {
                    TokenKind::Int(n) => {
                        self.bump();
                        Ok(Pattern::Literal(Literal::Int(-n), start))
                    }
                    TokenKind::Float(f) => {
                        self.bump();
                        Ok(Pattern::Literal(Literal::Float(-f), start))
                    }
                    _ => Err(Diagnostic::new(
                        "expected number literal after `-` in pattern",
                        start,
                    )),
                }
            }
            TokenKind::LBracket => self.parse_array_pattern(),
            TokenKind::LParen => {
                self.bump();
                if matches!(self.peek().kind, TokenKind::RParen) {
                    let end = self.bump().span;
                    return Ok(Pattern::Literal(Literal::Unit, start.merge(end)));
                }
                let mut pats = vec![self.parse_pattern()?];
                while self.eat(&TokenKind::Comma).is_some() {
                    pats.push(self.parse_pattern()?);
                }
                let end = self.expect(&TokenKind::RParen)?.span;
                if pats.len() == 1 {
                    Ok(pats.into_iter().next().unwrap())
                } else {
                    Ok(Pattern::Tuple(pats, start.merge(end)))
                }
            }
            TokenKind::LBrace => {
                // record pattern без типа
                self.parse_record_pattern(None, start)
            }
            TokenKind::Ident(_) => {
                let mut path = vec![self.parse_ident()?.0];
                while matches!(self.peek().kind, TokenKind::Dot)
                    && matches!(self.peek_at(1).kind, TokenKind::Ident(_))
                {
                    self.bump();
                    path.push(self.parse_ident()?.0);
                }
                // Variant с args?
                if matches!(self.peek().kind, TokenKind::LParen) {
                    self.bump();
                    let mut patterns = Vec::new();
                    let mut rest = false;
                    while !matches!(self.peek().kind, TokenKind::RParen) {
                        if self.eat(&TokenKind::DotDot).is_some() {
                            rest = true;
                            // .., больше элементов не разрешено
                            break;
                        }
                        patterns.push(self.parse_pattern()?);
                        if self.eat(&TokenKind::Comma).is_none() {
                            break;
                        }
                        self.skip_newlines();
                    }
                    let end = self.expect(&TokenKind::RParen)?.span;
                    return Ok(Pattern::Variant {
                        path,
                        kind: VariantPatternKind::Tuple { patterns, rest },
                        span: start.merge(end),
                    });
                }
                // Record pattern с типом?
                if matches!(self.peek().kind, TokenKind::LBrace) {
                    return self.parse_record_pattern(Some(path), start);
                }
                if path.len() == 1 {
                    let name = path.into_iter().next().unwrap();
                    // Заглавная буква — variant unit; иначе binding
                    if name
                        .chars()
                        .next()
                        .map(|c| c.is_ascii_uppercase())
                        .unwrap_or(false)
                    {
                        Ok(Pattern::Variant {
                            path: vec![name],
                            kind: VariantPatternKind::Unit,
                            span: start,
                        })
                    } else {
                        Ok(Pattern::Ident { name, span: start })
                    }
                } else {
                    Ok(Pattern::Variant {
                        path,
                        kind: VariantPatternKind::Unit,
                        span: start,
                    })
                }
            }
            other => Err(Diagnostic::new(
                format!("expected pattern, got {}", other.name()),
                start,
            )),
        }
    }

    fn parse_array_pattern(&mut self) -> Result<Pattern, Diagnostic> {
        let start = self.expect(&TokenKind::LBracket)?.span;
        let mut elems: Vec<ArrayPatternElem> = Vec::new();
        self.skip_newlines();
        while !matches!(self.peek().kind, TokenKind::RBracket) {
            if self.eat(&TokenKind::DotDot).is_some() {
                // ..rest или просто ..
                if let TokenKind::Ident(_) = self.peek().kind {
                    let (name, _) = self.parse_ident()?;
                    elems.push(ArrayPatternElem::RestBind(name));
                } else {
                    elems.push(ArrayPatternElem::Rest);
                }
            } else {
                elems.push(ArrayPatternElem::Item(self.parse_pattern()?));
            }
            if self.eat(&TokenKind::Comma).is_none() {
                break;
            }
            self.skip_newlines();
        }
        let end = self.expect(&TokenKind::RBracket)?.span;
        Ok(Pattern::Array {
            elems,
            span: start.merge(end),
        })
    }

    fn parse_record_pattern(
        &mut self,
        type_path: Option<Vec<String>>,
        start: Span,
    ) -> Result<Pattern, Diagnostic> {
        self.expect(&TokenKind::LBrace)?;
        let mut fields = Vec::new();
        let mut rest = false;
        self.skip_newlines();
        while !matches!(self.peek().kind, TokenKind::RBrace) {
            if self.eat(&TokenKind::DotDot).is_some() {
                rest = true;
                break;
            }
            let (name, name_span) = self.parse_ident()?;
            let pattern = if self.eat(&TokenKind::Colon).is_some() {
                Some(self.parse_pattern()?)
            } else {
                None // shorthand
            };
            fields.push(RecordPatternField {
                name,
                pattern,
                span: name_span,
            });
            if self.eat(&TokenKind::Comma).is_none() {
                break;
            }
            self.skip_newlines();
        }
        let end = self.expect(&TokenKind::RBrace)?.span;
        Ok(Pattern::Record {
            type_path,
            fields,
            rest,
            span: start.merge(end),
        })
    }
}

enum StmtOrExpr {
    Stmt(Stmt),
    Expr(Expr),
}

/// Удобная обёртка — лексирует и парсит исходник.
pub fn parse(src: &str) -> Result<Module, Diagnostic> {
    let tokens = crate::lexer::lex(src)?;
    let mut p = Parser::with_src(tokens, src.to_string());
    p.parse_module()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_or_panic(src: &str) -> Module {
        match parse(src) {
            Ok(m) => m,
            Err(e) => panic!("parse error: {} (span {})", e.message, e.span),
        }
    }

    #[test]
    fn empty_module() {
        let m = parse_or_panic("");
        assert!(m.items.is_empty());
    }

    #[test]
    fn module_decl() {
        let m = parse_or_panic("module a.b.c\n");
        assert_eq!(m.name, vec!["a", "b", "c"]);
    }

    #[test]
    fn fn_simple() {
        let m = parse_or_panic("fn double(x int) -> int => x * 2\n");
        assert_eq!(m.items.len(), 1);
        let Item::Fn(f) = &m.items[0] else { panic!() };
        assert_eq!(f.name, "double");
        assert_eq!(f.params.len(), 1);
        assert!(matches!(f.body, FnBody::Expr(_)));
    }

    #[test]
    fn fn_block_body() {
        let m = parse_or_panic(
            r#"
            fn area(r f64) -> f64 {
                let pi = 3.14
                pi * r * r
            }
            "#,
        );
        let Item::Fn(f) = &m.items[0] else { panic!() };
        assert!(matches!(f.body, FnBody::Block(_)));
    }

    #[test]
    fn fn_with_method_receiver() {
        let m = parse_or_panic("fn Point @magnitude() -> f64 => 0.0\n");
        let Item::Fn(f) = &m.items[0] else { panic!() };
        let r = f.receiver.as_ref().unwrap();
        assert_eq!(r.type_name, "Point");
        assert!(matches!(r.kind, ReceiverKind::Instance));
        assert_eq!(f.name, "magnitude");
    }

    #[test]
    fn fn_static_method() {
        let m = parse_or_panic("fn Account.new(owner str) -> Account => Account { }\n");
        let Item::Fn(f) = &m.items[0] else { panic!() };
        let r = f.receiver.as_ref().unwrap();
        assert_eq!(r.type_name, "Account");
        assert!(matches!(r.kind, ReceiverKind::Static));
    }

    #[test]
    fn type_record() {
        let m = parse_or_panic(
            r#"
            type User {
                readonly id u64
                name str
            }
            "#,
        );
        let Item::Type(t) = &m.items[0] else { panic!() };
        let TypeDeclKind::Record(fields) = &t.kind else {
            panic!()
        };
        assert_eq!(fields.len(), 2);
        assert!(fields[0].readonly);
    }

    #[test]
    fn type_sum() {
        let m = parse_or_panic("type Color | Red | Green | Blue\n");
        let Item::Type(t) = &m.items[0] else { panic!() };
        let TypeDeclKind::Sum(variants) = &t.kind else {
            panic!()
        };
        assert_eq!(variants.len(), 3);
    }

    #[test]
    fn type_protocol() {
        let m = parse_or_panic(
            r#"
            type Db protocol {
                query(q Sql) -> int
                exec(q Sql) -> int
            }
            "#,
        );
        let Item::Type(t) = &m.items[0] else { panic!() };
        let TypeDeclKind::Protocol(methods) = &t.kind else {
            panic!()
        };
        assert_eq!(methods.len(), 2);
    }

    #[test]
    fn match_with_arms() {
        let m = parse_or_panic(
            r#"
            fn f(x int) -> str => match x {
                0 => "zero"
                _ => "other"
            }
            "#,
        );
        assert_eq!(m.items.len(), 1);
    }

    #[test]
    fn array_lit_with_spread() {
        let m = parse_or_panic("fn t() -> int => [0, ...arr, 4]\n");
        let Item::Fn(f) = &m.items[0] else { panic!() };
        let FnBody::Expr(e) = &f.body else { panic!() };
        let ExprKind::ArrayLit(elems) = &e.kind else {
            panic!()
        };
        assert_eq!(elems.len(), 3);
        assert!(matches!(elems[1], ArrayElem::Spread(_)));
    }

    #[test]
    fn record_lit_with_spread() {
        let m = parse_or_panic(
            r#"
            fn make() -> User => { ...other, name: "bob" }
            "#,
        );
        let Item::Fn(f) = &m.items[0] else { panic!() };
        let FnBody::Expr(e) = &f.body else { panic!() };
        let ExprKind::RecordLit { fields, .. } = &e.kind else {
            panic!()
        };
        assert_eq!(fields.len(), 2);
        assert!(fields[0].is_spread);
        assert_eq!(fields[1].name, "name");
    }

    #[test]
    fn try_operator() {
        let m = parse_or_panic(
            r#"
            fn read() Throws -> int => parse(s)?
            "#,
        );
        assert_eq!(m.items.len(), 1);
    }

    #[test]
    fn if_let_pattern() {
        let m = parse_or_panic(
            r#"
            fn f(opt int) -> int {
                if let Some(x) = opt { x } else { 0 }
            }
            "#,
        );
        assert_eq!(m.items.len(), 1);
    }

    #[test]
    fn for_in_range() {
        let m = parse_or_panic(
            r#"
            fn loop_test() -> int {
                let mut s = 0
                for i in 0..10 {
                    s += i
                }
                s
            }
            "#,
        );
        assert_eq!(m.items.len(), 1);
    }

    #[test]
    fn handler_lit_in_with() {
        let m = parse_or_panic(
            r#"
            fn run() {
                with Db = handler Db {
                    query(q) => []
                    exec(q) => 0
                } {
                    work()
                }
            }
            "#,
        );
        assert_eq!(m.items.len(), 1);
    }

    #[test]
    fn test_decl() {
        let m = parse_or_panic(
            r#"
            test "addition works" {
                assert 1 + 1 == 2
            }
            "#,
        );
        assert_eq!(m.items.len(), 1);
        let Item::Test(t) = &m.items[0] else { panic!() };
        assert_eq!(t.name, "addition works");
    }

    #[test]
    fn array_pattern() {
        let m = parse_or_panic(
            r#"
            fn first(xs []int) -> int => match xs {
                [] => 0
                [x] => x
                [head, ..] => head
            }
            "#,
        );
        assert_eq!(m.items.len(), 1);
    }
}
