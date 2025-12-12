use ::std::fmt::{Debug, Display};
use ::std::ops::Range;

use crate::token::Token;

/// This is the output of the `Parser` stage.
/// Currently this only stores the AST from a single "root" module/project.
/// In future this will store multiple modules for multiple file compilations.
#[derive(Debug, PartialEq)]
pub struct ASTRoot {
    pub items: Vec<Item>,
    pub full_file_span: SourceSpan,
}

impl ASTRoot {
    #[inline]
    pub fn new_with_span(full_file_span: SourceSpan) -> Self {
        Self {
            items: Vec::new(),
            full_file_span,
        }
    }

    /// Pushes an item into the list of items for the current module.
    #[inline]
    pub fn push_item(&mut self, item: Item) {
        self.items.push(item);
    }
}

/// Represents the span of bytes in the source string that the error originates from. This span is
/// described as an exclusive span.
/// # Note
/// This span is in `bytes` and _not_ `characters`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SourceSpan {
    pub start: u32,
    pub end: u32,
}

impl SourceSpan {
    pub fn new(start: u32, end: u32) -> Self {
        Self { start, end }
    }

    pub fn null_span() -> Self {
        Self::new(0, 0)
    }

    pub fn is_null_span(&self) -> bool {
        self.start == 0 && self.end == 0
    }
}

impl From<(u32, u32)> for SourceSpan {
    fn from(value: (u32, u32)) -> Self {
        Self::new(value.0, value.1)
    }
}

impl<T: Into<u32>> From<Range<T>> for SourceSpan {
    fn from(value: Range<T>) -> SourceSpan {
        Self::new(value.start.into(), value.end.into())
    }
}

impl From<Token> for SourceSpan {
    fn from(value: Token) -> Self {
        Self::new(value.start, value.end)
    }
}

impl From<&Token> for SourceSpan {
    fn from(value: &Token) -> Self {
        Self::new(value.start, value.end)
    }
}

pub type IdentPathSegment = String;
pub type IdentPathSegments = Vec<IdentPathSegment>;

/// A "path" consisting of `Identifiers`. You can think of this as a file system path like:
/// "Users/Username/Downloads", except this describes paths between different modules/files in our code,
/// and instead of slashes we use the sequence of characters "::" as our seperator. So in our case
/// this would be represented as "Users::Username::Downloads".
/// # Notes
/// A path can be defined as relative to the root "module" instead of the current scope by starting the path
/// with the seperator, such as: `::Struct::Function`.
#[derive(Clone, Eq, PartialEq, PartialOrd, Hash)]
pub struct IdentPath {
    segments: IdentPathSegments,
    root_relative: bool,
}

impl IdentPath {
    /// Separator used to separate path segments.
    pub const PATH_SEP: &str = "::";

    /// Create a new path from a string representation.
    /// Automatically detects if the path is root-relative by checking for leading `::`.
    ///
    /// # Notes
    /// The parser ensures valid paths are provided, so this method assumes well-formed input.
    #[inline]
    pub fn new(src: impl AsRef<str>) -> Self {
        let src = src.as_ref();
        let (root_relative, remainder) = if let Some(stripped) = src.strip_prefix(Self::PATH_SEP) {
            (true, stripped)
        } else {
            (false, src)
        };

        let segments = remainder
            .split(Self::PATH_SEP)
            .map(|s| s.to_string())
            .collect();

        Self {
            segments,
            root_relative,
        }
    }

    /// Create an empty path.
    #[inline]
    pub fn new_empty(root_relative: bool) -> Self {
        Self {
            segments: Vec::new(),
            root_relative,
        }
    }

    /// Create a path from segments.
    #[inline]
    pub fn from_segments(segments: IdentPathSegments, root_relative: bool) -> Self {
        Self {
            segments,
            root_relative,
        }
    }

    /// Create a path from a segment slice.
    #[inline]
    pub fn from_segments_slice(segments: &[IdentPathSegment], root_relative: bool) -> Self {
        Self::from_segments(segments.to_vec(), root_relative)
    }

    /// Rebase this path onto another, returning `None` if this path is root-relative.
    #[inline]
    pub fn rebase_from_path_safe(&self, base_path: &Self) -> Option<Self> {
        if self.root_relative {
            return None;
        }

        let mut segments = base_path.segments.clone();
        segments.extend_from_slice(&self.segments);

        Some(Self {
            segments,
            root_relative: base_path.root_relative,
        })
    }

    /// Rebase this path onto another. Returns self if this path is root-relative.
    #[inline]
    pub fn rebase_from_path(&self, base_path: &Self) -> Self {
        if self.root_relative {
            self.clone()
        } else {
            let mut segments = base_path.segments.clone();
            segments.extend_from_slice(&self.segments);

            Self::from_segments(segments, base_path.root_relative)
        }
    }

    /// Rebase this path onto a string-based path.
    #[inline]
    pub fn rebase_from_string(&self, base_str: impl AsRef<str>) -> Self {
        self.rebase_from_path(&Self::new(base_str))
    }

    /// Convert to an `Identifier`.
    #[inline]
    pub fn to_ident(&self) -> Ident {
        Ident::new(self.to_string())
    }

    /// Check if this path is root-relative.
    #[inline]
    pub fn is_root_relative(&self) -> bool {
        self.root_relative
    }

    /// Check if this path is exactly one segment and not root-relative.
    #[inline]
    pub fn is_only_ident(&self) -> bool {
        self.segments.len() == 1 && !self.root_relative
    }

