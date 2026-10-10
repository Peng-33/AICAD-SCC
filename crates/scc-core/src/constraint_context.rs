use serde::{Deserialize, Serialize};

use crate::authority::{AuthorityLevel, ConstraintOrigin};
use crate::provenance::DerivationChain;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ConstraintContext {
    pub origin: ConstraintOrigin,
    pub authority: AuthorityLevel,
    pub derivation: DerivationChain,
    pub version_hint: Option<String>,
}
