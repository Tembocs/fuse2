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
        let mut annotations = Vec::new();
        while self.match_kind(TokenKind::At).is_some() {
            annotations.push(self.parse_annotation()?);
        }
        let is_pub = self.match_kind(TokenKind::Pub).is_some();
        match self.peek(0).kind {
            TokenKind::Import => Ok(Declaration::Import(self.parse_import()?)),
            TokenKind::Fn => {
                let mut function = self.parse_function()?;
                function.annotations = annotations;
                function.is_pub = is_pub;
                Ok(Declaration::Function(function))
            }
            TokenKind::Data => {
                let mut data_class = self.parse_data_class()?;
                data_class.annotations = annotations;
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
            TokenKind::Struct => {
                let mut struct_decl = self.parse_struct()?;
                struct_decl.annotations = annotations;
                struct_decl.is_pub = is_pub;
                Ok(Declaration::Struct(struct_decl))
            }
            TokenKind::Interface => {
                let mut iface = self.parse_interface()?;
                iface.is_pub = is_pub;
                Ok(Declaration::Interface(iface))
            }
            TokenKind::Val => {
                let mut const_decl = self.parse_const()?;
                const_decl.is_pub = is_pub;
                Ok(Declaration::Const(const_decl))
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
        let start = self.expect(TokenKind::Fn, "expected `fn`")?;
        let first =
            self.expect(TokenKind::Identifier, "expected function name or receiver type")?;
        let mut receiver_type = None;
        let mut name = first.text.clone();
        if self.match_kind(TokenKind::Dot).is_some() {
            receiver_type = Some(first.text);
            let method_token = self.take();
            if method_token.text.is_empty() || method_token.kind == TokenKind::Eof {
                return Err(self.syntax_error("expected extension function name", method_token.span));
            }
            name = method_token.text;
        }
        let mut type_params = Vec::new();
        if self.match_kind(TokenKind::Lt).is_some() {
            loop {
                let mut tp = self
                    .expect(TokenKind::Identifier, "expected type parameter name")?
                    .text;
                if self.match_kind(TokenKind::Colon).is_some() {
                    tp.push_str(": ");
                    tp.push_str(
                        &self
                            .expect(TokenKind::Identifier, "expected interface bound")?
                            .text,
                    );
                }
                type_params.push(tp);
                if self.match_kind(TokenKind::Comma).is_none() {
                    break;
                }
            }
            self.expect(TokenKind::Gt, "expected `>` after type parameters")?;
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
            type_params,
            params,
            return_type,
            body,
            is_pub: false,
            annotations: Vec::new(),
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
        let mut variadic = false;
        let type_name = if self.match_kind(TokenKind::Colon).is_some() {
            let raw = self.parse_type_name(&[TokenKind::Comma, TokenKind::RParen]);
            if let Some(stripped) = raw.strip_suffix("...") {
                variadic = true;
                Some(stripped.to_string())
            } else {
                Some(raw)
            }
        } else {
            None
        };
        Ok(Param {
            convention,
            name,
            type_name,
            variadic,
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
        let mut type_params = Vec::new();
        if self.match_kind(TokenKind::Lt).is_some() {
            loop {
                type_params.push(
                    self.expect(TokenKind::Identifier, "expected type parameter name")?
                        .text,
                );
                if self.match_kind(TokenKind::Comma).is_none() {
                    break;
                }
            }
            self.expect(TokenKind::Gt, "expected `>` after type parameters")?;
        }
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
        let mut implements = Vec::new();
        if self.match_kind(TokenKind::Implements).is_some() {
            loop {
                implements.push(
                    self.expect(TokenKind::Identifier, "expected interface name")?
                        .text,
                );
                if self.match_kind(TokenKind::Comma).is_none() {
                    break;
                }
            }
        }
        let mut methods = Vec::new();
        if self.match_kind(TokenKind::LBrace).is_some() {
            while self.peek(0).kind != TokenKind::RBrace {
                let mut annotations = Vec::new();
                while self.match_kind(TokenKind::At).is_some() {
                    annotations.push(self.parse_annotation()?);
                }
                let is_pub = self.match_kind(TokenKind::Pub).is_some();
                let mut function = self.parse_function()?;
                function.annotations = annotations;
                function.is_pub = is_pub;
                methods.push(function);
            }
            self.expect(TokenKind::RBrace, "expected `}` after data class body")?;
        }
        Ok(DataClassDecl {
            name,
            type_params,
            fields,
            methods,
            is_pub: false,
            annotations: Vec::new(),
            implements,
            span: start.span,
        })
    }

    fn parse_enum(&mut self) -> Result<EnumDecl, Diagnostic> {
        let start = self.expect(TokenKind::Enum, "expected `enum`")?;
        let name = self.expect(TokenKind::Identifier, "expected enum name")?.text;
        let mut implements = Vec::new();
        if self.match_kind(TokenKind::Implements).is_some() {
            loop {
                implements.push(
                    self.expect(TokenKind::Identifier, "expected interface name")?
                        .text,
                );
                if self.match_kind(TokenKind::Comma).is_none() {
                    break;
                }
            }
        }
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
            implements,
            span: start.span,
        })
    }

    fn parse_interface(&mut self) -> Result<InterfaceDecl, Diagnostic> {
        let start = self.expect(TokenKind::Interface, "expected `interface`")?;
        let name = self
            .expect(TokenKind::Identifier, "expected interface name")?
            .text;
        let mut type_params = Vec::new();
        if self.match_kind(TokenKind::Lt).is_some() {
            loop {
                type_params.push(
                    self.expect(TokenKind::Identifier, "expected type parameter name")?
                        .text,
                );
                if self.match_kind(TokenKind::Comma).is_none() {
                    break;
                }
            }
            self.expect(TokenKind::Gt, "expected `>` after type parameters")?;
        }
        let mut parents = Vec::new();
        if self.match_kind(TokenKind::Colon).is_some() {
            loop {
                parents.push(
                    self.expect(TokenKind::Identifier, "expected parent interface name")?
                        .text,
                );
                if self.match_kind(TokenKind::Comma).is_none() {
                    break;
                }
            }
        }
        self.expect(TokenKind::LBrace, "expected `{` after interface header")?;
        let mut methods = Vec::new();
        while self.peek(0).kind != TokenKind::RBrace {
            let method_start = self.expect(TokenKind::Fn, "expected `fn` for interface method")?;
            let method_name = self
                .expect(TokenKind::Identifier, "expected method name")?
                .text;
            self.expect(TokenKind::LParen, "expected `(` after method name")?;
            let mut params = Vec::new();
            if self.peek(0).kind != TokenKind::RParen {
                loop {
                    params.push(self.parse_param()?);
                    if self.match_kind(TokenKind::Comma).is_none() {
                        break;
                    }
                }
            }
            self.expect(TokenKind::RParen, "expected `)` after method parameters")?;
            let mut return_type = None;
            if self.match_kind(TokenKind::Arrow).is_some() {
                return_type = Some(self.parse_type_name(&[TokenKind::Fn, TokenKind::RBrace]));
            }
            methods.push(InterfaceMethod {
                name: method_name,
                params,
                return_type,
                span: method_start.span,
            });
        }
        self.expect(TokenKind::RBrace, "expected `}` after interface body")?;
        Ok(InterfaceDecl {
            name,
            type_params,
            parents,
            methods,
            is_pub: false,
            span: start.span,
        })
    }

    fn parse_const(&mut self) -> Result<ConstDecl, Diagnostic> {
        let start = self.expect(TokenKind::Val, "expected `val`")?;
        let owner = self
            .expect(TokenKind::Identifier, "expected type or module name")?
            .text;
        self.expect(TokenKind::Dot, "expected `.` after type name in constant")?;
        let name = self
            .expect(TokenKind::Identifier, "expected constant name")?
            .text;
        let type_name = if self.match_kind(TokenKind::Colon).is_some() {
            Some(self.parse_type_name(&[TokenKind::Eq]))
        } else {
            None
        };
        self.expect(TokenKind::Eq, "expected `=` in constant")?;
        let value = self.parse_expression()?;
        Ok(ConstDecl {
            owner,
            name,
            type_name,
            value,
            is_pub: false,
            span: start.span,
        })
    }

    fn parse_struct(&mut self) -> Result<StructDecl, Diagnostic> {
        let start = self.expect(TokenKind::Struct, "expected `struct`")?;
        let name = self
            .expect(TokenKind::Identifier, "expected struct name")?
            .text;
        let mut implements = Vec::new();
        if self.match_kind(TokenKind::Implements).is_some() {
            loop {
                implements.push(
                    self.expect(TokenKind::Identifier, "expected interface name")?
                        .text,
                );
                if self.match_kind(TokenKind::Comma).is_none() {
                    break;
                }
            }
        }
        self.expect(TokenKind::LBrace, "expected `{` after struct name")?;
        let mut fields = Vec::new();
        let mut methods = Vec::new();
        while self.peek(0).kind != TokenKind::RBrace {
            match self.peek(0).kind {
                TokenKind::Val | TokenKind::Var => {
                    let field_token = self.take();
                    let mutable = field_token.kind == TokenKind::Var;
                    let field_name = self
                        .expect(TokenKind::Identifier, "expected field name")?
                        .text;
                    let type_name = if self.match_kind(TokenKind::Colon).is_some() {
                        Some(self.parse_type_name(&[TokenKind::Val, TokenKind::Var, TokenKind::Fn, TokenKind::At, TokenKind::Pub, TokenKind::RBrace]))
                    } else {
                        None
                    };
                    fields.push(FieldDecl {
                        mutable,
                        name: field_name,
                        type_name,
                        span: field_token.span,
                    });
                }
                TokenKind::Fn | TokenKind::At | TokenKind::Pub => {
                    let mut annotations = Vec::new();
                    while self.match_kind(TokenKind::At).is_some() {
                        annotations.push(self.parse_annotation()?);
                    }
                    let is_pub = self.match_kind(TokenKind::Pub).is_some();
                    let mut function = self.parse_function()?;
                    function.annotations = annotations;
                    function.is_pub = is_pub;
                    methods.push(function);
                }
                _ => {
                    return Err(self.syntax_error(
                        format!("expected field or method in struct, got `{}`", self.peek(0).text),
                        self.peek(0).span,
                    ));
                }
            }
        }
        self.expect(TokenKind::RBrace, "expected `}` to close struct")?;
        Ok(StructDecl {
            name,
            fields,
            methods,
            is_pub: false,
            annotations: Vec::new(),
            implements,
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
            return_type = Some(self.parse_type_name(&[TokenKind::LBrace, TokenKind::FatArrow, TokenKind::Fn, TokenKind::Extern, TokenKind::Pub, TokenKind::Data, TokenKind::Enum, TokenKind::Import, TokenKind::At, TokenKind::Val, TokenKind::Var, TokenKind::Struct, TokenKind::Interface, TokenKind::Eof]));
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
        let mut annotations = Vec::new();
        while self.peek(0).kind == TokenKind::At {
            self.take(); // consume @
            annotations.push(self.parse_annotation()?);
        }
        let token = self.peek(0).clone();
        match token.kind {
            TokenKind::Val | TokenKind::Var => {
                if self.peek(1).kind == TokenKind::LParen {
                    return Ok(Statement::TupleDestruct(self.parse_tuple_destruct()?));
                }
                Ok(Statement::VarDecl(self.parse_var_decl(annotations)?))
            }
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

    fn parse_annotation(&mut self) -> Result<Annotation, Diagnostic> {
        let name_tok = self.expect(TokenKind::Identifier, "expected annotation name")?;
        let mut args = Vec::new();
        if self.match_kind(TokenKind::LParen).is_some() {
            loop {
                if self.peek(0).kind == TokenKind::RParen {
                    break;
                }
                let tok = self.peek(0).clone();
                let arg = match tok.kind {
                    TokenKind::Int => {
                        self.take();
                        AnnotationArg::Int(tok.text.parse().unwrap_or_default())
                    }
                    TokenKind::String => {
                        self.take();
                        AnnotationArg::String(tok.text.clone())
                    }
                    TokenKind::Identifier => {
                        self.take();
                        AnnotationArg::Name(tok.text.clone())
                    }
                    _ => return Err(self.syntax_error(
                        format!("unexpected token `{}` in annotation arguments", tok.text),
                        tok.span,
                    )),
                };
                args.push(arg);
                if self.match_kind(TokenKind::Comma).is_none() {
                    break;
                }
            }
            self.expect(TokenKind::RParen, "expected `)` after annotation arguments")?;
        }
        Ok(Annotation {
            name: name_tok.text,
            args,
            span: name_tok.span,
        })
    }

    fn parse_tuple_destruct(&mut self) -> Result<TupleDestructStmt, Diagnostic> {
        let start = self.take(); // val or var
        self.expect(TokenKind::LParen, "expected `(` for tuple destructuring")?;
        let mut names = vec![self
            .expect(TokenKind::Identifier, "expected binding name")?
            .text];
        while self.match_kind(TokenKind::Comma).is_some() {
            names.push(
                self.expect(TokenKind::Identifier, "expected binding name")?
                    .text,
            );
        }
        self.expect(TokenKind::RParen, "expected `)` after tuple bindings")?;
        self.expect(TokenKind::Eq, "expected `=` in tuple destructuring")?;
        let value = self.parse_expression()?;
        Ok(TupleDestructStmt {
            names,
            value,
            span: start.span,
        })
    }

    fn parse_var_decl(&mut self, annotations: Vec<Annotation>) -> Result<VarDecl, Diagnostic> {
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
            annotations,
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
        Ok(SpawnStmt {
            body: self.parse_block()?,
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
                if self.peek(0).kind == TokenKind::Int {
                    let index = self.take();
                    expr = Expr::Member(Member { object: Box::new(expr), name: index.text, optional: false, span: index.span });
                } else {
                    let name = self.take();
                    if name.text.is_empty() || name.kind == TokenKind::Eof {
                        return Err(self.syntax_error("expected member name after `.`", name.span));
                    }
                    expr = Expr::Member(Member { object: Box::new(expr), name: name.text, optional: false, span: name.span });
                }
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
                let first = self.parse_expression()?;
                if self.match_kind(TokenKind::Comma).is_some() {
                    let mut items = vec![first];
                    if self.peek(0).kind != TokenKind::RParen {
                        loop {
                            items.push(self.parse_expression()?);
                            if self.match_kind(TokenKind::Comma).is_none() {
                                break;
                            }
                        }
                    }
                    self.expect(TokenKind::RParen, "expected `)` after tuple")?;
                    Ok(Expr::Tuple(TupleExpr { items, span: token.span }))
                } else {
                    self.expect(TokenKind::RParen, "expected `)`")?;
                    Ok(first)
                }
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
            TokenKind::LParen => {
                self.take();
                let mut elements = vec![self.parse_pattern()?];
                while self.match_kind(TokenKind::Comma).is_some() {
                    elements.push(self.parse_pattern()?);
                }
                self.expect(TokenKind::RParen, "expected `)` after tuple pattern")?;
                Ok(Pattern::Tuple(TuplePattern { elements, span: token.span }))
            }
            _ => Err(self.syntax_error("unsupported match pattern", token.span)),
        }
    }
}