    /// Get the single identifier if this path contains exactly one segment and is not root-relative.
    #[inline]
    pub fn get_only_ident(&self) -> Option<String> {
        if self.is_only_ident() {
            Some(self.segments[0].clone())
        } else {
            None
        }
    }

    /// Get the last segment of the path (the "stem").
    ///
    /// # Panics
    /// Panics if the path has no segments.
    #[inline]
    pub fn path_stem(&self) -> &str {
        self.segments
            .last()
            .expect("Path must have at least one segment!")
    }

    /// Number of segments in this path.
    #[inline]
    pub fn len(&self) -> usize {
        self.segments.len()
    }

    /// Check if the path has no segments.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.segments.is_empty()
    }

    /// Get all segments as a slice.
    #[inline]
    pub fn segments(&self) -> &[IdentPathSegment] {
        &self.segments
    }

    /// Get mutable access to segments.
    #[inline]
    pub fn segments_mut(&mut self) -> &mut Vec<IdentPathSegment> {
        &mut self.segments
    }

    /// Remove the last segment.
    #[inline]
    pub fn pop(&mut self) {
        self.segments.pop();
    }

    /// Add a segment to the end.
    #[inline]
    pub fn push(&mut self, segment: &str) {
        self.segments.push(segment.to_string());
    }

    /// Add all segments from a slice of segments to the end of `self`.
    #[inline]
    pub fn push_segments(&mut self, path: &[IdentPathSegment]) {
        self.segments.extend_from_slice(path);
    }

    /// Add all segments from another path (if not root-relative).
    #[inline]
    pub fn push_path(&mut self, path: &Self) {
        if !path.root_relative {
            self.segments.extend_from_slice(&path.segments);
        }
    }

    /// Create a new path with an additional segment.
    #[inline]
    pub fn extend_from(&self, segment: &str) -> Self {
        let mut path = self.clone();
        path.push(segment);
        path
    }

    /// Create a new path with an additional segment.
    #[inline]
    pub fn extend(&self, segment: &str) -> Self {
        let mut path = self.clone();
        path.push(segment);
        path
    }

    /// Create a new path with an additional segment from an identifier.
    #[inline]
    pub fn extend_ident(&self, ident: &Ident) -> Self {
        self.extend(ident.str())
    }

    /// Create a new path with all segments from another path.
    #[inline]
    pub fn extend_path(&self, path: &IdentPath) -> Self {
        let mut new_path = self.clone();
        new_path.push_segments(&path.segments);
        new_path
    }

    /// Create a new path from a subpath of this path from the given range.
    #[inline]
    pub fn subpath(&self, start: usize, end: usize) -> Self {
        Self::from_segments_slice(&self.segments[start..end], self.root_relative)
    }

    /// Check if this path starts with all segments of another path.
    #[inline]
    pub fn is_subpath_of(&self, other: &IdentPath) -> bool {
        if self.len() < other.len() {
            return false;
        }

        self.segments()
            .iter()
            .zip(other.segments().iter())
            .all(|(a, b)| a == b)
    }

    #[inline]
    pub fn matching_segment_count(&self, other: &IdentPath) -> usize {
        self.segments()
            .iter()
            .zip(other.segments().iter())
            .take_while(|(a, b)| a == b)
            .count()
    }
}

impl Display for IdentPath {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.root_relative {
            write!(f, "{}", Self::PATH_SEP)?;
        }
        write!(f, "{}", self.segments.join(Self::PATH_SEP))
    }
}

impl Debug for IdentPath {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let path_str = self.to_string();
        let kind = if self.root_relative {
            "RootPath"
        } else {
            "Path"
        };
        write!(f, "{}('{}')", kind, path_str)
    }
}

#[derive(Clone, PartialEq, PartialOrd, Hash)]
pub struct SpannedIdentPath {
    pub path: IdentPath,
    pub span: SourceSpan,
}

impl SpannedIdentPath {
    pub fn new(path: IdentPath, span: SourceSpan) -> Self {
        Self { path, span }
    }
}

impl ::std::ops::Deref for SpannedIdentPath {
    type Target = IdentPath;

    fn deref(&self) -> &Self::Target {
        &self.path
    }
}

impl ::std::ops::DerefMut for SpannedIdentPath {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.path
    }
}

impl Debug for SpannedIdentPath {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "SpannedIdentPath {{ {}, {}..{} }}",
            self.path.to_ident().str(),
            self.span.start,
            self.span.end
        )
    }
}

impl Display for SpannedIdentPath {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        Display::fmt(&self.path, f)
    }
}

/// Describes whether an [`Item`] is able to be accessed/referenced from other modules/scopes.
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Hash)]
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

    /// Returns true if `self` is `Visibility::Public`.
    #[inline]
    pub fn is_public(self) -> bool {
        matches!(self, Visibility::Public)
    }
}

/// Describes the mutability of a reference or variable.
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Hash)]
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

    /// Returns true if the `self` is `Mutability::Mutable`.
    #[inline]
    pub fn is_mutable(self) -> bool {
        matches!(self, Mutability::Mutable)
    }
}

/// Kinds of "Unary" operations. A unary operation is an operation that acts on a _single_ value.
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Eq, Ord)]
pub enum UnaryOpKind {
    /// Dereference a reference or pointer. (`*variable`)
    Dereference,
    /// The "NOT" operation. (`!variable`)
    Not,
    /// Negation operation. (`-variable`)
    Negate,
}

