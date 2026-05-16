//! Recursive-descent parser для Nova.
//!
//! Один большой модуль: `Parser` — состояние с указателем на токены,
//! методы для каждого нетерминала. Никаких внешних парсер-комбинаторов:
//! минимум зависимостей в bootstrap'е.

use crate::ast::*;
use crate::diag::{Diagnostic, Span};
use crate::lexer::{Token, TokenKind};

/// Plan 33.1 (D24): contract-related атрибуты, собранные перед `fn`.
///
/// Передаются из `parse_item` в `parse_fn`. По умолчанию — все поля
/// в `Default`/`None`/`Unknown` (backward-compat для функций без
/// контрактов и атрибутов).
#[derive(Debug, Clone, Default)]
pub(crate) struct ContractAttrs {
    pub verify_mode: VerifyMode,
    pub verify_timeout_ms: Option<u32>,
    pub purity: Purity,
    pub is_trusted: bool,
}

impl ContractAttrs {
    pub(crate) fn is_empty(&self) -> bool {
        matches!(self.verify_mode, VerifyMode::Default)
            && self.verify_timeout_ms.is_none()
            && matches!(self.purity, Purity::Unknown)
            && !self.is_trusted
    }
}

pub struct Parser {
    tokens: Vec<Token>,
    pos: usize,
    /// Когда true — `Ident { ... }` не парсится как record-литерал
    /// (используется в head-позициях `if`/`while`/`match`-scrutinee
    /// и `for`-итераторах, чтобы `{` следующего блока не съедался).
    no_struct_lit: bool,
    /// Когда true — `expr(args) { ... }` не парсится как call-with-
    /// trailing-block. Используется в head-позиции `match`-scrutinee,
    /// чтобы `match foo() { Some(i) => ... }` не рассматривался как
    /// `foo()` с trailing-block'ом.
    no_trailing_block: bool,
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
            no_trailing_block: false,
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

    /// Аналогично, но также блокирует trailing-block-attachment к call'у.
    /// Используется в match-scrutinee позиции.
    fn with_no_struct_or_trailing<R>(
        &mut self,
        f: impl FnOnce(&mut Self) -> Result<R, Diagnostic>,
    ) -> Result<R, Diagnostic> {
        let saved_struct = self.no_struct_lit;
        let saved_trailing = self.no_trailing_block;
        self.no_struct_lit = true;
        self.no_trailing_block = true;
        let result = f(self);
        self.no_struct_lit = saved_struct;
        self.no_trailing_block = saved_trailing;
        result
    }

