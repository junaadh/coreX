use crate::frontend::resolver::ItemId;
use std::fmt::{self, Display, Formatter};

/// Canonical builtin semantic types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum BuiltinType {
    Bool,
    Char,
    String,
    I8,
    I16,
    I32,
    I64,
    U8,
    U16,
    U32,
    U64,
    ISize,
    USize,
    F32,
    F64,
    Void,
    Never,
}

/// Canonical mutability surface used by semantic types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Mutability {
    Const,
    Mut,
}

/// Canonical kind for item-backed nominal types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum NamedTypeKind {
    Struct,
    Enum,
    Protocol,
}

/// Canonical semantic type model used by later analysis phases.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Type {
    Builtin(BuiltinType),
    Named {
        item_id: ItemId,
        kind: NamedTypeKind,
    },
    Pointer {
        pointee: Box<Type>,
        mutability: Mutability,
    },
    Error,
}

impl Type {
    #[must_use]
    pub const fn builtin(builtin: BuiltinType) -> Self {
        Self::Builtin(builtin)
    }

    #[must_use]
    pub const fn named(item_id: ItemId, kind: NamedTypeKind) -> Self {
        Self::Named { item_id, kind }
    }

    #[must_use]
    pub fn pointer(pointee: Type, mutability: Mutability) -> Self {
        Self::Pointer {
            pointee: Box::new(pointee),
            mutability,
        }
    }

    #[must_use]
    pub const fn void() -> Self {
        Self::Builtin(BuiltinType::Void)
    }

    #[must_use]
    pub const fn error() -> Self {
        Self::Error
    }

    #[must_use]
    pub const fn is_error(&self) -> bool {
        matches!(self, Self::Error)
    }
}

impl Display for BuiltinType {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let text = match self {
            Self::Bool => "bool",
            Self::Char => "char",
            Self::String => "string",
            Self::I8 => "i8",
            Self::I16 => "i16",
            Self::I32 => "i32",
            Self::I64 => "i64",
            Self::U8 => "u8",
            Self::U16 => "u16",
            Self::U32 => "u32",
            Self::U64 => "u64",
            Self::ISize => "isize",
            Self::USize => "usize",
            Self::F32 => "f32",
            Self::F64 => "f64",
            Self::Void => "void",
            Self::Never => "never",
        };
        f.write_str(text)
    }
}

impl Display for Mutability {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::Const => f.write_str("const"),
            Self::Mut => f.write_str("mut"),
        }
    }
}

impl Display for Type {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::Builtin(builtin) => Display::fmt(builtin, f),
            Self::Named { item_id, kind } => write!(
                f,
                "{}#{}",
                match kind {
                    NamedTypeKind::Struct => "struct",
                    NamedTypeKind::Enum => "enum",
                    NamedTypeKind::Protocol => "protocol",
                },
                item_id.raw(),
            ),
            Self::Pointer {
                pointee,
                mutability,
            } => match mutability {
                Mutability::Const => write!(f, "*{pointee}"),
                Mutability::Mut => write!(f, "*mut {pointee}"),
            },
            Self::Error => f.write_str("<error>"),
        }
    }
}
