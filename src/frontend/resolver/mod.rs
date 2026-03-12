mod error;
mod model;
mod scope_resolver;

pub use error::ResolveError;
pub use model::{ResolvedScope, ResolvedScopeKind, ScopeGraph};
pub use scope_resolver::{ScopeResolver, resolve_project_scopes};