    /// Точка входа: парсит модуль (файл целиком).
    pub fn parse_module(&mut self) -> Result<Module, Diagnostic> {
        self.skip_newlines();
        let start = self.peek().span;

        // Plan 45 Ф.2 / D104: `//!` Inner doc-comments — собираются как
        // module-level. Спецификация: они валидны только в начале файла
        // (после `module X` и imports, до первой декларации), но для
        // robustness'а парсер собирает Inner-токены отовсюду на module-
        // уровне (вне function-bodies), сливая в `module_doc`.
        let mut module_doc: Option<crate::ast::DocBlock> =
            self.consume_doc_block_of_kind(crate::lexer::DocCommentKind::Inner);

        // Plan 42.16 Ф.2: module-level атрибуты (`#forbid` / `#cfg` /
        // `#doc`) идут **ПЕРЕД** `module` declaration — консистентно с
        // item-level атрибутами (`#cfg`/`#realtime`/`#pure` перед `fn`).
        // `#cfg` семантически — гейт «существует ли файл», логично
        // читать до `module`. AI-first: условия файла видны первыми.
        let module_attrs = self.parse_module_attrs()?;
        // Plan 45 Ф.22.1 / D105: module-level doc-attrs (`#stable`/etc.)
        // — могут идти между classic module attrs (`#cfg`/`#forbid`) и
        // `module` declaration. Используем тот же parse_doc_attrs() что
        // и для items.
        let module_doc_attrs = self.parse_doc_attrs()?;

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
            // Plan 45 Ф.2 / D104: на каждой итерации loop'а проверяем,
            // не появился ли Inner doc-comment (`//!`) — допустимо
            // между imports и first item; merge'им в `module_doc`.
            if let Some(extra_inner) = self
                .consume_doc_block_of_kind(crate::lexer::DocCommentKind::Inner)
            {
                module_doc = Some(match module_doc.take() {
                    None => extra_inner,
                    Some(prev) => crate::ast::DocBlock {
                        kind: crate::lexer::DocCommentKind::Inner,
                        content: format!("{}\n\n{}", prev.content, extra_inner.content),
                        span: prev.span.merge(extra_inner.span),
                    },
                });
                self.skip_newlines();
            }
            if matches!(self.peek().kind, TokenKind::Eof) {
                break;
            }
            // Plan 42.17 Ф.5: `#forbid` / `#doc` — module-level атрибуты,
            // валидны ТОЛЬКО перед `module` declaration. После — чёткая
            // ошибка (раньше падало в parse_item с misleading «expected
            // fn/type/...»). `#cfg` — исключение: легитимен как item-level
            // атрибут перед fn/type/const, его здесь не трогаем.
            if matches!(self.peek().kind, TokenKind::Hash) {
                let next_kind = self.tokens.get(self.pos + 1).map(|t| &t.kind);
                let is_forbid = matches!(next_kind, Some(TokenKind::KwForbid));
                // `#doc` неоднозначен: D101 `#doc "string"` (module-attr) vs
                // D105 `#doc(summary=...)` / `#doc(inline)` (item-attr).
                // Disambig по третьему токену: string → module-attr;
                // `(` → item-attr (передаём в parse_item).
                let is_doc_module_attr = if let Some(TokenKind::Ident(n)) = next_kind {
                    if n == "doc" {
                        let third = self.tokens.get(self.pos + 2).map(|t| &t.kind);
                        matches!(third, Some(TokenKind::Str(_)))
                    } else {
                        false
                    }
                } else {
                    false
                };
                if is_forbid || is_doc_module_attr {
                    let attr = if is_forbid { "#forbid" } else { "#doc" };
                    return Err(Diagnostic::new(
                        format!(
                            "`{attr}` is a module-level attribute — it must \
                             precede the `module` declaration, not follow it"),
                        self.peek().span));
                }
            }
            // Plan 45 Ф.24.11: `#doc_inline` / `#doc_no_inline` before import/re-export.
            // Collect doc-attrs that apply to an immediately-following import statement.
            let import_doc_attrs = if matches!(self.peek().kind, TokenKind::Hash) {
                let next_kind = self.tokens.get(self.pos + 1).map(|t| &t.kind);
                let is_import_doc_attr = matches!(next_kind,
                    Some(TokenKind::Ident(n)) if n == "doc_inline" || n == "no_inline" || n == "doc_no_inline");
                let is_doc_paren = if let Some(TokenKind::Ident(n)) = next_kind {
                    if n == "doc" {
                        let third = self.tokens.get(self.pos + 2).map(|t| &t.kind);
                        matches!(third, Some(TokenKind::LParen))
                    } else { false }
                } else { false };
                if is_import_doc_attr || is_doc_paren {
                    let attrs = self.parse_doc_attrs()?;
                    self.skip_newlines();
                    attrs
                } else {
                    Vec::new()
                }
            } else {
                Vec::new()
            };
            if matches!(self.peek().kind, TokenKind::KwImport | TokenKind::KwUse) {
                imports.push(self.parse_import_with_attrs(import_doc_attrs)?);
                continue;
            }
            // Plan 35 sub-plan 35.A (R26): `export import X` re-export
            // (D29). Lookahead-1 чтобы не съесть `export` для других items
            // (export fn, export type, etc.). Только `export import|use` —
            // дополнительный путь для парсинга import'а.
            if matches!(self.peek().kind, TokenKind::KwExport)
                && self.pos + 1 < self.tokens.len()
                && matches!(
                    self.tokens[self.pos + 1].kind,
                    TokenKind::KwImport | TokenKind::KwUse
                )
            {
                imports.push(self.parse_import_with_attrs(import_doc_attrs)?);
                continue;
            }
            // If doc_attrs were collected but no import follows, they belong to parse_item.
            // parse_item will re-read them from source — but we've consumed them here.
            // This is fine: if #doc_inline is before a non-import, it's an error in parse_doc_attrs
            // (unknown attr). Drop them silently (parse_item will re-encounter the next token).
            // Plan 42.14 Ф.2: parse_item возвращает Option — None если
            // item gated `#cfg(...)` predicate'ом который inactive для
            // current target/features → item дропается на parse-этапе.
            if let Some(item) = self.parse_item()? {
                items.push(item);
            }
        }
        let span = start.merge(self.peek().span);
        // Plan 42 Sub-plan 42.4: Module.peer_files оставляем пустым на
        // parser уровне — parser не знает path к исходнику. Caller'ы
        // (imports.rs::resolve_imports_inline / test_runner / cmd_check)
        // заполняют `peer_files` после parse.
        Ok(Module {
            name: module_name,
            imports,
            items,
            attrs: module_attrs,
            doc_attrs: module_doc_attrs,
            span,
            peer_files: Vec::new(),
            doc: module_doc,
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

    /// Plan 45 Ф.2 / D104: консумит подряд идущие `DocComment`-токены
    /// заданного `kind`'а (пропуская newline/semicolon между ними) и
    /// склеивает их content в один `DocBlock`. Возвращает `None`, если
    /// ни одного doc-токена заданного kind'а на текущей позиции нет.
    ///
    /// Несколько подряд идущих doc-блоков того же kind'а (после
    /// blank-line) объединяются: лексер уже сливает строки в один токен,
    /// но parser может встретить два таких токена, разделённых newline'ом.
    /// Этот метод объединяет их через `\n\n` (markdown paragraph break).
    fn consume_doc_block_of_kind(
        &mut self,
        kind: crate::lexer::DocCommentKind,
    ) -> Option<crate::ast::DocBlock> {
        // Сохраним стартовую позицию — чтобы можно было откатиться,
        // если ничего не нашли.
        let start_pos = self.pos;
        // Пропускаем ведущие newline (они между предыдущим item'ом и
        // следующим doc-блоком).
        while matches!(
            self.peek().kind,
            TokenKind::Newline | TokenKind::Semicolon
        ) {
            self.bump();
        }
        let mut accumulated: Option<crate::ast::DocBlock> = None;
        loop {
            match &self.peek().kind {
                TokenKind::DocComment {
                    kind: tok_kind,
                    content,
                } if *tok_kind == kind => {
                    let content = content.clone();
                    let span = self.peek().span;
                    self.bump();
                    accumulated = Some(match accumulated {
                        None => crate::ast::DocBlock {
                            kind,
                            content,
                            span,
                        },
                        Some(prev) => crate::ast::DocBlock {
                            kind,
                            content: format!("{}\n\n{}", prev.content, content),
                            span: prev.span.merge(span),
                        },
                    });
                    // Между doc-блоками допускаем blank-line.
                    while matches!(
                        self.peek().kind,
                        TokenKind::Newline | TokenKind::Semicolon
                    ) {
                        self.bump();
                    }
                }
                _ => break,
            }
        }
        if accumulated.is_none() {
            // Ничего не нашли — откатимся, чтобы caller'ские
            // `skip_newlines` отработали как раньше.
            self.pos = start_pos;
        }
        accumulated
    }

    /// Plan 45 Ф.2: helper для случаев, когда doc-token попался в
    /// неожиданной позиции (например, внутри тела функции). Тихо
    /// съедает их, чтобы не валить парсинг. Lint в Ф.3 даст warning
    /// «orphan doc-comment».
    fn skip_stray_doc_comments(&mut self) {
        while matches!(self.peek().kind, TokenKind::DocComment { .. }) {
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
        // Plan 35 sub-plan 35.A (R26): stop on `.{` — selective items follow.
        while matches!(self.peek().kind, TokenKind::Dot)
            && self.pos + 1 < self.tokens.len()
            && !matches!(self.tokens[self.pos + 1].kind, TokenKind::LBrace)
        {
            self.bump(); // .
            let (next, _) = self.parse_ident()?;
            parts.push(next);
        }
        Ok(parts)
    }

    // ─── module-level attributes ─────────────────────────────────────────

    /// Plan 42.16 Ф.2: парсит module-level атрибуты ПЕРЕД `module`
    /// declaration. `#forbid X, Y` / `#cfg(<expr>)` / `#doc "..."`.
    ///
    /// **Decision 2026-05-13:** `#requires` отвергнуто — implicit
    /// effects в function signatures противоречат Nova AI-first
    /// explicit principle (D62). `#forbid` оставлен как security boundary.
    fn parse_module_attrs(&mut self) -> Result<Vec<ModuleAttr>, Diagnostic> {
        let mut module_attrs = Vec::new();
        loop {
            self.skip_newlines();
            if !matches!(self.peek().kind, TokenKind::Hash) {
                break;
            }
            let next_kind = self.tokens.get(self.pos + 1).map(|t| t.kind.clone());
            // `forbid` is keyword (KwForbid) in Nova; `cfg` и `doc` — обычные idents.
            let is_forbid = matches!(next_kind, Some(TokenKind::KwForbid));
            let is_cfg = matches!(&next_kind, Some(TokenKind::Ident(name)) if name == "cfg");
            let is_doc = matches!(&next_kind, Some(TokenKind::Ident(name)) if name == "doc");
            let is_must_verify_module = matches!(&next_kind, Some(TokenKind::Ident(name)) if name == "must_verify_module");
            let is_proof_budget = matches!(&next_kind, Some(TokenKind::Ident(name)) if name == "proof_budget");
            if !is_forbid && !is_cfg && !is_doc && !is_must_verify_module && !is_proof_budget {
                break; // not a module-level attribute
            }
            let attr_start = self.peek().span;
            self.bump(); // #

            if is_must_verify_module {
                // Plan 33.3 Ф.13: `#must_verify_module` — все функции MustVerify.
                self.bump(); // must_verify_module (ident)
                self.expect_newline_or_eof()?;
                let attr_end = self.tokens[self.pos.saturating_sub(1)].span;
                module_attrs.push(ModuleAttr {
                    kind: ModuleAttrKind::MustVerifyModule,
                    effects: Vec::new(),
                    span: attr_start.merge(attr_end),
                });
                continue;
            }

            if is_proof_budget {
                // Ф.3.4 (Plan 33.6): `#proof_budget(timeout_ms=N, vc_count_max=M)`.
                self.bump(); // proof_budget
                let mut timeout_ms: Option<u32> = None;
                let mut vc_count_max: Option<u32> = None;
                if matches!(self.peek().kind, TokenKind::LParen) {
                    self.bump(); // (
                    loop {
                        if matches!(self.peek().kind, TokenKind::RParen) { break; }
                        let (key, key_span) = self.parse_ident()?;
                        if !matches!(self.peek().kind, TokenKind::Eq) {
                            return Err(Diagnostic::new(
                                "`#proof_budget` key must be followed by `=`", key_span));
                        }
                        self.bump(); // =
                        let val_span = self.peek().span;
                        let val = if let TokenKind::Int(n) = self.peek().kind {
                            let v = n as u32;
                            self.bump();
                            v
                        } else {
                            return Err(Diagnostic::new(
                                "`#proof_budget` value must be integer literal", val_span));
                        };
                        match key.as_str() {
                            "timeout_ms" => timeout_ms = Some(val),
                            "vc_count_max" => vc_count_max = Some(val),
                            _ => return Err(Diagnostic::new(
                                format!("unknown `#proof_budget` key `{}`; \
                                         expected `timeout_ms` or `vc_count_max`", key),
                                key_span)),
                        }
                        if matches!(self.peek().kind, TokenKind::Comma) { self.bump(); }
                    }
                    if !matches!(self.peek().kind, TokenKind::RParen) {
                        return Err(Diagnostic::new("expected `)` to close `#proof_budget(...)`", self.peek().span));
                    }
                    self.bump(); // )
                }
                self.expect_newline_or_eof()?;
                let attr_end = self.tokens[self.pos.saturating_sub(1)].span;
                module_attrs.push(ModuleAttr {
                    kind: ModuleAttrKind::ProofBudget { timeout_ms, vc_count_max },
                    effects: Vec::new(),
                    span: attr_start.merge(attr_end),
                });
                continue;
            }

            if is_doc {
                // Plan 42.11: `#doc "..."` — module-level documentation line.
                self.bump(); // doc (ident)
                let doc_span = self.peek().span;
                let text = if let TokenKind::Str(s) = &self.peek().kind {
                    let v = s.clone();
                    self.bump();
                    v
                } else {
                    return Err(Diagnostic::new(
                        "expected string literal after `#doc`", doc_span));
                };
                self.expect_newline_or_eof()?;
                let attr_end = self.tokens[self.pos.saturating_sub(1)].span;
                module_attrs.push(ModuleAttr {
                    kind: ModuleAttrKind::Doc(text),
                    effects: Vec::new(),
                    span: attr_start.merge(attr_end),
                });
                continue;
            }

            if is_forbid {
                self.bump(); // forbid
                let mut effects: Vec<String> = Vec::new();
                loop {
                    let (name, _) = self.parse_ident()?;
                    effects.push(name);
                    if matches!(self.peek().kind, TokenKind::Comma) {
                        self.bump();
                    } else {
                        break;
                    }
                }
                self.expect_newline_or_eof()?;
                let attr_end = self.tokens[self.pos.saturating_sub(1)].span;
                module_attrs.push(ModuleAttr {
                    kind: ModuleAttrKind::Forbid,
                    effects,
                    span: attr_start.merge(attr_end),
                });
            } else {
                // Plan 42.12 + 42.14 + 42.16: `#cfg(<expr>)` —
                // feature/target_os + операторы `|| && !`.
                self.bump(); // cfg (ident)
                if !matches!(self.peek().kind, TokenKind::LParen) {
                    return Err(Diagnostic::new("expected `(` after `#cfg`", self.peek().span));
                }
                self.bump(); // (
                let pred = self.parse_cfg_predicate()?;
                if !matches!(self.peek().kind, TokenKind::RParen) {
                    return Err(Diagnostic::new(
                        "expected `)` closing #cfg predicate", self.peek().span));
                }
                self.bump(); // )
                self.expect_newline_or_eof()?;
                let attr_end = self.tokens[self.pos.saturating_sub(1)].span;
                module_attrs.push(ModuleAttr {
                    kind: ModuleAttrKind::Cfg(pred),
                    effects: Vec::new(),
                    span: attr_start.merge(attr_end),
                });
            }
        }
        Ok(module_attrs)
    }

    // ─── #cfg predicate ──────────────────────────────────────────────────

    /// Plan 42.16 Ф.1: `#cfg` predicate с операторами `|| && !`
    /// (Go/C-style вместо функц-формы `any/all/not` из Plan 42.14).
    ///
    /// Grammar (precedence: `!` > `&&` > `||`, скобки override):
    /// ```text
    ///   cfg_expr := cfg_or
    ///   cfg_or   := cfg_and ('||' cfg_and)*
    ///   cfg_and  := cfg_not ('&&' cfg_not)*
    ///   cfg_not  := '!' cfg_not | cfg_atom
    ///   cfg_atom := '(' cfg_expr ')' | key '=' string
    /// ```
    ///
    /// AST: `a || b || c` → `Any([a,b,c])`, `a && b` → `All`, `!a` → `Not`
    /// (имена вариантов internal — пользователь видит только операторы).
    /// Caller уже consumed opening `(` после `#cfg`; этот метод парсит
    /// один cfg_expr (не consumes closing `)` верхнего уровня).
    fn parse_cfg_predicate(&mut self) -> Result<CfgPredicate, Diagnostic> {
        self.parse_cfg_or()
    }

    /// `cfg_or := cfg_and ('||' cfg_and)*` — самый низкий приоритет.
    fn parse_cfg_or(&mut self) -> Result<CfgPredicate, Diagnostic> {
        let first = self.parse_cfg_and()?;
        let mut alts = vec![first];
        while matches!(self.peek().kind, TokenKind::PipePipe) {
            self.bump(); // ||
            alts.push(self.parse_cfg_and()?);
        }
        if alts.len() == 1 {
            Ok(alts.into_iter().next().unwrap())
        } else {
            Ok(CfgPredicate::Any(alts))
        }
    }

    /// `cfg_and := cfg_not ('&&' cfg_not)*`
    fn parse_cfg_and(&mut self) -> Result<CfgPredicate, Diagnostic> {
        let first = self.parse_cfg_not()?;
        let mut parts = vec![first];
        while matches!(self.peek().kind, TokenKind::AmpAmp) {
            self.bump(); // &&
            parts.push(self.parse_cfg_not()?);
        }
        if parts.len() == 1 {
            Ok(parts.into_iter().next().unwrap())
        } else {
            Ok(CfgPredicate::All(parts))
        }
    }

    /// `cfg_not := '!' cfg_not | cfg_atom`
    fn parse_cfg_not(&mut self) -> Result<CfgPredicate, Diagnostic> {
        if matches!(self.peek().kind, TokenKind::Bang) {
            self.bump(); // !
            let inner = self.parse_cfg_not()?;
            Ok(CfgPredicate::Not(Box::new(inner)))
        } else {
            self.parse_cfg_atom()
        }
    }

    /// `cfg_atom := '(' cfg_expr ')' | key '=' string`
    fn parse_cfg_atom(&mut self) -> Result<CfgPredicate, Diagnostic> {
        // Скобочная группа.
        if matches!(self.peek().kind, TokenKind::LParen) {
            self.bump(); // (
            let inner = self.parse_cfg_or()?;
            if !matches!(self.peek().kind, TokenKind::RParen) {
                return Err(Diagnostic::new(
                    "expected `)` closing #cfg group", self.peek().span));
            }
            self.bump(); // )
            return Ok(inner);
        }
        // Атом: key '=' string.
        let start = self.peek().span;
        let (key, _) = self.parse_ident()?;
        if !matches!(self.peek().kind, TokenKind::Eq) {
            return Err(Diagnostic::new(
                format!("expected `=` after `{}` in #cfg predicate", key),
                self.peek().span));
        }
        self.bump(); // =
        let value_span = self.peek().span;
        let value = if let TokenKind::Str(s) = &self.peek().kind {
            let v = s.clone();
            self.bump();
            v
        } else {
            return Err(Diagnostic::new(
                "expected string literal in #cfg predicate", value_span));
        };
        match key.as_str() {
            "feature" => Ok(CfgPredicate::Feature(value)),
            "target_os" => Ok(CfgPredicate::TargetOs(value)),
            other => Err(Diagnostic::new(
                format!("unknown #cfg key `{}` — expected `feature` or `target_os` \
                         (composition via `||` `&&` `!`)", other),
                start)),
        }
    }

    // ─── imports ─────────────────────────────────────────────────────────

    fn parse_import_with_attrs(&mut self, doc_attrs: Vec<crate::ast::DocAttr>) -> Result<Import, Diagnostic> {
        self.parse_import_inner(doc_attrs)
    }

    fn parse_import(&mut self) -> Result<Import, Diagnostic> {
        self.parse_import_inner(Vec::new())
    }

    fn parse_import_inner(&mut self, doc_attrs: Vec<crate::ast::DocAttr>) -> Result<Import, Diagnostic> {
        // Plan 35 sub-plan 35.A: support `import X.Y.{A, B as C}` selective
        // и `export import X.{A}` re-export.
        // Detect leading `export` keyword. Парсер уже потребил `KwExport`
        // в parse_item только перед fn/type/const/let — не перед import.
        // Здесь мы стартуем с `import` или `export import` (или `use`).
        let start = self.peek().span;
        let is_export = if matches!(self.peek().kind, TokenKind::KwExport) {
            self.bump();
            true
        } else {
            false
        };
        // Принимаем как `import`, так и `use` — оба парсятся идентично:
        // `use` будет использоваться для embedding (D39), но в bootstrap
        // мы не различаем.
        self.bump();
        let path = self.parse_dotted_path()?;
        // Optional `.{Item1, Item2 as Alias, ...}` — selective items.
        // Префикс — `.` (= Dot), затем `{`. parse_dotted_path остановился на
        // `.` перед `{`, надо его съесть.
        let items = if matches!(self.peek().kind, TokenKind::Dot)
            && self.pos + 1 < self.tokens.len()
            && matches!(self.tokens[self.pos + 1].kind, TokenKind::LBrace)
        {
            self.bump(); // .
            self.bump(); // {
            let mut items = Vec::new();
            loop {
                if matches!(self.peek().kind, TokenKind::RBrace) {
                    break;
                }
                let item_start = self.peek().span;
                let (name, _) = self.parse_ident()?;
                let alias = if matches!(self.peek().kind, TokenKind::KwAs) {
                    self.bump();
                    let (a, _) = self.parse_ident()?;
                    Some(a)
                } else {
                    None
                };
                let item_end = self.tokens[self.pos.saturating_sub(1)].span;
                items.push(ImportItem {
                    name,
                    alias,
                    span: item_start.merge(item_end),
                });
                if matches!(self.peek().kind, TokenKind::Comma) {
                    self.bump();
                } else {
                    break;
                }
            }
            if !matches!(self.peek().kind, TokenKind::RBrace) {
                let span = self.peek().span;
                return Err(Diagnostic::new(
                    "expected `}` to close selective-import list",
                    span,
                ));
            }
            self.bump(); // }
            if items.is_empty() {
                let span = start;
                return Err(Diagnostic::new(
                    "selective-import list must contain at least one item",
                    span,
                ));
            }
            Some(items)
        } else {
            None
        };
        let alias = if matches!(self.peek().kind, TokenKind::KwAs) {
            self.bump();
            let (name, _) = self.parse_ident()?;
            Some(name)
        } else {
            None
        };
        // Mutual exclusivity: нельзя одновременно selective items и alias.
        if items.is_some() && alias.is_some() {
            return Err(Diagnostic::new(
                "cannot combine `import X.{A, B}` with `as` alias — use \
                 alias per-item: `import X.{A as Aliased, B}`",
                start,
            ));
        }
        self.expect_newline_or_eof()?;
        let span = start.merge(self.tokens[self.pos.saturating_sub(1)].span);
        Ok(Import { path, items, alias, is_export, span, doc_attrs })
    }

    // ─── top-level items ─────────────────────────────────────────────────

    fn parse_item(&mut self) -> Result<Option<Item>, Diagnostic> {
        // Plan 45 Ф.2 / D104: outer doc-comment (`///`) перед декларацией
        // — собираем; передаём дальше через `pending_doc`. Если за doc'ом
        // следует `let` / `test` (которые doc-поля не имеют) — doc
        // отбрасывается (lint warning в Ф.3).
        let pending_doc =
            self.consume_doc_block_of_kind(crate::lexer::DocCommentKind::Outer);

        // Plan 42.14 Ф.2: item-level `#cfg(...)`. Парсится ПЕРЕД
        // export/external/realtime/contract attrs. Если predicate inactive
        // для current target/features — item пропускается (return None),
        // но всё равно полностью парсится (для корректного advance токенов).
        let item_cfg: Option<CfgPredicate> = if matches!(self.peek().kind, TokenKind::Hash)
            && matches!(
                self.tokens.get(self.pos + 1).map(|t| &t.kind),
                Some(TokenKind::Ident(name)) if name == "cfg"
            )
        {
            self.bump(); // #
            self.bump(); // cfg
            if !matches!(self.peek().kind, TokenKind::LParen) {
                return Err(Diagnostic::new("expected `(` after `#cfg`", self.peek().span));
            }
            self.bump(); // (
            let pred = self.parse_cfg_predicate()?;
            if !matches!(self.peek().kind, TokenKind::RParen) {
                return Err(Diagnostic::new(
                    "expected `)` closing #cfg predicate", self.peek().span));
            }
            self.bump(); // )
            self.skip_newlines();
            Some(pred)
        } else {
            None
        };

        // Plan 45 Ф.3 / D105: doc-атрибуты `#deprecated(...)`, `#since(...)`,
        // `#stable[(...)]`, `#unstable(...)`, `#experimental(...)`,
        // `#hide_doc`, `#doc_alias(...)`, `#doc(...)`. Парсятся ПЕРЕД
        // export/external/realtime/contract attrs, ПОСЛЕ `#cfg`.
        // Несколько подряд — собираются в Vec; передаются в parse_fn /
        // parse_type_decl / parse_const_decl через pending_doc_attrs.
        let pending_doc_attrs = self.parse_doc_attrs()?;

        // Plan 52 Ф.1: `#from_fields` — маркер на декларации типа.
        // Помечает str-keyed map-тип для D55 map-coercion (`{field: v}`).
        // Парсится ПЕРЕД `export` (консистентно с `#cfg`) и только перед
        // `type`. Контекстный разбор после `#` (не keyword).
        let type_attrs = self.parse_type_attrs()?;

        let is_export = self.eat(&TokenKind::KwExport).is_some();
        // D82: `external` modifier — между `export` и `fn`. Только для fn.
        let is_external = self.eat(&TokenKind::KwExternal).is_some();
        if is_external {
            // Только `fn` допустимо после `external`.
            if !matches!(self.peek().kind, TokenKind::KwFn) {
                let span = self.peek().span;
                return Err(Diagnostic::new(
                    format!(
                        "`external` is only valid before `fn`, got {}",
                        self.peek().kind.name()
                    ),
                    span,
                ));
            }
        }
        // Plan 16 (D64 §3697) + Plan 33.1 (D-attr-syntax):
        // `#realtime` / `#realtime nogc` префикс перед `fn`. Эквивалент
        // оборачивания body в `realtime { ... }`. Атрибуты — через `#`
        // (а не `@`, чтобы не конфликтовать с receiver-prefix `@field`).
        // Парсим в RealtimeAttr enum, передаём в parse_fn.
        let realtime_attr = self.parse_realtime_attr()?;
        if !matches!(realtime_attr, RealtimeAttr::None)
            && !matches!(self.peek().kind, TokenKind::KwFn)
            && !matches!(self.peek().kind, TokenKind::Hash)
        {
            let span = self.peek().span;
            return Err(Diagnostic::new(
                "`#realtime` is only valid before `fn`",
                span,
            ));
        }
        // Plan 33.1 (D24): `#verify` / `#unverified` / `#verify_timeout(ms)` /
        // `#pure` — contract-related атрибуты перед `fn`. Парсятся
        // отдельно от `#realtime`, могут идти в любом порядке.
        // Не keyword'ы в лексере (контекстный разбор после `#`).
        let contract_attrs = self.parse_contract_attrs()?;
        if !contract_attrs.is_empty()
            && !matches!(self.peek().kind, TokenKind::KwFn | TokenKind::KwExternal)
        {
            let span = self.peek().span;
            return Err(Diagnostic::new(
                "contract attributes (`#verify` / `#unverified` / `#verify_timeout` / `#pure` / `#trusted`) are only valid before `fn` or `external fn`",
                span,
            ));
        }
        // Plan 33.3 Ф.13: #trusted external fn — парсим `external` здесь,
        // если contract_attrs содержат #trusted.
        let is_external = if contract_attrs.is_trusted && matches!(self.peek().kind, TokenKind::KwExternal) {
            self.bump(); // external
            if !matches!(self.peek().kind, TokenKind::KwFn) {
                let span = self.peek().span;
                return Err(Diagnostic::new("`external` is only valid before `fn`", span));
            }
            true
        } else {
            is_external
        };
        // Plan 52 Ф.1: `#from_fields` валиден только перед `type`-декларацией.
        if !type_attrs.is_empty()
            && !matches!(self.peek().kind, TokenKind::KwType)
        {
            let span = self.peek().span;
            return Err(Diagnostic::new(
                "`#from_fields` is only valid before `type`",
                span,
            ));
        }
        let parsed = match self.peek().kind {
            TokenKind::KwFn => Item::Fn(self.parse_fn(is_export, is_external, realtime_attr, contract_attrs, pending_doc.clone(), pending_doc_attrs.clone())?),
            TokenKind::KwType => Item::Type(self.parse_type_decl(is_export, type_attrs, pending_doc.clone(), pending_doc_attrs.clone())?),
            TokenKind::KwLet => {
                if let Some(d) = &pending_doc {
                    // Plan 45 Ф.3: orphan `///` warning — doc-comment'ы
                    // не имеют семантики для `let`/`test`/`lemma` items.
                    eprintln!(
                        "warning: doc-comment (`///`) before `let` is ignored \
                         — `let` declarations are not documented (Plan 45 Ф.3). \
                         span: {:?}",
                        d.span
                    );
                }
                Item::Let(self.parse_let_decl()?)
            }
            TokenKind::KwConst => Item::Const(self.parse_const_decl(is_export, pending_doc.clone(), pending_doc_attrs.clone())?),
            TokenKind::KwTest if !is_export => {
                if let Some(d) = &pending_doc {
                    eprintln!(
                        "warning: doc-comment (`///`) before `test` is ignored \
                         — `test` declarations are not documented (Plan 45 Ф.3). \
                         span: {:?}",
                        d.span
                    );
                }
                Item::Test(self.parse_test_decl()?)
            }
            // Plan 57: контекстный `bench` ident. Распознаём как bench-decl
            // только если за ним идёт string-literal: `bench "name" { ... }`.
            // Иначе — обычный ident-expr (`bench.opaque(v)`, etc.) который
            // тут не valid top-level item, ошибка ниже даст «expected fn/type/...».
            TokenKind::Ident(ref s) if s == "bench" && !is_export
                && matches!(self.peek_at(1).kind, TokenKind::Str(_)) =>
            {
                if let Some(d) = &pending_doc {
                    eprintln!(
                        "warning: doc-comment (`///`) before `bench` is ignored \
                         — `bench` declarations are not documented (Plan 57). \
                         span: {:?}",
                        d.span
                    );
                }
                Item::Bench(self.parse_bench_decl()?)
            }
            TokenKind::KwLemma if !is_export && !is_external => {
                if let Some(d) = &pending_doc {
                    eprintln!(
                        "warning: doc-comment (`///`) before `lemma` is ignored \
                         — `lemma` declarations are not documented (Plan 45 Ф.3). \
                         span: {:?}",
                        d.span
                    );
                }
                Item::Lemma(self.parse_lemma_decl()?)
            }
            _ => {
                let span = self.peek().span;
                return Err(Diagnostic::new(
                    format!(
                        "expected fn / type / let / const / test, got {}",
                        self.peek().kind.name()
                    ),
                    span,
                ));
            }
        };

        // Plan 42.14 Ф.2: item-level `#cfg` — eval predicate. Если inactive
        // для current target/features, item полностью parsed но дропается
        // (return None). Eval использует те же env-источники что imports.rs
        // (NOVA_TARGET_OS / NOVA_FEATURES) — consistency между фазами.
        if let Some(pred) = item_cfg {
            let target = crate::imports::current_target_os();
            let features = crate::imports::enabled_features();
            if !crate::imports::eval_cfg_predicate(&pred, target, &features) {
                return Ok(None);
            }
        }
        Ok(Some(parsed))
    }

    /// Plan 16 (D64 §3697) + Plan 33.1 (D-attr-syntax):
    /// parse `#realtime` или `#realtime nogc` атрибут перед fn-declaration.
    /// Возвращает RealtimeAttr::None если префикса нет.
    ///
    /// `realtime` — keyword (TokenKind::KwRealtime), `nogc` — обычный
    /// identifier (не keyword в lexer'е). Префикс `#`, не `@`
    /// (разделение от receiver-prefix).
    fn parse_realtime_attr(&mut self) -> Result<RealtimeAttr, Diagnostic> {
        if !matches!(self.peek().kind, TokenKind::Hash) {
            return Ok(RealtimeAttr::None);
        }
        // Look ahead: должно быть `#` затем `realtime` keyword.
        if !matches!(self.peek_at(1).kind, TokenKind::KwRealtime) {
            return Ok(RealtimeAttr::None);
        }
        self.bump(); // #
        self.bump(); // realtime
        // Optional `nogc` modifier (Ident, не keyword).
        let nogc = if let TokenKind::Ident(ref n) = self.peek().kind {
            if n == "nogc" {
                self.bump();
                true
            } else { false }
        } else { false };
        // Skip newline после атрибута, чтобы `fn` шёл на следующей строке.
        self.skip_newlines();
        Ok(if nogc { RealtimeAttr::RealtimeNogc } else { RealtimeAttr::Realtime })
    }

    /// Plan 33.1 (D24): contract-related атрибуты перед fn-declaration.
    ///
    /// Поддерживаемые:
    /// - `#verify` — SMT обязан доказать (D24 §50).
    /// - `#unverified` — отказ от SMT, всегда runtime fallback в debug,
    ///   стирается в release (D24 §53).
    /// - `#verify_timeout(N)` — локальный override SMT-timeout в ms.
    /// - `#pure` — assertion что функция чистая (использование в контрактах
    ///   composition через 33.2).
    ///
    /// Контекстный разбор: keyword'ов в лексере нет, парсер ищет
    /// `#` + Ident в position перед `fn`. Префикс `#` (не `@`) —
    /// разделение от receiver-prefix.
    /// Plan 45 Ф.3 / D105: парсит ноль или больше doc-атрибутов.
    /// Распознаваемые: `#deprecated`, `#since`, `#stable`, `#unstable`,
    /// `#experimental`, `#hide_doc`, `#doc_alias`, `#doc`. Каждый
    /// разделён newline'ами. Останавливается на первом `#name`, который
    /// не в whitelist'е (передаёт ход следующему parser'у —
    /// `#realtime` / `#verify` / etc.).
    fn parse_doc_attrs(&mut self) -> Result<Vec<crate::ast::DocAttr>, Diagnostic> {
        use crate::ast::DocAttr;
        let mut out: Vec<DocAttr> = Vec::new();
        loop {
            if !matches!(self.peek().kind, TokenKind::Hash) {
                break;
            }
            let name = match &self.peek_at(1).kind {
                TokenKind::Ident(n) => n.clone(),
                _ => break,
            };
            let attr = match name.as_str() {
                "deprecated" => {
                    self.bump(); // #
                    self.bump(); // deprecated
                    let (since, note, until) = if matches!(self.peek().kind, TokenKind::LParen) {
                        self.parse_deprecated_args()?
                    } else {
                        (None, None, None)
                    };
                    DocAttr::Deprecated { since, note, until }
                }
                "since" => {
                    self.bump(); // #
                    self.bump(); // since
                    let v = self.parse_doc_attr_single_string("since")?;
                    DocAttr::Since(v)
                }
                "stable" => {
                    self.bump(); // #
                    self.bump(); // stable
                    let since = if matches!(self.peek().kind, TokenKind::LParen) {
                        Some(self.parse_doc_attr_kv_string("since")?)
                    } else {
                        None
                    };
                    DocAttr::Stable { since }
                }
                "unstable" => {
                    self.bump();
                    self.bump();
                    let feature = if matches!(self.peek().kind, TokenKind::LParen) {
                        Some(self.parse_doc_attr_kv_string("feature")?)
                    } else {
                        None
                    };
                    DocAttr::Unstable { feature }
                }
                "experimental" => {
                    self.bump();
                    self.bump();
                    let note = if matches!(self.peek().kind, TokenKind::LParen) {
                        Some(self.parse_doc_attr_kv_string("note")?)
                    } else {
                        None
                    };
                    DocAttr::Experimental { note }
                }
                "hide_doc" => {
                    self.bump();
                    self.bump();
                    DocAttr::HideDoc
                }
                "doc_alias" => {
                    self.bump();
                    self.bump();
                    let aliases = self.parse_doc_attr_string_list()?;
                    DocAttr::DocAlias(aliases)
                }
                "doc" => {
                    self.bump();
                    self.bump();
                    self.parse_doc_attr_doc_variant()?
                }
                _ => break, // не наш — другому parser'у
            };
            out.push(attr);
            self.skip_newlines();
        }
        Ok(out)
    }

    /// `#deprecated(since = "X", note = "Y", until = "Z"?)` — парсит
    /// 1-3 named-args. Все опциональны.
    fn parse_deprecated_args(
        &mut self,
    ) -> Result<(Option<String>, Option<String>, Option<String>), Diagnostic> {
        let mut since = None;
        let mut note = None;
        let mut until = None;
        if !matches!(self.peek().kind, TokenKind::LParen) {
            return Err(Diagnostic::new("expected `(` after `#deprecated`", self.peek().span));
        }
        self.bump(); // (
        loop {
            if matches!(self.peek().kind, TokenKind::RParen) {
                break;
            }
            let key = match &self.peek().kind {
                TokenKind::Ident(n) => n.clone(),
                _ => return Err(Diagnostic::new(
                    "expected `since` / `note` / `until` key", self.peek().span,
                )),
            };
            self.bump(); // key
            // `key = "value"` (D96 named args).
            if !matches!(self.peek().kind, TokenKind::Eq) {
                return Err(Diagnostic::new(
                    "expected `=` after attribute key", self.peek().span,
                ));
            }
            self.bump(); // =
            let val = match self.peek().kind.clone() {
                TokenKind::Str(s) => { self.bump(); s }
                _ => return Err(Diagnostic::new(
                    "expected string literal", self.peek().span,
                )),
            };
            match key.as_str() {
                "since" => since = Some(val),
                "note" => note = Some(val),
                "until" => until = Some(val),
                _ => return Err(Diagnostic::new(
                    format!("unknown `#deprecated` key `{}`; expected since/note/until", key),
                    self.peek().span,
                )),
            }
            if matches!(self.peek().kind, TokenKind::Comma) {
                self.bump();
            }
        }
        if !matches!(self.peek().kind, TokenKind::RParen) {
            return Err(Diagnostic::new("expected `)` closing `#deprecated`", self.peek().span));
        }
        self.bump(); // )
        Ok((since, note, until))
    }

    /// `#name("value")` или `#name(key = "value")` — поддерживает обе
    /// формы для single-string attrs (e.g. `#since("0.1.0")` или
    /// `#since(version = "0.1.0")`).
    fn parse_doc_attr_single_string(&mut self, attr_name: &str) -> Result<String, Diagnostic> {
        if !matches!(self.peek().kind, TokenKind::LParen) {
            return Err(Diagnostic::new(
                format!("expected `(\"...\")` after `#{}`", attr_name),
                self.peek().span,
            ));
        }
        self.bump(); // (
        let val = match self.peek().kind.clone() {
            TokenKind::Str(s) => { self.bump(); s }
            TokenKind::Ident(_) => {
                // key=value форма
                self.bump(); // ident (key — ignored — single key allowed)
                if !matches!(self.peek().kind, TokenKind::Eq) {
                    return Err(Diagnostic::new(
                        format!("expected `=` or string literal in `#{}`", attr_name),
                        self.peek().span,
                    ));
                }
                self.bump(); // =
                match self.peek().kind.clone() {
                    TokenKind::Str(s) => { self.bump(); s }
                    _ => return Err(Diagnostic::new(
                        format!("expected string literal in `#{}`", attr_name),
                        self.peek().span,
                    )),
                }
            }
            _ => return Err(Diagnostic::new(
                format!("expected string literal in `#{}`", attr_name),
                self.peek().span,
            )),
        };
        if !matches!(self.peek().kind, TokenKind::RParen) {
            return Err(Diagnostic::new(
                format!("expected `)` closing `#{}`", attr_name),
                self.peek().span,
            ));
        }
        self.bump(); // )
        Ok(val)
    }

    /// `#name(key = "value")` — парсит ровно одну named-pair.
    fn parse_doc_attr_kv_string(&mut self, expected_key: &str) -> Result<String, Diagnostic> {
        if !matches!(self.peek().kind, TokenKind::LParen) {
            return Err(Diagnostic::new("expected `(`", self.peek().span));
        }
        self.bump(); // (
        let key = match &self.peek().kind {
            TokenKind::Ident(n) => n.clone(),
            _ => return Err(Diagnostic::new(
                format!("expected `{}` key", expected_key), self.peek().span,
            )),
        };
        if key != expected_key {
            return Err(Diagnostic::new(
                format!("expected `{}`, got `{}`", expected_key, key),
                self.peek().span,
            ));
        }
        self.bump(); // key
        if !matches!(self.peek().kind, TokenKind::Eq) {
            return Err(Diagnostic::new("expected `=`", self.peek().span));
        }
        self.bump(); // =
        let val = match self.peek().kind.clone() {
            TokenKind::Str(s) => { self.bump(); s }
            _ => return Err(Diagnostic::new("expected string literal", self.peek().span)),
        };
        if !matches!(self.peek().kind, TokenKind::RParen) {
            return Err(Diagnostic::new("expected `)`", self.peek().span));
        }
        self.bump(); // )
        Ok(val)
    }

    /// `#doc_alias("a", "b", ...)` — list of string literals.
    fn parse_doc_attr_string_list(&mut self) -> Result<Vec<String>, Diagnostic> {
        let mut out = Vec::new();
        if !matches!(self.peek().kind, TokenKind::LParen) {
            return Err(Diagnostic::new("expected `(` after `#doc_alias`", self.peek().span));
        }
        self.bump(); // (
        loop {
            if matches!(self.peek().kind, TokenKind::RParen) {
                break;
            }
            match self.peek().kind.clone() {
                TokenKind::Str(s) => { self.bump(); out.push(s); }
                _ => return Err(Diagnostic::new(
                    "expected string literal", self.peek().span,
                )),
            }
            if matches!(self.peek().kind, TokenKind::Comma) {
                self.bump();
            }
        }
        self.bump(); // )
        if out.is_empty() {
            return Err(Diagnostic::new(
                "`#doc_alias` requires at least one alias", self.peek().span,
            ));
        }
        Ok(out)
    }

    /// `#doc(inline)` / `#doc(no_inline)` / `#doc(summary = "...")` /
    /// `#doc(section = "...")` / `#doc(test_handlers = "...")`.
    fn parse_doc_attr_doc_variant(&mut self) -> Result<crate::ast::DocAttr, Diagnostic> {
        use crate::ast::DocAttr;
        if !matches!(self.peek().kind, TokenKind::LParen) {
            return Err(Diagnostic::new(
                "expected `(<variant>)` after `#doc`", self.peek().span,
            ));
        }
        self.bump(); // (
        let key = match &self.peek().kind {
            TokenKind::Ident(n) => n.clone(),
            _ => return Err(Diagnostic::new(
                "expected variant name after `#doc(`", self.peek().span,
            )),
        };
        self.bump(); // key
        let attr = match key.as_str() {
            "inline" => DocAttr::DocInline,
            "no_inline" => DocAttr::DocNoInline,
            "summary" | "section" | "test_handlers" => {
                if !matches!(self.peek().kind, TokenKind::Eq) {
                    return Err(Diagnostic::new(
                        format!("expected `=` after `#doc({}`", key),
                        self.peek().span,
                    ));
                }
                self.bump(); // =
                let val = match self.peek().kind.clone() {
                    TokenKind::Str(s) => { self.bump(); s }
                    _ => return Err(Diagnostic::new(
                        "expected string literal", self.peek().span,
                    )),
                };
                match key.as_str() {
                    "summary" => DocAttr::DocSummary(val),
                    "section" => DocAttr::DocSection(val),
                    "test_handlers" => DocAttr::DocTestHandlers(val),
                    _ => unreachable!(),
                }
            }
            _ => return Err(Diagnostic::new(
                format!("unknown `#doc(...)` variant `{}`; expected inline/no_inline/summary/section/test_handlers", key),
                self.peek().span,
            )),
        };
        if !matches!(self.peek().kind, TokenKind::RParen) {
            return Err(Diagnostic::new("expected `)` closing `#doc`", self.peek().span));
        }
        self.bump(); // )
        Ok(attr)
    }

    fn parse_contract_attrs(&mut self) -> Result<ContractAttrs, Diagnostic> {
        let mut attrs = ContractAttrs::default();
        loop {
            if !matches!(self.peek().kind, TokenKind::Hash) {
                break;
            }
            // Look ahead: `#` затем Ident с одним из contract-keyword'ов.
            let next_name = match &self.peek_at(1).kind {
                TokenKind::Ident(n) => n.clone(),
                _ => break, // не идентификатор после `#` — выходим
            };
            match next_name.as_str() {
                "verify" => {
                    if !matches!(attrs.verify_mode, VerifyMode::Default) {
                        let span = self.peek().span;
                        return Err(Diagnostic::new(
                            "duplicate or conflicting verify mode attribute",
                            span,
                        ));
                    }
                    self.bump(); // #
                    self.bump(); // verify
                    attrs.verify_mode = VerifyMode::MustVerify;
                }
                "unverified" => {
                    if !matches!(attrs.verify_mode, VerifyMode::Default) {
                        let span = self.peek().span;
                        return Err(Diagnostic::new(
                            "duplicate or conflicting verify mode attribute",
                            span,
                        ));
                    }
                    self.bump(); // #
                    self.bump(); // unverified
                    attrs.verify_mode = VerifyMode::Unverified;
                }
                "verify_timeout" => {
                    if attrs.verify_timeout_ms.is_some() {
                        let span = self.peek().span;
                        return Err(Diagnostic::new(
                            "duplicate `#verify_timeout` attribute",
                            span,
                        ));
                    }
                    self.bump(); // #
                    self.bump(); // verify_timeout
                    self.expect(&TokenKind::LParen)?;
                    let ms = match self.peek().kind {
                        TokenKind::Int(n) if n > 0 => {
                            let v = n as u32;
                            self.bump();
                            v
                        }
                        _ => {
                            let span = self.peek().span;
                            return Err(Diagnostic::new(
                                "`#verify_timeout(N)` expects positive integer milliseconds",
                                span,
                            ));
                        }
                    };
                    self.expect(&TokenKind::RParen)?;
                    attrs.verify_timeout_ms = Some(ms);
                }
                "pure" => {
                    if matches!(attrs.purity, Purity::Pure) {
                        let span = self.peek().span;
                        return Err(Diagnostic::new(
                            "duplicate `#pure` attribute",
                            span,
                        ));
                    }
                    self.bump(); // #
                    self.bump(); // pure
                    attrs.purity = Purity::Pure;
                }
                "trusted" => {
                    // Plan 33.3 Ф.13: #trusted — только для external fn.
                    // Enforcement (must be external) в type-checker или pipeline.
                    self.bump(); // #
                    self.bump(); // trusted
                    attrs.is_trusted = true;
                }
                _ => break, // unknown #-name — не contract-attr, выходим
            }
            self.skip_newlines();
        }
        Ok(attrs)
    }

    /// Plan 52 Ф.1 (D108): атрибуты-маркеры перед `type`-декларацией.
    ///
    /// Поддерживаемые:
    /// - `#from_fields` — помечает str-keyed map-тип, в который анонимный
    ///   record-литерал `{field: v}` коэрсится через D55 map-coercion.
    ///
    /// Контекстный разбор: `from_fields` — обычный Ident в лексере, парсер
    /// ищет `#` + Ident в позиции перед `type`. Префикс `#` (не `@`) —
    /// консистентно с `#realtime` / `#verify` / `#cfg`.
    fn parse_type_attrs(&mut self) -> Result<Vec<crate::ast::TypeAttr>, Diagnostic> {
        let mut attrs = Vec::new();
        loop {
            if !matches!(self.peek().kind, TokenKind::Hash) {
                break;
            }
            let next_name = match &self.peek_at(1).kind {
                TokenKind::Ident(n) => n.clone(),
                _ => break, // не идентификатор после `#` — выходим
            };
            match next_name.as_str() {
                "from_fields" => {
                    if attrs.contains(&crate::ast::TypeAttr::FromFields) {
                        let span = self.peek().span;
                        return Err(Diagnostic::new(
                            "duplicate `#from_fields` attribute",
                            span,
                        ));
                    }
                    self.bump(); // #
                    self.bump(); // from_fields
                    attrs.push(crate::ast::TypeAttr::FromFields);
                }
                "from_pairs" => {
                    // Plan 52 Ф.23: тип помечен как target для `[k:v]` desugar.
                    if attrs.contains(&crate::ast::TypeAttr::FromPairs) {
                        let span = self.peek().span;
                        return Err(Diagnostic::new(
                            "duplicate `#from_pairs` attribute",
                            span,
                        ));
                    }
                    self.bump(); // #
                    self.bump(); // from_pairs
                    attrs.push(crate::ast::TypeAttr::FromPairs);
                }
                _ => break, // unknown #-name — не type-attr, выходим
            }
            self.skip_newlines();
        }
        Ok(attrs)
    }

    /// Plan 33.1 (D24): парсит блок `requires <expr>` / `ensures <expr>`
    /// + Plan 33.2 (D24): `reads <expr>{, <expr>}*` / `modifies <expr>{, <expr>}*`
    /// после сигнатуры функции, перед телом (`=>` / `{`).
    ///
    /// Контекстный разбор: `requires` / `ensures` / `reads` / `modifies` —
    /// обычные Ident'ы в лексере, парсер распознаёт их в позиции после
    /// return-type до `=>`/`{`. Один clause на строке; разделение по newline.
    fn parse_contracts(&mut self) -> Result<(Vec<Contract>, Vec<FrameTarget>, Vec<FrameTarget>, Option<Expr>), Diagnostic> {
        let mut contracts = Vec::new();
        let mut reads = Vec::new();
        let mut modifies = Vec::new();
        let mut decreases: Option<Expr> = None;
        loop {
            // Пропустить newlines между контрактами.
            self.skip_newlines();
            match &self.peek().kind {
                TokenKind::Ident(n) if n == "requires" => {
                    let start = self.peek().span;
                    self.bump();
                    let expr = self.parse_expr()?;
                    let span = start.merge(expr.span);
                    contracts.push(Contract { kind: ContractKind::Requires, expr, span });
                }
                TokenKind::Ident(n) if n == "ensures" => {
                    let start = self.peek().span;
                    self.bump();
                    let expr = self.parse_expr()?;
                    let span = start.merge(expr.span);
                    contracts.push(Contract { kind: ContractKind::Ensures, expr, span });
                }
                TokenKind::Ident(n) if n == "ensures_fail" => {
                    let start = self.peek().span;
                    self.bump();
                    let expr = self.parse_expr()?;
                    let span = start.merge(expr.span);
                    contracts.push(Contract { kind: ContractKind::EnsuresFail, expr, span });
                }
                TokenKind::Ident(n) if n == "reads" => {
                    self.bump();
                    self.parse_frame_target_list(&mut reads)?;
                }
                TokenKind::Ident(n) if n == "modifies" => {
                    self.bump();
                    self.parse_frame_target_list(&mut modifies)?;
                }
                TokenKind::Ident(n) if n == "decreases" => {
                    if decreases.is_some() {
                        let sp = self.peek().span;
                        return Err(Diagnostic::new(
                            "duplicate `decreases` clause", sp));
                    }
                    self.bump();
                    let expr = self.parse_expr()?;
                    decreases = Some(expr);
                }
                _ => break,
            }
        }
        Ok((contracts, reads, modifies, decreases))
    }

    /// Plan 33.2: парсит `reads <expr>{, <expr>}*` или `modifies <expr>{, <expr>}*`.
    /// Each target — l-value: `name` | `name.field` | `arr[i]` | `arr[*]`.
    fn parse_frame_target_list(&mut self, out: &mut Vec<FrameTarget>) -> Result<(), Diagnostic> {
        loop {
            let target = self.parse_frame_target()?;
            out.push(target);
            if !matches!(self.peek().kind, TokenKind::Comma) {
                break;
            }
            self.bump(); // ,
            self.skip_newlines();
        }
        Ok(())
    }

    fn parse_frame_target(&mut self) -> Result<FrameTarget, Diagnostic> {
        let start = self.peek().span;
        // Parse base — Ident.
        let expr = self.parse_postfix()?;
        // Check shape: Ident → Whole; Member → Field; Index → ArrayElem.
        match &expr.kind {
            ExprKind::Member { obj, name } => Ok(FrameTarget::Field {
                receiver: (**obj).clone(),
                field: name.clone(),
                span: start.merge(expr.span),
            }),
            ExprKind::Index { obj, index } => {
                // `arr[*]` — special case: index = Ident("*") syntactic
                // pattern. Парсер видит `*` как BinOp, не Ident. На данный
                // момент detect через unary multiplication of nothing —
                // парсер сейчас не примет. Оставим как future TODO.
                Ok(FrameTarget::ArrayElem {
                    array: (**obj).clone(),
                    index: (**index).clone(),
                    span: start.merge(expr.span),
                })
            }
            _ => Ok(FrameTarget::Whole(expr)),
        }
    }

    // ─── fn ──────────────────────────────────────────────────────────────

    fn parse_fn(&mut self, is_export: bool, is_external: bool, realtime_attr: RealtimeAttr, contract_attrs: ContractAttrs, doc: Option<crate::ast::DocBlock>, doc_attrs: Vec<crate::ast::DocAttr>) -> Result<FnDecl, Diagnostic> {
        let start = self.peek().span;
        self.expect(&TokenKind::KwFn)?;

        // Special case: receiver `[]T` (slice-receiver, vec.nv style D38).
        // `fn []T @method(...)` — парсим как receiver type_name="[]T", где T —
        // generic param. Первый idеntifier синтезируется из bracket'ов
        // как "[]<elem>" (один логический name).
        let (first_ident, first_span);
        // Plan 15 (D72): generic-параметры в форме declaration (с optional
        // bound) до момента disambiguation между receiver и free fn.
        let mut generics_first_decl: Vec<GenericParam> = Vec::new();
        if matches!(self.peek().kind, TokenKind::LBracket)
            && matches!(self.peek_at(1).kind, TokenKind::RBracket)
        {
            let lb = self.bump().span;
            self.bump(); // ]
            // Парсим element-type. Обычно это `T` (generic param).
            let elem_ty = self.parse_type()?;
            let elem_span = elem_ty.span();
            // Сохраняем generic-параметр как fn_generics, а type_name = "[]T".
            // Bootstrap-codegen видит receiver_type "[]T" и ищет методы на "[]"-типе.
            let elem_name = match &elem_ty {
                TypeRef::Named { path, .. } if path.len() == 1 => path[0].clone(),
                _ => "T".into(),
            };
            first_ident = format!("[]{}", elem_name);
            first_span = lb.merge(elem_span);
        } else {
            // Сначала парсим первый идентификатор. Это либо имя fn, либо
            // имя receiver-типа.
            let (id, sp) = self.parse_ident()?;
            first_ident = id;
            first_span = sp;
            // Если за ним `[`, `<`, `mut`, `@` или `.` — это receiver.
            if matches!(self.peek().kind, TokenKind::LBracket) {
                // Plan 15 (D72): generic params могут быть либо declaration
                // (free fn — `fn name[T Hashable]`) либо instantiation
                // (receiver — `fn TypeName[T] @method`). Парсим как
                // declaration-form (с optional bound), потом disambiguation.
                generics_first_decl = self.parse_generic_decl_params()?;
            }
        }

        let receiver: Option<Receiver>;
        let name: String;
        let mut fn_generics: Vec<GenericParam> = Vec::new();
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
            // Receiver — instantiation context, bounds запрещены.
            let recv_generics = Self::generic_params_to_type_refs(generics_first_decl)?;
            receiver = Some(Receiver {
                type_name: first_ident.clone(),
                generics: recv_generics,
                kind,
                mutable: receiver_mut,
                span: first_span,
            });
            let (n, _) = self.parse_ident()?;
            name = n;
        } else if matches!(self.peek().kind, TokenKind::At) {
            self.bump();
            let recv_generics = Self::generic_params_to_type_refs(generics_first_decl)?;
            receiver = Some(Receiver {
                type_name: first_ident.clone(),
                generics: recv_generics,
                kind: ReceiverKind::Instance,
                mutable: false,
                span: first_span,
            });
            let (n, _) = self.parse_ident()?;
            name = n;
        } else if matches!(self.peek().kind, TokenKind::Dot) {
            self.bump();
            let recv_generics = Self::generic_params_to_type_refs(generics_first_decl)?;
            receiver = Some(Receiver {
                type_name: first_ident.clone(),
                generics: recv_generics,
                kind: ReceiverKind::Static,
                mutable: false,
                span: first_span,
            });
            let (n, _) = self.parse_ident()?;
            name = n;
        } else {
            // Свободная функция: `fn name[T](...)`. В этом случае
            // `generics_first_decl` — это generics функции (с optional
            // bounds), а `first_ident` — имя.
            receiver = None;
            name = first_ident;
            fn_generics.extend(generics_first_decl);
        }

        // Если у метода есть свои generics (D42 model B): `fn Repo[T] @bulk_load[K](...)`
        if receiver.is_some() && matches!(self.peek().kind, TokenKind::LBracket) {
            let method_generics_decl = self.parse_generic_decl_params()?;
            fn_generics.extend(method_generics_decl);
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
        // Plan 14 Ф.6 (D69): variadic-параметр обязан быть последним.
        // Если variadic не на последней позиции — compile error.
        for (i, p) in params.iter().enumerate() {
            if p.is_variadic && i != params.len() - 1 {
                return Err(Diagnostic::new(
                    format!("variadic-параметр `{}` должен быть последним в списке (D69)", p.name),
                    p.span,
                ));
            }
        }
        // Plan 46 (D102): параметры с дефолтом идут строго ПОСЛЕ
        // параметров без дефолта. `fn f(x int = 0, y int)` — error.
        let mut seen_default = false;
        for p in &params {
            if p.default.is_some() {
                seen_default = true;
            } else if seen_default && !p.is_variadic {
                return Err(Diagnostic::new(
                    format!(
                        "параметр `{}` без значения по умолчанию не может идти после \
                         параметра с дефолтом (D102)",
                        p.name
                    ),
                    p.span,
                ));
            }
        }

        // Effects: до `->` или до тела
        let effects = self.parse_effects_until_arrow_or_body()?;
        let return_type = if self.eat(&TokenKind::Arrow).is_some() {
            Some(self.parse_type()?)
        } else {
            None
        };

        // Plan 33.1+33.2 (D24): contracts + reads/modifies после сигнатуры,
        // до тела. `requires <expr>` / `ensures <expr>` / `reads ...` /
        // `modifies ...` на отдельных строках.
        let (contracts, reads, modifies, decreases) = self.parse_contracts()?;
        // external fn не может иметь контрактов (кроме #trusted external fn — Plan 33.3 Ф.13).
        if is_external && !contract_attrs.is_trusted && (!contracts.is_empty() || !reads.is_empty() || !modifies.is_empty() || decreases.is_some()) {
            let span = contracts.first().map(|c| c.span)
                .or_else(|| reads.first().map(|f| f.span()))
                .or_else(|| modifies.first().map(|f| f.span()))
                .unwrap_or(start);
            return Err(Diagnostic::new(
                format!(
                    "external function `{}` cannot have contracts in Plan 33.1 (use `#trusted` in Plan 33.3 when available)",
                    name
                ),
                span,
            ));
        }

        // Тело: `=> expr` или `{ block }`. Для `external fn` — тело
        // отсутствует (D82); следующий токен должен быть Newline/Eof.
        let (body, end_span) = if is_external {
            // Body должен отсутствовать.
            match self.peek().kind {
                TokenKind::FatArrow | TokenKind::LBrace => {
                    let span = self.peek().span;
                    return Err(Diagnostic::new(
                        format!(
                            "external function `{}` cannot have a body",
                            name
                        ),
                        span,
                    ));
                }
                _ => {}
            }
            let last_span = self.tokens[self.pos.saturating_sub(1)].span;
            (FnBody::External, last_span)
        } else {
            let b = self.parse_fn_body()?;
            let s = match &b {
                FnBody::Expr(e) => e.span,
                FnBody::Block(bk) => bk.span,
                FnBody::External => unreachable!(),
            };
            (b, s)
        };
        // Plan 51 Ф.2: `=>`-тело — record-литерал ⇒ тип ровно один раз.
        Self::check_record_lit_type_once(&return_type, receiver.as_ref(), &body)?;
        Ok(FnDecl {
            doc,
            doc_attrs,
            is_export,
            is_external,
            name,
            receiver,
            generics: fn_generics,
            params,
            effects,
            return_type,
            body,
            span: start.merge(end_span),
            realtime_attr,
            // Plan 33.1 (D24): contracts + verify attributes.
            // Backward-compat: пустой Vec для функций без контрактов;
            // Default verify_mode / Unknown purity для функций без атрибутов.
            contracts,
            // Plan 33.2 (D24): reads/modifies frame conditions +
            // decreases termination measure.
            reads,
            modifies,
            decreases,
            verify_mode: contract_attrs.verify_mode,
            verify_timeout_ms: contract_attrs.verify_timeout_ms,
            purity: contract_attrs.purity,
            is_trusted: contract_attrs.is_trusted,
        })
    }

    fn parse_param(&mut self) -> Result<Param, Diagnostic> {
        // Plan 14 Ф.6 (D69): `...` префикс перед именем — variadic param.
        // Только последний param в списке может быть variadic; check
        // выполняется в parse_fn после сбора всех params'ов.
        let is_variadic = self.eat(&TokenKind::DotDotDot).is_some();
        let (name, name_span) = self.parse_ident()?;
        // D6: mut-маркер на параметре говорит, что внутри fn значение
        // можно мутировать. В bootstrap'е GC + reference-семантика делают
        // это noop для семантики, но позволяет писать spec-faithful код.
        // Игнорируем `mut`, оставляем тип.
        if matches!(self.peek().kind, TokenKind::KwMut) {
            self.bump();
        }
        let ty = self.parse_type()?;
        // D69 constraint: тип variadic-param обязан быть `[]T` (TypeRef::Array).
        if is_variadic && !matches!(ty, TypeRef::Array(..)) {
            return Err(Diagnostic::new(
                format!("variadic-параметр `{}` должен иметь тип `[]T` (массив)", name),
                ty.span(),
            ));
        }
        // Plan 46 (D102): опциональное `= expr` — значение по умолчанию.
        // Variadic-параметр не может иметь дефолт (его дефолт — пустой
        // пакет). Правило «default после required» проверяется в parse_fn
        // после сбора всех params (нужен весь список).
        let default = if matches!(self.peek().kind, TokenKind::Eq) {
            if is_variadic {
                return Err(Diagnostic::new(
                    format!("variadic-параметр `{}` не может иметь значение по умолчанию (D102)", name),
                    self.peek().span,
                ));
            }
            self.bump(); // =
            // Default-выражение без struct-literal ambiguity (как в других
            // expr-position внутри сигнатуры).
            Some(self.with_no_struct_or_trailing(|p| p.parse_expr())?)
        } else {
            None
        };
        let span_end = default.as_ref().map(|e| e.span).unwrap_or_else(|| ty.span());
        Ok(Param {
            name,
            ty: ty.clone(),
            span: name_span.merge(span_end),
            is_variadic,
            default,
        })
    }

    /// Парсит список эффектов между `)` и (`->` | `{` | `=>`).
    /// Эффект — TypeRef (обычно Named, но может быть с generics: Fail[E]).
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

    /// Plan 51 Ф.2: когда `=>`-тело функции/замыкания — record-литерал,
    /// тип берётся из return-аннотации; писать его И в литерале нельзя
    /// (тип объявляется ровно один раз). `-> Self` резолвится к типу
    /// receiver'а (`-> Self => Counter{}` в методе `Counter` — тоже
    /// избыточно). `path ≠ return` (sum-coercion, `-> Shape => Circle{}`)
    /// — не трогаем. Используется и `parse_fn`, и `parse_closure_full`.
    fn check_record_lit_type_once(
        return_type: &Option<TypeRef>,
        receiver: Option<&crate::ast::Receiver>,
        body: &FnBody,
    ) -> Result<(), Diagnostic> {
        let FnBody::Expr(e) = body else { return Ok(()); };
        let ExprKind::RecordLit { type_name: Some(lit_path), .. } = &e.kind else {
            return Ok(());
        };
        let resolve = |p: &Vec<String>| -> Vec<String> {
            if p.len() == 1 && p[0] == "Self" {
                if let Some(r) = receiver {
                    return vec![r.type_name.clone()];
                }
            }
            p.clone()
        };
        match return_type {
            None => Err(Diagnostic::new(
                "a function whose `=>` body is a record literal must declare \
                 its return type — write `fn ... -> T => { ... }`",
                e.span)),
            Some(TypeRef::Named { path: ret_path, .. }) => {
                if resolve(lit_path) == resolve(ret_path) {
                    Err(Diagnostic::new(
                        format!(
                            "redundant type prefix on record literal — the return \
                             type `-> {}` already declares it; write `=> {{ ... }}`",
                            ret_path.join(".")),
                        e.span))
                } else {
                    Ok(())
                }
            }
            Some(_) => Ok(()),
        }
    }

    // ─── type declarations ───────────────────────────────────────────────

    fn parse_type_decl(&mut self, is_export: bool, attrs: Vec<crate::ast::TypeAttr>, doc: Option<crate::ast::DocBlock>, doc_attrs: Vec<crate::ast::DocAttr>) -> Result<TypeDecl, Diagnostic> {
        let start = self.peek().span;
        self.expect(&TokenKind::KwType)?;
        let (name, _) = self.parse_ident()?;

        // Plan 15 (D72): generics в форме `[T]` или `[T Hashable]`.
        // Bound — protocol-тип, проверяется в type-checker'е на use-site.
        let generics: Vec<GenericParam> = if matches!(self.peek().kind, TokenKind::LBracket) {
            self.parse_generic_decl_params()?
        } else {
            Vec::new()
        };

        // Тело типа может идти на следующей строке для multi-line sum'ов
        // и эффектов. Skip newlines перед body.
        self.skip_newlines();

        // Тело: `effect { ... }` | `protocol { ... }` | `alias TYPE` |
        // `{ fields }` | `| variant | variant` | `TYPE` (newtype) |
        // начинается с `|` для sum.
        //
        // Plan 15 D53 strict: protocol/effect — отдельные kind'ы
        // несмотря на синтаксическое сходство. По D62 тело идентично
        // (parse_effect_methods переиспользуется), но семантика
        // разная:
        //   - effect: capability с runtime vtable + handler-dispatch.
        //   - protocol: compile-time структурный контракт; usage как
        //     bound (D72) и тип-значение.
        // Codegen эмитит vtable только для Effect-kind. BoundCtx
        // (D72 enforcement) регистрирует только Protocol-kind.
        // Plan 33.3 Ф.9: axioms собираются внутри effect-блока (после
        // методов и pure_view-объявлений). Для protocol — всегда пусто.
        let mut effect_axioms: Vec<EffectAxiom> = Vec::new();
        let kind = match self.peek().kind {
            TokenKind::KwEffect => {
                self.bump();
                self.expect(&TokenKind::LBrace)?;
                let methods = self.parse_effect_methods()?;
                effect_axioms = self.parse_effect_axioms()?;
                self.expect(&TokenKind::RBrace)?;
                TypeDeclKind::Effect(methods)
            }
            TokenKind::KwProtocol => {
                self.bump();
                self.expect(&TokenKind::LBrace)?;
                let methods = self.parse_effect_methods()?;
                // Plan 33.3 Ф.9 (refactor): protocol также может содержать
                // axioms. Verification impl-handler'а отложена на V2
                // (#verify_impl / #trusted_impl) — в V1 axioms в protocol
                // трактуются как trusted-by-default (любая impl верна, что
                // декларирует axiom). Symmetry с #trusted handler'ами.
                effect_axioms = self.parse_effect_axioms()?;
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
        // Plan 33.2 Ф.7 (D24): `invariant <expr>` clauses на record-типах.
        // Парсятся после type-body, могут быть несколько. Для не-record
        // типов — error (sum/protocol/alias/newtype invariants — будущее).
        let mut invariants: Vec<Contract> = Vec::new();
        loop {
            self.skip_newlines();
            match &self.peek().kind {
                TokenKind::Ident(n) if n == "invariant" => {
                    if !matches!(kind, TypeDeclKind::Record(_)) {
                        let sp = self.peek().span;
                        return Err(Diagnostic::new(
                            "`invariant` clauses are only supported on record types in Plan 33.2 \
                             (sum/protocol/alias invariants — future)",
                            sp,
                        ));
                    }
                    let cstart = self.peek().span;
                    self.bump();
                    let expr = self.parse_expr()?;
                    let cspan = cstart.merge(expr.span);
                    invariants.push(Contract {
                        kind: ContractKind::Ensures, // invariants are 'ensures'-like
                        expr,
                        span: cspan,
                    });
                }
                _ => break,
            }
        }
        let span = start.merge(self.tokens[self.pos.saturating_sub(1)].span);
        Ok(TypeDecl {
            doc,
            doc_attrs,
            is_export,
            name,
            generics,
            kind,
            span,
            attrs,
            invariants,
            axioms: effect_axioms,
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
            // D39 / Plan 11 Ф.9: `use name Type` (named embed) или
            // `use _ Type` (anonymous embed).
            let is_embed = self.eat(&TokenKind::KwUse).is_some();
            let (name, name_span, anonymous) = if is_embed {
                // После `use` ожидаем ident (alias name) или `_` для anonymous.
                let (n, sp) = self.parse_ident()?;
                if n == "_" {
                    // Anonymous: имя пока пустое, заполним после parse_type
                    // синтетическим `__embed_<TypeName>`.
                    (String::new(), sp, true)
                } else {
                    (n, sp, false)
                }
            } else {
                let (n, sp) = self.parse_ident()?;
                (n, sp, false)
            };
            let ty = self.parse_type()?;
            // Synthesize anonymous embed name на основе типа (для уникальности
            // в record-схеме и доступа). По convention: `__embed_<TypeName>`.
            let final_name = if anonymous {
                let type_name = match &ty {
                    TypeRef::Named { path, .. } => path.join("_"),
                    _ => "Anon".to_string(),
                };
                format!("__embed_{}", type_name)
            } else {
                name
            };
            fields.push(RecordField {
                name: final_name,
                ty: ty.clone(),
                readonly,
                mutable,
                is_embed,
                embed_anonymous: anonymous,
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

    fn parse_effect_methods(&mut self) -> Result<Vec<EffectMethod>, Diagnostic> {
        let mut methods = Vec::new();
        self.skip_newlines();
        while !matches!(self.peek().kind, TokenKind::RBrace) {
            // Plan 33.3 Ф.9: `axiom <name>(...) => <formula>` обрабатывается
            // отдельной функцией. Останавливаемся как только видим `axiom`.
            if let TokenKind::Ident(n) = &self.peek().kind {
                if n == "axiom" { break; }
            }
            // Plan 33.3 Ф.9 (refactor): `#pure` атрибут перед operation.
            // Раньше использовался keyword `pure_view` — заменили на `#pure`
            // для consistency с другими `#`-атрибутами Nova.
            // Разрешён в обоих effect и protocol (axiom + pure_view как
            // declarative spec; в protocol verification impl'а — V2 через
            // #verify_impl/#trusted_impl).
            let op_kind = if matches!(self.peek().kind, TokenKind::Hash) {
                if let TokenKind::Ident(n) = &self.peek_at(1).kind {
                    if n == "pure" {
                        self.bump(); // #
                        self.bump(); // pure
                        EffectOpKind::PureView
                    } else {
                        EffectOpKind::Operation
                    }
                } else {
                    EffectOpKind::Operation
                }
            } else {
                EffectOpKind::Operation
            };
            let (name, name_span) = self.parse_ident()?;
            // Plan 15 (D72): generics — declaration form с optional bounds.
            let generics: Vec<GenericParam> = if matches!(self.peek().kind, TokenKind::LBracket) {
                self.parse_generic_decl_params()?
            } else {
                Vec::new()
            };
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
            // Plan 33.5 Ф.5.1: контракты метода эффекта — requires/ensures.
            // Используются для Liskov-верификации handler'ов (Ф.5.2).
            let mut contracts: Vec<Contract> = Vec::new();
            self.skip_newlines();
            loop {
                let cstart = self.peek().span;
                match self.peek().kind.clone() {
                    TokenKind::Ident(ref n) if n == "requires" => {
                        self.bump();
                        let expr = self.parse_expr()?;
                        let span = cstart.merge(expr.span);
                        contracts.push(Contract { kind: ContractKind::Requires, expr, span });
                        self.skip_newlines();
                    }
                    TokenKind::Ident(ref n) if n == "ensures" => {
                        self.bump();
                        let expr = self.parse_expr()?;
                        let span = cstart.merge(expr.span);
                        contracts.push(Contract { kind: ContractKind::Ensures, expr, span });
                        self.skip_newlines();
                    }
                    _ => break,
                }
            }

            let end = self.tokens[self.pos.saturating_sub(1)].span;
            // Plan 33.3 Ф.9: `#pure` op обязан иметь return type
            // (что-то наблюдать). Без `-> R` объявление бессмысленно.
            if op_kind == EffectOpKind::PureView && return_type.is_none() {
                return Err(Diagnostic::new(
                    "`#pure` operation must declare a return type \
                     (`-> <Type>`); without it it observes nothing",
                    name_span.merge(end),
                ));
            }
            methods.push(EffectMethod {
                name,
                generics,
                params,
                effects,
                return_type,
                span: name_span.merge(end),
                kind: op_kind,
                contracts,
            });
            self.skip_newlines();
        }
        Ok(methods)
    }

    /// Plan 33.3 Ф.9: `axiom <name>(binders) => <formula>` внутри effect-блока.
    ///
    /// `binders` — список идентификаторов без типов (V1; типы выводятся
    /// из usage). `formula` — обычное Nova-выражение типа `bool`.
    /// Семантика: глобальное assert'ение, видимое во всех контрактах
    /// где импортирован эффект.
    fn parse_effect_axioms(&mut self) -> Result<Vec<EffectAxiom>, Diagnostic> {
        let mut axioms = Vec::new();
        self.skip_newlines();
        loop {
            let is_axiom = matches!(
                &self.peek().kind,
                TokenKind::Ident(n) if n == "axiom"
            );
            if !is_axiom { break; }
            let start = self.peek().span;
            self.bump(); // consume `axiom`
            let (ax_name, _) = self.parse_ident()?;
            // Plan 33.3 (refactor): generic params `axiom name[T](id T) => ...`
            let generics: Vec<GenericParam> = if matches!(self.peek().kind, TokenKind::LBracket) {
                self.parse_generic_decl_params()?
            } else {
                Vec::new()
            };
            self.expect(&TokenKind::LParen)?;
            // Typed binders: `axiom name(id int, x str) => ...`
            // Untyped:       `axiom name(id, x) => ...`  (type inferred)
            // Generic refs:  `axiom name[T](id T) => ...` → Generic("T")
            let generic_names: std::collections::HashSet<String> =
                generics.iter().map(|g| g.name.clone()).collect();
            let mut binders: Vec<crate::ast::BinderDef> = Vec::new();
            while !matches!(self.peek().kind, TokenKind::RParen) {
                let (b, b_span) = self.parse_ident()?;
                // Если следующий токен — не запятая и не ')' — это тип.
                let kind = if !matches!(self.peek().kind,
                    TokenKind::Comma | TokenKind::RParen) {
                    let ty = self.parse_type()?;
                    // Проверяем: тип = единственный Named{path:[T]} где T generic?
                    if let crate::ast::TypeRef::Named { path, generics: g, .. } = &ty {
                        if g.is_empty() && path.len() == 1
                            && generic_names.contains(&path[0])
                        {
                            crate::ast::BinderType::Generic(path[0].clone())
                        } else {
                            crate::ast::BinderType::Typed(ty)
                        }
                    } else {
                        crate::ast::BinderType::Typed(ty)
                    }
                } else {
                    crate::ast::BinderType::Untyped
                };
                binders.push(crate::ast::BinderDef { name: b, kind, span: b_span });
                if self.eat(&TokenKind::Comma).is_none() {
                    break;
                }
                self.skip_newlines();
            }
            self.expect(&TokenKind::RParen)?;
            self.expect(&TokenKind::FatArrow)?;
            let formula = self.parse_expr()?;
            let span = start.merge(formula.span);
            axioms.push(EffectAxiom {
                name: ax_name,
                generics,
                binders,
                formula,
                span,
            });
            self.skip_newlines();
        }
        Ok(axioms)
    }

    // ─── let / const / test ──────────────────────────────────────────────

    fn parse_let_decl(&mut self) -> Result<LetDecl, Diagnostic> {
        let start = self.peek().span;
        // Plan 33.3 (D24): `ghost let` / `ghost var` — spec-only binding.
        // Прификс `ghost` перед `let`/`var`. Контекстный keyword.
        let is_ghost = if let TokenKind::Ident(n) = &self.peek().kind {
            if n == "ghost" && matches!(self.peek_at(1).kind, TokenKind::KwLet) {
                self.bump();
                true
            } else { false }
        } else { false };
        self.expect(&TokenKind::KwLet)?;
        let mutable = self.eat(&TokenKind::KwMut).is_some();
        let pattern = self.parse_pattern()?;
        let ty = if !matches!(self.peek().kind, TokenKind::Eq) {
            Some(self.parse_type()?)
        } else {
            None
        };
        self.expect(&TokenKind::Eq)?;
        // Allow newline after `=` so that `let x =\n    if ... else ...` works (D49 #5).
        self.skip_newlines();
        let value = self.parse_expr()?;
        let span = start.merge(value.span);
        self.expect_newline_or_eof().ok();
        // Plan 51 Ф.2: `let x T = T { ... }` — тип объявлен дважды (в
        // аннотации и в литерале). Тип пишется ровно один раз: каноничные
        // формы — `let x T = { ... }` либо `let x = T { ... }`.
        if let Some(TypeRef::Named { path: ann_path, .. }) = &ty {
            if let ExprKind::RecordLit { type_name: Some(lit_path), .. } = &value.kind {
                if ann_path == lit_path {
                    return Err(Diagnostic::new(
                        format!(
                            "redundant type prefix on record literal — the `let` \
                             annotation already declares `{}`; write `= {{ ... }}`",
                            ann_path.join(".")),
                        value.span));
                }
            }
        }
        Ok(LetDecl {
            mutable,
            pattern,
            ty,
            value,
            span,
            is_ghost,
        })
    }

    fn parse_const_decl(&mut self, is_export: bool, doc: Option<crate::ast::DocBlock>, doc_attrs: Vec<crate::ast::DocAttr>) -> Result<ConstDecl, Diagnostic> {
        let start = self.peek().span;
        self.expect(&TokenKind::KwConst)?;
        let (name, _) = self.parse_ident()?;
        let ty = if !matches!(self.peek().kind, TokenKind::Eq) {
            Some(self.parse_type()?)
        } else {
            None
        };
        self.expect(&TokenKind::Eq)?;
        self.skip_newlines();
        let value = self.parse_expr()?;
        let value_span = value.span;
        self.expect_newline_or_eof().ok();
        Ok(ConstDecl {
            doc,
            doc_attrs,
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

    /// Plan 57: `bench "name" { setup_stmts; measure { measured_body } teardown_stmts }`
    ///
    /// Внутри body парсим обычные statements; при встрече ключевого
    /// слова `measure` — переключаемся, парсим один measure-блок, затем
    /// продолжаем как teardown.
    ///
    /// Ровно один `measure { ... }` блок в body. Иначе диагностика.
    fn parse_bench_decl(&mut self) -> Result<BenchDecl, Diagnostic> {
        let start = self.peek().span;
        // Caller already verified `bench` ident + string-literal lookahead;
        // here consume ident, then string.
        match &self.peek().kind {
            TokenKind::Ident(s) if s == "bench" => { self.bump(); }
            _ => return Err(Diagnostic::new(
                "expected `bench` keyword",
                self.peek().span,
            )),
        }
        let name = match &self.peek().kind {
            TokenKind::Str(s) => {
                let n = s.clone();
                self.bump();
                n
            }
            _ => {
                return Err(Diagnostic::new(
                    "expected bench name as string literal",
                    self.peek().span,
                ))
            }
        };
        // Plan 57.B.3: optional parameter sweep — `(IDENT in [v1, v2, ...])`
        // ДО opening brace.
        let params = if matches!(self.peek().kind, TokenKind::LParen) {
            let lp_span = self.peek().span;
            self.bump();  // (
            let var_name = match self.peek().kind.clone() {
                TokenKind::Ident(n) => { self.bump(); n }
                _ => return Err(Diagnostic::new(
                    "expected parameter name after `(` in bench-sweep",
                    self.peek().span)),
            };
            self.expect(&TokenKind::KwIn)?;
            self.expect(&TokenKind::LBracket)?;
            let mut values = Vec::new();
            loop {
                if matches!(self.peek().kind, TokenKind::RBracket) { break; }
                match self.peek().kind.clone() {
                    TokenKind::Int(n) => { values.push(n); self.bump(); }
                    _ => return Err(Diagnostic::new(
                        "bench-sweep values must be integer literals",
                        self.peek().span)),
                }
                if matches!(self.peek().kind, TokenKind::Comma) {
                    self.bump();
                }
            }
            let rb_span = self.expect(&TokenKind::RBracket)?.span;
            let rp_span = self.expect(&TokenKind::RParen)?.span;
            if values.is_empty() {
                return Err(Diagnostic::new(
                    "bench-sweep values list cannot be empty",
                    lp_span.merge(rp_span)));
            }
            Some(BenchParams {
                var_name,
                values,
                span: lp_span.merge(rb_span),
            })
        } else {
            None
        };
        let brace_open = self.expect(&TokenKind::LBrace)?.span;
        self.skip_newlines();
        let mut setup: Vec<Stmt> = Vec::new();
        let mut measure_body: Option<Block> = None;
        let mut teardown: Vec<Stmt> = Vec::new();
        while !matches!(self.peek().kind, TokenKind::RBrace) {
            // `measure` — контекстный keyword: distinguishes by next-token-is-`{`.
            // `measure_var = 1` (assignment to var named measure) парсится как stmt.
            let is_measure_block = match &self.peek().kind {
                TokenKind::Ident(s) if s == "measure"
                    && matches!(self.peek_at(1).kind, TokenKind::LBrace) => true,
                _ => false,
            };
            if is_measure_block {
                if measure_body.is_some() {
                    return Err(Diagnostic::new(
                        "bench body must contain exactly one `measure { ... }` block",
                        self.peek().span,
                    ));
                }
                self.bump(); // consume `measure` ident
                let mb = self.parse_block()?;
                measure_body = Some(mb);
                self.skip_newlines();
                continue;
            }
            let so = self.parse_stmt_or_expr()?;
            let s = match so {
                StmtOrExpr::Stmt(s) => s,
                StmtOrExpr::Expr(e) => Stmt::Expr(e),
            };
            if measure_body.is_none() {
                setup.push(s);
            } else {
                teardown.push(s);
            }
            self.skip_newlines();
        }
        let brace_close = self.expect(&TokenKind::RBrace)?.span;
        let measure_body = measure_body.ok_or_else(|| {
            Diagnostic::new(
                "bench body must contain `measure { ... }` block",
                brace_open.merge(brace_close),
            )
        })?;
        Ok(BenchDecl {
            name,
            setup,
            measure_body,
            teardown,
            params,
            span: start.merge(brace_close),
        })
    }

    // ─── lemma ───────────────────────────────────────────────────────────

    /// Plan 33.5 Ф.4.1: `lemma name(params) requires P ensures Q { body }`.
    ///
    /// Синтаксис — упрощённый fn без effects/return_type/decreases/modifies.
    /// Контракты: только `requires` и `ensures` (body доказывает ensures при requires).
    fn parse_lemma_decl(&mut self) -> Result<LemmaDecl, Diagnostic> {
        let start = self.expect(&TokenKind::KwLemma)?.span;
        let name = match self.peek().kind.clone() {
            TokenKind::Ident(n) => { self.bump(); n }
            _ => return Err(Diagnostic::new(
                "expected lemma name",
                self.peek().span,
            )),
        };

        // Generics: `[T]` form (optional).
        let generics = if matches!(self.peek().kind, TokenKind::LBracket) {
            self.parse_generic_decl_params()?
        } else {
            Vec::new()
        };

        // Params.
        self.expect(&TokenKind::LParen)?;
        let mut params = Vec::new();
        while !matches!(self.peek().kind, TokenKind::RParen) {
            params.push(self.parse_param()?);
            if !matches!(self.peek().kind, TokenKind::RParen) {
                self.expect(&TokenKind::Comma)?;
            }
        }
        self.expect(&TokenKind::RParen)?;

        // Contracts: requires / ensures (same parsing as in parse_fn).
        let mut contracts = Vec::new();
        loop {
            self.skip_newlines();
            let cstart = self.peek().span;
            match self.peek().kind.clone() {
                TokenKind::Ident(ref n) if n == "requires" => {
                    self.bump();
                    let expr = self.parse_expr()?;
                    let span = cstart.merge(expr.span);
                    contracts.push(Contract { kind: ContractKind::Requires, expr, span });
                }
                TokenKind::Ident(ref n) if n == "ensures" => {
                    self.bump();
                    let expr = self.parse_expr()?;
                    let span = cstart.merge(expr.span);
                    contracts.push(Contract { kind: ContractKind::Ensures, expr, span });
                }
                _ => break,
            }
        }

        // Body: `=> expr` или `{ ... }` block (как у fn).
        let (body, end_span) = if matches!(self.peek().kind, TokenKind::FatArrow) {
            self.bump(); // consume `=>`
            let expr = self.parse_expr()?;
            let sp = expr.span;
            (FnBody::Expr(expr), sp)
        } else {
            let b = self.parse_block()?;
            let sp = b.span;
            (FnBody::Block(b), sp)
        };
        Ok(LemmaDecl {
            name,
            generics,
            params,
            contracts,
            body,
            span: start.merge(end_span),
        })
    }

    /// Plan 33.5 Ф.4.2: `calc { expr; == expr; == expr; }`.
    ///
    /// Синтаксис:
    ///   calc {
    ///     expr1 ;
    ///     == expr2 ;    // или <=, <, >=, >
    ///     == expr3 ;
    ///   }
    ///
    /// Первый шаг — просто expr (без отношения). Остальные начинаются с rel.
    fn parse_calc_stmt(&mut self, start: Span) -> Result<Stmt, Diagnostic> {
        self.expect(&TokenKind::LBrace)?;
        let mut steps: Vec<CalcStep> = Vec::new();

        loop {
            self.skip_newlines();
            if matches!(self.peek().kind, TokenKind::RBrace) { break; }

            // Первый шаг — без отношения; последующие начинаются с rel-оператора.
            let rel = if steps.is_empty() {
                None
            } else {
                // Ожидаем rel-оператор: ==, <=, <, >=, >
                let rel = match &self.peek().kind {
                    TokenKind::EqEq => { self.bump(); CalcRel::Eq }
                    TokenKind::Le => { self.bump(); CalcRel::Le }
                    TokenKind::Lt => { self.bump(); CalcRel::Lt }
                    TokenKind::Ge => { self.bump(); CalcRel::Ge }
                    TokenKind::Gt => { self.bump(); CalcRel::Gt }
                    _ => return Err(Diagnostic::new(
                        "expected relation operator (==, <=, <, >=, >) in `calc` step",
                        self.peek().span,
                    )),
                };
                Some(rel)
            };

            let expr = self.parse_expr()?;
            let step_span = expr.span;

            // Опциональная точка с запятой после выражения.
            self.skip_newlines();
            if matches!(self.peek().kind, TokenKind::Semicolon) {
                self.bump();
            }

            steps.push(CalcStep { rel, expr, span: step_span });
        }

        let end = self.expect(&TokenKind::RBrace)?.span;

        if steps.is_empty() {
            return Err(Diagnostic::new("empty `calc` block", start));
        }

        Ok(Stmt::Calc { steps, span: start.merge(end) })
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

    /// Plan 15 (D72): parse generic-DECLARATION params `[name [bound], ...]`.
    ///
    /// Используется для declaration-сайтов: free fn `[T Hashable]`,
    /// type decl `type HashMap[K Hashable, V]`, method-extra-generics
    /// `fn Repo[T] @bulk[K Ord]`, effect-method generics.
    ///
    /// Каждый параметр — простой identifier (имя), за которым может
    /// идти optional bound (любой тип, обычно protocol). Bound парсится
    /// если следующий после имени токен НЕ `,`/`]` (т.е. что-то ещё).
    ///
    /// Forward-references проверяются ниже type-checker'ом
    /// (текущий список параметров доступен только слева направо).
    fn parse_generic_decl_params(&mut self) -> Result<Vec<GenericParam>, Diagnostic> {
        self.expect(&TokenKind::LBracket)?;
        let mut params = Vec::new();
        self.skip_newlines();
        while !matches!(self.peek().kind, TokenKind::RBracket) {
            let (name, name_span) = self.parse_ident()?;
            // Bound: если следующий токен — не `,`, `]`, `=`, парсим как тип.
            let bound = if matches!(
                self.peek().kind,
                TokenKind::Comma | TokenKind::RBracket | TokenKind::Eq
            ) {
                None
            } else {
                Some(self.parse_type()?)
            };
            // Plan 19, C10 (D88): default-значение generic'а через `=`.
            // Грамматика: `name [bound] [= default]`. Если `=` после
            // bound (или после name если bound отсутствует) — парсим
            // default-тип.
            let default = if self.eat(&TokenKind::Eq).is_some() {
                Some(self.parse_type()?)
            } else {
                None
            };
            let end_span = default
                .as_ref()
                .map(|t| t.span())
                .or_else(|| bound.as_ref().map(|t| t.span()))
                .unwrap_or(name_span);
            params.push(GenericParam {
                name,
                bound,
                default,
                span: name_span.merge(end_span),
            });
            if self.eat(&TokenKind::Comma).is_none() {
                break;
            }
            self.skip_newlines();
        }
        // D88 constraint: параметры с default'ом должны идти после
        // обязательных. Проверяем после сборки.
        let mut seen_default = false;
        for p in &params {
            if p.default.is_some() {
                seen_default = true;
            } else if seen_default {
                return Err(Diagnostic::new(
                    format!(
                        "generic-параметр без default'а `{}` следует после параметра с default — \
                         параметры с default'ом должны идти последними (D88)",
                        p.name
                    ),
                    p.span,
                ));
            }
        }
        self.expect(&TokenKind::RBracket)?;
        Ok(params)
    }

    /// Plan 15 (D72): convert `Vec<GenericParam>` → `Vec<TypeRef>` для
    /// receiver / instantiation context. Все params обязаны быть простыми
    /// именами без bound (bound допустим только в declaration).
    fn generic_params_to_type_refs(params: Vec<GenericParam>) -> Result<Vec<TypeRef>, Diagnostic> {
        let mut out = Vec::with_capacity(params.len());
        for p in params {
            if let Some(b) = p.bound {
                return Err(Diagnostic::new(
                    format!(
                        "generic bound `{}` не разрешён в receiver/instantiation context — \
                         bounds допустимы только в declaration `[T Bound]`",
                        match &b {
                            TypeRef::Named { path, .. } => path.join("."),
                            _ => "<complex>".to_string(),
                        }
                    ),
                    b.span(),
                ));
            }
            if let Some(d) = p.default {
                return Err(Diagnostic::new(
                    "generic default не разрешён в receiver/instantiation context — \
                     defaults допустимы только в declaration `[T = Default]` (D88)".to_string(),
                    d.span(),
                ));
            }
            out.push(TypeRef::Named {
                path: vec![p.name],
                generics: Vec::new(),
                span: p.span,
            });
        }
        Ok(out)
    }

    /// D38 turbofish disambiguation в expression-position. Caller — на токене
    /// `[`. Speculative-parse: пробуем разобрать `[T1, T2, ...]` как type-args;
    /// если получилось до `]`, проверяем next token:
    ///   - `(`            → call — turbofish (`func[T](args)`)
    ///   - `.` IDENT `(`  → method call — turbofish (`Type[T].method(...)`)
    ///   - `?`            → try — turbofish (`func[T]?`)
    ///   - иначе          → не turbofish, rollback (это Index).
    /// Если parse_type fails внутри — rollback (Index). Возвращает Some((args,
    /// end_span_of_RBracket)) при успехе и оставляет позицию **сразу за `]`**;
    /// возвращает None и оставляет позицию **на `[`** при rollback.
    fn try_parse_turbofish_args(&mut self) -> Option<(Vec<TypeRef>, Span)> {
        debug_assert!(matches!(self.peek().kind, TokenKind::LBracket));
        let saved_pos = self.pos;
        // Bump `[`
        self.bump();
        // Empty `[]` нелегально для turbofish — rollback.
        if matches!(self.peek().kind, TokenKind::RBracket) {
            self.pos = saved_pos;
            return None;
        }
        let mut args: Vec<TypeRef> = Vec::new();
        loop {
            // Speculative parse_type. Если ошибка — rollback.
            let before = self.pos;
            let ty = match self.parse_type() {
                Ok(t) => t,
                Err(_) => {
                    self.pos = saved_pos;
                    let _ = before;
                    return None;
                }
            };
            args.push(ty);
            if self.eat(&TokenKind::Comma).is_none() {
                break;
            }
        }
        // Должны быть на `]`.
        let end_span = match self.peek().kind {
            TokenKind::RBracket => self.peek().span,
            _ => {
                self.pos = saved_pos;
                return None;
            }
        };
        // Bump `]`.
        self.bump();
        // Post-`]` continuation check.
        let is_turbofish = match &self.peek().kind {
            TokenKind::LParen => true,
            TokenKind::Question => true,
            TokenKind::Dot => {
                // `.` IDENT `(` — method call. Голый `.field` — не turbofish.
                matches!(self.peek_at(1).kind, TokenKind::Ident(_))
                    && matches!(self.peek_at(2).kind, TokenKind::LParen)
            }
            _ => false,
        };
        if !is_turbofish {
            self.pos = saved_pos;
            return None;
        }
        Some((args, end_span))
    }

    // ─── expressions ─────────────────────────────────────────────────────

    pub fn parse_expr(&mut self) -> Result<Expr, Diagnostic> {
        self.parse_implication()
    }

    /// Plan 33.1 (D24): `==>` (impl) и `<==>` (iff) — приоритет ниже `||`,
    /// правоассоциативные. Используются в контрактах. Семантика:
    /// - `A ==> B` ≡ `!A || B`.
    /// - `A <==> B` ≡ `A == B` (только для bool).
    fn parse_implication(&mut self) -> Result<Expr, Diagnostic> {
        let left = self.parse_or()?;
        // Right-associative: if we see ==> or <==>, recurse for right side.
        let op = match self.peek().kind {
            TokenKind::Implies => BinOp::Implies,
            TokenKind::Iff => BinOp::Iff,
            _ => return Ok(left),
        };
        self.bump();
        self.skip_newlines();
        let right = self.parse_implication()?;
        let span = left.span.merge(right.span);
        Ok(Expr::new(
            ExprKind::Binary {
                op,
                left: Box::new(left),
                right: Box::new(right),
            },
            span,
        ))
    }

    fn parse_or(&mut self) -> Result<Expr, Diagnostic> {
        let mut left = self.parse_and()?;
        loop {
            // D49 newline-tolerance: `or` keyword after newline continues the
            // expression (`a\nor b`). We do NOT extend this to `||` because
            // `||` is also the no-arg closure syntax (`|| body`). Allowing a
            // newline before `||` would mis-parse:
            //   let x = 42
            //   || closure_body   ← new closure, not binary-OR continuation
            // Use `or` for multi-line logical-OR instead.
            let saved_pos = self.pos;
            if matches!(self.peek().kind, TokenKind::Newline) {
                self.skip_newlines();
                // Only `or` keyword is safe to continue after a newline.
                // `||` after a newline is a new closure expression, not binary OR.
                if !matches!(self.peek().kind, TokenKind::KwOr) {
                    self.pos = saved_pos;
                    break;
                }
            }
            if !matches!(self.peek().kind, TokenKind::PipePipe | TokenKind::KwOr) {
                self.pos = saved_pos;
                break;
            }
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
        loop {
            let saved_pos = self.pos;
            if matches!(self.peek().kind, TokenKind::Newline) {
                self.skip_newlines();
            }
            if !matches!(self.peek().kind, TokenKind::AmpAmp | TokenKind::KwAnd) {
                self.pos = saved_pos;
                break;
            }
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
        let mut left = self.parse_bit_or()?;
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
            let right = self.parse_bit_or()?;
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

    /// Bitwise-or `|` (level 7 в spec). Не путаем с `||`.
    fn parse_bit_or(&mut self) -> Result<Expr, Diagnostic> {
        let mut left = self.parse_bit_xor()?;
        loop {
            // D49 newline-tolerance: leading `|` after newline продолжает expression.
            // Аналогично `||`/`&&` (rule 6). Ценен для multi-line bitwise expr'ов
            // в hash/codec алгоритмах (base64 / md5 / sha / fnv-style packs).
            let saved_pos = self.pos;
            if matches!(self.peek().kind, TokenKind::Newline) {
                self.skip_newlines();
            }
            if !matches!(self.peek().kind, TokenKind::Pipe) {
                self.pos = saved_pos;
                break;
            }
            self.bump();
            self.skip_newlines();
            let right = self.parse_bit_xor()?;
            let span = left.span.merge(right.span);
            left = Expr::new(
                ExprKind::Binary {
                    op: BinOp::BitOr,
                    left: Box::new(left),
                    right: Box::new(right),
                },
                span,
            );
        }
        Ok(left)
    }

    /// Bitwise-xor `^` (level 8).
    fn parse_bit_xor(&mut self) -> Result<Expr, Diagnostic> {
        let mut left = self.parse_bit_and()?;
        loop {
            // D49 newline-tolerance: leading `^` after newline.
            let saved_pos = self.pos;
            if matches!(self.peek().kind, TokenKind::Newline) {
                self.skip_newlines();
            }
            if !matches!(self.peek().kind, TokenKind::Caret) {
                self.pos = saved_pos;
                break;
            }
            self.bump();
            self.skip_newlines();
            let right = self.parse_bit_and()?;
            let span = left.span.merge(right.span);
            left = Expr::new(
                ExprKind::Binary {
                    op: BinOp::BitXor,
                    left: Box::new(left),
                    right: Box::new(right),
                },
                span,
            );
        }
        Ok(left)
    }

    /// Bitwise-and `&` (level 9). Не путаем с `&&`.
    fn parse_bit_and(&mut self) -> Result<Expr, Diagnostic> {
        let mut left = self.parse_shift()?;
        loop {
            // D49 newline-tolerance: leading `&` after newline.
            let saved_pos = self.pos;
            if matches!(self.peek().kind, TokenKind::Newline) {
                self.skip_newlines();
            }
            if !matches!(self.peek().kind, TokenKind::Amp) {
                self.pos = saved_pos;
                break;
            }
            self.bump();
            self.skip_newlines();
            let right = self.parse_shift()?;
            let span = left.span.merge(right.span);
            left = Expr::new(
                ExprKind::Binary {
                    op: BinOp::BitAnd,
                    left: Box::new(left),
                    right: Box::new(right),
                },
                span,
            );
        }
        Ok(left)
    }

    /// Shift `<<` / `>>` (level 10).
    fn parse_shift(&mut self) -> Result<Expr, Diagnostic> {
        let mut left = self.parse_range()?;
        loop {
            // D49 newline-tolerance: leading `<<`/`>>` after newline.
            let saved_pos = self.pos;
            if matches!(self.peek().kind, TokenKind::Newline) {
                self.skip_newlines();
            }
            let op = match self.peek().kind {
                TokenKind::Shl => BinOp::Shl,
                TokenKind::Shr => BinOp::Shr,
                _ => {
                    self.pos = saved_pos;
                    break;
                }
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
                        TokenKind::At => {
                            // `obj.@method` — bound method value (Plan 11 Ф.4).
                            // `Type.@method` — unbound method value.
                            // Префикс "@" в имени маркирует method-value seman-
                            // тику для codegen (bound vs unbound — по obj kind).
                            let at_span = self.peek().span;
                            self.bump(); // consume @
                            if !matches!(self.peek().kind, TokenKind::Ident(_)) {
                                return Err(Diagnostic::new(
                                    "expected method name after `.@`",
                                    self.peek().span,
                                ));
                            }
                            let (mname, mname_span) = self.parse_ident()?;
                            (format!("@{}", mname), at_span.merge(mname_span))
                        }
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
                    // D38 turbofish: `Type[T1, T2].method(...)` или `func[T](args)`.
                    // Disambiguation: speculative parse `[...]` as type-args; если
                    // успешно (все элементы — типы) И post-`]` token — `(`, `.IDENT(`
                    // или `?`, это turbofish; иначе rollback к Index.
                    //
                    // Rationale: index-доступ всегда single-arg expression,
                    // turbofish — N type-args + обязательный postfix-continuation
                    // (call / method-call / try). Multi-arg внутри `[...]` →
                    // однозначно turbofish (Index не имеет comma).
                    if let Some((type_args, end_span)) = self.try_parse_turbofish_args() {
                        expr = Expr::new(
                            ExprKind::TurboFish {
                                base: Box::new(expr.clone()),
                                type_args,
                            },
                            expr.span.merge(end_span),
                        );
                    } else {
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
                }
                TokenKind::LParen => {
                    self.bump();
                    let mut args: Vec<CallArg> = Vec::new();
                    self.skip_newlines();
                    while !matches!(self.peek().kind, TokenKind::RParen) {
                        // Plan 14 Ф.6 (D69): `...expr` в call-args — spread.
                        // Mirroring parse_array_lit pattern: check DotDotDot
                        // BEFORE parse_expr (parse_expr не понимает `...`
                        // как prefix-operator).
                        if self.eat(&TokenKind::DotDotDot).is_some() {
                            let inner = self.parse_expr()?;
                            args.push(CallArg::Spread(inner));
                        } else if matches!(self.peek().kind, TokenKind::Ident(_))
                            && matches!(
                                self.tokens.get(self.pos + 1).map(|t| &t.kind),
                                Some(TokenKind::Colon)
                            )
                        {
                            // Plan 46 (D102): `name: expr` — именованный
                            // аргумент. Внутри `(...)` вызова `ident ':'`
                            // всегда named-arg (коллизии с record-литералом
                            // нет — record это `Имя { ... }`).
                            let (arg_name, _) = self.parse_ident()?;
                            self.bump(); // :
                            self.skip_newlines();
                            let value = self.parse_expr()?;
                            args.push(CallArg::Named { name: arg_name, value });
                        } else {
                            args.push(CallArg::Item(self.parse_expr()?));
                        }
                        if self.eat(&TokenKind::Comma).is_some() {
                            self.skip_newlines();
                        } else {
                            break;
                        }
                    }
                    let end = self.expect(&TokenKind::RParen)?.span;
                    // Trailing-конструкция после `)`. Plan 19 D43-rev:
                    // - `{` → trailing-block (без params), либо legacy
                    //   `{ x => body }` (с params, до миграции).
                    // - `fn` → trailing-fn `f(args) fn(p) body`.
                    // Skip when inside match-scrutinee context
                    // (see no_trailing_block flag) — `match f(x) { ... }`
                    // should be parsed as `match` over `f(x)`, not
                    // `f(x){...}` call-with-block.
                    let trailing = if !self.no_trailing_block {
                        match self.peek().kind {
                            TokenKind::LBrace => {
                                // Plan 19, C13: trailing-block только
                                // без params (D43-rev). LegacyBlockWithParams
                                // удалён из parser-path (сам enum-вариант
                                // оставлен для совместимости с тестами,
                                // но parser его больше не создаёт).
                                let tb = self.parse_trailing_block()?;
                                Some(crate::ast::Trailing::Block(Box::new(tb.body)))
                            }
                            // Plan 19, C4: trailing-fn `fn(p) body`.
                            // Парсим как closure-full без имени:
                            // переиспользуем parse_closure_full и
                            // распаковываем результат в FnSigBody.
                            TokenKind::KwFn => {
                                let fn_start = self.peek().span;
                                let cf_expr = self.parse_closure_full(fn_start)?;
                                let ExprKind::ClosureFull(sb) = cf_expr.kind else {
                                    unreachable!(
                                        "parse_closure_full must produce ExprKind::ClosureFull"
                                    );
                                };
                                Some(crate::ast::Trailing::Fn(sb))
                            }
                            _ => None,
                        }
                    } else {
                        None
                    };
                    let span = expr.span.merge(end);
                    expr = Expr::new(
                        ExprKind::Call {
                            func: Box::new(expr),
                            args,
                            trailing,
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
                // Plan 19, C7 (D85): `expr!!` postfix-throw оператор.
                // Лексер не объединяет `!!` в один токен (было бы
                // конфликтно с prefix `!!cond` = `!(!cond)`); вместо
                // этого парсер ловит два Bang подряд в postfix-position.
                // В postfix `expr` уже распарсен как operand, поэтому
                // `expr!!` однозначно postfix-throw.
                //
                // На Some(v)/Ok(v) разворачивает; на None/Err(e)
                // бросает через Fail[E].
                TokenKind::Bang
                    if matches!(self.peek_at(1).kind, TokenKind::Bang) =>
                {
                    self.bump(); // first '!'
                    self.bump(); // second '!'
                    let span = expr.span;
                    expr = Expr::new(ExprKind::Bang(Box::new(expr)), span);
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
                self.desugar_string_interpolation(s, start)
            }
            TokenKind::Char(cp) => {
                self.bump();
                Ok(Expr::new(ExprKind::CharLit(cp), start))
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
            TokenKind::Ident(ref kw) if kw == "forall" || kw == "exists" => {
                let is_forall = kw == "forall";
                self.parse_quantifier(is_forall)
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
                // Plan 08 Ф.2: primitive type-names — `int`, `f64`, `bool`,
                // `char`, `byte`, `str`, `u8`-`u64`, `i8`-`i64`, `f32`/`f64` —
                // могут быть subject'ом static-method'а (`int.try_from`,
                // `str.from`, `f64.try_from`). Lowercase, поэтому не path
                // через PascalCase-rule. Делаем явное исключение.
                let is_primitive_type = matches!(first.as_str(),
                    "int" | "i8" | "i16" | "i32" | "i64"
                    | "u8" | "u16" | "u32" | "u64"
                    | "f32" | "f64" | "byte" | "bool" | "char" | "str"
                );
                if (starts_uppercase || is_primitive_type)
                    && matches!(self.peek().kind, TokenKind::Dot)
                    && matches!(self.peek_at(1).kind, TokenKind::Ident(_))
                {
                    // Один шаг path: primitive.method (стесняемся продолжать,
                    // primitives не имеют sub-namespace'ов).
                    if is_primitive_type && !starts_uppercase {
                        self.bump(); // dot
                        path.push(self.parse_ident()?.0);
                    }
                }
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
                if matches!(self.peek().kind, TokenKind::LBrace) {
                    // Plan 52 Ф.1: keyword в field-position → actionable error
                    // с HELP-подсказкой, до того как `{` уйдёт в блок-ветку.
                    if let Some(diag) = self.record_lit_keyword_field_error() {
                        return Err(diag);
                    }
                    if self.looks_like_record_lit() {
                        return self.parse_record_lit_after_path(path, first_span);
                    }
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
                // Plan 52 Ф.1: keyword в field-position → actionable error
                // с HELP-подсказкой, до того как `{` уйдёт в блок-ветку.
                if let Some(diag) = self.record_lit_keyword_field_error() {
                    return Err(diag);
                }
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
                    // Plan 19 C13: zero-arg lambda `() => ...` удалена
                    // (Plan 19 D22-rev). Используется `||` для no-arg
                    // closure-light. Здесь `()` — unit-литерал.
                    let end = self.bump().span;
                    return Ok(Expr::new(ExprKind::UnitLit, start.merge(end)));
                }
                // Plan 19, C13: legacy `(params) => ...` lambda
                // полностью удалена. Если в коде встречается
                // — выдаём понятную ошибку с подсказкой использовать
                // `|x|` (closure-light) или `fn(x)` (closure-full).
                // try_parse_lambda больше не вызывается; tuple/group
                // парсится прямо.
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
                    // Plan 19 C13: detect legacy lambda
                    // `(p1, p2) => body` и выдать понятную ошибку.
                    if matches!(
                        self.peek().kind,
                        TokenKind::FatArrow | TokenKind::Arrow
                    ) {
                        return Err(Diagnostic::new(
                            "legacy lambda `(params) => body` removed in Plan 19 D22-rev — \
                             use closure-light `|x, y| body` or closure-full `fn(x T) -> R body`".to_string(),
                            self.peek().span,
                        ));
                    }
                    Ok(Expr::new(ExprKind::TupleLit(elems), start.merge(end)))
                } else {
                    self.expect(&TokenKind::RParen)?;
                    if matches!(
                        self.peek().kind,
                        TokenKind::FatArrow | TokenKind::Arrow
                    ) {
                        return Err(Diagnostic::new(
                            "legacy lambda `(x) => body` removed in Plan 19 D22-rev — \
                             use closure-light `|x| body` or closure-full `fn(x T) -> R body`".to_string(),
                            self.peek().span,
                        ));
                    }
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
            TokenKind::KwSupervised => self.parse_supervised(),
            TokenKind::KwParallel => self.parse_parallel_for(),
            TokenKind::KwDetach => self.parse_detach(),
            TokenKind::KwThrow => {
                // D25/D65: `throw expr` as expression (type Never).
                // Stmt-level throw уже обрабатывается parse_stmt_or_expr;
                // expression-level — здесь, для match-arm body, ternary,
                // тd. Codegen эмитирует как Nova_Fail_fail(msg) +
                // zero-of-target-type dummy.
                let start_span = self.bump().span;
                let value = self.parse_expr()?;
                let span = start_span.merge(value.span);
                Ok(Expr::new(ExprKind::Throw(Box::new(value)), span))
            }
            TokenKind::KwHandler => self.parse_handler_lit(),
            TokenKind::KwForbid => self.parse_forbid(),
            TokenKind::KwRealtime => self.parse_realtime(),
            TokenKind::KwSelect => self.parse_select(),
            // Plan 19, C2: closure-light `|x| body` / `||` / `|_|`.
            // В expression-position (parse_primary) `|` всегда означает
            // начало closure-light. В infix-position он остаётся
            // bitwise OR (см. parse_bit_or в parser-цепочке).
            TokenKind::Pipe => self.parse_closure_light_with_params(start),
            // `||` — closure-light без параметров. Disambiguation от
            // logical-OR работает по позиции: в expression-position
            // (start) `||` всегда no-arg closure; в infix-position —
            // logical-OR (см. parse_logical_or).
            TokenKind::PipePipe => self.parse_closure_light_no_params(start),
            // Plan 19, C3: closure-full `fn(x int) Effects -> R body` —
            // анонимная типизированная fn в expression-position. В
            // отличие от item-level `fn name(...)`, имени нет —
            // следующий токен после `fn` обязан быть `(`.
            //
            // Type-expression `fn(int) -> bool` парсится отдельно
            // через parse_type, не сюда; в expression-position
            // type-expr не появляется.
            TokenKind::KwFn => self.parse_closure_full(start),
            other => Err(Diagnostic::new(
                format!("unexpected {} in expression", other.name()),
                start,
            )),
        }
    }

    /// Plan 19, C2: парсит closure-light с параметрами.
    ///
    /// Ожидает текущим токеном `|`. После `|` идут идентификаторы
    /// (имена параметров, разделённые запятой), затем закрывающий `|`,
    /// затем тело — bare expression или block.
    ///
    /// Wildcard `_` разрешён как имя параметра (D59 расширение).
    /// Типы параметров **запрещены** — closure-light всегда untyped.
    /// Если нужны типы — программист использует closure-full
    /// (`fn(x int) ...`, см. parse_closure_full).
    fn parse_closure_light_with_params(&mut self, start: Span) -> Result<Expr, Diagnostic> {
        // Съедаем открывающий `|`
        let open = self.expect(&TokenKind::Pipe)?.span;
        self.skip_newlines();

        let mut params = Vec::new();
        // Пустой `|...|` без параметров — некорректно, программист
        // должен писать `||`. Однако `|_|` валиден (один wildcard).
        if !matches!(self.peek().kind, TokenKind::Pipe) {
            loop {
                let p = self.parse_closure_light_param()?;
                params.push(p);
                self.skip_newlines();
                if self.eat(&TokenKind::Comma).is_none() {
                    break;
                }
                self.skip_newlines();
            }
        }
        // Закрывающий `|`. Если параметров нет — это была ошибка
        // программиста (`||` ловится отдельной веткой), здесь сразу
        // expect — даст понятную диагностику.
        let _close = self.expect(&TokenKind::Pipe).map_err(|d| {
            // Делаем сообщение чётче: подсказываем форму записи.
            let mut d = d;
            d.message = format!(
                "{} (in closure-light parameter list — `|x|`, `|x, y|`, `|_|`, или `||` для no-arg)",
                d.message
            );
            d
        })?;
        let _ = open;
        self.parse_closure_light_body(start, params)
    }

    /// Plan 19, C2: парсит closure-light без параметров (`|| body`).
    ///
    /// Текущий токен — `||` (двойной pipe). Тело — bare expression
    /// или block, как у обычной closure-light.
    fn parse_closure_light_no_params(&mut self, start: Span) -> Result<Expr, Diagnostic> {
        // Съедаем `||` целиком.
        let _ = self.expect(&TokenKind::PipePipe)?;
        self.parse_closure_light_body(start, Vec::new())
    }

    /// Парсит один параметр closure-light: имя или wildcard `_`.
    /// Типы запрещены — если программист написал `|x int|`, парсер
    /// даёт явную ошибку с подсказкой переключиться на closure-full.
    fn parse_closure_light_param(&mut self) -> Result<crate::ast::ClosureLightParam, Diagnostic> {
        let tok = self.peek().clone();
        let (name, span) = match &tok.kind {
            // В лексере Nova `_` парсится как `Ident("_")` (lexer/mod.rs:417);
            // wildcard и обычный identifier различаются по строковому значению.
            TokenKind::Ident(s) => {
                self.bump();
                (s.clone(), tok.span)
            }
            _ => {
                return Err(Diagnostic::new(
                    format!(
                        "expected closure-light parameter name (identifier or `_`), got {}",
                        tok.kind.name()
                    ),
                    tok.span,
                ))
            }
        };
        // Параметр не должен иметь тип (это было бы closure-full).
        // Если за именем идёт type-ish токен — даём понятную ошибку.
        if self.is_closure_light_type_after_name() {
            return Err(Diagnostic::new(
                "closure-light parameters are untyped — use `fn(x T)` syntax for typed closures (closure-full)".to_string(),
                self.peek().span,
            ));
        }
        Ok(crate::ast::ClosureLightParam { name, span })
    }

    /// Эвристика «после имени параметра идёт тип»: первый токен,
    /// который выглядит как начало type-expression. Используется
    /// для генерации хорошей ошибки в parse_closure_light_param.
    ///
    /// Знаем что после имени допустимы только: `,` (следующий param)
    /// или `|` (закрытие списка). Всё остальное — type-like.
    fn is_closure_light_type_after_name(&self) -> bool {
        !matches!(
            self.peek().kind,
            TokenKind::Comma | TokenKind::Pipe
        )
    }

    /// Общая часть: после съеденного `|...|` парсит тело closure'а.
    /// Тело — `Block` (если следующий токен `{`) или bare `Expr`.
    fn parse_closure_light_body(
        &mut self,
        start: Span,
        params: Vec<crate::ast::ClosureLightParam>,
    ) -> Result<Expr, Diagnostic> {
        // closure-light не использует `=>` — это часть «освобождения
        // `=>` от роли лямбда-стрелки» (D22-rev). Если программист
        // написал `|x| => expr`, даём явную ошибку.
        if matches!(self.peek().kind, TokenKind::FatArrow) {
            let span = self.peek().span;
            return Err(Diagnostic::new(
                "closure-light body starts immediately after `|...|`, no `=>` is used (D22-rev). Drop the `=>` or use a named fn / `fn(...)` if you need `=>`".to_string(),
                span,
            ));
        }
        // Block-форма: `|x| { stmts; expr }`.
        // В отличие от parse_primary, здесь НЕ применяем
        // record-литерал-эвристику — `|x| { name: ... }` это
        // block-body с record-литералом внутри был бы крайне редким
        // паттерном; для consistency block-форма всегда побеждает.
        if matches!(self.peek().kind, TokenKind::LBrace) {
            let block = self.parse_block()?;
            let span = start.merge(block.span);
            return Ok(Expr::new(
                ExprKind::ClosureLight {
                    params,
                    body: crate::ast::ClosureBody::Block(block),
                },
                span,
            ));
        }
        // Expression-форма: `|x| expr`. Тело — одно выражение.
        // Как у старой Lambda, парсим через parse_expr (полный
        // pratt-парсер).
        let body = self.parse_expr()?;
        let span = start.merge(body.span);
        Ok(Expr::new(
            ExprKind::ClosureLight {
                params,
                body: crate::ast::ClosureBody::Expr(Box::new(body)),
            },
            span,
        ))
    }

    /// Plan 19, C3: парсит closure-full — анонимную типизированную fn.
    ///
    /// Грамматика идентична named fn без имени:
    /// ```text
    /// closure-full = 'fn' '(' params ')' [ effects ] [ '->' type ] body
    /// body         = '=>' expression | block
    /// ```
    ///
    /// Где `params` — обычные `Param` с типами (как у named fn).
    /// **Generics на closure-full в bootstrap не поддерживаются** —
    /// rank-2 polymorphism это открытый вопрос (см. Q-rank2 / D61).
    /// Если после `fn` идёт `[`, парсер даст понятную ошибку.
    ///
    /// Тело — `=> expr` или `{ block }`, переиспользует parse_fn_body.
    fn parse_closure_full(&mut self, start: Span) -> Result<Expr, Diagnostic> {
        // Съедаем `fn`.
        let _ = self.expect(&TokenKind::KwFn)?;

        // Запрещаем generics: `fn[T](x T) -> T => x` не поддерживается
        // в bootstrap'е. Если потребуется — отдельный D-decision.
        if matches!(self.peek().kind, TokenKind::LBracket) {
            return Err(Diagnostic::new(
                "generics on closure-full are not supported in bootstrap (rank-2 polymorphism, see Q-rank2). Use a named fn or workaround through erasure".to_string(),
                self.peek().span,
            ));
        }

        // (params) — переиспользуем существующий parse_param.
        self.expect(&TokenKind::LParen)?;
        self.skip_newlines();
        let mut params = Vec::new();
        while !matches!(self.peek().kind, TokenKind::RParen) {
            params.push(self.parse_param()?);
            self.skip_newlines();
            if !matches!(self.peek().kind, TokenKind::RParen) {
                self.expect(&TokenKind::Comma)?;
                self.skip_newlines();
            }
        }
        self.expect(&TokenKind::RParen)?;

        // Variadic check (D69): variadic-параметр обязан быть последним.
        for (i, p) in params.iter().enumerate() {
            if p.is_variadic && i != params.len() - 1 {
                return Err(Diagnostic::new(
                    format!("variadic-параметр `{}` должен быть последним в списке (D69)", p.name),
                    p.span,
                ));
            }
        }

        // Effects между `)` и (`->` | body).
        let effects = self.parse_effects_until_arrow_or_body()?;
        let return_type = if self.eat(&TokenKind::Arrow).is_some() {
            Some(self.parse_type()?)
        } else {
            None
        };

        // Тело — `=> expr` или `{ block }`. Переиспользуем parse_fn_body.
        let body = self.parse_fn_body()?;
        let body_span = match &body {
            FnBody::Expr(e) => e.span,
            FnBody::Block(b) => b.span,
            FnBody::External => unreachable!(
                "closure-full cannot be `external` — only named fns can; \
                 parse_fn_body would have returned External only for top-level \
                 external fn parsing path"
            ),
        };
        let span = start.merge(body_span);

        // Plan 51 Ф.2: `=>`-тело замыкания — record-литерал ⇒ тип ровно
        // один раз (как для named fn; receiver у замыкания нет).
        Self::check_record_lit_type_once(&return_type, None, &body)?;

        Ok(Expr::new(
            ExprKind::ClosureFull(Box::new(crate::ast::FnSigBody {
                params,
                effects,
                return_type,
                body,
                span,
            })),
            span,
        ))
    }

    /// D.1.3: парсит `forall x in lo..hi : P(x)` или `exists x in lo..hi : P(x)`.
    ///
    /// Вызывается из parse_primary когда текущий токен — Ident("forall")
    /// или Ident("exists"). Оба являются контекстными ключевыми словами
    /// (не TokenKind), поэтому диспатч через проверку содержимого Ident.
    fn parse_quantifier(&mut self, is_forall: bool) -> Result<Expr, Diagnostic> {
        let start = self.bump().span; // consume "forall" / "exists"
        let (var_name, _var_span) = self.parse_ident()?; // bound variable
        self.expect(&TokenKind::KwIn)?;
        // Диапазон — lo..hi. Отключаем trailing/struct чтобы `:` не
        // поглощалось как named-argument или record-поле.
        let range = self.with_no_struct_or_trailing(|p| p.parse_expr())?;
        self.expect(&TokenKind::Colon)?;
        let body = self.parse_expr()?;
        let span = start.merge(body.span);
        if is_forall {
            Ok(Expr::new(ExprKind::Forall {
                var: var_name,
                range: Box::new(range),
                body: Box::new(body),
            }, span))
        } else {
            Ok(Expr::new(ExprKind::Exists {
                var: var_name,
                range: Box::new(range),
                body: Box::new(body),
            }, span))
        }
    }

    /// Эвристика: `{` перед нами — это начало record-литерала?
    /// Смотрим первый «значимый» токен внутри: `Ident :` или `...` или `}`.
    /// Plan 52 Ф.1 (D108): диагностика «keyword как имя поля в `{...}`».
    ///
    /// `{type: 1}` — `type` это keyword, не валидное имя поля (D83). Без
    /// этой проверки `looks_like_record_lit` вернул бы `false` (keyword ≠
    /// `Ident`), `{` распарсился бы как блок, и ошибка парсера была бы
    /// непонятной. Возвращает `Some(Diagnostic)` если после `{` (через
    /// newlines) стоит keyword в field-position (`kw :` / `kw ,` / `kw }`),
    /// с HELP-подсказкой использовать map-литерал `["kw": value]`.
    fn record_lit_keyword_field_error(&self) -> Option<Diagnostic> {
        if self.no_struct_lit {
            return None;
        }
        let mut i = self.pos + 1; // после `{`
        while i < self.tokens.len()
            && matches!(self.tokens[i].kind, TokenKind::Newline | TokenKind::Semicolon)
        {
            i += 1;
        }
        let tok = self.tokens.get(i)?;
        let kw_text = match &tok.kind {
            TokenKind::KwModule => "module",
            TokenKind::KwImport => "import",
            TokenKind::KwUse => "use",
            TokenKind::KwExport => "export",
            TokenKind::KwExternal => "external",
            TokenKind::KwFn => "fn",
            TokenKind::KwType => "type",
            TokenKind::KwProtocol => "protocol",
            TokenKind::KwEffect => "effect",
            TokenKind::KwHandler => "handler",
            TokenKind::KwAlias => "alias",
            TokenKind::KwLet => "let",
            TokenKind::KwConst => "const",
            TokenKind::KwMut => "mut",
            TokenKind::KwReadonly => "readonly",
            TokenKind::KwIf => "if",
            TokenKind::KwElse => "else",
            TokenKind::KwMatch => "match",
            TokenKind::KwFor => "for",
            TokenKind::KwWhile => "while",
            TokenKind::KwLoop => "loop",
            TokenKind::KwIn => "in",
            TokenKind::KwReturn => "return",
            TokenKind::KwBreak => "break",
            TokenKind::KwContinue => "continue",
            TokenKind::KwTest => "test",
            TokenKind::KwWith => "with",
            TokenKind::KwThrow => "throw",
            TokenKind::KwAs => "as",
            TokenKind::KwIs => "is",
            TokenKind::KwSpawn => "spawn",
            TokenKind::KwSupervised => "supervised",
            TokenKind::KwParallel => "parallel",
            TokenKind::KwDetach => "detach",
            TokenKind::KwInterrupt => "interrupt",
            TokenKind::KwForbid => "forbid",
            TokenKind::KwRealtime => "realtime",
            TokenKind::KwDefer => "defer",
            TokenKind::KwErrDefer => "errdefer",
            TokenKind::KwSelect => "select",
            _ => return None, // не keyword — обычный путь
        };
        // Keyword в field-position только если за ним `:` / `,` / `}`.
        let next = self.tokens.get(i + 1);
        if matches!(
            next.map(|t| &t.kind),
            Some(TokenKind::Colon | TokenKind::Comma | TokenKind::RBrace)
        ) {
            Some(Diagnostic::new(
                format!(
                    "keyword `{kw_text}` cannot be used as a field name in a \
                     record/map-coercion literal — use map-literal syntax \
                     instead: [\"{kw_text}\": value]"
                ),
                tok.span,
            ))
        } else {
            None
        }
    }

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
        // Plan 52.2 Ф.1: пустой `{}` — это **пустой блок**, не пустой
        // anonymous record (D55 §5 ревизия). Без этого parser'у удавалось
        // создать `RecordLit { type_name: None, fields: [] }` который
        // codegen не может обработать (нет struct-name для inference).
        // Empty block — это valid `nova_unit` value, что и ожидается в
        // позициях типа `_ => {}` (match-arm body), `if cond { } else ...`.
        if matches!(self.tokens[i].kind, TokenKind::RBrace) {
            return false;
        }
        if matches!(self.tokens[i].kind, TokenKind::DotDotDot) {
            return true;
        }
        // `@`-shorthand в record-литерале: `@field` punning. Но `@method(...)`
        // — это method call (statement). Различаем по двум следующим
        // токенам: `@ Ident (Comma|RBrace|Colon)` — punning; иначе — call.
        if matches!(self.tokens[i].kind, TokenKind::At) {
            let after_at = self.tokens.get(i + 1);
            let after_ident = self.tokens.get(i + 2);
            if matches!(after_at.map(|t| &t.kind), Some(TokenKind::Ident(_)))
                && matches!(after_ident.map(|t| &t.kind),
                    Some(TokenKind::Comma | TokenKind::RBrace | TokenKind::Colon))
            {
                return true;
            }
            // Bare `@` (без ident) — self-value; в record-lit это
            // нонсенс, но в expression-блоке валидно. Не record-lit.
            return false;
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
                // Plan 52 Ф.10: заполняется type-checker'ом (MapLitAnnotator)
                // если запись стоит в позиции #from_fields-типа.
                inferred_map_v: None,
            },
            start.merge(end),
        ))
    }

    /// Парсит `[...]` — array-литерал (D27/D38) ИЛИ map-литерал (D108).
    ///
    /// Парсинг **локальный, без type-directed** (D108):
    /// 1. `[]` пустой → `ArrayLit(vec![])` — array-или-map, разрешается на
    ///    type-check по ожидаемому типу.
    /// 2. Иначе парсим первое выражение; если первый элемент — `...spread`,
    ///    это всегда array. Следующий токен после первого expr:
    ///    - `:` → map-литерал, дальше пары `expr : expr`;
    ///    - `,` / `]` → array-литерал.
    /// 3. Смешение форм (`[a, b: c]`) → actionable error.
    fn parse_array_lit(&mut self) -> Result<Expr, Diagnostic> {
        let start = self.expect(&TokenKind::LBracket)?.span;
        self.skip_newlines();

        // Пустой `[]` — array-или-map, разрешается на type-check.
        // D38 array-type-static-method: `[]T.method(...)` — empty литерал
        // immediately followed by Ident `.` означает array-type prefix, не
        // empty literal. Превращаем в Path(["__array", "<T>"]) — codegen
        // эмитит как `nova_array_new_<T>` для `.new()` / `.with_capacity()`.
        if matches!(self.peek().kind, TokenKind::RBracket) {
            let end = self.expect(&TokenKind::RBracket)?.span;
            if let TokenKind::Ident(_) = &self.peek().kind {
                if matches!(self.peek_at(1).kind, TokenKind::Dot) {
                    let (elem_type_name, ty_span) = self.parse_ident()?;
                    return Ok(Expr::new(
                        ExprKind::Path(vec!["__array".to_string(), elem_type_name]),
                        start.merge(ty_span),
                    ));
                }
            }
            return Ok(Expr::new(ExprKind::ArrayLit(Vec::new()), start.merge(end)));
        }

        // Первый элемент: `...spread`. Spread может быть **либо** в array
        // (`[...arr1, x, ...arr2]`), **либо** в map (`[...defaults, k:v]`).
        // Plan 55 followup (D108 spread): lookahead на second элемент чтобы
        // определить mode:
        // - `...spread` потом `, expr :` → map literal с map-spread.
        // - `...spread` потом `, expr ,` / `, expr ]` / `, ...spread` → array.
        // Edge cases: одиночный `[...spread]` — рассматривается как array
        // (legacy default, более частый use case).
        if self.eat(&TokenKind::DotDotDot).is_some() {
            let v = self.parse_expr()?;
            // Lookahead: после `, <expr> :` → map mode.
            if self.peek().kind == TokenKind::Comma {
                // Snapshot saved-position для potential rollback. Используем
                // peek_at для non-destructive lookahead.
                // Skip newlines после comma логически — но peek_at не делает
                // skip; используем sliding scan через peek_at.
                let mut i = 1usize; // skip comma at offset 0
                // Skip newlines tokens (TokenKind::Newline).
                while matches!(self.peek_at(i).kind, TokenKind::Newline) { i += 1; }
                // Если следующий не-newline токен — `...` → array (still).
                // Иначе парсим expression и проверяем `:`. Чтобы не парсить
                // дважды — scan на наличие `:` до `,`/`]` на верхнем уровне.
                // Bootstrap: используем простой scan с depth counter.
                // Inline scan на верхнем уровне `[...]` — ищем `:` до `,`/`]`.
                // Depth counter для nested brackets/parens/braces.
                let mut depth = 0i32;
                let mut is_map = false;
                let mut j = i;
                loop {
                    let t = &self.peek_at(j).kind;
                    match t {
                        TokenKind::LParen | TokenKind::LBracket | TokenKind::LBrace => depth += 1,
                        TokenKind::RParen | TokenKind::RBrace => {
                            if depth == 0 { break; }
                            depth -= 1;
                        }
                        TokenKind::RBracket => {
                            if depth == 0 { break; }
                            depth -= 1;
                        }
                        TokenKind::Comma if depth == 0 => break,
                        TokenKind::Colon if depth == 0 => { is_map = true; break; }
                        TokenKind::Eof => break,
                        _ => {}
                    }
                    j += 1;
                    if j > i + 256 { break; } // safety — limit lookahead.
                }
                if is_map {
                    // Map mode: convert уже распарсенный spread в MapElem.
                    return self.parse_map_lit_rest(start, vec![MapElem::Spread(v)]);
                }
            }
            return self.parse_array_lit_rest(start, vec![ArrayElem::Spread(v)]);
        }
        let first = self.parse_expr()?;
        if matches!(self.peek().kind, TokenKind::Colon) {
            self.bump(); // :
            self.skip_newlines();
            let first_val = self.parse_expr()?;
            return self.parse_map_lit_rest(start, vec![MapElem::Pair(first, first_val)]);
        }
        // Array-литерал: первый элемент уже распарсен.
        self.parse_array_lit_rest(start, vec![ArrayElem::Item(first)])
    }

    /// Продолжает парсинг array-литерала после уже распарсенного первого
    /// элемента. Обрабатывает `,`-разделители, `...spread`, trailing comma.
    /// Если внутри встречается `expr :` — actionable error (смешение форм).
    fn parse_array_lit_rest(
        &mut self,
        start: Span,
        mut elems: Vec<ArrayElem>,
    ) -> Result<Expr, Diagnostic> {
        // После первого элемента: либо `,` (ещё элементы), либо `]`.
        loop {
            self.skip_newlines();
            if matches!(self.peek().kind, TokenKind::RBracket) {
                break;
            }
            if self.eat(&TokenKind::Comma).is_none() {
                // Нет `,` и нет `]` — синтаксическая ошибка. Частый случай:
                // `[a b]` (забыли запятую).
                let span = self.peek().span;
                return Err(Diagnostic::new(
                    format!(
                        "expected `,` or `]` in array literal, got {}",
                        self.peek().kind.name()
                    ),
                    span,
                ));
            }
            self.skip_newlines();
            if matches!(self.peek().kind, TokenKind::RBracket) {
                break; // trailing comma
            }
            if self.eat(&TokenKind::DotDotDot).is_some() {
                let v = self.parse_expr()?;
                elems.push(ArrayElem::Spread(v));
            } else {
                let v = self.parse_expr()?;
                // Смешение форм: `[a, b: c]` — после array-элемента видим `:`.
                if matches!(self.peek().kind, TokenKind::Colon) {
                    return Err(Diagnostic::new(
                        "cannot mix array and map syntax in `[...]` — either all \
                         elements are `k: v` pairs (map literal) or none are (array \
                         literal)",
                        v.span,
                    ));
                }
                elems.push(ArrayElem::Item(v));
            }
        }
        let end = self.expect(&TokenKind::RBracket)?.span;
        Ok(Expr::new(ExprKind::ArrayLit(elems), start.merge(end)))
    }

    /// Продолжает парсинг map-литерала после уже распарсенной первой пары
    /// `k: v`. Обрабатывает `,`-разделители, trailing comma. Если внутри
    /// встречается элемент без `:` — actionable error (смешение форм).
    fn parse_map_lit_rest(
        &mut self,
        start: Span,
        mut elems: Vec<MapElem>,
    ) -> Result<Expr, Diagnostic> {
        loop {
            self.skip_newlines();
            if matches!(self.peek().kind, TokenKind::RBracket) {
                break;
            }
            if self.eat(&TokenKind::Comma).is_none() {
                let span = self.peek().span;
                return Err(Diagnostic::new(
                    format!(
                        "expected `,` or `]` in map literal, got {}",
                        self.peek().kind.name()
                    ),
                    span,
                ));
            }
            self.skip_newlines();
            if matches!(self.peek().kind, TokenKind::RBracket) {
                break; // trailing comma
            }
            // Plan 55 followup (D108-spread): `...m` в map-литерале —
            // spread другой map. Должен быть совместимого типа.
            if self.eat(&TokenKind::DotDotDot).is_some() {
                let v = self.parse_expr()?;
                elems.push(MapElem::Spread(v));
                continue;
            }
            let k = self.parse_expr()?;
            // Смешение форм: `[k: v, x]` — после map-пары элемент без `:`.
            if !matches!(self.peek().kind, TokenKind::Colon) {
                return Err(Diagnostic::new(
                    "cannot mix map and array syntax in `[...]` — every entry of a \
                     map literal must be `key: value` (или `...map-spread`)",
                    k.span,
                ));
            }
            self.bump(); // :
            self.skip_newlines();
            let v = self.parse_expr()?;
            elems.push(MapElem::Pair(k, v));
        }
        let end = self.expect(&TokenKind::RBracket)?.span;
        // Plan 52 Ф.7: inferred_key/value заполняются type-checker'ом
        // через MapLitCtx::annotate_module — для генерации turbofish
        // `HashMap[K,V].with_capacity(n)` в десугаринге. Парсер не имеет
        // type-info, оставляет None.
        Ok(Expr::new(
            ExprKind::MapLit {
                elems,
                inferred_key: None,
                inferred_value: None,
                inferred_target_type: None,
            },
            start.merge(end),
        ))
    }

    // Plan 19, C13: try_parse_lambda удалена. Старая `(params) =>`
    // grammar отменена в Plan 19 D22-rev. Closure-light `|x| body`
    // и closure-full `fn(x T) -> R body` — единственные формы
    // безымянной функции.

    fn parse_if(&mut self) -> Result<Expr, Diagnostic> {
        let start = self.expect(&TokenKind::KwIf)?.span;
        // `if let pattern = expr { ... }` — D34
        if matches!(self.peek().kind, TokenKind::KwLet) {
            self.bump();
            let pattern = self.parse_pattern()?;
            self.expect(&TokenKind::Eq)?;
            let scrutinee = self.with_no_struct_or_trailing(|p| p.parse_expr())?;
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
        let cond = self.with_no_struct_or_trailing(|p| p.parse_expr())?;
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
        let scrutinee = self.with_no_struct_or_trailing(|p| p.parse_expr())?;
        self.expect(&TokenKind::LBrace)?;
        let mut arms = Vec::new();
        self.skip_newlines();
        while !matches!(self.peek().kind, TokenKind::RBrace) {
            // Pattern alternation: `pat1 | pat2 | pat3 =>`. Собираем
            // в Pattern::Or если есть хотя бы один `|` после первого
            // pattern'а (но до `=>` / `if`-guard'а).
            let first = self.parse_pattern()?;
            let pattern = if matches!(self.peek().kind, TokenKind::Pipe) {
                let mut alts = vec![first];
                let start_span = alts[0].span();
                while matches!(self.peek().kind, TokenKind::Pipe) {
                    self.bump();
                    self.skip_newlines();
                    alts.push(self.parse_pattern()?);
                }
                let end_span = alts.last().map(|p| p.span()).unwrap_or(start_span);
                Pattern::Or {
                    alternatives: alts,
                    span: start_span.merge(end_span),
                }
            } else {
                first
            };
            let guard = if matches!(self.peek().kind, TokenKind::KwIf) {
                self.bump();
                Some(self.parse_expr()?)
            } else {
                None
            };
            self.expect(&TokenKind::FatArrow)?;
            self.skip_newlines();
            // body: либо `{ block }` (D19 исключение), либо expr.
            // Special case: `return X` / `break` / `continue` — control-flow
            // statements в arm body. Идиоматично для early-exit:
            //   match opt { Some(v) => v, None => return defaults }
            // Эмитим как Block с одним stmt'ом, тип unit (! фактически).
            let body = if matches!(self.peek().kind,
                TokenKind::KwReturn | TokenKind::KwBreak | TokenKind::KwContinue)
            {
                let stmt_or_expr = self.parse_stmt_or_expr()?;
                let stmts = match stmt_or_expr {
                    StmtOrExpr::Stmt(s) => vec![s],
                    StmtOrExpr::Expr(e) => vec![Stmt::Expr(e)],
                };
                let last_span = stmts.last().map(|s| match s {
                    Stmt::Return { span, .. } => *span,
                    Stmt::Break(s) | Stmt::Continue(s) => *s,
                    Stmt::Expr(e) => e.span,
                    _ => pattern.span(),
                }).unwrap_or_else(|| pattern.span());
                MatchArmBody::Block(Block {
                    stmts,
                    trailing: None,
                    span: pattern.span().merge(last_span),
                })
            } else if matches!(self.peek().kind, TokenKind::LBrace) {
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
            // D49: разделитель между arms — newline или `,`. Опциональная
            // trailing-запятая после последнего arm'а тоже допустима.
            self.eat(&TokenKind::Comma);
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
        let iter = self.with_no_struct_or_trailing(|p| p.parse_expr())?;
        // Plan 33.2 Ф.6 + 33.3 Ф.9.3/9.5/9.8: loop invariants/decreases.
        let (invs, decr) = self.parse_loop_clauses()?;
        let mut body = self.parse_block()?;
        Self::inject_loop_invariants(invs.clone(), &mut body);
        Self::inject_loop_decreases(decr.clone(), &mut body);
        let end = body.span;
        let loop_expr = Expr::new(
            ExprKind::For {
                pattern,
                iter: Box::new(iter),
                body,
                invariants: invs.clone(),
                decreases: decr.map(Box::new),
            },
            start.merge(end),
        );
        Ok(Self::wrap_loop_with_preentry_check(loop_expr, &invs))
    }

    /// `parallel for x in iter { body }` — D14 fan-out (D50 supervised + spawn).
    fn parse_parallel_for(&mut self) -> Result<Expr, Diagnostic> {
        let start = self.expect(&TokenKind::KwParallel)?.span;
        self.expect(&TokenKind::KwFor)?;
        let pattern = self.parse_pattern()?;
        self.expect(&TokenKind::KwIn)?;
        let iter = self.with_no_struct_or_trailing(|p| p.parse_expr())?;
        let body = self.parse_block()?;
        let end = body.span;
        Ok(Expr::new(
            ExprKind::ParallelFor {
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
            let scrutinee = self.with_no_struct_or_trailing(|p| p.parse_expr())?;
            let body = self.parse_block()?;
            let end = body.span;
            return Ok(Expr::new(
                ExprKind::WhileLet {
                    pattern,
                    scrutinee: Box::new(scrutinee),
                    body,
                    invariants: vec![],
                    decreases: None,
                },
                start.merge(end),
            ));
        }
        let cond = self.with_no_struct_or_trailing(|p| p.parse_expr())?;
        // Plan 33.2 Ф.6 + 33.3 Ф.9.3/9.5/9.8: loop invariants/decreases.
        let (invs, decr) = self.parse_loop_clauses()?;
        let mut body = self.parse_block()?;
        Self::inject_loop_invariants(invs.clone(), &mut body);
        Self::inject_loop_decreases(decr.clone(), &mut body);
        let end = body.span;
        let loop_expr = Expr::new(
            ExprKind::While {
                cond: Box::new(cond),
                body,
                invariants: invs.clone(),
                decreases: decr.map(Box::new),
            },
            start.merge(end),
        );
        Ok(Self::wrap_loop_with_preentry_check(loop_expr, &invs))
    }

    fn parse_loop(&mut self) -> Result<Expr, Diagnostic> {
        let start = self.expect(&TokenKind::KwLoop)?.span;
        // Plan 33.2 Ф.6 + 33.3 Ф.9.3/9.5/9.8: loop invariants/decreases.
        let (invs, decr) = self.parse_loop_clauses()?;
        let mut body = self.parse_block()?;
        Self::inject_loop_invariants(invs.clone(), &mut body);
        Self::inject_loop_decreases(decr.clone(), &mut body);
        let end = body.span;
        let loop_expr = Expr::new(ExprKind::Loop { body, invariants: invs.clone(), decreases: decr.map(Box::new) }, start.merge(end));
        Ok(Self::wrap_loop_with_preentry_check(loop_expr, &invs))
    }

    /// Plan 33.2 Ф.6 + 33.3 Ф.9.3 (D24): парсит loop-attached clauses.
    /// - `invariant <expr>` (multiple lines).
    /// - `decreases <expr>` (single, optional).
    ///
    /// Возвращает Vec<Expr> с invariants. `decreases` пока проглатывается
    /// (runtime check для loop-decreases — отдельная задача).
    ///
    /// **Plan 33.3 Ф.9.3**: invariants теперь возвращаются caller'у вместо
    /// игнорирования. Caller (parse_while/for/loop) inject'ит их в body
    /// как `assert_static`-statements: pre-loop, post-iteration. Это даёт
    /// runtime-check в debug-сборке. SMT verify (полноценный havoc-based)
    /// ждёт Z3 backend.
    fn parse_loop_clauses(&mut self) -> Result<(Vec<Expr>, Option<Expr>), Diagnostic> {
        let mut invariants = Vec::new();
        let mut decreases: Option<Expr> = None;
        loop {
            self.skip_newlines();
            match &self.peek().kind {
                TokenKind::Ident(n) if n == "invariant" => {
                    self.bump();
                    let e = self.with_no_struct_or_trailing(|p| p.parse_expr())?;
                    invariants.push(e);
                }
                TokenKind::Ident(n) if n == "decreases" => {
                    if decreases.is_some() {
                        let sp = self.peek().span;
                        return Err(Diagnostic::new(
                            "duplicate `decreases` clause on loop", sp));
                    }
                    self.bump();
                    let e = self.with_no_struct_or_trailing(|p| p.parse_expr())?;
                    decreases = Some(e);
                }
                _ => break,
            }
        }
        // Skip trailing newlines чтобы caller'у parse_block видеть `{`.
        self.skip_newlines();
        Ok((invariants, decreases))
    }

    /// Plan 33.3 Ф.9.8: inject loop `decreases` runtime check.
    /// До body эмитим `let _nova_decr_old = <decreases_expr>` (snapshot).
    /// После body эмитим `assert_static <decreases_expr> < _nova_decr_old`
    /// (проверка decrement).
    fn inject_loop_decreases(decreases: Option<Expr>, body: &mut Block) {
        let Some(d) = decreases else { return };
        let span = d.span;
        // Synthesize: let _nova_decr_old = <d>
        let snapshot_let = Stmt::Let(LetDecl {
            mutable: false,
            pattern: Pattern::Ident { name: "_nova_decr_old".into(), span },
            ty: None,
            value: d.clone(),
            span,
            is_ghost: false,
        });
        // Synthesize: assert_static (<d>) < _nova_decr_old
        let check_expr = Expr::new(
            ExprKind::Binary {
                op: BinOp::Lt,
                left: Box::new(d.clone()),
                right: Box::new(Expr::new(ExprKind::Ident("_nova_decr_old".into()), span)),
            },
            span,
        );
        let check_stmt = Stmt::AssertStatic { expr: check_expr, span };
        // Snapshot — в начало body, check — в конец.
        body.stmts.insert(0, snapshot_let);
        body.stmts.push(check_stmt);
    }

    /// Inject invariant'ы в body цикла как assert_static-stmt'ы.
    /// Per-iteration: prepend invariants в начало body — check срабатывает
    /// перед каждой итерацией (после первой).
    ///
    /// Pre-entry check делается отдельно через `wrap_loop_with_preentry_check`
    /// (Plan 33.3 Ф.9.5) — wrap'ит loop-expr в Block с pre-entry asserts
    /// перед loop'ом.
    fn inject_loop_invariants(invariants: Vec<Expr>, body: &mut Block) {
        for inv in invariants.into_iter().rev() {
            let span = inv.span;
            body.stmts.insert(0, Stmt::AssertStatic { expr: inv, span });
        }
    }

    /// Plan 33.3 Ф.9.5: wrap loop-expression в Block с pre-entry assert_static
    /// для каждого invariant. Это catches violation **до** первой итерации
    /// (когда invariant ложен от старта или loop никогда не выполняется).
    fn wrap_loop_with_preentry_check(loop_expr: Expr, invariants: &[Expr]) -> Expr {
        if invariants.is_empty() {
            return loop_expr;
        }
        let span = loop_expr.span;
        let stmts: Vec<Stmt> = invariants.iter()
            .map(|inv| Stmt::AssertStatic { expr: inv.clone(), span: inv.span })
            .collect();
        // Trailing: loop сам — final expr block'а (loop возвращает unit).
        let block = Block {
            stmts,
            trailing: Some(Box::new(loop_expr)),
            span,
        };
        Expr::new(ExprKind::Block(block), span)
    }

    fn parse_with(&mut self) -> Result<Expr, Diagnostic> {
        let start = self.expect(&TokenKind::KwWith)?.span;
        let mut bindings = Vec::new();
        loop {
            // Plan 33.3 Ф.9.6: optional `#verify_handler` или `#trusted_handler`
            // перед effect-name. Применяется к ЭТОЙ конкретной binding'е.
            let verification = self.parse_handler_verification_attr()?;
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
                verification,
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

    /// Plan 33.3 Ф.9.6: парсит `#verify` или `#trusted` (без аргументов)
    /// перед with-binding'ом. Без атрибута — Unverified.
    /// Дублирующиеся / противоречащие атрибуты → error.
    ///
    /// Refactor: раньше `#verify_handler` / `#trusted_handler` —
    /// упростили до `#verify` / `#trusted` (контекст определяет
    /// смысл — внутри with-binding это про handler).
    fn parse_handler_verification_attr(&mut self) -> Result<HandlerVerification, Diagnostic> {
        if !matches!(self.peek().kind, TokenKind::Hash) {
            return Ok(HandlerVerification::Unverified);
        }
        let name = match &self.peek_at(1).kind {
            TokenKind::Ident(n) => n.clone(),
            _ => return Ok(HandlerVerification::Unverified),
        };
        let result = match name.as_str() {
            "verify" => HandlerVerification::Verify,
            "trusted" => HandlerVerification::Trusted,
            _ => return Ok(HandlerVerification::Unverified),
        };
        self.bump(); // #
        self.bump(); // ident
        self.skip_newlines();
        // Disallow stacking `#verify #trusted` (контрадикция).
        if matches!(self.peek().kind, TokenKind::Hash) {
            if let TokenKind::Ident(next) = &self.peek_at(1).kind {
                if next == "verify" || next == "trusted" {
                    let span = self.peek().span;
                    return Err(Diagnostic::new(
                        "duplicate or conflicting handler verification attribute \
                         (`#verify` and `#trusted` are mutually exclusive)",
                        span,
                    ));
                }
            }
        }
        Ok(result)
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
        // Handler — это выражение в позиции `with E = <expr> { body }`.
        // Чтобы `(e) => interrupt Some(e)` не "сожрало" следующий `{`-block
        // как trailing-block, парсим в режиме no_trailing_block. С этим
        // флагом `interrupt Some(e) { body }` остановится на `interrupt Some(e)`,
        // а `{ body }` достанется внешнему with-парсеру.
        let saved_trailing = self.no_trailing_block;
        self.no_trailing_block = true;
        let result = self.parse_expr();
        self.no_trailing_block = saved_trailing;
        result
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
        // Опциональные generic-параметры эффекта (Fail[Error], etc.)
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

    /// `supervised { body }` / `supervised(cancel: tok) { body }` —
    /// structured-concurrency scope (D50 / D75 revised, Plan 47).
    ///
    /// После `supervised` опционально идёт `( cancel : expr )` — единственный
    /// допустимый именованный аргумент keyword-конструкции (V1). Прочие имена
    /// или позиционная форма → diagnostic.
    fn parse_supervised(&mut self) -> Result<Expr, Diagnostic> {
        let start = self.expect(&TokenKind::KwSupervised)?.span;
        let cancel = if matches!(self.peek().kind, TokenKind::LParen) {
            self.bump(); // (
            self.skip_newlines();
            // Имя аргумента — обязано быть `cancel`.
            match &self.peek().kind {
                TokenKind::Ident(name) if name == "cancel" => {
                    self.bump(); // cancel
                }
                other => {
                    return Err(Diagnostic::new(
                        format!(
                            "`supervised` accepts only the named argument `cancel:` \
                             (got {}); use `supervised(cancel: tok) {{ ... }}`",
                            other.name()
                        ),
                        self.peek().span,
                    ));
                }
            }
            self.expect(&TokenKind::Colon)?;
            self.skip_newlines();
            let expr = self.parse_expr()?;
            self.skip_newlines();
            self.expect(&TokenKind::RParen)?;
            Some(Box::new(expr))
        } else {
            None
        };
        let block = self.parse_block()?;
        let end = block.span;
        Ok(Expr::new(
            ExprKind::Supervised { body: block, cancel },
            start.merge(end),
        ))
    }

    /// `detach { body }` — fire-and-forget, global supervisor (D50).
    fn parse_detach(&mut self) -> Result<Expr, Diagnostic> {
        let start = self.expect(&TokenKind::KwDetach)?.span;
        let block = self.parse_block()?;
        let end = block.span;
        Ok(Expr::new(ExprKind::Detach(block), start.merge(end)))
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

    /// `select { arm* }` --- D94 multiplexed channel operation.
    fn parse_select(&mut self) -> Result<Expr, Diagnostic> {
        let start = self.expect(&TokenKind::KwSelect)?.span;
        self.expect(&TokenKind::LBrace)?;
        let mut arms: Vec<SelectArm> = Vec::new();
        self.skip_newlines();
        while !matches!(self.peek().kind, TokenKind::RBrace) {
            let arm_start = self.peek().span;
            let op = self.parse_select_op()?;
            let guard = if matches!(self.peek().kind, TokenKind::KwIf) {
                self.bump();
                Some(self.parse_expr()?)
            } else {
                None
            };
            self.expect(&TokenKind::FatArrow)?;
            self.skip_newlines();
            let body = self.parse_block()?;
            let arm_span = arm_start.merge(body.span);
            arms.push(SelectArm { op, guard, body, span: arm_span });
            self.eat(&TokenKind::Comma);
            self.skip_newlines();
        }
        let end = self.expect(&TokenKind::RBrace)?.span;
        Ok(Expr::new(ExprKind::Select { arms }, start.merge(end)))
    }

    fn parse_select_op(&mut self) -> Result<SelectOp, Diagnostic> {
        // `Some(ident) = expr` --- recv arm with binding
        if matches!(self.peek().kind, TokenKind::Ident(ref s) if s == "Some") {
            let saved = self.pos;
            self.bump();
            if matches!(self.peek().kind, TokenKind::LParen) {
                self.bump();
                if let TokenKind::Ident(binding_s) = self.peek().kind.clone() {
                    self.bump();
                    if matches!(self.peek().kind, TokenKind::RParen) {
                        self.bump();
                        if matches!(self.peek().kind, TokenKind::Eq) {
                            self.bump();
                            let chan = self.parse_expr()?;
                            return Ok(SelectOp::Recv { binding: Some(binding_s), chan: Box::new(chan) });
                        }
                    }
                }
            }
            self.pos = saved;
        }
        // `_ = expr` recv arm or `_` default arm
        if matches!(self.peek().kind, TokenKind::Ident(ref s) if s == "_") {
            let saved = self.pos;
            self.bump();
            if matches!(self.peek().kind, TokenKind::Eq) {
                self.bump();
                let chan = self.parse_expr()?;
                return Ok(SelectOp::Recv { binding: None, chan: Box::new(chan) });
            }
            self.pos = saved;
            self.bump();
            return Ok(SelectOp::Default);
        }
        // Send arm: `chan.send(value)`
        let chan = self.parse_primary()?;
        self.expect(&TokenKind::Dot)?;
        let method_span = self.peek().span;
        match &self.peek().kind {
            TokenKind::Ident(s) if s == "send" => { self.bump(); }
            _ => return Err(Diagnostic::new(
                "select send arm: expected `.send(value)` after channel expression".to_string(),
                method_span,
            )),
        }
        self.expect(&TokenKind::LParen)?;
        let value = self.parse_expr()?;
        self.expect(&TokenKind::RParen)?;
        Ok(SelectOp::Send { chan: Box::new(chan), value: Box::new(value) })
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
            // Plan 33.3 (D24): `ghost let` — контекстный keyword `ghost`.
            TokenKind::Ident(ref n) if n == "ghost" && matches!(self.peek_at(1).kind, TokenKind::KwLet) => {
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
            // D90: `defer body` — scope-level cleanup. body — expression
            // (включая block-expression `{ ... }`).
            TokenKind::KwDefer => {
                self.bump();
                let body = self.parse_expr()?;
                let span = start.merge(body.span);
                Ok(StmtOrExpr::Stmt(Stmt::Defer { body, span }))
            }
            // D90: `errdefer body` — cleanup только на throw/panic-exit.
            TokenKind::KwErrDefer => {
                self.bump();
                let body = self.parse_expr()?;
                let span = start.merge(body.span);
                Ok(StmtOrExpr::Stmt(Stmt::ErrDefer { body, span }))
            }
            // Plan 33.2 Ф.8 (D24): `assert_static <bool>` — intermediate
            // proof obligation. Контекстный keyword (Ident в лексере).
            TokenKind::Ident(ref n) if n == "assert_static" => {
                self.bump();
                let expr = self.parse_expr()?;
                let span = start.merge(expr.span);
                Ok(StmtOrExpr::Stmt(Stmt::AssertStatic { expr, span }))
            }
            // Plan 33.3 (D24): `assume <bool>` — escape hatch.
            TokenKind::Ident(ref n) if n == "assume" => {
                self.bump();
                let expr = self.parse_expr()?;
                let span = start.merge(expr.span);
                Ok(StmtOrExpr::Stmt(Stmt::Assume { expr, span }))
            }
            // Plan 33.5 Ф.4.1: `apply lemma_name(args)` — активация lemma.
            // Ф.13.1 (Plan 33.6): `apply lemma_name` (без скобок) — auto-inference из scope.
            // Контекстуальный keyword: `apply` не резервируем глобально.
            TokenKind::Ident(ref n) if n == "apply" => {
                self.bump();
                let name = match self.peek().kind.clone() {
                    TokenKind::Ident(n) => { self.bump(); n }
                    _ => return Err(Diagnostic::new(
                        "expected lemma name after `apply`",
                        self.peek().span,
                    )),
                };
                // Ф.13.1: args скобки опциональны. `apply lemma` без `(...)` →
                // empty args → verify-side auto-inference.
                let mut args = Vec::new();
                let end = if matches!(self.peek().kind, TokenKind::LParen) {
                    self.bump(); // (
                    while !matches!(self.peek().kind, TokenKind::RParen) {
                        args.push(self.parse_expr()?);
                        if !matches!(self.peek().kind, TokenKind::RParen) {
                            self.expect(&TokenKind::Comma)?;
                        }
                    }
                    self.expect(&TokenKind::RParen)?.span
                } else {
                    // Без скобок — auto-mode, span до имени.
                    self.tokens[self.pos.saturating_sub(1)].span
                };
                let span = start.merge(end);
                Ok(StmtOrExpr::Stmt(Stmt::Apply { lemma: name, args, span }))
            }
            // Plan 33.5 Ф.4.2: `calc { ... }` — структурированное доказательство.
            // Контекстуальный keyword (не резервируем `calc` глобально).
            TokenKind::Ident(ref n) if n == "calc" => {
                self.bump(); // consume `calc`
                Ok(StmtOrExpr::Stmt(self.parse_calc_stmt(start)?))
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
        // Plan 19, C13: `{ params => body }` отменён. Trailing-block
        // только без params (DSL-форма по D43-rev). Trailing с
        // параметрами — отдельная конструкция `f(args) fn(p) body`.
        self.expect(&TokenKind::LBrace)?;
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
            params: Vec::new(),
            body: Block {
                stmts,
                trailing,
                span: start.merge(end),
            },
            span: start.merge(end),
        })
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
            TokenKind::Char(cp) => {
                self.bump();
                Ok(Pattern::Literal(Literal::Char(cp), start))
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

    /// D44 string interpolation (Plan 17 Ф.4): `"... ${expr} ..."` →
    /// `ExprKind::InterpolatedStr { parts }`.
    ///
    /// Вход — сырая строка после lex'а; literal `\${` уже преобразован
    /// lexer'ом в sentinel `\x01$` (SOH+$), чтобы парсер мог отличить
    /// настоящий `${` от escape'нутого. Sentinel удаляется при сборке
    /// literal-частей.
    ///
    /// Если интерполяций нет — возвращаем обычный `StrLit`.
    /// Иначе codegen сам построит StringBuilder-цепочку (одна
    /// аллокация с pre-size estimate, без O(N²) от `+`).
    fn desugar_string_interpolation(
        &mut self,
        raw: String,
        span: Span,
    ) -> Result<Expr, Diagnostic> {
        let bytes = raw.as_bytes();
        let mut parts: Vec<InterpPart> = Vec::new();
        let mut cur_lit = String::new();
        let mut i = 0;
        while i < bytes.len() {
            let b = bytes[i];
            if b == 0x01 {
                // SOH sentinel: следующий байт — буквальный `$` (escape \$).
                if i + 1 < bytes.len() {
                    cur_lit.push(bytes[i + 1] as char);
                    i += 2;
                    continue;
                } else {
                    i += 1;
                    continue;
                }
            }
            if b == b'$' && i + 1 < bytes.len() && bytes[i + 1] == b'{' {
                // ${expr} — flush литерала и парсим выражение.
                if !cur_lit.is_empty() {
                    parts.push(InterpPart::Lit(std::mem::take(&mut cur_lit)));
                }
                // Найти `}` с балансом скобок (поддержка nested {}).
                let expr_start = i + 2;
                let mut depth: i32 = 1;
                let mut j = expr_start;
                while j < bytes.len() && depth > 0 {
                    match bytes[j] {
                        b'{' => depth += 1,
                        b'}' => {
                            depth -= 1;
                            if depth == 0 {
                                break;
                            }
                        }
                        _ => {}
                    }
                    j += 1;
                }
                if depth != 0 {
                    return Err(Diagnostic::new(
                        "unterminated ${...} interpolation in string literal",
                        span,
                    ));
                }
                let expr_src = &raw[expr_start..j];
                if expr_src.trim().is_empty() {
                    return Err(Diagnostic::new(
                        "empty `${}` interpolation in string literal",
                        span,
                    ));
                }
                // Sub-lex и sub-parse выражение из ${...}.
                let tokens = crate::lexer::lex(expr_src).map_err(|e| {
                    Diagnostic::new(
                        format!("invalid expression in `${{...}}`: {}", e.message),
                        span,
                    )
                })?;
                let mut sub = Parser::with_src(tokens, expr_src.to_string());
                let inner = sub.parse_expr().map_err(|e| {
                    Diagnostic::new(
                        format!("invalid expression in `${{...}}`: {}", e.message),
                        span,
                    )
                })?;
                parts.push(InterpPart::Expr(inner));
                i = j + 1;
                continue;
            }
            // Обычный байт — берём целиком codepoint.
            let ch_len = parser_utf8_char_len(b);
            let end = (i + ch_len).min(bytes.len());
            cur_lit.push_str(&raw[i..end]);
            i = end;
        }
        if !cur_lit.is_empty() {
            parts.push(InterpPart::Lit(cur_lit));
        }
        // Если только Lit-части (или ничего) — обычный StrLit
        // (без interpolation).
        if parts.iter().all(|p| matches!(p, InterpPart::Lit(_))) {
            let s: String = parts
                .into_iter()
                .map(|p| match p {
                    InterpPart::Lit(s) => s,
                    _ => unreachable!(),
                })
                .collect();
            return Ok(Expr::new(ExprKind::StrLit(s), span));
        }
        // Конвертим InterpPart → InterpStrPart (для AST).
        let ast_parts: Vec<InterpStrPart> = parts
            .into_iter()
            .map(|p| match p {
                InterpPart::Lit(s) => InterpStrPart::Lit(s),
                InterpPart::Expr(e) => InterpStrPart::Expr(Box::new(e)),
            })
            .collect();
        Ok(Expr::new(
            ExprKind::InterpolatedStr { parts: ast_parts },
            span,
        ))
    }
}

enum InterpPart {
    Lit(String),
    Expr(Expr),
}

fn parser_utf8_char_len(first_byte: u8) -> usize {
    match first_byte {
        b if b < 0x80 => 1,
        b if b < 0xC0 => 1,
        b if b < 0xE0 => 2,
        b if b < 0xF0 => 3,
        _ => 4,
    }
}

enum StmtOrExpr {
    Stmt(Stmt),
    Expr(Expr),
}

/// Удобная обёртка — лексирует и парсит исходник.
/// `file_id = MAIN_FILE_ID` (backward compat).
pub fn parse(src: &str) -> Result<Module, Diagnostic> {
    parse_with_file_id(src, crate::diag::MAIN_FILE_ID)
}

/// Plan 42 Sub-plan 42.4 шаг 2: parse с explicit FileId.
/// Все Span'ы AST получат указанный file_id (через token spans от lexer).
pub fn parse_with_file_id(src: &str, file_id: crate::diag::FileId) -> Result<Module, Diagnostic> {
    let tokens = crate::lexer::lex_with_file_id(src, file_id)?;
    // Plan 45 Ф.2: doc-comment токены остаются в стриме; парсер
    // консумит их через `consume_doc_block_of_kind` (Outer — перед
    // item'ами; Inner — на уровне модуля). Interim shim из Ф.1
    // удалён.
    let mut p = Parser::with_src(tokens, src.to_string());
    p.parse_module()
}

/// Plan 45 Ф.24.6: parse a Nova type expression string → TypeRef AST.
/// Returns Err if the input is not a valid type expression.
pub fn parse_type_str(ty: &str) -> Result<crate::ast::TypeRef, crate::diag::Diagnostic> {
    let tokens = crate::lexer::lex(ty)?;
    let mut p = Parser::with_src(tokens, ty.to_string());
    p.parse_type()
}

#[cfg(test)]
mod doc_attach_tests {
    //! Plan 45 Ф.2: проверяем, что парсер прикрепляет doc-блоки к
    //! item'ам и к модулю. Тесты идут поверх существующего parser-pipeline
    //! (lex → parser); doc-token'ы попадают в стрим из lexer'а Ф.1.
    use super::*;
    use crate::ast::{Item, TypeDeclKind};
    use crate::lexer::DocCommentKind;

    fn parse_or_panic(src: &str) -> crate::ast::Module {
        super::parse(src).unwrap_or_else(|e| panic!("parse failed: {:?}", e))
    }

    #[test]
    fn outer_doc_attaches_to_fn() {
        let src = "\
module m

/// Returns the absolute value of `x`.
fn abs(x int) -> int => x
";
        let m = parse_or_panic(src);
        let fn_decl = m.items.iter().find_map(|it| match it {
            Item::Fn(f) => Some(f),
            _ => None,
        }).expect("fn must exist");
        let doc = fn_decl.doc.as_ref().expect("doc must be attached");
        assert_eq!(doc.kind, DocCommentKind::Outer);
        assert_eq!(doc.content, "Returns the absolute value of `x`.");
    }

    #[test]
    fn outer_doc_multi_line_attaches() {
        let src = "\
module m

/// Summary line.
///
/// Long description spanning
/// multiple lines.
fn foo() -> int => 1
";
        let m = parse_or_panic(src);
        let fn_decl = m.items.iter().find_map(|it| match it {
            Item::Fn(f) => Some(f),
            _ => None,
        }).expect("fn must exist");
        let doc = fn_decl.doc.as_ref().expect("doc must be attached");
        assert!(doc.content.starts_with("Summary line."));
        assert!(doc.content.contains("Long description"));
    }

    #[test]
    fn outer_doc_attaches_to_type() {
        let src = "\
module m

/// A point in 2D space.
type Point { x int; y int }
";
        let m = parse_or_panic(src);
        let ty = m.items.iter().find_map(|it| match it {
            Item::Type(t) => Some(t),
            _ => None,
        }).expect("type must exist");
        let doc = ty.doc.as_ref().expect("doc must be attached");
        assert_eq!(doc.content, "A point in 2D space.");
        assert!(matches!(ty.kind, TypeDeclKind::Record(_)));
    }

    #[test]
    fn outer_doc_attaches_to_const() {
        let src = "\
module m

/// Maximum buffer size in bytes.
const MAX_BUF int = 4096
";
        let m = parse_or_panic(src);
        let c = m.items.iter().find_map(|it| match it {
            Item::Const(c) => Some(c),
            _ => None,
        }).expect("const must exist");
        let doc = c.doc.as_ref().expect("doc must be attached");
        assert_eq!(doc.content, "Maximum buffer size in bytes.");
    }

    #[test]
    fn inner_doc_attaches_to_module() {
        let src = "\
//! This module provides examples for testing.

module m

fn foo() -> int => 1
";
        let m = parse_or_panic(src);
        let doc = m.doc.as_ref().expect("module doc must be attached");
        assert_eq!(doc.kind, DocCommentKind::Inner);
        assert_eq!(doc.content, "This module provides examples for testing.");
    }

    #[test]
    fn outer_doc_with_attrs_in_between() {
        // `///` then `#realtime` then `fn` — doc must attach to fn, not lost.
        let src = "\
module m

/// Realtime-safe abs.
#realtime
fn abs(x int) -> int => x
";
        let m = parse_or_panic(src);
        let fn_decl = m.items.iter().find_map(|it| match it {
            Item::Fn(f) => Some(f),
            _ => None,
        }).expect("fn must exist");
        let doc = fn_decl.doc.as_ref().expect("doc must be attached");
        assert_eq!(doc.content, "Realtime-safe abs.");
    }

    #[test]
    fn no_doc_means_none() {
        let src = "\
module m

fn no_doc_fn() -> int => 1
";
        let m = parse_or_panic(src);
        let fn_decl = m.items.iter().find_map(|it| match it {
            Item::Fn(f) => Some(f),
            _ => None,
        }).expect("fn must exist");
        assert!(fn_decl.doc.is_none());
        assert!(m.doc.is_none());
    }
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
    fn type_effect() {
        let m = parse_or_panic(
            r#"
            type Db effect {
                query(q Sql) -> int
                exec(q Sql) -> int
            }
            "#,
        );
        let Item::Type(t) = &m.items[0] else { panic!() };
        let TypeDeclKind::Effect(methods) = &t.kind else {
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

    // ─── Plan 19, C2: closure-light parsing ────────────────────────

    /// Helper: ищет верхнеуровневое closure-light выражение в первом
    /// `let`-биндинге модуля. Используется в тестах ниже.
    fn first_let_closure_light(m: &Module) -> (&Vec<crate::ast::ClosureLightParam>, &crate::ast::ClosureBody) {
        let Item::Let(l) = &m.items[0] else {
            panic!("expected first item to be `let`, got {:?}", m.items[0]);
        };
        let ExprKind::ClosureLight { params, body } = &l.value.kind else {
            panic!(
                "expected ClosureLight in let value, got {:?}",
                l.value.kind
            );
        };
        (params, body)
    }

    #[test]
    fn closure_light_one_param_expr() {
        let m = parse_or_panic("let inc = |x| x + 1\n");
        let (params, body) = first_let_closure_light(&m);
        assert_eq!(params.len(), 1);
        assert_eq!(params[0].name, "x");
        assert!(matches!(body, crate::ast::ClosureBody::Expr(_)));
    }

    #[test]
    fn closure_light_no_params() {
        let m = parse_or_panic("let zero = || 0\n");
        let (params, body) = first_let_closure_light(&m);
        assert!(params.is_empty());
        assert!(matches!(body, crate::ast::ClosureBody::Expr(_)));
    }

    #[test]
    fn closure_light_wildcard_param() {
        let m = parse_or_panic("let any = |_| 42\n");
        let (params, _body) = first_let_closure_light(&m);
        assert_eq!(params.len(), 1);
        assert_eq!(params[0].name, "_");
    }

    #[test]
    fn closure_light_multi_params() {
        let m = parse_or_panic("let add = |a, b| a + b\n");
        let (params, _body) = first_let_closure_light(&m);
        assert_eq!(params.len(), 2);
        assert_eq!(params[0].name, "a");
        assert_eq!(params[1].name, "b");
    }

    #[test]
    fn closure_light_block_body() {
        let m = parse_or_panic(
            r#"
            let f = |x| {
                let y = x * 2
                y + 1
            }
            "#,
        );
        let (params, body) = first_let_closure_light(&m);
        assert_eq!(params.len(), 1);
        assert_eq!(params[0].name, "x");
        assert!(matches!(body, crate::ast::ClosureBody::Block(_)));
    }

    #[test]
    fn closure_light_no_params_block_body() {
        let m = parse_or_panic(
            r#"
            let g = || {
                let x = 10
                x * x
            }
            "#,
        );
        let (params, body) = first_let_closure_light(&m);
        assert!(params.is_empty());
        assert!(matches!(body, crate::ast::ClosureBody::Block(_)));
    }

    #[test]
    fn closure_light_in_call_arg() {
        // Closure-light внутри args вызова — частый use-case (HOF).
        let m = parse_or_panic("let r = list.filter(|x| x > 0)\n");
        let Item::Let(l) = &m.items[0] else { panic!() };
        // r = ExprKind::Call { ... args: [Closure...] }
        let ExprKind::Call { args, .. } = &l.value.kind else {
            panic!("expected Call, got {:?}", l.value.kind);
        };
        assert_eq!(args.len(), 1);
        let CallArg::Item(arg_expr) = &args[0] else { panic!() };
        assert!(matches!(arg_expr.kind, ExprKind::ClosureLight { .. }));
    }

    #[test]
    fn closure_light_typed_param_rejected() {
        // |x int| — невалидно, типы только в closure-full.
        let result = parse("let bad = |x int| x + 1\n");
        assert!(result.is_err(), "typed param must be rejected in closure-light");
        let err = result.unwrap_err();
        assert!(
            err.message.contains("untyped") || err.message.contains("fn(x T)"),
            "error message should hint at closure-full, got: {}",
            err.message
        );
    }

    #[test]
    fn closure_light_arrow_in_body_rejected() {
        // |x| => expr — невалидно (D22-rev: closure-light не использует =>).
        let result = parse("let bad = |x| => x + 1\n");
        assert!(result.is_err(), "`|x| => expr` must be rejected");
        let err = result.unwrap_err();
        assert!(
            err.message.contains("=>") || err.message.contains("D22-rev") || err.message.contains("closure-light body"),
            "error message should explain `=>` is not used, got: {}",
            err.message
        );
    }

    #[test]
    fn closure_light_does_not_break_binary_or() {
        // `|` в infix-position — binary OR, не closure.
        // 5 | 2 — bitwise OR, должно дать значение 7 (но мы парсим, не вычисляем).
        let m = parse_or_panic("let r = 5 | 2\n");
        let Item::Let(l) = &m.items[0] else { panic!() };
        // Должен быть Binary, не ClosureLight.
        assert!(
            matches!(l.value.kind, ExprKind::Binary { .. }),
            "expected Binary OR, got {:?}",
            l.value.kind
        );
    }

    #[test]
    fn closure_light_does_not_break_logical_or() {
        // `||` в infix-position — logical OR, не no-arg closure.
        let m = parse_or_panic("let r = true || false\n");
        let Item::Let(l) = &m.items[0] else { panic!() };
        assert!(
            matches!(l.value.kind, ExprKind::Binary { .. }),
            "expected logical OR (Binary), got {:?}",
            l.value.kind
        );
    }

    // ─── Plan 19, C3: closure-full parsing ─────────────────────────

    /// Helper: достаёт closure-full из первого let'а.
    fn first_let_closure_full(m: &Module) -> &crate::ast::FnSigBody {
        let Item::Let(l) = &m.items[0] else {
            panic!("expected first item to be `let`, got {:?}", m.items[0]);
        };
        let ExprKind::ClosureFull(sb) = &l.value.kind else {
            panic!(
                "expected ClosureFull in let value, got {:?}",
                l.value.kind
            );
        };
        sb
    }

    #[test]
    fn closure_full_typed_expr_body() {
        let m = parse_or_panic("let f = fn(x int) -> int => x * 2\n");
        let sb = first_let_closure_full(&m);
        assert_eq!(sb.params.len(), 1);
        assert_eq!(sb.params[0].name, "x");
        assert!(sb.return_type.is_some());
        assert!(matches!(sb.body, FnBody::Expr(_)));
    }

    #[test]
    fn closure_full_typed_block_body() {
        let m = parse_or_panic(
            r#"
            let f = fn(x int, y int) -> int {
                let z = x + y
                z * 2
            }
            "#,
        );
        let sb = first_let_closure_full(&m);
        assert_eq!(sb.params.len(), 2);
        assert!(matches!(sb.body, FnBody::Block(_)));
    }

    #[test]
    fn closure_full_with_effects() {
        let m = parse_or_panic(
            "let mid = fn(req int) Db Log -> int => req + 1\n",
        );
        let sb = first_let_closure_full(&m);
        assert_eq!(sb.effects.len(), 2);
        assert!(sb.return_type.is_some());
    }

    #[test]
    fn closure_full_no_params() {
        let m = parse_or_panic("let pure = fn() -> int => 42\n");
        let sb = first_let_closure_full(&m);
        assert!(sb.params.is_empty());
        assert!(sb.return_type.is_some());
    }

    #[test]
    fn closure_full_no_return_type() {
        let m = parse_or_panic("let logger = fn(s str) Log { let x = s }\n");
        let sb = first_let_closure_full(&m);
        assert_eq!(sb.params.len(), 1);
        assert!(sb.return_type.is_none());
        assert_eq!(sb.effects.len(), 1);
    }

    #[test]
    fn closure_full_generics_rejected() {
        let result = parse("let f = fn[T](x T) -> T => x\n");
        assert!(result.is_err(), "generics on closure-full must be rejected in bootstrap");
        let err = result.unwrap_err();
        assert!(
            err.message.contains("generics") || err.message.contains("rank-2"),
            "error should mention generics/rank-2, got: {}",
            err.message
        );
    }

    #[test]
    fn closure_full_in_call_arg() {
        let m = parse_or_panic(
            "let r = list.map(fn(x int) -> int => x * 2)\n",
        );
        let Item::Let(l) = &m.items[0] else { panic!() };
        let ExprKind::Call { args, .. } = &l.value.kind else {
            panic!("expected Call, got {:?}", l.value.kind);
        };
        let CallArg::Item(arg_expr) = &args[0] else { panic!() };
        assert!(matches!(arg_expr.kind, ExprKind::ClosureFull(_)));
    }

    #[test]
    fn closure_full_does_not_break_named_fn() {
        // top-level `fn foo()` всё ещё parses нормально — это item, не expr.
        let m = parse_or_panic("fn foo(x int) -> int => x + 1\n");
        assert_eq!(m.items.len(), 1);
        let Item::Fn(f) = &m.items[0] else { panic!() };
        assert_eq!(f.name, "foo");
    }

    // ─── Plan 19, C10 (D88): default generic params ───────────────

    #[test]
    fn generic_default_simple() {
        let m = parse_or_panic(
            "fn run[T = int](a T) -> T => a\n"
        );
        let Item::Fn(f) = &m.items[0] else { panic!() };
        assert_eq!(f.generics.len(), 1);
        assert_eq!(f.generics[0].name, "T");
        assert!(f.generics[0].default.is_some());
    }

    #[test]
    fn generic_default_with_bound() {
        let m = parse_or_panic(
            "fn run[T Numeric = f64](a T) -> T => a\n"
        );
        let Item::Fn(f) = &m.items[0] else { panic!() };
        assert_eq!(f.generics.len(), 1);
        assert!(f.generics[0].bound.is_some());
        assert!(f.generics[0].default.is_some());
    }

    #[test]
    fn generic_default_must_be_after_required() {
        // [T = int, U] — error: required after default.
        let result = parse("fn bad[T = int, U](x T, y U) -> T => x\n");
        assert!(result.is_err(), "params with default must precede defaults");
        let err = result.unwrap_err();
        assert!(
            err.message.contains("default"),
            "error should mention default ordering, got: {}",
            err.message
        );
    }

    #[test]
    fn generic_default_on_type() {
        let m = parse_or_panic(
            "type Complex[T = f64] { re T, im T }\n"
        );
        let Item::Type(t) = &m.items[0] else { panic!() };
        assert_eq!(t.generics.len(), 1);
        assert!(t.generics[0].default.is_some());
    }

    // ─── Plan 19, C9 (D87): Handler[E, IRT] ──────────────────────

    #[test]
    fn handler_two_param_generic() {
        // `Handler[Logger, int]` — двухпараметрический generic
        // (E + IRT). Парсер trustы любое количество type-args через
        // обычный type-parsing path.
        let m = parse_or_panic(
            "fn make_h() -> Handler[Logger, int] => some_handler\n"
        );
        let Item::Fn(f) = &m.items[0] else { panic!() };
        let Some(TypeRef::Named { path, generics, .. }) = &f.return_type else {
            panic!("expected named return type, got {:?}", f.return_type);
        };
        assert_eq!(path, &vec!["Handler".to_string()]);
        assert_eq!(generics.len(), 2);
    }

    #[test]
    fn handler_single_param_default_irt() {
        // `Handler[E]` ≡ `Handler[E, Never]` через D88 default.
        // Парсится как одноаргументный generic (default подставляется
        // в monomorphization, а не на parse-стадии).
        let m = parse_or_panic(
            "fn make_h() -> Handler[Logger] => some_handler\n"
        );
        let Item::Fn(f) = &m.items[0] else { panic!() };
        let Some(TypeRef::Named { path, generics, .. }) = &f.return_type else {
            panic!()
        };
        assert_eq!(path, &vec!["Handler".to_string()]);
        assert_eq!(generics.len(), 1);
    }

    // ─── Plan 19, C4: trailing parsing ─────────────────────────────

    #[test]
    fn trailing_block_no_params() {
        // `f() { body }` — DSL-форма (D43-rev).
        let m = parse_or_panic(
            r#"
            fn dummy(x fn() -> int) -> int => x()
            let r = dummy() {
                42
            }
            "#,
        );
        // dummy — Item 0, let — Item 1.
        let Item::Let(l) = &m.items[1] else {
            panic!("expected let at items[1], got {:?}", m.items[1]);
        };
        let ExprKind::Call { trailing, .. } = &l.value.kind else { panic!() };
        let t = trailing.as_ref().unwrap();
        assert!(matches!(t, crate::ast::Trailing::Block(_)));
    }

    #[test]
    fn trailing_block_legacy_form_rejected() {
        // Plan 19 C13: `f() { x => body }` legacy форма удалена
        // (заменена на `f() fn(x int) -> ... body`). Парсер
        // её больше не принимает: внутри `{ ... }` `x =>` парсится
        // как match-arm в block, что даёт parse-error дальше.
        // (Точная форма ошибки зависит от того, как парсер
        // интерпретирует `x =>` в начале block-statement'а — нам
        // важно лишь что результат **не** Trailing::LegacyBlockWithParams.)
        let result = parse(
            r#"
            fn dummy(x fn(int) -> int) -> int => x(0)
            let r = dummy() { x => x + 1 }
            "#,
        );
        // Либо parse fail, либо parse OK но trailing — Block (без params)
        // с инородным `x => x + 1` внутри. В обоих случаях нет
        // LegacyBlockWithParams.
        if let Ok(m) = result {
            if let Item::Let(l) = &m.items[1] {
                if let ExprKind::Call { trailing, .. } = &l.value.kind {
                    if let Some(t) = trailing {
                        assert!(
                            !matches!(t, crate::ast::Trailing::LegacyBlockWithParams(_)),
                            "Plan 19 C13 should not produce LegacyBlockWithParams"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn trailing_fn_typed_expr_body() {
        // `f() fn(x) => x > 0` — D43-rev trailing-fn.
        let m = parse_or_panic(
            r#"
            fn dummy(x fn(int) -> bool) -> bool => x(1)
            let r = dummy() fn(x int) -> bool => x > 0
            "#,
        );
        let Item::Let(l) = &m.items[1] else { panic!() };
        let ExprKind::Call { trailing, .. } = &l.value.kind else { panic!() };
        let t = trailing.as_ref().unwrap();
        let crate::ast::Trailing::Fn(sb) = t else {
            panic!("expected Trailing::Fn, got {:?}", t);
        };
        assert_eq!(sb.params.len(), 1);
        assert!(matches!(sb.body, FnBody::Expr(_)));
    }

    #[test]
    fn trailing_fn_block_body() {
        let m = parse_or_panic(
            r#"
            fn dummy(x fn(int, int) -> int) -> int => x(1, 2)
            let r = dummy() fn(a int, b int) -> int {
                let s = a + b
                s * 2
            }
            "#,
        );
        let Item::Let(l) = &m.items[1] else { panic!() };
        let ExprKind::Call { trailing, .. } = &l.value.kind else { panic!() };
        let t = trailing.as_ref().unwrap();
        let crate::ast::Trailing::Fn(sb) = t else { panic!() };
        assert_eq!(sb.params.len(), 2);
        assert!(matches!(sb.body, FnBody::Block(_)));
    }

    #[test]
    fn trailing_fn_with_effects() {
        let m = parse_or_panic(
            r#"
            fn dummy(x fn(int) Db -> int) Db -> int => x(1)
            let r = dummy() fn(n int) Db -> int => n
            "#,
        );
        let Item::Let(l) = &m.items[1] else { panic!() };
        let ExprKind::Call { trailing, .. } = &l.value.kind else { panic!() };
        let t = trailing.as_ref().unwrap();
        let crate::ast::Trailing::Fn(sb) = t else { panic!() };
        assert_eq!(sb.effects.len(), 1);
    }
}
