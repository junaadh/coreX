use super::types::NativeType;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Signature {
    params: Vec<NativeType>,
    ret: NativeType,
}

impl Signature {
    #[must_use]
    pub fn new(params: Vec<NativeType>, ret: NativeType) -> Self {
        Self { params, ret }
    }

    #[must_use]
    pub fn params(&self) -> &[NativeType] {
        &self.params
    }

    #[must_use]
    pub fn ret(&self) -> NativeType {
        self.ret
    }
}
