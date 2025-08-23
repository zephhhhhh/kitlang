use std::{fmt::Debug, ops::Range};

#[derive(Debug, Default, PartialEq)]
pub struct ASTRoot {
    pub items: Vec<Item>,
}

impl ASTRoot {
    #[inline]
    pub fn new() -> Self {
        Self { items: Vec::new() }
    }

    #[inline]
    pub fn push_item(&mut self, item: Item) {
        self.items.push(item);
    }
}

#[derive(Debug, Clone, PartialEq, PartialOrd)]
pub enum Visibility {
    Public,
    Private,
}

impl Visibility {
    /// Construct a [`Visibility`] from a bool denoting if the `vis` is `public` or `private`.
    #[inline]
    pub fn from_is_public(v: bool) -> Self {
        if v { Self::Public } else { Self::Private }
    }
}

#[derive(Debug, Clone, PartialEq, PartialOrd)]
pub enum Mutability {
    Mutable,
    Immutable,
}

impl Mutability {
    /// Construct a [`Mutability`] from a bool denoting if the `mutability` is `Mutable` (`true`) or `Immutable` (`false`).
    #[inline]
    pub fn from_is_mutable(v: bool) -> Self {
        if v { Self::Mutable } else { Self::Immutable }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Eq, Ord)]
pub enum UnaryOpKind {
    Dereference,
    Not,
    Negate,
}

#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Eq, Ord)]
pub enum BinaryOpKind {
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    And,
    Or,
    BitwiseXOR,
    BitwiseAND,
    BitwiseOR,
    ShiftLeft,
    ShiftRight,
    Equal,
    NotEqual,
    LessThan,
    GreaterThan,
    LessThanOrEqual,
    GreaterThanOrEqual,
}

impl BinaryOpKind {
    /// Returns how many tokens the operation takes to represent.
    /// (I.e. `Equal (=)` = 1, `And (&&)` = 2)
    #[inline]
    pub fn token_count(self) -> u32 {
        match self {
            Self::And
            | Self::Or
            | Self::NotEqual
            | Self::Equal
            | Self::LessThanOrEqual
            | Self::GreaterThanOrEqual
            | Self::ShiftLeft
            | Self::ShiftRight => 2,
            _ => 1,
        }
    }
}

#[derive(Clone, PartialEq)]
pub enum ItemKind {
    Use,
    Const(Box<Constant>),
    Fn(Box<Function>),
    Mod(Box<Module>),
    Enum(Box<Enum>),
    Struct(Box<Struct>),
    Impl(Box<Impl>),
}

impl ItemKind {
    pub fn get_name(&self) -> &'static str {
        match self {
            ItemKind::Use => "Use",
            ItemKind::Const(_) => "Const",
            ItemKind::Fn(_) => "Function",
            ItemKind::Mod(_) => "Module",
            ItemKind::Enum(_) => "Enum",
            ItemKind::Struct(_) => "Struct",
            ItemKind::Impl(_) => "Impl",
        }
    }
}

impl ::std::fmt::Debug for ItemKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Use => write!(f, "Use"),
            Self::Const(arg0) => arg0.fmt(f),
            Self::Fn(arg0) => arg0.fmt(f),
            Self::Mod(arg0) => arg0.fmt(f),
            Self::Enum(arg0) => arg0.fmt(f),
            Self::Struct(arg0) => arg0.fmt(f),
            Self::Impl(arg0) => arg0.fmt(f),
        }
    }
}

#[derive(Clone, PartialEq, PartialOrd, Hash)]
pub struct Ident(pub String);

impl Ident {
    #[inline]
    pub fn new(src: impl AsRef<str>) -> Self {
        Self(src.as_ref().to_string())
    }
}

#[derive(Clone, PartialEq, PartialOrd, Hash)]
pub enum Ty {
    Infer,
    Type(String),
}

impl Ty {
    #[inline]
    pub fn new(src: impl AsRef<str>) -> Self {
        Self::Type(src.as_ref().to_string())
    }
}

impl<T> From<T> for Ident
where
    T: AsRef<str>,
{
    fn from(value: T) -> Self {
        Self::new(value)
    }
}

impl<T> From<T> for Ty
where
    T: AsRef<str>,
{
    fn from(value: T) -> Self {
        Self::new(value)
    }
}

