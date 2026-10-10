use crate::error::validate_detailed;
use crate::schema::Proposal;
use scc_core::conflict::Conflict;
use scc_core::error::SccError;

/// Legacy compatible wrapper around [`crate::error::validate_detailed`].
///
/// Returns one `SccError::Structured` or an `SccError::StructuredBatch` when
/// static validation fails, preserving every deterministic diagnostic.
pub fn validate(proposal: &Proposal) -> Result<(Proposal, Vec<Conflict>), SccError> {
    validate_detailed(proposal).map_err(|e| e.into())
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
    fn valid_proposal_passes() {
        let p = minimal_proposal();
        let (out, conflicts) = validate(&p).unwrap();
        assert_eq!(out, p);
        assert!(conflicts.is_empty());
    }

    #[test]
    fn distance_relation_requires_value() {
        let mut p = minimal_proposal();
        p.relations.push(ProposalRelation {
            kind: "distance".into(),
            subject: "e1".into(),
            reference: "e1".into(),
            axis: None,
            value: None,
        });
        let result = validate(&p);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("distance relation requires value"));
    }

    #[test]
    fn distance_relation_with_value_passes() {
        let mut p = minimal_proposal();
        p.relations.push(ProposalRelation {
            kind: "distance".into(),
            subject: "e1".into(),
            reference: "e1".into(),
            axis: None,
            value: Some(12.5),
        });
        let (out, conflicts) = validate(&p).unwrap();
        assert_eq!(out.relations[0].value, Some(12.5));
        assert!(conflicts.is_empty());
    }

    #[test]
    fn non_distance_relation_without_value_passes() {
        let mut p = minimal_proposal();
        p.relations.push(ProposalRelation {
            kind: "parallel_to".into(),
            subject: "e1".into(),
            reference: "e1".into(),
            axis: None,
            value: None,
        });
        assert!(validate(&p).is_ok());
    }

    #[test]
    fn multiple_distance_violations_reported_first() {
        let mut p = minimal_proposal();
        p.relations.push(ProposalRelation {
            kind: "distance".into(),
            subject: "e1".into(),
            reference: "e1".into(),
            axis: None,
            value: None,
        });
        p.relations.push(ProposalRelation {
            kind: "distance".into(),
            subject: "e1".into(),
            reference: "e1".into(),
            axis: None,
            value: None,
        });
        let result = validate(&p);
        assert!(result.is_err());
    }

    #[test]
    fn validate_returns_clone() {
        let p = minimal_proposal();
        let (out, _) = validate(&p).unwrap();
        // Returned proposal should be a functional clone.
        assert_eq!(out.version, p.version);
        assert_eq!(out.entities.len(), p.entities.len());
    }
}
