//! The language-neutral domain model the printers print. Every choice (a name, a type, an
//! obligation) is made here once, so the three languages agree.

/// A column's value type before a printer names it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scalar {
    Int,
    String,
    Bool,
    Money,
    Decimal,
    Date,
    DateTime,
}

impl Scalar {
    /// The scalars whose representation a project picks: printed in the prelude.
    pub const PRELUDE: [Scalar; 4] = [
        Scalar::Money,
        Scalar::Decimal,
        Scalar::Date,
        Scalar::DateTime,
    ];

    /// The prelude type name.
    pub fn name(self) -> &'static str {
        match self {
            Scalar::Int => "Int",
            Scalar::String => "String",
            Scalar::Bool => "Bool",
            Scalar::Money => "Money",
            Scalar::Decimal => "Decimal",
            Scalar::Date => "Date",
            Scalar::DateTime => "DateTime",
        }
    }

    /// What a prelude scalar holds by default, for its doc comment.
    pub fn default_doc(self) -> &'static str {
        match self {
            Scalar::Money => "A monetary amount as decimal text, such as \"1200.50\".",
            Scalar::Decimal => "A decimal number as text, such as \"0.3\".",
            Scalar::Date => "A date in ISO 8601, such as \"2026-11-07\".",
            Scalar::DateTime => {
                "A date and time in ISO 8601 with offset, such as \"2026-11-07T09:00:00+09:00\"."
            }
            Scalar::Int | Scalar::String | Scalar::Bool => "",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FieldType {
    Scalar(Scalar),
    /// A generated enum, by its type name.
    Enum(String),
    /// A generated id type, by its type name.
    Id(String),
}

#[derive(Clone, Debug)]
pub struct EnumValue {
    /// The value as the model and the wire spell it.
    pub wire: String,
    /// The value as an identifier: PascalCase.
    pub ident: String,
    pub label: Option<String>,
}

#[derive(Clone, Debug)]
pub struct EnumDecl {
    pub name: String,
    pub doc: Vec<String>,
    pub values: Vec<EnumValue>,
}

/// The id of an entity with a one-column primary key.
#[derive(Clone, Debug)]
pub struct IdDecl {
    pub name: String,
    pub entity: String,
    pub repr: Scalar,
}

#[derive(Clone, Debug)]
pub struct Field {
    /// The column name, as the model and the wire spell it.
    pub wire: String,
    /// The column name as an exported identifier: PascalCase.
    pub pascal: String,
    pub ty: FieldType,
    pub nullable: bool,
    pub doc: Vec<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RecordKind {
    Entity,
    Dto,
}

#[derive(Clone, Debug)]
pub struct RecordDecl {
    pub name: String,
    pub kind: RecordKind,
    pub doc: Vec<String>,
    pub fields: Vec<Field>,
}

/// What the verification assumed and a type cannot enforce: the code that finishes the
/// output must keep it.
#[derive(Clone, Debug)]
pub struct Obligation {
    /// `O1`, `O2`, ...
    pub id: String,
    /// The invariant `check` and `verify` name it by, when they check it.
    pub invariant: Option<String>,
    pub text: String,
    pub site: Option<String>,
}

#[derive(Clone, Debug, Default)]
pub struct DomainModel {
    pub enums: Vec<EnumDecl>,
    pub ids: Vec<IdDecl>,
    pub records: Vec<RecordDecl>,
    pub obligations: Vec<Obligation>,
}