impl UnaryOpKind {
    /// Returns the combination of characters that represent this operation.
    #[inline]
    pub fn symbols(self) -> &'static str {
        match self {
            UnaryOpKind::Dereference => "*",
            UnaryOpKind::Not => "!",
            UnaryOpKind::Negate => "-",
        }
    }
}

/// Kinds of "Binary" operations. A binary operation is an operation that acts on _two_ values.
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Eq, Ord)]
pub enum BinaryOpKind {
    /// Addition. (`a + b`)
    Add,
    /// Subtraction. (`a - b`)
    Sub,
    /// Multiplication. (`a * b`)
    Mul,
    /// Division. (`a / b`)
    Div,
    /// Modulus. (`a % b`)
    Mod,
    /// Logical AND. (`a && b`)
    And,
    /// Logical OR. (`a || b`)
    Or,
    /// Bitwise XOR. (`a ^ b`)
    BitwiseXOR,
    /// Bitwise AND. (`a & b`)
    BitwiseAND,
    /// Bitwise OR. (`a | b`)
    BitwiseOR,
    /// Bitwise shift left. (`a << b`)
    ShiftLeft,
    /// Bitwise shift right. (`a >> b`)
    ShiftRight,
    /// Equality comparison. (`a == b`)
    Equal,
    /// Negated equality comparison. (`a != b`)
    NotEqual,
    /// Less than comparison. (`a < b`)
    LessThan,
    /// Greater than comparison. (`a > b`)
    GreaterThan,
    /// Less than or equal to comparison. (`a >= b`)
    LessThanOrEqual,
    /// Greater than or equal to comparison. (`a <= b`)
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

    /// Returns the combination of characters that represent this operation.
    #[inline]
    pub fn symbols(self) -> &'static str {
        match self {
            Self::Add => "+",
            Self::Sub => "-",
            Self::Mul => "*",
            Self::Div => "/",
            Self::Mod => "%",
            Self::And => "&&",
            Self::Or => "||",
            Self::BitwiseXOR => "^",
            Self::BitwiseAND => "&",
            Self::BitwiseOR => "|",
            Self::ShiftLeft => "<<",
            Self::ShiftRight => ">>",
            Self::Equal => "==",
            Self::NotEqual => "!=",
            Self::LessThan => "<",
            Self::GreaterThan => ">",
            Self::LessThanOrEqual => "<=",
            Self::GreaterThanOrEqual => ">=",
        }
    }
}

/// Describes each possible kind of an [`Item`] in the AST.
/// # Notes
/// Examples of an [`Item`] would include a function, struct or constant.
#[derive(Clone, PartialEq)]
pub enum ItemKind {
    /// A `use` declaration, akin to an import.
    Use(UseImport),
    /// A `const` expression.
    /// # Kit Example
    /// `pub const PI: f32 = 3.141;`
    Const(Box<Constant>),
    /// Function declaration with an optional function body.
    Fn(Box<Function>),
    /// Module declaration.
    Mod(Box<Module>),
    /// Enumeration declaration, contains information about it's possible variants.
    Enum(Box<Enum>),
    /// Structure declaration, contains information about it's fields.
    Struct(Box<Struct>),
    /// Implementation block, contains either constant definitions or function definitions.
    Impl(Box<Impl>),
}

impl ItemKind {
    /// Get the human readable name _only_ of the [`ItemKind`].
    /// # Example usage
    /// `ItemKind::Fn(..).get_name()` = "Function"
    #[inline]
    pub fn get_name(&self) -> &'static str {
        match self {
            Self::Use(_) => "Use",
            Self::Const(_) => "Const",
            Self::Fn(_) => "Function",
            Self::Mod(_) => "Module",
            Self::Enum(_) => "Enum",
            Self::Struct(_) => "Struct",
            Self::Impl(_) => "Impl",
        }
    }

    /// Returns the `Identifier` of the item kind, if applicable.
    #[inline]
    pub fn ident(&self) -> Option<String> {
        match self {
            Self::Const(constant) => Some(constant.ident.string()),
            Self::Fn(function) => Some(function.ident.string()),
            Self::Mod(module) => Some(module.ident.string()),
            Self::Enum(e) => Some(e.ident.string()),
            Self::Struct(s) => Some(s.ident.string()),
            _ => None,
        }
    }
}

impl Debug for ItemKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Use(arg0) => arg0.fmt(f),
            Self::Const(arg0) => arg0.fmt(f),
            Self::Fn(arg0) => arg0.fmt(f),
            Self::Mod(arg0) => arg0.fmt(f),
            Self::Enum(arg0) => arg0.fmt(f),
            Self::Struct(arg0) => arg0.fmt(f),
            Self::Impl(arg0) => arg0.fmt(f),
        }
    }
}

/// An `Identifier`. This is is defined as a unique type so it does not get confused as to what it
/// describes.
#[derive(Clone, PartialEq, PartialOrd, Hash)]
pub struct Ident(pub String);

impl Ident {
    /// Create a new identifier from a source string.
    #[inline]
    pub fn new(src: impl AsRef<str>) -> Self {
        Self(src.as_ref().to_string())
    }

    /// Clones the inner string.
    pub fn string(&self) -> String {
        self.0.clone()
    }

    /// Reference to the inner string as a slice.
    pub fn str(&self) -> &str {
        &self.0
    }
}

impl<T: AsRef<str>> From<T> for Ident {
    fn from(value: T) -> Self {
        Self::new(value)
    }
}

