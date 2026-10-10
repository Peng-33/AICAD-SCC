use crate::schema::Proposal;
use scc_core::error::{ErrorStage, SccError, StructuredError};
use scc_core::ids::EntityId;
use std::collections::BTreeSet;

/// A single static validation failure detected while inspecting a [`Proposal`].
///
/// This type is independent of semantic checks such as reference existence or
/// type compatibility; those are handled by `scc-analysis`.
#[derive(Debug, Clone, PartialEq)]
pub enum ProposalValidationError {
    /// JSON parsing or deserialization failure.
    InvalidJson {
        message: String,
        path: Option<String>,
    },
    /// A required field is missing or empty.
    MissingRequiredField { path: String },
    /// `Proposal.version` is empty.
    EmptyVersion,
    /// An entity id is empty.
    EmptyEntityId { index: usize },
    /// An entity id appears more than once.
    DuplicateEntityId { index: usize, id: String },
    /// A `distance` relation is missing its numeric `value`.
    MissingDistanceValue { index: usize },
    /// A numeric value is `NaN`, `+inf`, or `-inf`.
    NonFiniteValue { path: String, value: f64 },
}

impl ProposalValidationError {
    /// Stable error code assigned to this variant.
    pub fn code(&self) -> &'static str {
        match self {
            ProposalValidationError::InvalidJson { .. } => "SCC-PROP-001",
            ProposalValidationError::MissingRequiredField { .. } => "SCC-PROP-002",
            ProposalValidationError::EmptyVersion => "SCC-PROP-004",
            ProposalValidationError::EmptyEntityId { .. } => "SCC-PROP-005",
            ProposalValidationError::DuplicateEntityId { .. } => "SCC-PROP-006",
            ProposalValidationError::MissingDistanceValue { .. } => "SCC-PROP-007",
            ProposalValidationError::NonFiniteValue { .. } => "SCC-PROP-008",
        }
    }
}

impl From<ProposalValidationError> for StructuredError {
    fn from(err: ProposalValidationError) -> Self {
        let code = err.code();
        let base = StructuredError::new(code, ErrorStage::Proposal, err.message());
        match err {
            ProposalValidationError::InvalidJson { path, .. } => {
                let structured = if let Some(path) = path {
                    base.with_path(path)
                } else {
                    base
                };
                structured.with_recoverable(false)
            }
            ProposalValidationError::MissingRequiredField { path } => {
                base.with_path(path).with_recoverable(true)
            }
            ProposalValidationError::EmptyVersion => {
                base.with_path("/version").with_recoverable(true)
            }
            ProposalValidationError::EmptyEntityId { index } => base
                .with_path(format!("/entities/{}/id", index))
                .with_recoverable(true),
            ProposalValidationError::DuplicateEntityId { index, id } => base
                .with_path(format!("/entities/{}/id", index))
                .with_entity_ids(vec![EntityId(id)])
                .with_recoverable(true)
                .with_suggestion("rename or remove the duplicate entity"),
            ProposalValidationError::MissingDistanceValue { index } => base
                .with_path(format!("/relations/{}/value", index))
                .with_recoverable(true)
                .with_suggestion("add a numeric value to the distance relation"),
            ProposalValidationError::NonFiniteValue { path, .. } => {
                base.with_path(path).with_recoverable(true)
            }
        }
    }
}

impl From<ProposalValidationError> for SccError {
    fn from(err: ProposalValidationError) -> Self {
        SccError::Structured(Box::new(err.into()))
    }
}

impl ProposalValidationError {
    fn message(&self) -> String {
        match self {
            ProposalValidationError::InvalidJson { message, .. } => {
                format!("JSON parsing failed: {}", message)
            }
            ProposalValidationError::MissingRequiredField { path } => {
                format!("missing required field: {}", path)
            }
            ProposalValidationError::EmptyVersion => "proposal version is empty".into(),
            ProposalValidationError::EmptyEntityId { .. } => "entity id is empty".into(),
            ProposalValidationError::DuplicateEntityId { id, .. } => {
                format!("duplicate entity id: {}", id)
            }
            ProposalValidationError::MissingDistanceValue { .. } => {
                "distance relation requires value".into()
            }
            ProposalValidationError::NonFiniteValue { value, .. } => {
                format!("value must be finite, got {}", value)
            }
        }
    }
}

