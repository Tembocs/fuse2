use crate::ast::nodes::*;
use crate::error::Diagnostic;
use crate::lexer::{lex, Token, TokenKind};

pub fn parse_source(source: &str, filename: &str) -> Result<Program, Diagnostic> {
    let tokens = lex(source, filename)?;
    Parser::new(tokens, filename).parse()
}

struct Parser {
    tokens: Vec<Token>,
    filename: String,
    index: usize,
}

impl Parser {
    fn new(tokens: Vec<Token>, filename: &str) -> Self {
        Self {
            tokens,
            filename: filename.to_string(),
            index: 0,
        }
    }

    fn parse(mut self) -> Result<Program, Diagnostic> {
        let mut declarations = Vec::new();
        while self.peek(0).kind != TokenKind::Eof {
            declarations.push(self.parse_top_level()?);
        }
        Ok(Program {
            declarations,
            filename: self.filename,
        })
    }

    fn peek(&self, offset: usize) -> &Token {
        self.tokens
            .get(self.index + offset)
            .unwrap_or_else(|| self.tokens.last().expect("parser needs EOF token"))
    }

    fn take(&mut self) -> Token {
        let token = self.peek(0).clone();
        if self.index < self.tokens.len() {
            self.index += 1;
        }
        token
    }

    fn match_kind(&mut self, kind: TokenKind) -> Option<Token> {
        if self.peek(0).kind == kind {
            Some(self.take())
        } else {
            None
        }
    }

    fn expect(&mut self, kind: TokenKind, message: &str) -> Result<Token, Diagnostic> {
        let token = self.peek(0).clone();
        if token.kind != kind {
            return Err(self.syntax_error(message, token.span));
        }
        Ok(self.take())
    }

    fn syntax_error(&self, message: impl Into<String>, span: crate::error::Span) -> Diagnostic {
        Diagnostic::error(&message.into(), &self.filename, span, None)
    }

    fn parse_top_level(&mut self) -> Result<Declaration, Diagnostic> {
        let mut decorators = Vec::new();
        while self.match_kind(TokenKind::At).is_some() {
            decorators.push(
                self.expect(TokenKind::Identifier, "expected decorator name")?
                    .text,
            );
        }
        let is_pub = self.match_kind(TokenKind::Pub).is_some();
        match self.peek(0).kind {
            TokenKind::Import => Ok(Declaration::Import(self.parse_import()?)),
            TokenKind::Fn | TokenKind::Async | TokenKind::Suspend => {
                let mut function = self.parse_function()?;
                function.decorators = decorators;
                function.is_pub = is_pub;
                Ok(Declaration::Function(function))
            }
            TokenKind::Data => {
                let mut data_class = self.parse_data_class()?;
                data_class.decorators = decorators;
                data_class.is_pub = is_pub;
                Ok(Declaration::DataClass(data_class))
            }
            TokenKind::Enum => {
                let mut enum_decl = self.parse_enum()?;
                enum_decl.is_pub = is_pub;
                Ok(Declaration::Enum(enum_decl))
            }
            TokenKind::Extern => {
                let mut extern_fn = self.parse_extern_fn()?;
                extern_fn.is_pub = is_pub;
                Ok(Declaration::ExternFn(extern_fn))
            }
            _ => Err(self.syntax_error(
                format!("unexpected top-level token {}", self.peek(0).text),
                self.peek(0).span,
            )),
        }
    }