impl Debug for Ident {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Ident({})", self.0)
    }
}

/// An identifier with an associated `Span` describing the range of bytes in the source code the
/// identifier occupies.
#[derive(Clone, PartialEq, PartialOrd, Hash)]
pub struct SpannedIdent {
    pub ident: Ident,
    pub span: SourceSpan,
}

impl SpannedIdent {
    pub fn new(ident: impl AsRef<str>, span: SourceSpan) -> Self {
        Self {
            ident: Ident::new(ident),
            span,
        }
    }

    /// Clones the inner string.
    pub fn string(&self) -> String {
        self.ident.string()
    }

    /// Reference to the inner string as a slice.
    pub fn str(&self) -> &str {
        self.ident.str()
    }

    /// Clones `self.ident`.
    pub fn ident(&self) -> Ident {
        self.ident.clone()
    }
}

impl From<(String, SourceSpan)> for SpannedIdent {
    fn from(value: (String, SourceSpan)) -> Self {
        Self {
            ident: value.0.into(),
            span: value.1,
        }
    }
}

impl Debug for SpannedIdent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "SpannedIdent {{ {}, {}..{} }}",
            self.ident.str(),
            self.span.start,
            self.span.end
        )
    }
}

/// Describes a `Type`. This structure is `self-referential`, I.e. an array will be described as a
/// `Ty::Array(Ty::Type(Identifier))`.
#[derive(Clone, PartialEq, PartialOrd, Hash)]
pub enum Ty {
    /// The type is the unit '()' type. (void)
    Unit(SourceSpan),
    /// The type is not specified, and has to be inferred.
    Infer,
    /// `Self`.
    This(SourceSpan),
    /// A reference to a [`Ty`], with a description of if it is `mutable` or not.
    Ref(Box<Ty>, Mutability, SourceSpan),
    /// Just a plain type, no reference or anything else.
    Type(SpannedIdentPath),
    /// An array of a specified [`Ty`].
    Array(Box<Ty>, SourceSpan),
    /// A tuple of multiple [`Ty`]'s.
    Tuple(Vec<Box<Ty>>, SourceSpan),
}

impl Ty {
    /// Construct a [`Ty`] from a path.
    /// # Returns
    /// `Ty::Type(src)`
    #[inline]
    pub fn new(src: SpannedIdentPath) -> Self {
        Self::Type(src)
    }

    /// Returns the underlying type, if possible.
    /// I.e.
    /// *   `Ty::Ref(Ty::Type(i32), Mutability::Mutable)`, returns `Some("i32")`.
    /// *   `Ty::Infer`, returns `None`.
    /// *   `Ty::This`, returns `None`, since we need to have context about it to deduce the type.
    #[inline]
    pub fn get_type_ident(&self) -> Option<String> {
        match self {
            Ty::Unit(_) => Some("()".to_string()),
            Ty::Infer => None,
            Ty::This(_) => None,
            Ty::Ref(ty, _, _) => ty.get_type_ident(),
            Ty::Type(t) => Some(t.to_string()),
            Ty::Array(ty, _) => ty.get_type_ident(),
            Ty::Tuple(_, _) => None, // TODO: ?
        }
    }

    /// Returns the span of the type specifier if possible.
    #[inline]
    pub fn get_span(&self) -> Option<SourceSpan> {
        match self {
            Ty::Unit(s) => Some(*s),
            Ty::Infer => None,
            Ty::This(s) => Some(*s),
            Ty::Ref(_, _, s) => Some(*s),
            Ty::Type(t) => Some(t.span),
            Ty::Array(_, s) => Some(*s),
            Ty::Tuple(_, s) => Some(*s), // TODO: ?
        }
    }
}

impl Debug for Ty {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Ty::Unit(_) => write!(f, "Unit"),
            Ty::Infer => write!(f, "Infer"),
            Ty::This(_) => write!(f, "Self"),
            Ty::Ref(t, mutable, _) => {
                if mutable.is_mutable() {
                    write!(f, "MutRef({t:?})")
                } else {
                    write!(f, "Ref({t:?})")
                }
            }
            Ty::Array(t, _) => write!(f, "Array({t:?})"),
            Ty::Tuple(t, _) => {
                write!(f, "Tuple(")?;
                for (i, ty) in t.iter().enumerate() {
                    if i == 0 {
                        write!(f, "{ty:?}")?;
                    } else {
                        write!(f, ", {ty:?}")?;
                    }
                }
                write!(f, ")")
            }
            Ty::Type(t) => write!(f, "Type({})", t.path),
        }
    }
}

// TODO: Add a `SourceSpan` to everything.

/// A "top-level" item in the AST.
/// Functions, Constants, Structs, etc..
#[derive(Clone, PartialEq)]
pub struct Item {
    pub vis: Visibility,
    pub kind: ItemKind,
    pub span: SourceSpan,
}

impl Item {
    #[inline]
    pub fn new(kind: ItemKind, vis: Visibility, span: SourceSpan) -> Self {
        Self { vis, kind, span }
    }
}

impl Debug for Item {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Item")
            .field("vis", &self.vis)
            .field("kind", &self.kind)
            .finish()
    }
}

