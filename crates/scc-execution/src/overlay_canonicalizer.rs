use crate::overlay_view::ConstraintIrWithOverlay;
use crate::runtime::runtime_error::RuntimeError;
use serde::Serialize;

pub struct OverlayCanonicalizer;

fn serialize_canonical_part<T: Serialize + ?Sized>(
    value: &T,
    target: &'static str,
) -> Result<Vec<u8>, RuntimeError> {
    serde_json::to_vec(value).map_err(|error| RuntimeError::CanonicalizationFailed {
        target,
        message: error.to_string(),
    })
}

impl OverlayCanonicalizer {
    /// Returns a deterministic, stable 64-bit identifier for the patched IR view.
    ///
    /// The canonicalizer serializes the base IR and the overlay patch to JSON and
    /// hashes the combined bytes with a deterministic FNV-1a hash, so the same input
    /// always produces the same root ID and execution traces remain reproducible.
    pub fn canonicalize(view: &ConstraintIrWithOverlay) -> Result<u64, RuntimeError> {
        fn fnv1a(data: &[u8]) -> u64 {
            const OFFSET_BASIS: u64 = 0xcbf29ce484222325;
            const FNV_PRIME: u64 = 0x100000001b3;
            let mut hash = OFFSET_BASIS;
            for byte in data {
                hash ^= *byte as u64;
                hash = hash.wrapping_mul(FNV_PRIME);
            }
            hash
        }

        let mut ir_bytes = serialize_canonical_part(view.base_ir, "base_ir")?;
        ir_bytes.extend_from_slice(b"\0OVERLAY\0");
        ir_bytes.extend_from_slice(&serialize_canonical_part(view.overlay, "overlay")?);
        Ok(fnv1a(&ir_bytes))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::overlay_view::MutationOp;
    use scc_core::constraint_ir::{ConstraintIr, IrConstraintLevels, IrMetadata};
    use scc_core::entity::{Entity, EntityKind};
    use scc_core::graph::DeclarationGraph;
    use scc_core::ids::EntityId;
    use serde::Serializer;

    fn minimal_ir() -> ConstraintIr {
        ConstraintIr {
            entities: vec![Entity {
                id: EntityId("entity".into()),
                kind: EntityKind::Circle,
            }],
            constraints: vec![],
            graph: DeclarationGraph {
                nodes: vec![],
                edges: vec![],
            },
            metadata: IrMetadata {
                constraint_levels: IrConstraintLevels::default(),
            },
        }
    }

    #[test]
    fn canonicalization_is_deterministic() {
        let ir = minimal_ir();
        let overlay = vec![MutationOp::UpdateParameter {
            target_id: "entity".into(),
            parameter: "radius".into(),
            value: 5.0,
        }];
        let view = ConstraintIrWithOverlay {
            base_ir: &ir,
            overlay: &overlay,
        };

        assert_eq!(
            OverlayCanonicalizer::canonicalize(&view).unwrap(),
            OverlayCanonicalizer::canonicalize(&view).unwrap()
        );
    }

    #[test]
    fn different_overlays_produce_different_roots() {
        let ir = minimal_ir();
        let first = vec![MutationOp::UpdateParameter {
            target_id: "entity".into(),
            parameter: "radius".into(),
            value: 5.0,
        }];
        let second = vec![MutationOp::UpdateParameter {
            target_id: "entity".into(),
            parameter: "radius".into(),
            value: 6.0,
        }];

        let first_root = OverlayCanonicalizer::canonicalize(&ConstraintIrWithOverlay {
            base_ir: &ir,
            overlay: &first,
        })
        .unwrap();
        let second_root = OverlayCanonicalizer::canonicalize(&ConstraintIrWithOverlay {
            base_ir: &ir,
            overlay: &second,
        })
        .unwrap();

        assert_ne!(first_root, second_root);
    }

    struct FailingSerialize;

    impl Serialize for FailingSerialize {
        fn serialize<S>(&self, _serializer: S) -> Result<S::Ok, S::Error>
        where
            S: Serializer,
        {
            Err(serde::ser::Error::custom(
                "intentional canonicalization failure",
            ))
        }
    }

    #[test]
    fn serialization_failure_is_not_silently_hashed() {
        let error = serialize_canonical_part(&FailingSerialize, "overlay").unwrap_err();
        match error {
            RuntimeError::CanonicalizationFailed { target, message } => {
                assert_eq!(target, "overlay");
                assert!(message.contains("intentional canonicalization failure"));
            }
            other => panic!("expected canonicalization failure, got {other:?}"),
        }
    }
}