    fn parse_import(&mut self) -> Result<ImportDecl, Diagnostic> {
        let start = self.expect(TokenKind::Import, "expected `import`")?;
        let mut parts = vec![self
            .expect(TokenKind::Identifier, "expected import path segment")?
            .text];
        while self.peek(0).kind == TokenKind::Dot && self.peek(1).kind == TokenKind::Identifier {
            self.take();
            parts.push(
                self.expect(TokenKind::Identifier, "expected import path segment")?
                    .text,
            );
        }
        let mut items = None;
        if self.peek(0).kind == TokenKind::Dot && self.peek(1).kind == TokenKind::LBrace {
            self.take();
            self.expect(TokenKind::LBrace, "expected `{` after import path")?;
            let mut imported = vec![self
                .expect(TokenKind::Identifier, "expected import item")?
                .text];
            while self.match_kind(TokenKind::Comma).is_some() {
                imported.push(
                    self.expect(TokenKind::Identifier, "expected import item")?
                        .text,
                );
            }
            self.expect(TokenKind::RBrace, "expected `}` after import items")?;
            items = Some(imported);
        }
        Ok(ImportDecl {
            module_path: parts.join("."),
            items,
            span: start.span,
        })
    }

    fn parse_function(&mut self) -> Result<FunctionDecl, Diagnostic> {
        let mut is_async = false;
        let mut is_suspend = false;
        while matches!(self.peek(0).kind, TokenKind::Async | TokenKind::Suspend) {
            match self.take().kind {
                TokenKind::Async => is_async = true,
                TokenKind::Suspend => is_suspend = true,
                _ => unreachable!(),
            }
        }
        let start = self.expect(TokenKind::Fn, "expected `fn`")?;
        let first =
            self.expect(TokenKind::Identifier, "expected function name or receiver type")?;
        let mut receiver_type = None;
        let mut name = first.text.clone();
        if self.match_kind(TokenKind::Dot).is_some() {
            receiver_type = Some(first.text);
            name = self
                .expect(TokenKind::Identifier, "expected extension function name")?
                .text;
        }
        self.expect(TokenKind::LParen, "expected `(` after function name")?;
        let mut params = Vec::new();
        if self.peek(0).kind != TokenKind::RParen {
            loop {
                params.push(self.parse_param()?);
                if self.match_kind(TokenKind::Comma).is_none() {
                    break;
                }
            }
        }
        self.expect(TokenKind::RParen, "expected `)` after parameters")?;
        let mut return_type = None;
        if self.match_kind(TokenKind::Arrow).is_some() {
            return_type = Some(self.parse_type_name(&[TokenKind::LBrace, TokenKind::FatArrow]));
        }
        let body = if self.match_kind(TokenKind::FatArrow).is_some() {
            let expr = self.parse_expression()?;
            Block {
                statements: vec![Statement::Expr(ExprStmt {
                    expr: expr.clone(),
                    span: expr.span(),
                })],
                span: start.span,
            }
        } else {
            self.parse_block()?
        };
        Ok(FunctionDecl {
            name,
            params,
            return_type,
            body,
            is_pub: false,
            decorators: Vec::new(),
            is_async,
            is_suspend,
            receiver_type,
            span: start.span,
        })
    }

    fn parse_param(&mut self) -> Result<Param, Diagnostic> {
        let start = self.peek(0).span;
        let convention = match self.peek(0).kind {
            TokenKind::Ref => Some("ref".to_string()),
            TokenKind::MutRef => Some("mutref".to_string()),
            TokenKind::Owned => Some("owned".to_string()),
            _ => None,
        };
        if convention.is_some() {
            self.take();
        }
        let name = self
            .expect(TokenKind::Identifier, "expected parameter name")?
            .text;
        let type_name = if self.match_kind(TokenKind::Colon).is_some() {
            Some(self.parse_type_name(&[TokenKind::Comma, TokenKind::RParen]))
        } else {
            None
        };
        Ok(Param {
            convention,
            name,
            type_name,
            span: start,
        })
    }

    fn parse_type_name(&mut self, stop: &[TokenKind]) -> String {
        let mut out = String::new();
        let mut depth = 0_i32;
        loop {
            let token = self.peek(0);
            if token.kind == TokenKind::Eof {
                break;
            }
            if depth == 0 && stop.contains(&token.kind) {
                break;
            }
            match token.kind {
                TokenKind::Lt | TokenKind::LParen | TokenKind::LBracket => depth += 1,
                TokenKind::Gt | TokenKind::RParen | TokenKind::RBracket => depth -= 1,
                _ => {}
            }
            out.push_str(&token.text);
            self.take();
        }
        out.trim().to_string()
    }