/// Describes the "order" or "precedence" of each type of expression.
/// This is implemented as an enum with an ordering derive, so that the enum can be compared with
/// itself to determine which of the expressions has higher precendence.
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
pub enum ExpressionOrder {
    /// `return`, `break`, etc..
    Jump,
    /// `=`, `+=`, `-=`, `*=`, `/=`, `%=`, `&=`, `|=`, `^=`, `>>=`, `<<=`, etc..
    Assign,
    /// `||`
    LogicalOr,
    /// `&&`
    LogicalAnd,
    /// `==`, `!=`, `<`, `>`, `<=`, `>=`
    Compare,
    /// `|`
    BitwiseOr,
    /// `^`
    BitwiseXor,
    /// `&`
    BitwiseAnd,
    /// `<<`, `>>`
    Shift,
    /// `+`, `-`
    Sum,
    /// `*`, `/`, `%`
    Product,
    /// as
    Cast,
    /// Unary operations. `-`, `*`, `!`, `&`, `&mut`
    Prefix,
    /// Loops, function calls, array indexing, field expressions, method calls, values, etc..
    Unambiguous,
}

/// Describes the association of an expression. I.e. If the left-hand side or the right-hand side
/// should be evaluated first.
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
    #[inline]
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

    #[inline]
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

/// Describes each possible kind of expressions.
#[derive(Clone, PartialEq)]
pub enum ExpressionKind {
    /// A "block" of code.
    /// # Kit Example
    /// ```ignore
    /// <Expression>;
    /// // Outer block..
    /// let x = 20;
    /// {
    ///     // Inside of block..
    ///     let y = 20;
    /// }
    /// ```
    Block(Box<Block>),
    /// A [`Literal`] value, of either a number, boolean or string type.
    /// # Kit Example
    /// `20`, `"Hello!"`, `true`.
    Literal(Literal),
    /// Binary operation between two expressions.
    /// # Notes
    /// The tuple values are: ([`BinaryOpKind`], lhs, rhs).
    /// # Kit Example
    /// `20 + 70`, `42 == 42`, `random_number() + 42`.
    BinaryOp(BinaryOpKind, Box<Expression>, Box<Expression>),
    /// A unary operation on an expression.
    /// # Kit Example
    /// `!variable`.
    UnaryOp(UnaryOpKind, Box<Expression>),
    /// If statement, with a block or "body", and an optional "else" case block.
    /// # Notes
    /// The tuple values are: (condition_expression, if_true_block, else_block).
    /// # Kit Example
    /// ```ignore
    /// if random_number() == 20 {
    ///     <Expression>;
    /// }
    /// ```
    If(Box<Expression>, Box<Block>, Option<Box<Expression>>),
    /// While loop, with a block or "body", and an optional "else" case block.
    /// # Notes
    /// The tuple values are: (condition_expression, loop_block).
    /// # Kit Example
    /// ```ignore
    /// while i > 0 {
    ///     i = i - 1
    /// }
    /// ```
    While(Box<Expression>, Box<Block>),
    /// Assignment of an already declared value.
    /// # Notes
    /// The tuple values are: (variable_to_be_assigned_to, new_value)
    /// # Kit Example
    /// `x = x + 1`
    Assign(Box<Expression>, Box<Expression>),
    /// Calling a free function, or using the call operator of an object.
    /// # Notes
    /// The tuple values are: (call_target, parameters)
    /// # Kit Example
    /// ```ignore
    /// random_number(0, 20)
    /// ```
    Call(Box<Expression>, Vec<Box<Expression>>),
    /// Call a method function of an object, or the 'dot' operator.
    /// # Kit Example
    /// ```ignore
    /// vec2.to_string()
    /// ```
    MethodCall(Box<MethodCall>),
    /// Index into an array or an object that implements the indexing operator.
    /// # Notes
    /// The tuple values are: (object_to_be_indexed, index)
    /// # Kit Example
    /// ```ignore
    /// list_of_names[2]
    /// ```
    Index(Box<Expression>, Box<Expression>),
    /// Access a data field on an object.
    /// # Notes
    /// The tuple values are: (object_to_access, field_to_access)
    /// # Kit Examples
    /// ```ignore
    /// vec2.x
    /// ```
    FieldAccess(Box<Expression>, Ident),
    /// Initialise a struct with specified field values.
    /// # Kit Example
    /// ```ignore
    /// Vec2 {
    ///     x: 10,
    ///     y: 20
    /// }
    /// ```
    StructInit(Box<StructInitialisation>),
    /// An `Identifier` or `Path`.
    /// # Kit Example
    /// `Module::Path::To::Function`, `x`.
    IdentPath(SpannedIdentPath),
    /// Skip the rest of the loop code and proceed to the next iteration early.
    Continue,
    /// Break out of the contained loop.
    Break,
    /// Return from the function, with an optional expression to use as the return value.
    Return(Option<Box<Expression>>),
    /// Cast an expression to another type.
    Cast(Box<Expression>, Ty),
}

impl ExpressionKind {
    /// Returns `true` if the expression is allowed to omit the trailing semi-colon without being
    /// the last statement in a block.
    #[inline]
    pub fn can_be_non_semi(&self) -> bool {
        matches!(
            self,
            ExpressionKind::Block(_) | ExpressionKind::If(_, _, _) | ExpressionKind::While(_, _)
        )
    }

    /// Get the "precendence" of the expression.
    #[inline]
    pub fn get_order(&self) -> ExpressionOrder {
        match self {
            ExpressionKind::BinaryOp(binary_op_kind, _, _) => binary_op_kind.get_order(),
            ExpressionKind::UnaryOp(_, _) => ExpressionOrder::Prefix,
            ExpressionKind::Assign(_, _) => ExpressionOrder::Assign,
            ExpressionKind::Continue | ExpressionKind::Return(_) | ExpressionKind::Break => {
                ExpressionOrder::Jump
            }
            _ => ExpressionOrder::Unambiguous,
        }
    }

