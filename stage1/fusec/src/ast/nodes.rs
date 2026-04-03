use crate::error::Span;

#[derive(Clone, Debug)]
pub struct Program {
    pub declarations: Vec<Declaration>,
    pub filename: String,
}

#[derive(Clone, Debug)]
pub enum Declaration {
    Import(ImportDecl),
    Function(FunctionDecl),
    DataClass(DataClassDecl),
    Enum(EnumDecl),
}

#[derive(Clone, Debug)]
pub struct ImportDecl {
    pub module_path: String,
    pub items: Option<Vec<String>>,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub struct Param {
    pub convention: Option<String>,
    pub name: String,
    pub type_name: Option<String>,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub struct FieldDecl {
    pub mutable: bool,
    pub name: String,
    pub type_name: Option<String>,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub struct FunctionDecl {
    pub name: String,
    pub params: Vec<Param>,
    pub return_type: Option<String>,
    pub body: Block,
    pub is_pub: bool,
    pub decorators: Vec<String>,
    pub receiver_type: Option<String>,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub struct DataClassDecl {
    pub name: String,
    pub fields: Vec<FieldDecl>,
    pub methods: Vec<FunctionDecl>,
    pub is_pub: bool,
    pub decorators: Vec<String>,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub struct EnumVariant {
    pub name: String,
    pub arity: usize,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub struct EnumDecl {
    pub name: String,
    pub variants: Vec<EnumVariant>,
    pub is_pub: bool,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub struct Block {
    pub statements: Vec<Statement>,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub enum Statement {
    VarDecl(VarDecl),
    Assign(Assign),
    Return(ReturnStmt),
    Break(Span),
    Continue(Span),
    While(WhileStmt),
    For(ForStmt),
    Loop(LoopStmt),
    Defer(DeferStmt),
    Expr(ExprStmt),
}

#[derive(Clone, Debug)]
pub struct VarDecl {
    pub mutable: bool,
    pub name: String,
    pub type_name: Option<String>,
    pub value: Expr,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub struct Assign {
    pub target: Expr,
    pub value: Expr,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub struct ReturnStmt {
    pub value: Option<Expr>,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub struct WhileStmt {
    pub condition: Expr,
    pub body: Block,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub struct ForStmt {
    pub name: String,
    pub iterable: Expr,
    pub body: Block,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub struct LoopStmt {
    pub body: Block,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub struct DeferStmt {
    pub expr: Expr,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub struct ExprStmt {
    pub expr: Expr,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub enum Expr {
    Literal(Literal),
    FString(FString),
    Name(Name),
    List(ListExpr),
    Unary(UnaryOp),
    Binary(BinaryOp),
    Call(Call),
    Member(Member),
    Move(MoveExpr),
    Ref(RefExpr),
    MutRef(MutRefExpr),
    Question(QuestionExpr),
    If(IfExpr),
    Match(MatchExpr),
    When(WhenExpr),
}

impl Expr {
    pub fn span(&self) -> Span {
        match self {
            Self::Literal(node) => node.span,
            Self::FString(node) => node.span,
            Self::Name(node) => node.span,
            Self::List(node) => node.span,
            Self::Unary(node) => node.span,
            Self::Binary(node) => node.span,
            Self::Call(node) => node.span,
            Self::Member(node) => node.span,
            Self::Move(node) => node.span,
            Self::Ref(node) => node.span,
            Self::MutRef(node) => node.span,
            Self::Question(node) => node.span,
            Self::If(node) => node.span,
            Self::Match(node) => node.span,
            Self::When(node) => node.span,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Literal {
    pub value: LiteralValue,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub enum LiteralValue {
    Int(i64),
    Float(f64),
    String(String),
    Bool(bool),
}

#[derive(Clone, Debug)]
pub struct FString {
    pub template: String,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub struct Name {
    pub value: String,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub struct ListExpr {
    pub items: Vec<Expr>,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub struct UnaryOp {
    pub op: String,
    pub value: Box<Expr>,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub struct BinaryOp {
    pub left: Box<Expr>,
    pub op: String,
    pub right: Box<Expr>,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub struct Call {
    pub callee: Box<Expr>,
    pub args: Vec<Expr>,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub struct Member {
    pub object: Box<Expr>,
    pub name: String,
    pub optional: bool,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub struct MoveExpr {
    pub value: Box<Expr>,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub struct RefExpr {
    pub value: Box<Expr>,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub struct MutRefExpr {
    pub value: Box<Expr>,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub struct QuestionExpr {
    pub value: Box<Expr>,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub struct IfExpr {
    pub condition: Box<Expr>,
    pub then_branch: Block,
    pub else_branch: Option<ElseBranch>,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub enum ElseBranch {
    Block(Block),
    IfExpr(Box<IfExpr>),
}

#[derive(Clone, Debug)]
pub struct MatchExpr {
    pub subject: Box<Expr>,
    pub arms: Vec<MatchArm>,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub struct MatchArm {
    pub pattern: Pattern,
    pub body: ArmBody,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub struct WhenExpr {
    pub arms: Vec<WhenArm>,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub struct WhenArm {
    pub condition: Option<Expr>,
    pub body: ArmBody,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub enum ArmBody {
    Block(Block),
    Expr(Expr),
}

#[derive(Clone, Debug)]
pub enum Pattern {
    Wildcard(Span),
    Literal(LiteralPattern),
    Name(NamePattern),
    Variant(VariantPattern),
}

impl Pattern {
    pub fn span(&self) -> Span {
        match self {
            Self::Wildcard(span) => *span,
            Self::Literal(pattern) => pattern.span,
            Self::Name(pattern) => pattern.span,
            Self::Variant(pattern) => pattern.span,
        }
    }
}

#[derive(Clone, Debug)]
pub struct LiteralPattern {
    pub value: LiteralValue,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub struct NamePattern {
    pub name: String,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub struct VariantPattern {
    pub name: String,
    pub args: Vec<Pattern>,
    pub span: Span,
}