impl ::std::fmt::Debug for Ident {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Ident({})", self.0)
    }
}

impl ::std::fmt::Debug for Ty {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Ty::Infer => write!(f, "Infer"),
            Ty::Type(t) => write!(f, "Type({})", t),
        }
    }
}

#[repr(transparent)]
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ASTNodeID(pub u32);

pub const PLACEHOLDER_NODE_ID: ASTNodeID = ASTNodeID(u32::MAX);

impl Default for ASTNodeID {
    fn default() -> Self {
        PLACEHOLDER_NODE_ID
    }
}

impl From<u32> for ASTNodeID {
    fn from(value: u32) -> Self {
        Self(value)
    }
}

impl From<&u32> for ASTNodeID {
    fn from(value: &u32) -> Self {
        Self(*value)
    }
}

#[derive(Clone, PartialEq)]
pub struct Item {
    pub id: ASTNodeID,
    pub vis: Visibility,
    pub kind: ItemKind,
    // Maybe span?
}

impl Item {
    #[inline]
    pub fn new(kind: ItemKind, vis: Visibility) -> Self {
        Self {
            id: PLACEHOLDER_NODE_ID,
            vis,
            kind,
        }
    }
}

impl ::std::fmt::Debug for Item {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Item")
            .field("vis", &self.vis)
            .field("kind", &self.kind)
            .finish()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
pub enum ExpressionOrder {
    /// return, break..
    Jump,
    /// = += -= *= /= %= &= |= ^= <<= >>=
    Assign,
    /// ||
    LogicalOr,
    /// &&
    LogicalAnd,
    /// == != < > <= >=
    Compare,
    /// |
    BitwiseOr,
    /// ^
    BitwiseXor,
    /// &
    BitwiseAnd,
    /// << >>
    Shift,
    /// + -
    Sum,
    /// * / %
    Product,
    /// as
    Cast,
    /// unary - * ! & &mut
    Prefix,
    /// Loops, function calls, array indexing, field expressions, method calls, values, etc..
    Unambiguous,
}

#[derive(PartialEq, Debug)]
pub enum ExpressionAssociation {
    /// Left-associative
    Left,
    /// Right-associative
    Right,
    /// Not associative
    None,
}

impl BinaryOpKind {
    pub fn get_order(self) -> ExpressionOrder {
        match self {
            Self::Add | Self::Sub => ExpressionOrder::Sum,
            Self::Mul | Self::Div | Self::Mod => ExpressionOrder::Product,
            Self::And => ExpressionOrder::LogicalAnd,
            Self::Or => ExpressionOrder::LogicalOr,
            Self::BitwiseXOR => ExpressionOrder::BitwiseXor,
            Self::BitwiseAND => ExpressionOrder::BitwiseAnd,
            Self::BitwiseOR => ExpressionOrder::BitwiseOr,
            Self::ShiftLeft | Self::ShiftRight => ExpressionOrder::Shift,
            Self::Equal
            | Self::NotEqual
            | Self::LessThan
            | Self::LessThanOrEqual
            | Self::GreaterThan
            | Self::GreaterThanOrEqual => ExpressionOrder::Compare,
        }
    }

    pub fn get_association(self) -> ExpressionAssociation {
        match self {
            Self::Add
            | Self::Sub
            | Self::Mul
            | Self::Div
            | Self::Mod
            | Self::And
            | Self::Or
            | Self::BitwiseXOR
            | Self::BitwiseAND
            | Self::BitwiseOR
            | Self::ShiftLeft
            | Self::ShiftRight => ExpressionAssociation::Left,
            Self::Equal
            | Self::NotEqual
            | Self::LessThan
            | Self::LessThanOrEqual
            | Self::GreaterThan
            | Self::GreaterThanOrEqual => ExpressionAssociation::None,
        }
    }
}

#[derive(Clone, PartialEq)]
pub enum ExpressionKind {
    Block(Box<Block>),
    Literal(Literal),
    BinaryOp(BinaryOpKind, Box<Expression>, Box<Expression>),
    Unary(UnaryOpKind, Box<Expression>),
    If(Box<Expression>, Box<Block>, Option<Box<Expression>>),
    While(Box<Expression>, Box<Block>),
    Assign(Box<Expression>, Box<Expression>),
    Call(Box<Expression>, Vec<Box<Expression>>),
    MethodCall(Box<MethodCall>),
    Index(Box<Expression>, Box<Expression>),
    FieldAccess(Box<Expression>, Ident),
    StructInit(Box<StructInitialisation>),
    Ident(Ident),
    Continue,
    Break,
    Return(Option<Box<Expression>>),
}

impl ExpressionKind {
    pub fn can_be_non_semi(&self) -> bool {
        matches!(
            self,
            ExpressionKind::Block(_) | ExpressionKind::If(_, _, _) | ExpressionKind::While(_, _)
        )
    }

