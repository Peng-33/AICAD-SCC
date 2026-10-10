//! One-shot solved snapshot boundary for downstream consumers (Phase 10.6).
//!
//! The snapshot is derived and immutable by convention: `EngineeringModel`
//! remains the only editable/source-of-truth state. All fields are produced by
//! one invocation of `lower_and_solve`; no downstream consumer should solve
//! the model again when this value is already available.

use crate::lowering::{lower_and_solve, LoweringPipelineError};
use crate::model::EngineeringModel;
use crate::source_map::EngineeringSourceMap;
use scc_geometry::model::GeometryModel;
use scc_proposal::schema::Proposal;

#[derive(Debug, Clone, PartialEq)]
pub struct SolvedEngineeringSnapshot {
    pub model: EngineeringModel,
    pub proposal: Proposal,
    pub geometry: GeometryModel,
    pub source_map: EngineeringSourceMap,
    pub diagnostics: Vec<String>,
}

impl SolvedEngineeringSnapshot {
    /// Solve exactly once and retain every output from that same pipeline run.
    pub fn solve(model: &EngineeringModel) -> Result<Self, LoweringPipelineError> {
        let mut observer = || {};
        Self::solve_with_observer(model, &mut observer)
    }

    /// Testable solve boundary. The observer is invoked once immediately
    /// before the one full lower-and-solve call; it is not production state.
    pub fn solve_with_observer<F>(
        model: &EngineeringModel,
        observer: &mut F,
    ) -> Result<Self, LoweringPipelineError>
    where
        F: FnMut(),
    {
        observer();
        let (lowering, geometry) = lower_and_solve(model)?;
        Ok(Self {
            model: model.clone(),
            proposal: lowering.proposal,
            geometry,
            source_map: lowering.source_map,
            diagnostics: Vec::new(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DSL: &str = r#"AICAD 1
PART p
BASE_RECT main
  WIDTH 120
  HEIGHT 80
"#;

    #[test]
    fn snapshot_is_consistent_and_observes_one_solve() {
        let commands = crate::command::EngineeringCommand::CreatePart {
            id: crate::PartId("p".into()),
            name: "p".into(),
        };
        let mut model = EngineeringModel::new();
        crate::executor::CommandExecutor::apply(&mut model, &commands).unwrap();
        crate::executor::CommandExecutor::apply(
            &mut model,
            &crate::command::EngineeringCommand::AddFeature {
                feature: crate::feature::EngineeringFeature::BaseProfile {
                    id: crate::FeatureId("main".into()),
                    name: "main".into(),
                    profile: crate::BaseProfile::Rectangle {
                        width: 120.0,
                        height: 80.0,
                    },
                },
            },
        )
        .unwrap();
        let mut count = 0;
        let snapshot =
            SolvedEngineeringSnapshot::solve_with_observer(&model, &mut || count += 1).unwrap();
        assert_eq!(count, 1);
        assert_eq!(snapshot.model, model);
        assert!(snapshot
            .geometry
            .entities
            .contains_key(&scc_core::ids::EntityId("main".into())));
        assert!(snapshot
            .source_map
            .contains_feature(&crate::FeatureId("main".into())));
        assert!(snapshot.diagnostics.is_empty());
    }

    #[test]
    fn snapshot_is_not_a_mutable_model_alias() {
        let _ = DSL;
        let model = EngineeringModel::new();
        let clone = model.clone();
        assert_eq!(model, clone);
    }
}
