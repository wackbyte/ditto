use serde::{Deserialize, Serialize};

/// A source span.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct Span {
    /// The start byte offset.
    pub start_offset: usize,
    /// The end byte offset.
    pub end_offset: usize,
}

impl Span {
    /// Merge two spans, returning a new [Span] spanning `self` and `other`.
    #[must_use]
    pub fn merge(&self, other: &Self) -> Self {
        Self {
            start_offset: self.start_offset.min(other.start_offset),
            end_offset: self.end_offset.max(other.end_offset),
        }
    }
}

/// A syntactic element.
///
/// Each token consists of its source location, surrounding comments, and
/// the token value itself.
///
/// ```ditto
/// -- leading comment
/// -- leading comment
/// token -- trailing comment
/// ```
#[derive(Debug, Clone)]
pub struct Token<Value> {
    /// The source location of this token.
    pub span: Span,
    /// Optional leading comments (zero or more).
    pub leading_comments: Vec<Comment>,
    /// Optional trailing comment (zero or one).
    pub trailing_comment: Option<Comment>,
    /// The actual token value.
    pub value: Value,
}

impl<Value> Token<Value> {
    /// Does this token have any comments?
    pub fn has_comments(&self) -> bool {
        self.has_leading_comments() || self.has_trailing_comment()
    }
    /// Does this token have any leading comments?
    pub fn has_leading_comments(&self) -> bool {
        !self.leading_comments.is_empty()
    }
    /// Does this token have a trailing comment?
    pub fn has_trailing_comment(&self) -> bool {
        self.trailing_comment.is_some()
    }
    /// Drop the value associated with this [Token].
    pub fn to_empty(&self) -> EmptyToken {
        EmptyToken {
            span: self.span,
            leading_comments: self.leading_comments.clone(),
            trailing_comment: self.trailing_comment.clone(),
            value: (),
        }
    }
}

/// A string token prefixed with `"--"`.
#[derive(Debug, Clone, PartialEq)]
pub struct Comment(pub String);

/// A [String] syntax node.
pub type StringToken = Token<String>;

/// An empty syntax node.
///
/// Empty because the contents are implied, and in the interest of efficieny.
///
/// Use cases include single characters (`=`, `:`) and keywords (`import`, `type`).
pub type EmptyToken = Token<()>;

/// `.`
#[derive(Debug, Clone)]
pub struct Dot(pub EmptyToken);

/// `..`
#[derive(Debug, Clone)]
pub struct DoubleDot(pub EmptyToken);

/// `,`
#[derive(Debug, Clone)]
pub struct Comma(pub EmptyToken);

/// `:`
#[derive(Debug, Clone)]
pub struct Colon(pub EmptyToken);

/// `;`
#[derive(Debug, Clone)]
pub struct Semicolon(pub EmptyToken);

/// `=`
#[derive(Debug, Clone)]
pub struct Equals(pub EmptyToken);

/// `(`
#[derive(Debug, Clone)]
pub struct OpenParen(pub EmptyToken);

/// `)`
#[derive(Debug, Clone)]
pub struct CloseParen(pub EmptyToken);

/// `[`
#[derive(Debug, Clone)]
pub struct OpenBracket(pub EmptyToken);

/// `]`
#[derive(Debug, Clone)]
pub struct CloseBracket(pub EmptyToken);

/// `<-`
#[derive(Debug, Clone)]
pub struct LeftArrow(pub EmptyToken);

/// `->`
#[derive(Debug, Clone)]
pub struct RightArrow(pub EmptyToken);

/// `|`
#[derive(Debug, Clone)]
pub struct Pipe(pub EmptyToken);

/// `module`
#[derive(Debug, Clone)]
pub struct ModuleKeyword(pub EmptyToken);

/// `exports`
#[derive(Debug, Clone)]
pub struct ExportsKeyword(pub EmptyToken);

/// `import`
#[derive(Debug, Clone)]
pub struct ImportKeyword(pub EmptyToken);

/// `as`
#[derive(Debug, Clone)]
pub struct AsKeyword(pub EmptyToken);

/// `true`
#[derive(Debug, Clone)]
pub struct TrueKeyword(pub EmptyToken);

/// `false`
#[derive(Debug, Clone)]
pub struct FalseKeyword(pub EmptyToken);

/// `unit`
#[derive(Debug, Clone)]
pub struct UnitKeyword(pub EmptyToken);

/// `if`
#[derive(Debug, Clone)]
pub struct IfKeyword(pub EmptyToken);

/// `then`
#[derive(Debug, Clone)]
pub struct ThenKeyword(pub EmptyToken);

/// `else`
#[derive(Debug, Clone)]
pub struct ElseKeyword(pub EmptyToken);

/// `type`
#[derive(Debug, Clone)]
pub struct TypeKeyword(pub EmptyToken);

/// `foreign`
#[derive(Debug, Clone)]
pub struct ForeignKeyword(pub EmptyToken);