    /// Get the "association" of the expression.
    #[inline]
    pub fn get_association(&self) -> ExpressionAssociation {
        match self {
            ExpressionKind::BinaryOp(binary_op_kind, _, _) => binary_op_kind.get_association(),
            ExpressionKind::UnaryOp(_, _) => ExpressionAssociation::None,
            ExpressionKind::Assign(_, _) => ExpressionAssociation::Right,
            _ => ExpressionAssociation::None,
        }
    }
}

impl Debug for ExpressionKind {
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
            Self::UnaryOp(arg0, arg1) => f.debug_tuple("Unary").field(arg0).field(arg1).finish(),
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
            Self::IdentPath(arg0) => Debug::fmt(&arg0, f),
            Self::Continue => write!(f, "Continue"),
            Self::Break => write!(f, "Break"),
            Self::Return(arg0) => f.debug_tuple("Return").field(arg0).finish(),
            Self::Cast(arg0, arg1) => f.debug_tuple("Cast").field(arg0).field(arg1).finish(),
        }
    }
}

/// An expression node in the AST.
#[derive(Clone, PartialEq)]
pub struct Expression {
    pub kind: ExpressionKind,
    pub span: SourceSpan,
}

impl Expression {
    #[inline]
    pub fn new(kind: ExpressionKind, span: SourceSpan) -> Self {
        Self { kind, span }
    }

    #[inline]
    pub fn new_boxed(kind: ExpressionKind, span: SourceSpan) -> Box<Self> {
        Box::new(Self::new(kind, span))
    }

    /// Get the "precendence" of the expression.
    #[inline]
    pub fn get_order(&self) -> ExpressionOrder {
        self.kind.get_order()
    }

    /// Get the "association" of the expression.
    #[inline]
    pub fn get_association(&self) -> ExpressionAssociation {
        self.kind.get_association()
    }
}

impl Debug for Expression {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.kind.fmt(f)
    }
}

/// Describes all possible kinds of statement in the AST.
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

impl Debug for StatementKind {
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

/// A statement node within the AST.
#[derive(Clone, PartialEq)]
pub struct Statement {
    pub kind: StatementKind,
    pub span: SourceSpan,
}

impl Statement {
    #[inline]
    pub fn new(kind: StatementKind, source_span: SourceSpan) -> Self {
        Self {
            kind,
            span: source_span,
        }
    }
}

impl Debug for Statement {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.kind.fmt(f)
    }
}

/// Describes a kind of literal, with it's associated parsed value.
#[derive(Clone, PartialEq, PartialOrd)]
pub enum Literal {
    String(String),
    Float(f64),
    Integer(i64),
    Boolean(bool),
}

impl Debug for Literal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::String(arg0) => write!(f, "String(\"{}\")", arg0),
            Self::Float(arg0) => write!(f, "Float({})", arg0),
            Self::Integer(arg0) => write!(f, "Integer({})", arg0),
            Self::Boolean(arg0) => write!(f, "Boolean({})", arg0),
        }
    }
}

/// Describes whether a local variable is just a declaration, or a declaration and an initial assignment.
#[derive(Debug, Clone, PartialEq)]
pub enum LocalKind {
    Declaration,
    Initialise(Box<Expression>),
}

/// A type of statement that declares a local variable, and optionally assigns it an
/// initial value.
#[derive(Clone, PartialEq)]
pub struct Local {
    pub ident: SpannedIdent,
    pub ty: Ty,
    pub kind: LocalKind,
    pub mutable: Mutability,
}

impl Local {
    #[inline]
    pub fn new(ident: SpannedIdent, ty: Ty, kind: LocalKind, mutable: Mutability) -> Self {
        Self {
            ident,
            ty,
            kind,
            mutable,
        }
    }

    #[inline]
    pub fn new_boxed(
        ident: SpannedIdent,
        ty: Ty,
        kind: LocalKind,
        mutable: Mutability,
    ) -> Box<Self> {
        Box::new(Self::new(ident, ty, kind, mutable))
    }

    #[inline]
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

impl Debug for Local {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.fmt_with_name(f, "Local")
    }
}

/// A declaration of a constant, with the expression it should be evaluated to.
#[derive(Clone, PartialEq)]
pub struct Constant {
    pub ident: SpannedIdent,
    pub ty: Ty,
    pub expr: Box<Expression>,
}

impl Constant {
    #[inline]
    pub fn new(ident: SpannedIdent, ty: Ty, expr: Box<Expression>) -> Self {
        Self { ident, ty, expr }
    }

    #[inline]
    pub fn new_boxed(ident: SpannedIdent, ty: Ty, expr: Box<Expression>) -> Box<Self> {
        Box::new(Self::new(ident, ty, expr))
    }
}

impl Debug for Constant {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Constant")
            .field("ident", &self.ident)
            .field("type", &self.ty)
            .field("expr", &self.expr)
            .finish()
    }
}

/// A declaration of a block, with a list of all statements inside the block.
#[derive(Clone, PartialEq)]
pub struct Block {
    pub statements: Vec<Statement>,
    pub span: SourceSpan,
}

impl Debug for Block {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_list().entries(&self.statements).finish()
    }
}

impl Block {
    #[inline]
    pub fn new(statements: Vec<Statement>, span: SourceSpan) -> Self {
        Self { statements, span }
    }
}