    fn parse_data_class(&mut self) -> Result<DataClassDecl, Diagnostic> {
        let start = self.expect(TokenKind::Data, "expected `data`")?;
        self.expect(TokenKind::Class, "expected `class` after `data`")?;
        let name = self
            .expect(TokenKind::Identifier, "expected data class name")?
            .text;
        self.expect(TokenKind::LParen, "expected `(` after data class name")?;
        let mut fields = Vec::new();
        if self.peek(0).kind != TokenKind::RParen {
            loop {
                let field_token = self.take();
                let mutable = match field_token.kind {
                    TokenKind::Val => false,
                    TokenKind::Var => true,
                    _ => return Err(self.syntax_error("expected `val` or `var` field", field_token.span)),
                };
                let field_name = self
                    .expect(TokenKind::Identifier, "expected field name")?
                    .text;
                let type_name = if self.match_kind(TokenKind::Colon).is_some() {
                    Some(self.parse_type_name(&[TokenKind::Comma, TokenKind::RParen]))
                } else {
                    None
                };
                fields.push(FieldDecl {
                    mutable,
                    name: field_name,
                    type_name,
                    span: field_token.span,
                });
                if self.match_kind(TokenKind::Comma).is_none() {
                    break;
                }
            }
        }
        self.expect(TokenKind::RParen, "expected `)` after fields")?;
        let mut methods = Vec::new();
        if self.match_kind(TokenKind::LBrace).is_some() {
            while self.peek(0).kind != TokenKind::RBrace {
                let mut decorators = Vec::new();
                while self.match_kind(TokenKind::At).is_some() {
                    decorators.push(
                        self.expect(TokenKind::Identifier, "expected decorator name")?
                            .text,
                    );
                }
                let is_pub = self.match_kind(TokenKind::Pub).is_some();
                let mut function = self.parse_function()?;
                function.decorators = decorators;
                function.is_pub = is_pub;
                methods.push(function);
            }
            self.expect(TokenKind::RBrace, "expected `}` after data class body")?;
        }
        Ok(DataClassDecl {
            name,
            fields,
            methods,
            is_pub: false,
            decorators: Vec::new(),
            span: start.span,
        })
    }

    fn parse_enum(&mut self) -> Result<EnumDecl, Diagnostic> {
        let start = self.expect(TokenKind::Enum, "expected `enum`")?;
        let name = self.expect(TokenKind::Identifier, "expected enum name")?.text;
        self.expect(TokenKind::LBrace, "expected `{` after enum name")?;
        let mut variants = Vec::new();
        while self.peek(0).kind != TokenKind::RBrace {
            let variant = self.expect(TokenKind::Identifier, "expected variant name")?;
            let mut arity = 0;
            if self.match_kind(TokenKind::LParen).is_some() {
                if self.peek(0).kind != TokenKind::RParen {
                    loop {
                        self.parse_type_name(&[TokenKind::Comma, TokenKind::RParen]);
                        arity += 1;
                        if self.match_kind(TokenKind::Comma).is_none() {
                            break;
                        }
                    }
                }
                self.expect(TokenKind::RParen, "expected `)` after enum payload")?;
            }
            variants.push(EnumVariant {
                name: variant.text,
                arity,
                span: variant.span,
            });
            self.match_kind(TokenKind::Comma);
        }
        self.expect(TokenKind::RBrace, "expected `}` after enum")?;
        Ok(EnumDecl {
            name,
            variants,
            is_pub: false,
            span: start.span,
        })
    }