/// Multiple static validation failures returned together.
#[derive(Debug, Clone, PartialEq)]
pub struct ProposalValidationErrors {
    pub errors: Vec<ProposalValidationError>,
}

impl From<ProposalValidationErrors> for SccError {
    fn from(errors: ProposalValidationErrors) -> Self {
        let structured: Vec<StructuredError> = errors
            .errors
            .into_iter()
            .map(StructuredError::from)
            .collect();
        match structured.len() {
            0 => SccError::Structured(Box::new(StructuredError::new(
                "SCC-PROP-009",
                ErrorStage::Proposal,
                "internal proposal validation invariant failed",
            ))),
            1 => SccError::Structured(Box::new(structured.into_iter().next().unwrap())),
            _ => SccError::StructuredBatch(structured),
        }
    }
}

/// Collect every static validation failure in a [`Proposal`].
///
/// Errors are returned in a deterministic order: version first, then entities,
/// relations, dimensions, arrays, and anchors, each in array index order.
pub fn collect_validation_errors(proposal: &Proposal) -> Vec<ProposalValidationError> {
    let mut errors = Vec::new();

    if proposal.version.trim().is_empty() {
        errors.push(ProposalValidationError::EmptyVersion);
    }

    let mut seen_ids = BTreeSet::new();
    for (index, entity) in proposal.entities.iter().enumerate() {
        if entity.id.trim().is_empty() {
            errors.push(ProposalValidationError::EmptyEntityId { index });
        } else if !seen_ids.insert(entity.id.clone()) {
            errors.push(ProposalValidationError::DuplicateEntityId {
                index,
                id: entity.id.clone(),
            });
        }
    }

    for (index, relation) in proposal.relations.iter().enumerate() {
        if relation.kind == "distance" && relation.value.is_none() {
            errors.push(ProposalValidationError::MissingDistanceValue { index });
        }
        if let Some(value) = relation.value {
            if !value.is_finite() {
                errors.push(ProposalValidationError::NonFiniteValue {
                    path: format!("/relations/{}/value", index),
                    value,
                });
            }
        }
    }

    for (index, dimension) in proposal.dimensions.iter().enumerate() {
        if !dimension.value.is_finite() {
            errors.push(ProposalValidationError::NonFiniteValue {
                path: format!("/dimensions/{}/value", index),
                value: dimension.value,
            });
        }
    }

    for (index, array) in proposal.arrays.iter().enumerate() {
        for (field, value) in [
            ("spacing", array.spacing),
            ("radius", array.radius),
            ("row_spacing", array.row_spacing),
            ("col_spacing", array.col_spacing),
        ] {
            if let Some(value) = value {
                if !value.is_finite() {
                    errors.push(ProposalValidationError::NonFiniteValue {
                        path: format!("/arrays/{}/{}", index, field),
                        value,
                    });
                }
            }
        }
    }

    errors
}