    pub fn get_order(&self) -> ExpressionOrder {
        match self {
            ExpressionKind::BinaryOp(binary_op_kind, _, _) => binary_op_kind.get_order(),
            ExpressionKind::Unary(_, _) => ExpressionOrder::Prefix,
            ExpressionKind::Assign(_, _) => ExpressionOrder::Assign,
            ExpressionKind::Continue | ExpressionKind::Return(_) | ExpressionKind::Break => {
                ExpressionOrder::Jump
            }
            _ => ExpressionOrder::Unambiguous,
        }
    }

    pub fn get_association(&self) -> ExpressionAssociation {
        match self {
            ExpressionKind::BinaryOp(binary_op_kind, _, _) => binary_op_kind.get_association(),
            ExpressionKind::Unary(_, _) => ExpressionAssociation::None,
            ExpressionKind::Assign(_, _) => ExpressionAssociation::Right,
            _ => ExpressionAssociation::None,
        }
    }
}

impl ::std::fmt::Debug for ExpressionKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Block(arg0) => f.debug_tuple("Block").field(arg0).finish(),
            Self::Literal(arg0) => arg0.fmt(f),
            Self::BinaryOp(arg0, arg1, arg2) => f
                .debug_tuple("BinaryOp")
                .field(arg0)
                .field(arg1)
                .field(arg2)
                .finish(),
            Self::Unary(arg0, arg1) => f.debug_tuple("Unary").field(arg0).field(arg1).finish(),
            Self::If(arg0, arg1, arg2) => f
                .debug_tuple("If")
                .field(arg0)
                .field(arg1)
                .field(arg2)
                .finish(),
            Self::While(arg0, arg1) => f.debug_tuple("While").field(arg0).field(arg1).finish(),
            Self::Assign(arg0, arg1) => f.debug_tuple("Assign").field(arg0).field(arg1).finish(),
            Self::Call(arg0, arg1) => f.debug_tuple("Call").field(arg0).field(arg1).finish(),
            Self::MethodCall(arg0) => arg0.fmt(f),
            Self::Index(arg0, arg1) => f.debug_tuple("Index").field(arg0).field(arg1).finish(),
            Self::FieldAccess(arg0, arg1) => f
                .debug_tuple("FieldAccess")
                .field(arg0)
                .field(arg1)
                .finish(),
            Self::StructInit(arg0) => arg0.fmt(f),
            Self::Ident(arg0) => arg0.fmt(f),
            Self::Continue => write!(f, "Continue"),
            Self::Break => write!(f, "Break"),
            Self::Return(arg0) => f.debug_tuple("Return").field(arg0).finish(),
        }
    }
}

#[derive(Clone, PartialEq)]
pub struct Expression {
    pub id: ASTNodeID,
    pub kind: ExpressionKind,
    // Maybe span?
}

impl Expression {
    #[inline]
    pub fn new(kind: ExpressionKind) -> Self {
        Self {
            id: PLACEHOLDER_NODE_ID,
            kind,
        }
    }

    #[inline]
    pub fn new_boxed(kind: ExpressionKind) -> Box<Self> {
        Box::new(Self::new(kind))
    }

    #[inline]
    pub fn get_order(&self) -> ExpressionOrder {
        self.kind.get_order()
    }

    #[inline]
    pub fn get_association(&self) -> ExpressionAssociation {
        self.kind.get_association()
    }
}

impl ::std::fmt::Debug for Expression {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.kind.fmt(f)
    }
}

#[derive(Clone, PartialEq)]
pub enum StatementKind {
    /// Variable declaration.
    Let(Box<Local>),
    /// Item definition. (Function, struct, etc..)
    Item(Box<Item>),
    /// Expression without a semi-colon
    Expr(Box<Expression>),
    /// Expression with a semi-colon.
    Semi(Box<Expression>),
    /// Just a semi-colon.
    Empty,
}

