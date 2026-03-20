//! Canonical call signature model for function-like items.
//!
//! This module provides a view of function signatures that includes external label
//! information needed for overload resolution. Call signatures distinguish functions
//! by their parameter labels, enabling later phases to resolve calls based on
//! label matching.

use crate::frontend::hir::{
    HirFunction, HirFunctionParam, HirInitOrigin, HirParamLabel,
};
use std::fmt;

/// External label form for a call-site parameter.
///
/// This mirrors `HirParamLabel` but is specifically for call-site matching
/// rather than HIR representation.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum CallParamLabel {
    /// No external label accepted (`_ x: T`)
    None,
    /// Explicit external label required (`label x: T`)
    Explicit(String),
    /// External label derived from parameter name (`x: T`)
    FromName,
}

impl CallParamLabel {
    /// Convert from HIR param label.
    fn from_hir_param_label(hir_label: &HirParamLabel) -> Self {
        match hir_label {
            HirParamLabel::None => CallParamLabel::None,
            HirParamLabel::Explicit(label) => {
                CallParamLabel::Explicit(label.clone())
            }
            HirParamLabel::FromName => CallParamLabel::FromName,
        }
    }

    /// Get the effective label string for call-site matching.
    ///
    /// - `None` returns `None` (no label accepted)
    /// - `Explicit(name)` returns `Some(name)`
    /// - `FromName` returns `None` (label is derived from arg at call-site)
    pub fn effective_label(&self) -> Option<&str> {
        match self {
            CallParamLabel::None => None,
            CallParamLabel::Explicit(label) => Some(label),
            CallParamLabel::FromName => None,
        }
    }

    /// Whether this parameter requires an explicit label at the call site.
    pub fn requires_label(&self) -> bool {
        matches!(self, CallParamLabel::Explicit(_))
    }
}

/// A single parameter in a call signature.
#[derive(Debug, Clone)]
pub struct CallParam {
    /// External label for this parameter
    pub label: CallParamLabel,
    /// Internal parameter name (for FromName resolution)
    pub internal_name: String,
}

impl PartialEq for CallParam {
    fn eq(&self, other: &Self) -> bool {
        self.label == other.label
        // Internal names don't matter for call-site signature matching
    }
}

impl Eq for CallParam {}

impl CallParam {
    /// Create a CallParam from HIR function parameter.
    pub fn from_hir_param(hir_param: &HirFunctionParam) -> Self {
        CallParam {
            label: CallParamLabel::from_hir_param_label(
                &hir_param.external_label,
            ),
            internal_name: hir_param.name.clone(),
        }
    }
}

/// Canonical call signature for function-like items.
///
/// This signature captures all information needed to distinguish functions
/// for overload resolution based on parameter labels and count.
///
/// Two signatures are equal if they have the same:
/// - parameter count
/// - external labels for each parameter (internal names don't matter)
/// - init origin
#[derive(Debug, Clone)]
pub struct CallSignature {
    /// Parameters in order
    pub params: Vec<CallParam>,
    /// Init origin, if this is an init function
    pub init_origin: Option<HirInitOrigin>,
}

impl PartialEq for CallSignature {
    fn eq(&self, other: &Self) -> bool {
        self.params == other.params && self.init_origin == other.init_origin
    }
}

impl Eq for CallSignature {}

impl CallSignature {
    /// Create a call signature from a HIR function.
    pub fn from_hir_function(function: &HirFunction) -> Self {
        CallSignature {
            params: function
                .signature
                .params
                .iter()
                .map(CallParam::from_hir_param)
                .collect(),
            init_origin: function.init_origin,
        }
    }

    /// Get the parameter count.
    pub fn param_count(&self) -> usize {
        self.params.len()
    }

    /// Get the sequence of external labels for parameters.
    ///
    /// This returns the effective label for each parameter:
    /// - `None` for `_ x: T` (no label)
    /// - `Some("label")` for `label x: T` (explicit label)
    /// - `None` for `x: T` (FromName - label comes from argument)
    pub fn external_labels(&self) -> Vec<Option<String>> {
        self.params
            .iter()
            .map(|p| p.label.effective_label().map(String::from))
            .collect()
    }