/// An individual parameter to a function, includes the name `Identifier`, the [`Ty`] of the
/// parameter, as well as if it is declared as `mutable` or not.
#[derive(Clone, PartialEq, PartialOrd)]
pub struct Parameter {
    pub ident: SpannedIdent,
    pub ty: Ty,
    pub mutable: Mutability,
    pub span: SourceSpan,
}

impl Parameter {
    #[inline]
    pub fn new(ident: SpannedIdent, ty: Ty, mutable: Mutability, span: SourceSpan) -> Self {
        Self {
            ident,
            ty,
            mutable,
            span,
        }
    }
}

impl Debug for Parameter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Parameter {{ name: {:?}, type: {:?}, mutable: {:?} }}",
            self.ident, self.ty, self.mutable
        )
    }
}

/// The return type of a function. If not specified this will default to
/// `FunctionReturnTy::Default` which is the unit type `()`, otherwise this holds the specified
/// return type.
#[derive(Debug, Clone, PartialEq, PartialOrd)]
pub enum FunctionReturnTy {
    /// No return type specified.
    Default,
    /// Specified return type.
    Ty(Box<Ty>),
}

/// The "signature" of a function, holds the return type and the parameters.
#[derive(Debug, Clone, PartialEq, PartialOrd)]
pub struct FunctionSig {
    pub parameters: Vec<Parameter>,
    pub output: FunctionReturnTy,
    pub span: SourceSpan,
}

/// A function [`Item`] declaration, holds information on the name of the function, all parameter
/// info and the return type, with an optional body to the function, this is `None` if the
/// function is just a declaration.
#[derive(Clone, PartialEq)]
pub struct Function {
    pub ident: SpannedIdent,
    pub native: bool,
    pub sig: FunctionSig,
    pub decl_span: SourceSpan,
    pub is_method: bool,
    pub is_global: bool,
    pub body: Option<Box<Block>>,
}

impl Function {
    #[inline]
    pub fn new(
        ident: SpannedIdent,
        native: bool,
        sig: FunctionSig,
        decl_span: SourceSpan,
        is_method: bool,
        is_global: bool,
        body: Option<Box<Block>>,
    ) -> Self {
        Self {
            ident,
            native,
            sig,
            decl_span,
            is_method,
            is_global,
            body,
        }
    }
}

impl Debug for Function {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Function")
            .field("ident", &self.ident.str())
            .field("sig", &self.sig)
            .field("method", &self.is_method)
            .field("body", &self.body)
            .finish()
    }
}

/// A field of a struct, containing the name of the field and it's associated [`Ty`].
#[derive(Clone, PartialEq)]
pub struct StructField {
    pub ident: SpannedIdent,
    pub ty: Ty,
    pub vis: Visibility,
    pub span: SourceSpan,
}

impl StructField {
    #[inline]
    pub fn new(ident: SpannedIdent, ty: Ty, vis: Visibility, span: SourceSpan) -> Self {
        Self {
            ident,
            ty,
            vis,
            span,
        }
    }
}

impl Debug for StructField {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StructField")
            .field("ident", &self.ident)
            .field("type", &self.ty)
            .field("vis", &self.vis)
            .finish()
    }
}

/// A `Struct` [`Item`] in the AST, with an `Identifier` for it's name, with all it's associated fields.
#[derive(Clone, PartialEq)]
pub struct Struct {
    pub ident: SpannedIdent,
    pub fields: Vec<StructField>,
}

impl Struct {
    #[inline]
    pub fn new(ident: SpannedIdent, fields: Vec<StructField>) -> Self {
        Self { ident, fields }
    }

    #[inline]
    pub fn new_boxed(ident: SpannedIdent, fields: Vec<StructField>) -> Box<Self> {
        Box::new(Self::new(ident, fields))
    }
}

impl Debug for Struct {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Struct")
            .field("ident", &self.ident)
            .field("fields", &self.fields)
            .finish()
    }
}

/// The kind of variant of an [`Enum`].
/// # Kit Example
/// ```ignore
/// enum Person {
///     Alive(u32),
///     Dead {
///          age_at_death: u32
///     },
///     None
/// }
/// ```
/// *   `Alive` is a `Tuple`-type enum variant.
/// *   `Dead` is a `Struct`-type enum variant.
/// *   `None` is a `unit`-type enum variant.
#[derive(Debug, Clone, PartialEq)]
pub enum VariantData {
    Struct(Vec<StructField>),
    Tuple(Vec<Ty>),
    Unit,
}

/// Describes one of the variants of an [`Enum`], includes the name/`Identifier` of the variant and
/// what kind of variant it is: `Tuple`, `Struct` or `Unit`.
#[derive(Clone, PartialEq)]
pub struct EnumVariant {
    pub ident: Ident,
    pub data: VariantData,
}

impl EnumVariant {
    #[inline]
    pub fn new(ident: Ident, data: VariantData) -> Self {
        Self { ident, data }
    }
}

impl Debug for EnumVariant {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EnumVariant")
            .field("ident", &self.ident)
            .field("data", &self.data)
            .finish()
    }
}

/// Enumeration [`Item`] in the AST, with a specified name `Identifier`, and all possible `variants`
/// of the [`Enum`].
#[derive(Clone, PartialEq)]
pub struct Enum {
    pub ident: SpannedIdent,
    pub variants: Vec<EnumVariant>,
}

impl Enum {
    #[inline]
    pub fn new(ident: SpannedIdent, variants: Vec<EnumVariant>) -> Self {
        Self { ident, variants }
    }