impl ::std::fmt::Debug for StatementKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Let(arg0) => arg0.fmt_with_name(f, "Let"),
            Self::Item(arg0) => arg0.fmt(f),
            Self::Expr(arg0) => f.debug_tuple("Expr").field(arg0).finish(),
            Self::Semi(arg0) => f.debug_tuple("Semi").field(arg0).finish(),
            Self::Empty => write!(f, "Empty"),
        }
    }
}

#[derive(Clone, PartialEq)]
pub struct Statement {
    pub id: ASTNodeID,
    pub kind: StatementKind,
    pub source_span: Range<u32>, // Maybe span?
}

impl Statement {
    pub fn new(kind: StatementKind, source_span: Range<u32>) -> Self {
        Self {
            id: PLACEHOLDER_NODE_ID,
            kind,
            source_span,
        }
    }
}

impl ::std::fmt::Debug for Statement {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.kind.fmt(f)
    }
}

#[derive(Clone, PartialEq, PartialOrd)]
pub enum Literal {
    String(String),
    Float(f64),
    Integer(i64),
    Boolean(bool),
}

impl ::std::fmt::Debug for Literal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::String(arg0) => write!(f, "String(\"{}\")", arg0),
            Self::Float(arg0) => write!(f, "Float({})", arg0),
            Self::Integer(arg0) => write!(f, "Integer({})", arg0),
            Self::Boolean(arg0) => write!(f, "Boolean({})", arg0),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum LocalKind {
    Declaration,
    Initialise(Box<Expression>),
}

#[derive(Clone, PartialEq)]
pub struct Local {
    pub id: ASTNodeID,
    pub ident: Ident,
    pub ty: Ty,
    pub kind: LocalKind,
    pub mutable: Mutability,
}

impl Local {
    #[inline]
    pub fn new(ident: Ident, ty: Ty, kind: LocalKind, mutable: Mutability) -> Self {
        Self {
            id: PLACEHOLDER_NODE_ID,
            ident,
            ty,
            kind,
            mutable,
        }
    }

    #[inline]
    pub fn new_boxed(ident: Ident, ty: Ty, kind: LocalKind, mutable: Mutability) -> Box<Self> {
        Box::new(Self::new(ident, ty, kind, mutable))
    }

    pub fn fmt_with_name(&self, f: &mut std::fmt::Formatter<'_>, name: &str) -> std::fmt::Result {
        let info = format!(
            "{{ name: {:?}, type: {:?}, {:?} }}",
            self.ident, self.ty, self.mutable
        );
        f.debug_struct(name)
            .field_with("info", |a| write!(a, "{}", info))
            .field("kind", &self.kind)
            .finish()
    }
}

impl ::std::fmt::Debug for Local {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.fmt_with_name(f, "Local")
    }
}

#[derive(Clone, PartialEq)]
pub struct Constant {
    pub id: ASTNodeID,
    pub ident: Ident,
    pub ty: Ty,
    pub expr: Box<Expression>,
}

impl Constant {
    #[inline]
    pub fn new(ident: Ident, ty: Ty, expr: Box<Expression>) -> Self {
        Self {
            id: PLACEHOLDER_NODE_ID,
            ident,
            ty,
            expr,
        }
    }

    #[inline]
    pub fn new_boxed(ident: Ident, ty: Ty, expr: Box<Expression>) -> Box<Self> {
        Box::new(Self::new(ident, ty, expr))
    }
}

impl ::std::fmt::Debug for Constant {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Constant")
            .field("ident", &self.ident)
            .field("type", &self.ty)
            .field("expr", &self.expr)
            .finish()
    }
}

#[derive(Clone, PartialEq)]
pub struct Block {
    pub id: ASTNodeID,
    pub statements: Vec<Statement>,
    // Maybe span
}

impl ::std::fmt::Debug for Block {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_list().entries(&self.statements).finish()
    }
}

impl Block {
    pub fn new(statements: Vec<Statement>) -> Self {
        Self {
            id: PLACEHOLDER_NODE_ID,
            statements,
        }
    }
}

#[derive(Clone, PartialEq, PartialOrd)]
pub struct Parameter {
    pub id: ASTNodeID,
    pub ident: Ident,
    pub ty: Ty,
    pub mutable: Mutability,
}