    fn parse_extern_fn(&mut self) -> Result<ExternFnDecl, Diagnostic> {
        let start = self.expect(TokenKind::Extern, "expected `extern`")?;
        self.expect(TokenKind::Fn, "expected `fn` after `extern`")?;
        let name = self
            .expect(TokenKind::Identifier, "expected extern function name")?
            .text;
        self.expect(TokenKind::LParen, "expected `(` after extern function name")?;
        let mut params = Vec::new();
        if self.peek(0).kind != TokenKind::RParen {
            loop {
                params.push(self.parse_param()?);
                if self.match_kind(TokenKind::Comma).is_none() {
                    break;
                }
            }
        }
        self.expect(TokenKind::RParen, "expected `)` after parameters")?;
        let mut return_type = None;
        if self.match_kind(TokenKind::Arrow).is_some() {
            return_type = Some(self.parse_type_name(&[TokenKind::LBrace, TokenKind::FatArrow, TokenKind::Fn, TokenKind::Extern, TokenKind::Pub, TokenKind::Data, TokenKind::Enum, TokenKind::Import, TokenKind::At, TokenKind::Eof]));
        }
        Ok(ExternFnDecl {
            name,
            params,
            return_type,
            is_pub: false,
            span: start.span,
        })
    }

    fn parse_block(&mut self) -> Result<Block, Diagnostic> {
        let start = self.expect(TokenKind::LBrace, "expected `{` to start block")?;
        let mut statements = Vec::new();
        while self.peek(0).kind != TokenKind::RBrace {
            statements.push(self.parse_statement()?);
            self.match_kind(TokenKind::Semi);
        }
        self.expect(TokenKind::RBrace, "expected `}` to close block")?;
        Ok(Block {
            statements,
            span: start.span,
        })
    }

    fn parse_statement(&mut self) -> Result<Statement, Diagnostic> {
        let rank = if self.peek(0).kind == TokenKind::At {
            Some(self.parse_rank_decorator()?)
        } else {
            None
        };
        let token = self.peek(0).clone();
        match token.kind {
            TokenKind::Val | TokenKind::Var => Ok(Statement::VarDecl(self.parse_var_decl(rank)?)),
            TokenKind::Return => {
                self.take();
                let value = if matches!(self.peek(0).kind, TokenKind::RBrace | TokenKind::Semi) {
                    None
                } else {
                    Some(self.parse_expression()?)
                };
                Ok(Statement::Return(ReturnStmt { value, span: token.span }))
            }
            TokenKind::Break => {
                self.take();
                Ok(Statement::Break(token.span))
            }
            TokenKind::Continue => {
                self.take();
                Ok(Statement::Continue(token.span))
            }
            TokenKind::Spawn => Ok(Statement::Spawn(self.parse_spawn()?)),
            TokenKind::While => Ok(Statement::While(self.parse_while()?)),
            TokenKind::For => Ok(Statement::For(self.parse_for()?)),
            TokenKind::Loop => Ok(Statement::Loop(self.parse_loop()?)),
            TokenKind::Defer => {
                self.take();
                let expr = self.parse_expression()?;
                Ok(Statement::Defer(DeferStmt { expr, span: token.span }))
            }
            _ => {
                let expr = self.parse_expression()?;
                if self.match_kind(TokenKind::Eq).is_some() {
                    let value = self.parse_expression()?;
                    Ok(Statement::Assign(Assign { target: expr, value, span: token.span }))
                } else {
                    Ok(Statement::Expr(ExprStmt { expr, span: token.span }))
                }
            }
        }
    }

    fn parse_rank_decorator(&mut self) -> Result<i64, Diagnostic> {
        self.expect(TokenKind::At, "expected `@`")?;
        let name = self.expect(TokenKind::Identifier, "expected decorator name")?;
        if name.text != "rank" {
            return Err(self.syntax_error("only `@rank(...)` is supported on statements", name.span));
        }
        self.expect(TokenKind::LParen, "expected `(` after `@rank`")?;
        let value = self.expect(TokenKind::Int, "expected rank integer")?;
        self.expect(TokenKind::RParen, "expected `)` after rank value")?;
        Ok(value.text.parse().unwrap_or_default())
    }