    #[inline]
    pub fn new_boxed(ident: SpannedIdent, variants: Vec<EnumVariant>) -> Box<Self> {
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

impl Debug for Enum {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Enum")
            .field("ident", &self.ident)
            .field("variants", &self.variants)
            .finish()
    }
}

/// Describes the possible kinds of a "module".
/// This can either just be the declaration of a module, or a module [`Item`] with a body.
#[derive(Debug, Clone, PartialEq)]
pub enum ModuleKind {
    Declaration,
    Definition(Vec<Item>),
}

/// A `Module` [`Item`] in the AST, contains the name/`Identifier` of the module, as well as an
/// optional body.
#[derive(Clone, PartialEq)]
pub struct Module {
    pub ident: SpannedIdent,
    pub kind: ModuleKind,
}

impl Module {
    #[inline]
    pub fn new(ident: SpannedIdent, kind: ModuleKind) -> Self {
        Self { ident, kind }
    }

    #[inline]
    pub fn new_boxed(ident: SpannedIdent, kind: ModuleKind) -> Box<Self> {
        Box::new(Self::new(ident, kind))
    }
}

impl Debug for Module {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Module")
            .field("ident", &self.ident)
            .field("kind", &self.kind)
            .finish()
    }
}

/// An `Implementation` block [`Item`] in the AST, contains for which [`Ty`] the `impl` block is
/// for, as well as a list of all [`Item`]'s contained within.
#[derive(Clone, PartialEq)]
pub struct Impl {
    pub target_path: SpannedIdentPath,
    pub lang_item: bool,
    pub items: Vec<Item>,
}

impl Impl {
    #[inline]
    pub fn new(target_path: SpannedIdentPath, lang_item: bool, items: Vec<Item>) -> Self {
        Self {
            target_path,
            lang_item,
            items,
        }
    }

    #[inline]
    pub fn new_boxed(
        target_path: SpannedIdentPath,
        lang_item: bool,
        items: Vec<Item>,
    ) -> Box<Self> {
        Box::new(Self::new(target_path, lang_item, items))
    }
}

impl Debug for Impl {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Impl")
            .field("target_ident", &self.target_path)
            .field("items", &self.items)
            .finish()
    }
}

/// Describes a method call expression, contains the target of the method call, the `Identifier`
/// specified which method to call, and the parameters to pass it.
#[derive(Clone, PartialEq)]
pub struct MethodCall {
    pub target_expr: Box<Expression>,
    pub method_ident: SpannedIdent,
    pub args: Vec<Box<Expression>>,
}

impl MethodCall {
    #[inline]
    pub fn new(
        target_expr: Box<Expression>,
        method_ident: SpannedIdent,
        args: Vec<Box<Expression>>,
    ) -> Self {
        Self {
            target_expr,
            method_ident,
            args,
        }
    }

    #[inline]
    pub fn new_boxed(
        target_expr: Box<Expression>,
        method_ident: SpannedIdent,
        args: Vec<Box<Expression>>,
    ) -> Box<Self> {
        Box::new(Self::new(target_expr, method_ident, args))
    }
}

impl Debug for MethodCall {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MethodCall")
            .field("target_expr", &self.target_expr)
            .field("method_ident", &self.method_ident)
            .field("args", &self.args)
            .finish()
    }
}

/// Describes the initialisation of a field within a [`StructInitialisation`] expression, contains
/// the name/`Identifier` of the field to initialise, and an expression that will be the value of
/// the field.
#[derive(Clone, PartialEq)]
pub struct FieldInitialisation {
    pub ident: Ident,
    pub expr: Box<Expression>,
    pub span: SourceSpan,
}

impl FieldInitialisation {
    #[inline]
    pub fn new(ident: Ident, expr: Box<Expression>, span: SourceSpan) -> Self {
        Self { ident, expr, span }
    }
}

impl Debug for FieldInitialisation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FieldInitialisation")
            .field("ident", &self.ident)
            .field("expr", &self.expr)
            .finish()
    }
}

/// Describes an expression that initialises an instance of a [`Struct`], contains the [`Ty`] of
/// [`Struct`] to initialise, as well as the values to initialise all of the fields to.
#[derive(Clone, PartialEq)]
pub struct StructInitialisation {
    pub path: SpannedIdentPath,
    pub fields: Vec<FieldInitialisation>,
}

impl StructInitialisation {
    #[inline]
    pub fn new(path: SpannedIdentPath, fields: Vec<FieldInitialisation>) -> Self {
        Self { path, fields }
    }

    #[inline]
    pub fn new_boxed(path: SpannedIdentPath, fields: Vec<FieldInitialisation>) -> Box<Self> {
        Box::new(Self::new(path, fields))
    }
}

impl Debug for StructInitialisation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StructInitialisation")
            .field("ident", &self.path)
            .field("fields", &self.fields)
            .finish()
    }
}

// TODO: Re-do this after redoing the path system.

/// Describes a `use` [`Item`] in the AST.
#[derive(Clone, PartialEq)]
pub struct UseImport {
    pub span: SourceSpan,
    /// Paths to import.
    pub imports: Vec<IdentPath>,
}

impl UseImport {
    #[inline]
    pub fn new(span: SourceSpan, imports: Vec<IdentPath>) -> Self {
        Self { span, imports }
    }
}

impl Debug for UseImport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("UseImport")
            .field("imports", &self.imports)
            .finish()
    }
}