impl Parameter {
    pub fn new(ident: Ident, ty: Ty, mutable: Mutability) -> Self {
        Self {
            id: PLACEHOLDER_NODE_ID,
            ident,
            ty,
            mutable,
        }
    }
}

impl ::std::fmt::Debug for Parameter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Parameter {{ name: {:?}, type: {:?}, mutable: {:?} }}",
            self.ident, self.ty, self.mutable
        )
        // f.debug_struct("Parameter")
        //     .field("name", &self.ident)
        //     .field("type", &self.ty)
        //     .field("mutable", &self.mutable)
        //     .finish()
    }
}

#[derive(Debug, Clone, PartialEq, PartialOrd)]
pub enum FunctionReturnTy {
    /// No return type specified.
    Default,
    /// Specified return type.
    Ty(Box<Ty>),
}

#[derive(Debug, Clone, PartialEq, PartialOrd)]
pub struct FunctionSig {
    pub parameters: Vec<Parameter>,
    pub output: FunctionReturnTy,
}

#[derive(Clone, PartialEq)]
pub struct Function {
    pub id: ASTNodeID,
    pub ident: Ident,
    pub sig: FunctionSig,
    pub body: Option<Box<Block>>,
}

impl Function {
    pub fn new(ident: Ident, sig: FunctionSig, body: Option<Box<Block>>) -> Self {
        Self {
            id: PLACEHOLDER_NODE_ID,
            ident,
            sig,
            body,
        }
    }
}

impl ::std::fmt::Debug for Function {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Function")
            .field("ident", &self.ident.0)
            .field("sig", &self.sig)
            .field("body", &self.body)
            .finish()
    }
}

#[derive(Clone, PartialEq)]
pub struct StructField {
    pub id: ASTNodeID,
    pub ident: Ident,
    pub ty: Ty,
    pub vis: Visibility,
}

impl StructField {
    pub fn new(ident: Ident, ty: Ty, vis: Visibility) -> Self {
        Self {
            id: PLACEHOLDER_NODE_ID,
            ident,
            ty,
            vis,
        }
    }
}

impl ::std::fmt::Debug for StructField {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StructField")
            .field("ident", &self.ident)
            .field("type", &self.ty)
            .field("vis", &self.vis)
            .finish()
    }
}

#[derive(Clone, PartialEq)]
pub struct Struct {
    pub id: ASTNodeID,
    pub ident: Ident,
    pub fields: Vec<StructField>,
}

impl Struct {
    pub fn new(ident: Ident, fields: Vec<StructField>) -> Self {
        Self {
            id: PLACEHOLDER_NODE_ID,
            ident,
            fields,
        }
    }

    pub fn new_boxed(ident: Ident, fields: Vec<StructField>) -> Box<Self> {
        Box::new(Self::new(ident, fields))
    }
}

impl ::std::fmt::Debug for Struct {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Struct")
            .field("ident", &self.ident)
            .field("fields", &self.fields)
            .finish()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum VariantData {
    Struct(Vec<StructField>),
    Tuple(Vec<Ty>),
    Unit,
}

#[derive(Clone, PartialEq)]
pub struct EnumVariant {
    pub id: ASTNodeID,
    pub ident: Ident,
    pub data: VariantData,
}

impl EnumVariant {
    #[inline]
    pub fn new(ident: Ident, data: VariantData) -> Self {
        Self {
            id: PLACEHOLDER_NODE_ID,
            ident,
            data,
        }
    }
}

impl ::std::fmt::Debug for EnumVariant {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EnumVariant")
            .field("ident", &self.ident)
            .field("data", &self.data)
            .finish()
    }
}

#[derive(Clone, PartialEq)]
pub struct Enum {
    pub id: ASTNodeID,
    pub ident: Ident,
    pub variants: Vec<EnumVariant>,
}

impl Enum {
    #[inline]
    pub fn new(ident: Ident, variants: Vec<EnumVariant>) -> Self {
        Self {
            id: PLACEHOLDER_NODE_ID,
            ident,
            variants,
        }
    }

    #[inline]
    pub fn new_boxed(ident: Ident, variants: Vec<EnumVariant>) -> Box<Self> {
        Box::new(Self::new(ident, variants))
    }

    /// Does the enum have 'zero' variants?
    #[inline]
    pub fn is_never_type(&self) -> bool {
        self.variants.is_empty()
    }