    fn parse_var_decl(&mut self, rank: Option<i64>) -> Result<VarDecl, Diagnostic> {
        let start = self.take();
        let name = self.expect(TokenKind::Identifier, "expected binding name")?.text;
        let type_name = if self.match_kind(TokenKind::Colon).is_some() {
            Some(self.parse_type_name(&[TokenKind::Eq]))
        } else {
            None
        };
        self.expect(TokenKind::Eq, "expected `=` in binding")?;
        let value = self.parse_expression()?;
        Ok(VarDecl {
            rank,
            mutable: start.kind == TokenKind::Var,
            name,
            type_name,
            value,
            span: start.span,
        })
    }

    fn parse_while(&mut self) -> Result<WhileStmt, Diagnostic> {
        let start = self.expect(TokenKind::While, "expected `while`")?;
        let condition = self.parse_expression()?;
        let body = self.parse_block()?;
        Ok(WhileStmt { condition, body, span: start.span })
    }

    fn parse_for(&mut self) -> Result<ForStmt, Diagnostic> {
        let start = self.expect(TokenKind::For, "expected `for`")?;
        let name = self.expect(TokenKind::Identifier, "expected loop variable")?.text;
        self.expect(TokenKind::In, "expected `in` in for loop")?;
        let iterable = self.parse_expression()?;
        let body = self.parse_block()?;
        Ok(ForStmt { name, iterable, body, span: start.span })
    }

    fn parse_loop(&mut self) -> Result<LoopStmt, Diagnostic> {
        let start = self.expect(TokenKind::Loop, "expected `loop`")?;
        Ok(LoopStmt { body: self.parse_block()?, span: start.span })
    }

    fn parse_spawn(&mut self) -> Result<SpawnStmt, Diagnostic> {
        let start = self.expect(TokenKind::Spawn, "expected `spawn`")?;
        let is_async = self.match_kind(TokenKind::Async).is_some();
        Ok(SpawnStmt {
            body: self.parse_block()?,
            is_async,
            span: start.span,
        })
    }

    fn parse_expression(&mut self) -> Result<Expr, Diagnostic> { self.parse_elvis() }

    fn parse_elvis(&mut self) -> Result<Expr, Diagnostic> {
        let mut expr = self.parse_or()?;
        while let Some(op) = self.match_kind(TokenKind::Elvis) {
            let right = self.parse_or()?;
            expr = Expr::Binary(BinaryOp { left: Box::new(expr), op: "?:".to_string(), right: Box::new(right), span: op.span });
        }
        Ok(expr)
    }

    fn parse_or(&mut self) -> Result<Expr, Diagnostic> {
        let mut expr = self.parse_and()?;
        while let Some(op) = self.match_kind(TokenKind::Or) {
            let right = self.parse_and()?;
            expr = Expr::Binary(BinaryOp { left: Box::new(expr), op: "or".to_string(), right: Box::new(right), span: op.span });
        }
        Ok(expr)
    }

    fn parse_and(&mut self) -> Result<Expr, Diagnostic> {
        let mut expr = self.parse_equality()?;
        while let Some(op) = self.match_kind(TokenKind::And) {
            let right = self.parse_equality()?;
            expr = Expr::Binary(BinaryOp { left: Box::new(expr), op: "and".to_string(), right: Box::new(right), span: op.span });
        }
        Ok(expr)
    }

    fn parse_equality(&mut self) -> Result<Expr, Diagnostic> {
        let mut expr = self.parse_compare()?;
        while matches!(self.peek(0).kind, TokenKind::EqEq | TokenKind::Ne) {
            let op = self.take();
            let right = self.parse_compare()?;
            expr = Expr::Binary(BinaryOp { left: Box::new(expr), op: op.text, right: Box::new(right), span: op.span });
        }
        Ok(expr)
    }

    fn parse_compare(&mut self) -> Result<Expr, Diagnostic> {
        let mut expr = self.parse_term()?;
        while matches!(self.peek(0).kind, TokenKind::Lt | TokenKind::Gt | TokenKind::Le | TokenKind::Ge) {
            let op = self.take();
            let right = self.parse_term()?;
            expr = Expr::Binary(BinaryOp { left: Box::new(expr), op: op.text, right: Box::new(right), span: op.span });
        }
        Ok(expr)
    }