/// Validate a [`Proposal`] and return all static failures, if any.
pub fn validate_detailed(
    proposal: &Proposal,
) -> Result<(Proposal, Vec<scc_core::conflict::Conflict>), ProposalValidationErrors> {
    let errors = collect_validation_errors(proposal);
    if errors.is_empty() {
        Ok((proposal.clone(), Vec::new()))
    } else {
        Err(ProposalValidationErrors { errors })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::{ProposalEntity, ProposalRelation};

    fn minimal_proposal() -> Proposal {
        Proposal {
            version: "1.0".into(),
            entities: vec![ProposalEntity {
                id: "e1".into(),
                kind: "circle".into(),
            }],
            relations: vec![],
            dimensions: vec![],
            arrays: vec![],
            anchors: vec![],
            metadata: serde_json::Value::Null,
        }
    }

    #[test]
    fn valid_proposal_has_no_errors() {
        let p = minimal_proposal();
        assert!(collect_validation_errors(&p).is_empty());
        assert!(validate_detailed(&p).is_ok());
    }

    #[test]
    fn empty_version_reported() {
        let mut p = minimal_proposal();
        p.version = "".into();
        let errors = collect_validation_errors(&p);
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].code(), "SCC-PROP-004");
        let structured: StructuredError = errors[0].clone().into();
        assert_eq!(structured.path, Some("/version".into()));
        structured.validate_contract().unwrap();
    }

    #[test]
    fn empty_entity_id_reported() {
        let mut p = minimal_proposal();
        p.entities.push(ProposalEntity {
            id: "".into(),
            kind: "circle".into(),
        });
        let errors = collect_validation_errors(&p);
        assert!(errors
            .iter()
            .any(|e| matches!(e, ProposalValidationError::EmptyEntityId { index: 1 })));
        let structured: StructuredError = errors
            .iter()
            .find(|e| matches!(e, ProposalValidationError::EmptyEntityId { .. }))
            .unwrap()
            .clone()
            .into();
        assert_eq!(structured.path, Some("/entities/1/id".into()));
        structured.validate_contract().unwrap();
    }

    #[test]
    fn duplicate_entity_id_reported() {
        let mut p = minimal_proposal();
        p.entities.push(ProposalEntity {
            id: "e1".into(),
            kind: "circle".into(),
        });
        let errors = collect_validation_errors(&p);
        assert!(errors.iter().any(|e| matches!(
            e,
            ProposalValidationError::DuplicateEntityId { index: 1, id }
            if id == "e1"
        )));
        let structured: StructuredError = errors
            .iter()
            .find(|e| matches!(e, ProposalValidationError::DuplicateEntityId { .. }))
            .unwrap()
            .clone()
            .into();
        assert_eq!(structured.code, "SCC-PROP-006");
        assert_eq!(structured.path, Some("/entities/1/id".into()));
        assert_eq!(structured.entity_ids, Some(vec![EntityId("e1".into())]));
        structured.validate_contract().unwrap();
    }

    #[test]
    fn missing_distance_value_reported() {
        let mut p = minimal_proposal();
        p.relations.push(ProposalRelation {
            kind: "distance".into(),
            subject: "e1".into(),
            reference: "e1".into(),
            axis: None,
            value: None,
        });
        let errors = collect_validation_errors(&p);
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].code(), "SCC-PROP-007");
        let structured: StructuredError = errors[0].clone().into();
        assert_eq!(structured.path, Some("/relations/0/value".into()));
        structured.validate_contract().unwrap();
    }

    #[test]
    fn non_finite_value_reported() {
        let mut p = minimal_proposal();
        p.relations.push(ProposalRelation {
            kind: "distance".into(),
            subject: "e1".into(),
            reference: "e1".into(),
            axis: None,
            value: Some(f64::NAN),
        });
        let errors = collect_validation_errors(&p);
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].code(), "SCC-PROP-008");
        let structured: StructuredError = errors[0].clone().into();
        assert_eq!(structured.path, Some("/relations/0/value".into()));
        structured.validate_contract().unwrap();
    }

    #[test]
    fn multiple_errors_are_collected_in_order() {
        let mut p = minimal_proposal();
        p.version = "".into();
        p.entities.push(ProposalEntity {
            id: "e1".into(),
            kind: "circle".into(),
        });
        p.relations.push(ProposalRelation {
            kind: "distance".into(),
            subject: "e1".into(),
            reference: "e1".into(),
            axis: None,
            value: None,
        });
        let errors = collect_validation_errors(&p);
        assert_eq!(errors.len(), 3);
        assert_eq!(errors[0].code(), "SCC-PROP-004");
        assert_eq!(errors[1].code(), "SCC-PROP-006");
        assert_eq!(errors[2].code(), "SCC-PROP-007");
        for err in errors {
            let structured: StructuredError = err.into();
            structured.validate_contract().unwrap();
        }
    }

    #[test]
    fn validate_wrapper_returns_structured_scc_error() {
        let mut p = minimal_proposal();
        p.version = "".into();
        let result = crate::validate::validate(&p);
        assert!(result.is_err());
        match result.unwrap_err() {
            SccError::Structured(e) => {
                assert_eq!(e.code, "SCC-PROP-004");
                e.validate_contract().unwrap();
            }
            other => panic!("expected structured error, got {:?}", other),
        }
    }

    #[test]
    fn invalid_json_maps_to_prop_001() {
        let err = ProposalValidationError::InvalidJson {
            message: "trailing comma".into(),
            path: None,
        };
        assert_eq!(err.code(), "SCC-PROP-001");
        let structured: StructuredError = err.into();
        assert_eq!(structured.code, "SCC-PROP-001");
        structured.validate_contract().unwrap();
    }
}