    /// Whether this signature accepts labeled arguments at the given position.
    ///
    /// Returns `true` only if the parameter has an `Explicit` label.
    pub fn accepts_label_at(&self, position: usize) -> bool {
        self.params
            .get(position)
            .map(|p| p.label.requires_label())
            .unwrap_or(false)
    }
}

impl fmt::Display for CallSignature {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "(")?;
        for (i, param) in self.params.iter().enumerate() {
            if i > 0 {
                write!(f, ", ")?;
            }
            match &param.label {
                CallParamLabel::None => write!(f, "_ {}", param.internal_name),
                CallParamLabel::Explicit(label) => {
                    write!(f, "{} {}", label, param.internal_name)
                }
                CallParamLabel::FromName => {
                    write!(f, "{}", param.internal_name)
                }
            }?;
        }
        write!(f, ")")?;
        if let Some(init_origin) = &self.init_origin {
            write!(f, " [init: {:?}]", init_origin)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frontend::hir::{HirFunctionSignature, HirTypeId};

    fn make_hir_param(
        external_label: HirParamLabel,
        name: &str,
        ty_id: HirTypeId,
    ) -> HirFunctionParam {
        HirFunctionParam {
            external_label,
            name: name.to_string(),
            ty: ty_id,
        }
    }

    fn make_test_ty_id() -> HirTypeId {
        HirTypeId::new(0)
    }

    #[test]
    fn test_signature_foo_x_int() {
        // fn foo(x: I32)
        let params = vec![make_hir_param(
            HirParamLabel::FromName,
            "x",
            make_test_ty_id(),
        )];
        let signature = HirFunctionSignature {
            generic_params: vec![],
            params,
            return_type: None,
        };

        let hir_func = HirFunction {
            name: "foo".to_string(),
            init_origin: None,
            signature,
            body: crate::frontend::hir::HirBodyId::new(0),
        };

        let call_sig = CallSignature::from_hir_function(&hir_func);

        assert_eq!(call_sig.param_count(), 1);
        assert_eq!(call_sig.external_labels(), vec![None]);
        assert!(!call_sig.accepts_label_at(0));
    }

    #[test]
    fn test_signature_foo_y_int() {
        // fn foo(y: I32)
        let params = vec![make_hir_param(
            HirParamLabel::FromName,
            "y",
            make_test_ty_id(),
        )];
        let signature = HirFunctionSignature {
            generic_params: vec![],
            params,
            return_type: None,
        };

        let hir_func = HirFunction {
            name: "foo".to_string(),
            init_origin: None,
            signature,
            body: crate::frontend::hir::HirBodyId::new(0),
        };

        let call_sig = CallSignature::from_hir_function(&hir_func);

        assert_eq!(call_sig.param_count(), 1);
        assert_eq!(call_sig.external_labels(), vec![None]);
        assert!(!call_sig.accepts_label_at(0));
    }

    #[test]
    fn test_signature_foo_underscore_x_int() {
        // fn foo(_ x: I32)
        let params =
            vec![make_hir_param(HirParamLabel::None, "x", make_test_ty_id())];
        let signature = HirFunctionSignature {
            generic_params: vec![],
            params,
            return_type: None,
        };

        let hir_func = HirFunction {
            name: "foo".to_string(),
            init_origin: None,
            signature,
            body: crate::frontend::hir::HirBodyId::new(0),
        };

        let call_sig = CallSignature::from_hir_function(&hir_func);

        assert_eq!(call_sig.param_count(), 1);
        assert_eq!(call_sig.external_labels(), vec![None]);
        assert!(!call_sig.accepts_label_at(0));
    }

    #[test]
    fn test_signatures_differ_by_internal_name() {
        // fn foo(x: I32) vs fn foo(y: I32)
        // These have the same call signature since internal names don't matter
        let params_x = vec![make_hir_param(
            HirParamLabel::FromName,
            "x",
            make_test_ty_id(),
        )];
        let signature_x = HirFunctionSignature {
            generic_params: vec![],
            params: params_x,
            return_type: None,
        };

        let params_y = vec![make_hir_param(
            HirParamLabel::FromName,
            "y",
            make_test_ty_id(),
        )];
        let signature_y = HirFunctionSignature {
            generic_params: vec![],
            params: params_y,
            return_type: None,
        };

        let hir_func_x = HirFunction {
            name: "foo".to_string(),
            init_origin: None,
            signature: signature_x,
            body: crate::frontend::hir::HirBodyId::new(0),
        };

        let hir_func_y = HirFunction {
            name: "foo".to_string(),
            init_origin: None,
            signature: signature_y,
            body: crate::frontend::hir::HirBodyId::new(1),
        };

        let call_sig_x = CallSignature::from_hir_function(&hir_func_x);
        let call_sig_y = CallSignature::from_hir_function(&hir_func_y);

        // Both have FromName, so signatures are the same
        assert_eq!(call_sig_x, call_sig_y);
    }

    #[test]
    fn test_signature_none_vs_from_name_differ() {
        // fn foo(_ x: I32) vs fn foo(x: I32)
        // These have DIFFERENT call signatures
        let params_none =
            vec![make_hir_param(HirParamLabel::None, "x", make_test_ty_id())];
        let signature_none = HirFunctionSignature {
            generic_params: vec![],
            params: params_none,
            return_type: None,
        };

        let params_from_name = vec![make_hir_param(
            HirParamLabel::FromName,
            "x",
            make_test_ty_id(),
        )];
        let signature_from_name = HirFunctionSignature {
            generic_params: vec![],
            params: params_from_name,
            return_type: None,
        };

        let hir_func_none = HirFunction {
            name: "foo".to_string(),
            init_origin: None,
            signature: signature_none,
            body: crate::frontend::hir::HirBodyId::new(0),
        };

        let hir_func_from_name = HirFunction {
            name: "foo".to_string(),
            init_origin: None,
            signature: signature_from_name,
            body: crate::frontend::hir::HirBodyId::new(1),
        };

        let call_sig_none = CallSignature::from_hir_function(&hir_func_none);
        let call_sig_from_name =
            CallSignature::from_hir_function(&hir_func_from_name);

        // Signatures differ: None vs FromName
        assert_ne!(call_sig_none, call_sig_from_name);
        assert_eq!(call_sig_none.params[0].label, CallParamLabel::None);
        assert_eq!(
            call_sig_from_name.params[0].label,
            CallParamLabel::FromName
        );
    }

    #[test]
    fn test_signature_explicit_label() {
        // fn foo(label x: I32)
        let params = vec![make_hir_param(
            HirParamLabel::Explicit("label".to_string()),
            "x",
            make_test_ty_id(),
        )];
        let signature = HirFunctionSignature {
            generic_params: vec![],
            params,
            return_type: None,
        };

        let hir_func = HirFunction {
            name: "foo".to_string(),
            init_origin: None,
            signature,
            body: crate::frontend::hir::HirBodyId::new(0),
        };

        let call_sig = CallSignature::from_hir_function(&hir_func);

        assert_eq!(call_sig.param_count(), 1);
        assert_eq!(call_sig.external_labels(), vec![Some("label".to_string())]);
        assert!(call_sig.accepts_label_at(0));
        assert_eq!(call_sig.params[0].internal_name, "x");
    }

    #[test]
    fn test_signature_display() {
        // fn foo(_ x: I32, y: I32, label z: I32)
        let params = vec![
            make_hir_param(HirParamLabel::None, "x", make_test_ty_id()),
            make_hir_param(HirParamLabel::FromName, "y", make_test_ty_id()),
            make_hir_param(
                HirParamLabel::Explicit("label".to_string()),
                "z",
                make_test_ty_id(),
            ),
        ];
        let signature = HirFunctionSignature {
            generic_params: vec![],
            params,
            return_type: None,
        };

        let hir_func = HirFunction {
            name: "foo".to_string(),
            init_origin: None,
            signature,
            body: crate::frontend::hir::HirBodyId::new(0),
        };

        let call_sig = CallSignature::from_hir_function(&hir_func);

        assert_eq!(format!("{}", call_sig), "(_ x, y, label z)");
    }
}