    fn parse_term(&mut self) -> Result<Expr, Diagnostic> {
        let mut expr = self.parse_factor()?;
        while matches!(self.peek(0).kind, TokenKind::Plus | TokenKind::Minus) {
            let op = self.take();
            let right = self.parse_factor()?;
            expr = Expr::Binary(BinaryOp { left: Box::new(expr), op: op.text, right: Box::new(right), span: op.span });
        }
        Ok(expr)
    }

    fn parse_factor(&mut self) -> Result<Expr, Diagnostic> {
        let mut expr = self.parse_unary()?;
        while matches!(self.peek(0).kind, TokenKind::Star | TokenKind::Slash | TokenKind::Percent) {
            let op = self.take();
            let right = self.parse_unary()?;
            expr = Expr::Binary(BinaryOp { left: Box::new(expr), op: op.text, right: Box::new(right), span: op.span });
        }
        Ok(expr)
    }

    fn parse_unary(&mut self) -> Result<Expr, Diagnostic> {
        let token = self.peek(0).clone();
        match token.kind {
            TokenKind::Minus | TokenKind::Not => {
                self.take();
                Ok(Expr::Unary(UnaryOp { op: token.text, value: Box::new(self.parse_unary()?), span: token.span }))
            }
            TokenKind::Move => {
                self.take();
                Ok(Expr::Move(MoveExpr { value: Box::new(self.parse_unary()?), span: token.span }))
            }
            TokenKind::Ref => {
                self.take();
                Ok(Expr::Ref(RefExpr { value: Box::new(self.parse_unary()?), span: token.span }))
            }
            TokenKind::MutRef => {
                self.take();
                Ok(Expr::MutRef(MutRefExpr { value: Box::new(self.parse_unary()?), span: token.span }))
            }
            TokenKind::Await => {
                self.take();
                Ok(Expr::Await(AwaitExpr { value: Box::new(self.parse_unary()?), span: token.span }))
            }
            _ => self.parse_postfix(),
        }
    }

    fn parse_postfix(&mut self) -> Result<Expr, Diagnostic> {
        let mut expr = self.parse_primary()?;
        loop {
            if self.match_kind(TokenKind::LParen).is_some() {
                let mut args = Vec::new();
                if self.peek(0).kind != TokenKind::RParen {
                    loop {
                        args.push(self.parse_expression()?);
                        if self.match_kind(TokenKind::Comma).is_none() { break; }
                    }
                }
                let close = self.expect(TokenKind::RParen, "expected `)` after arguments")?;
                expr = Expr::Call(Call { callee: Box::new(expr), args, span: close.span });
                continue;
            }
            if self.match_kind(TokenKind::Dot).is_some() {
                let name =
                    self.expect(TokenKind::Identifier, "expected member name after `.`")?;
                expr = Expr::Member(Member { object: Box::new(expr), name: name.text, optional: false, span: name.span });
                continue;
            }
            if self.match_kind(TokenKind::QDot).is_some() {
                let name =
                    self.expect(TokenKind::Identifier, "expected member name after `?.`")?;
                expr = Expr::Member(Member { object: Box::new(expr), name: name.text, optional: true, span: name.span });
                continue;
            }
            if let Some(token) = self.match_kind(TokenKind::Question) {
                expr = Expr::Question(QuestionExpr { value: Box::new(expr), span: token.span });
                continue;
            }
            break;
        }
        Ok(expr)
    }

