use crate::midend::type_check::BuiltinType;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LiteralDefaultKind {
    Integer,
    Float,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InferenceDefaults {
    pub integer_default: BuiltinType,
    pub float_default: BuiltinType,
}

impl Default for InferenceDefaults {
    fn default() -> Self {
        Self {
            integer_default: BuiltinType::I32,
            float_default: BuiltinType::F64,
        }
    }
}
