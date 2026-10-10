use scc_core::constraint_ir::ConstraintIr;

use crate::overlay_view::{ConstraintIrWithOverlay, MutationOp};

pub struct IrPatcher;

impl IrPatcher {
    pub fn patch<'a>(
        base_ir: &'a ConstraintIr,
        overlay: &'a [MutationOp],
    ) -> ConstraintIrWithOverlay<'a> {
        ConstraintIrWithOverlay { base_ir, overlay }
    }
}