    fn parse_primary(&mut self) -> Result<Expr, Diagnostic> {
        let token = self.peek(0).clone();
        match token.kind {
            TokenKind::Int => {
                self.take();
                Ok(Expr::Literal(Literal { value: LiteralValue::Int(token.text.parse().unwrap_or_default()), span: token.span }))
            }
            TokenKind::Float => {
                self.take();
                Ok(Expr::Literal(Literal { value: LiteralValue::Float(token.text.parse().unwrap_or_default()), span: token.span }))
            }
            TokenKind::String => {
                self.take();
                Ok(Expr::Literal(Literal { value: LiteralValue::String(token.text), span: token.span }))
            }
            TokenKind::FString => {
                self.take();
                Ok(Expr::FString(FString { template: token.text, span: token.span }))
            }
            TokenKind::True => {
                self.take();
                Ok(Expr::Literal(Literal { value: LiteralValue::Bool(true), span: token.span }))
            }
            TokenKind::False => {
                self.take();
                Ok(Expr::Literal(Literal { value: LiteralValue::Bool(false), span: token.span }))
            }
            TokenKind::Identifier => {
                let mut value = self.take().text;
                if self.peek(0).kind == TokenKind::ColonColon {
                    while matches!(
                        self.peek(0).kind,
                        TokenKind::ColonColon
                            | TokenKind::Lt
                            | TokenKind::Gt
                            | TokenKind::Comma
                            | TokenKind::Identifier
                            | TokenKind::Int
                    ) {
                        value.push_str(&self.take().text);
                    }
                }
                Ok(Expr::Name(Name { value, span: token.span }))
            }
            TokenKind::LParen => {
                self.take();
                let expr = self.parse_expression()?;
                self.expect(TokenKind::RParen, "expected `)`")?;
                Ok(expr)
            }
            TokenKind::LBracket => {
                self.take();
                let mut items = Vec::new();
                if self.peek(0).kind != TokenKind::RBracket {
                    loop {
                        items.push(self.parse_expression()?);
                        if self.match_kind(TokenKind::Comma).is_none() { break; }
                    }
                }
                self.expect(TokenKind::RBracket, "expected `]`")?;
                Ok(Expr::List(ListExpr { items, span: token.span }))
            }
            TokenKind::If => Ok(Expr::If(self.parse_if_expr()?)),
            TokenKind::Match => Ok(Expr::Match(self.parse_match_expr()?)),
            TokenKind::When => Ok(Expr::When(self.parse_when_expr()?)),
            TokenKind::Fn => Ok(Expr::Lambda(self.parse_lambda()?)),
            _ => Err(self.syntax_error(format!("unexpected token {}", token.text), token.span)),
        }
    }

    fn parse_if_expr(&mut self) -> Result<IfExpr, Diagnostic> {
        let start = self.expect(TokenKind::If, "expected `if`")?;
        let condition = self.parse_expression()?;
        let then_branch = self.parse_block()?;
        let else_branch = if self.match_kind(TokenKind::Else).is_some() {
            Some(if self.peek(0).kind == TokenKind::If {
                ElseBranch::IfExpr(Box::new(self.parse_if_expr()?))
            } else {
                ElseBranch::Block(self.parse_block()?)
            })
        } else { None };
        Ok(IfExpr { condition: Box::new(condition), then_branch, else_branch, span: start.span })
    }

    fn parse_match_expr(&mut self) -> Result<MatchExpr, Diagnostic> {
        let start = self.expect(TokenKind::Match, "expected `match`")?;
        let subject = self.parse_expression()?;
        self.expect(TokenKind::LBrace, "expected `{` after match subject")?;
        let mut arms = Vec::new();
        while self.peek(0).kind != TokenKind::RBrace {
            let pattern = self.parse_pattern()?;
            let arrow = self.expect(TokenKind::FatArrow, "expected `=>` in match arm")?;
            let body = if self.peek(0).kind == TokenKind::LBrace {
                ArmBody::Block(self.parse_block()?)
            } else {
                ArmBody::Expr(self.parse_expression()?)
            };
            arms.push(MatchArm { pattern, body, span: arrow.span });
            self.match_kind(TokenKind::Comma);
        }
        self.expect(TokenKind::RBrace, "expected `}` after match arms")?;
        Ok(MatchExpr { subject: Box::new(subject), arms, span: start.span })
    }