    /// Does the enum have exactly 'one' variant?
    #[inline]
    pub fn is_unit_type(&self) -> bool {
        self.variants.len() == 1
    }
}

impl ::std::fmt::Debug for Enum {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Enum")
            .field("ident", &self.ident)
            .field("variants", &self.variants)
            .finish()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum ModuleKind {
    Declaration,
    Definition(Vec<Item>),
}

#[derive(Clone, PartialEq)]
pub struct Module {
    pub id: ASTNodeID,
    pub ident: Ident,
    pub kind: ModuleKind,
}

impl Module {
    #[inline]
    pub fn new(ident: Ident, kind: ModuleKind) -> Self {
        Self {
            id: PLACEHOLDER_NODE_ID,
            ident,
            kind,
        }
    }

    #[inline]
    pub fn new_boxed(ident: Ident, kind: ModuleKind) -> Box<Self> {
        Box::new(Self::new(ident, kind))
    }
}

impl ::std::fmt::Debug for Module {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Module")
            .field("ident", &self.ident)
            .field("kind", &self.kind)
            .finish()
    }
}

#[derive(Clone, PartialEq)]
pub struct Impl {
    pub id: ASTNodeID,
    pub target_ident: Ident,
    pub items: Vec<Item>,
}

impl Impl {
    #[inline]
    pub fn new(target_ident: Ident, items: Vec<Item>) -> Self {
        Self {
            id: PLACEHOLDER_NODE_ID,
            target_ident,
            items,
        }
    }

    #[inline]
    pub fn new_boxed(target_ident: Ident, items: Vec<Item>) -> Box<Self> {
        Box::new(Self::new(target_ident, items))
    }
}

impl ::std::fmt::Debug for Impl {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Impl")
            .field("target_ident", &self.target_ident)
            .field("items", &self.items)
            .finish()
    }
}

#[derive(Clone, PartialEq)]
pub struct MethodCall {
    pub id: ASTNodeID,
    pub target_expr: Box<Expression>,
    pub method_ident: Ident,
    pub args: Vec<Box<Expression>>,
}

impl MethodCall {
    #[inline]
    pub fn new(
        target_expr: Box<Expression>,
        method_ident: Ident,
        args: Vec<Box<Expression>>,
    ) -> Self {
        Self {
            id: PLACEHOLDER_NODE_ID,
            target_expr,
            method_ident,
            args,
        }
    }

    #[inline]
    pub fn new_boxed(
        target_expr: Box<Expression>,
        method_ident: Ident,
        args: Vec<Box<Expression>>,
    ) -> Box<Self> {
        Box::new(Self::new(target_expr, method_ident, args))
    }
}

impl ::std::fmt::Debug for MethodCall {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MethodCall")
            .field("target_expr", &self.target_expr)
            .field("method_ident", &self.method_ident)
            .field("args", &self.args)
            .finish()
    }
}

#[derive(Clone, PartialEq)]
pub struct FieldInitialisation {
    pub id: ASTNodeID,
    pub ident: Ident,
    pub expr: Box<Expression>,
}

impl FieldInitialisation {
    #[inline]
    pub fn new(ident: Ident, expr: Box<Expression>) -> Self {
        Self {
            id: PLACEHOLDER_NODE_ID,
            ident,
            expr,
        }
    }
}

impl ::std::fmt::Debug for FieldInitialisation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FieldInitialisation")
            .field("ident", &self.ident)
            .field("expr", &self.expr)
            .finish()
    }
}

#[derive(Clone, PartialEq)]
pub struct StructInitialisation {
    pub id: ASTNodeID,
    pub ident: Ident,
    pub fields: Vec<FieldInitialisation>,
}

impl StructInitialisation {
    #[inline]
    pub fn new(ident: Ident, fields: Vec<FieldInitialisation>) -> Self {
        Self {
            id: PLACEHOLDER_NODE_ID,
            ident,
            fields,
        }
    }

    #[inline]
    pub fn new_boxed(ident: Ident, fields: Vec<FieldInitialisation>) -> Box<Self> {
        Box::new(Self::new(ident, fields))
    }
}

impl ::std::fmt::Debug for StructInitialisation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StructInitialisation")
            .field("ident", &self.ident)
            .field("fields", &self.fields)
            .finish()
    }
}