    fn parse_when_expr(&mut self) -> Result<WhenExpr, Diagnostic> {
        let start = self.expect(TokenKind::When, "expected `when`")?;
        self.expect(TokenKind::LBrace, "expected `{` after `when`")?;
        let mut arms = Vec::new();
        while self.peek(0).kind != TokenKind::RBrace {
            let condition = if self.match_kind(TokenKind::Else).is_some() { None } else { Some(self.parse_expression()?) };
            self.expect(TokenKind::FatArrow, "expected `=>` in when arm")?;
            let body = if self.peek(0).kind == TokenKind::LBrace {
                ArmBody::Block(self.parse_block()?)
            } else {
                ArmBody::Expr(self.parse_expression()?)
            };
            arms.push(WhenArm { condition, body, span: start.span });
            self.match_kind(TokenKind::Comma);
        }
        self.expect(TokenKind::RBrace, "expected `}` after when arms")?;
        Ok(WhenExpr { arms, span: start.span })
    }

    fn parse_lambda(&mut self) -> Result<LambdaExpr, Diagnostic> {
        let start = self.expect(TokenKind::Fn, "expected `fn`")?;
        self.expect(TokenKind::LParen, "expected `(` after `fn` in lambda")?;
        let mut params = Vec::new();
        if self.peek(0).kind != TokenKind::RParen {
            loop {
                params.push(self.parse_param()?);
                if self.match_kind(TokenKind::Comma).is_none() {
                    break;
                }
            }
        }
        self.expect(TokenKind::RParen, "expected `)` after lambda parameters")?;
        let mut return_type = None;
        if self.match_kind(TokenKind::Arrow).is_some() {
            return_type = Some(self.parse_type_name(&[TokenKind::LBrace, TokenKind::FatArrow]));
        }
        let body = if self.match_kind(TokenKind::FatArrow).is_some() {
            let expr = self.parse_expression()?;
            Block {
                statements: vec![Statement::Expr(ExprStmt {
                    expr: expr.clone(),
                    span: expr.span(),
                })],
                span: start.span,
            }
        } else {
            self.parse_block()?
        };
        Ok(LambdaExpr {
            params,
            return_type,
            body,
            span: start.span,
        })
    }

    fn parse_pattern(&mut self) -> Result<Pattern, Diagnostic> {
        let token = self.peek(0).clone();
        if token.kind == TokenKind::Identifier && token.text == "_" {
            self.take();
            return Ok(Pattern::Wildcard(token.span));
        }
        match token.kind {
            TokenKind::Int | TokenKind::String | TokenKind::True | TokenKind::False => {
                let expr = self.parse_primary()?;
                let value = match expr {
                    Expr::Literal(node) => node.value,
                    Expr::FString(node) => LiteralValue::String(node.template),
                    _ => unreachable!(),
                };
                Ok(Pattern::Literal(LiteralPattern { value, span: token.span }))
            }
            TokenKind::Identifier => {
                let mut name = self.take().text;
                if self.match_kind(TokenKind::Dot).is_some() {
                    name.push('.');
                    name.push_str(
                        &self
                            .expect(
                                TokenKind::Identifier,
                                "expected variant segment after `.`",
                            )?
                            .text,
                    );
                }
                if self.match_kind(TokenKind::LParen).is_some() {
                    let mut args = Vec::new();
                    if self.peek(0).kind != TokenKind::RParen {
                        loop {
                            args.push(self.parse_pattern()?);
                            if self.match_kind(TokenKind::Comma).is_none() { break; }
                        }
                    }
                    self.expect(TokenKind::RParen, "expected `)` after pattern args")?;
                    return Ok(Pattern::Variant(VariantPattern { name, args, span: token.span }));
                }
                if name.chars().next().is_some_and(|ch| ch.is_uppercase()) {
                    return Ok(Pattern::Variant(VariantPattern { name, args: Vec::new(), span: token.span }));
                }
                Ok(Pattern::Name(NamePattern { name, span: token.span }))
            }
            _ => Err(self.syntax_error("unsupported match pattern", token.span)),
        }
    }
}

