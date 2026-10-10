use scc_core::conflict::{
    Conflict, ConflictCategory, ConflictFacts, ConflictType, MinimalConflictSubgraph,
};
use scc_core::ids::{ConstraintId, EntityId};
use scc_core::semantic_ir::{SemanticDimension, SemanticIr, SemanticRelation};
use std::collections::BTreeMap;

/// Extracts semantic conflicts from the analyzed proposal.
///
/// Detects two kinds of minimal conflict subgraphs:
///
/// 1. Duplicate dimensions of the same kind on the same target that specify
///    different values (e.g. two width values for the same entity).
/// 2. Mutually exclusive relations on the same pair of entities:
///    - `inside` and `outside` on the same pair
///    - `parallel_to` and `perpendicular_to` on the same pair
///    - multiple `distance` relations with different values on the same pair
pub fn extract_conflicts(semantic_ir: &SemanticIr) -> Vec<Conflict> {
    let mut conflicts = Vec::new();
    conflicts.extend(extract_dimension_conflicts(&semantic_ir.dimensions));
    conflicts.extend(extract_relation_conflicts(&semantic_ir.relations));
    conflicts
}

fn extract_dimension_conflicts(dims: &[SemanticDimension]) -> Vec<Conflict> {
    let mut groups: BTreeMap<(EntityId, String), Vec<(usize, f64)>> = BTreeMap::new();

    for (idx, dim) in dims.iter().enumerate() {
        let (target, kind, value) = match dim {
            SemanticDimension::Diameter { target, value } => (target, "diameter", *value),
            SemanticDimension::Radius { target, value } => (target, "radius", *value),
            SemanticDimension::Width { target, value } => (target, "width", *value),
            SemanticDimension::Height { target, value } => (target, "height", *value),
            SemanticDimension::Length { target, value } => (target, "length", *value),
            SemanticDimension::Offset { target, value } => (target, "offset", *value),
            SemanticDimension::X { target, value } => (target, "x", *value),
            SemanticDimension::Y { target, value } => (target, "y", *value),
        };
        groups
            .entry((target.clone(), kind.to_string()))
            .or_default()
            .push((idx, value));
    }

    let mut conflicts = Vec::new();
    for ((target, kind), entries) in groups {
        let ids: Vec<ConstraintId> = entries
            .iter()
            .map(|(idx, _)| ConstraintId(format!("dim_{idx}")))
            .collect();

        let unique_values: BTreeMap<u64, ()> = entries
            .iter()
            .map(|(_, value)| (value.to_bits(), ()))
            .collect();

        if unique_values.len() > 1 {
            let value_list: Vec<String> = entries.iter().map(|(_, v)| format!("{v}")).collect();
            conflicts.push(Conflict {
                category: ConflictCategory::Semantic,
                conflict_type: ConflictType::TypeMismatch,
                mcs: MinimalConflictSubgraph {
                    conflicting_constraints: ids.clone(),
                    conflict_type: ConflictType::TypeMismatch,
                },
                facts: ConflictFacts {
                    affected_entities: vec![target.clone()],
                    affected_constraints: ids,
                    message: format!(
                        "conflicting {kind} values for entity {}: {}",
                        target.0,
                        value_list.join(", ")
                    ),
                },
            });
        }
    }

    conflicts
}

fn extract_relation_conflicts(relations: &[SemanticRelation]) -> Vec<Conflict> {
    let mut groups: BTreeMap<(EntityId, EntityId), Vec<(usize, SemanticRelation)>> =
        BTreeMap::new();

    for (idx, rel) in relations.iter().enumerate() {
        let (subject, reference) = relation_endpoints(rel);
        let pair = if subject.0 <= reference.0 {
            (subject.clone(), reference.clone())
        } else {
            (reference.clone(), subject.clone())
        };
        groups.entry(pair).or_default().push((idx, rel.clone()));
    }

    let mut conflicts = Vec::new();
    for (pair, entries) in groups {
        // Inside + Outside on the same pair is contradictory.
        let inside_ids: Vec<ConstraintId> = entries
            .iter()
            .filter(|(_, rel)| matches!(rel, SemanticRelation::Inside { .. }))
            .map(|(idx, _)| ConstraintId(format!("rel_{idx}")))
            .collect();
        let outside_ids: Vec<ConstraintId> = entries
            .iter()
            .filter(|(_, rel)| matches!(rel, SemanticRelation::Outside { .. }))
            .map(|(idx, _)| ConstraintId(format!("rel_{idx}")))
            .collect();
        if !inside_ids.is_empty() && !outside_ids.is_empty() {
            let mut ids = inside_ids;
            ids.extend(outside_ids);
            conflicts.push(Conflict {
                category: ConflictCategory::Semantic,
                conflict_type: ConflictType::TypeMismatch,
                mcs: MinimalConflictSubgraph {
                    conflicting_constraints: ids.clone(),
                    conflict_type: ConflictType::TypeMismatch,
                },
                facts: ConflictFacts {
                    affected_entities: vec![pair.0.clone(), pair.1.clone()],
                    affected_constraints: ids.clone(),
                    message: format!(
                        "inside and outside relations conflict between {} and {}",
                        pair.0, pair.1
                    ),
                },
            });
        }

        // Parallel + Perpendicular on the same pair is contradictory.
        let parallel_ids: Vec<ConstraintId> = entries
            .iter()
            .filter(|(_, rel)| matches!(rel, SemanticRelation::ParallelTo { .. }))
            .map(|(idx, _)| ConstraintId(format!("rel_{idx}")))
            .collect();
        let perpendicular_ids: Vec<ConstraintId> = entries
            .iter()
            .filter(|(_, rel)| matches!(rel, SemanticRelation::PerpendicularTo { .. }))
            .map(|(idx, _)| ConstraintId(format!("rel_{idx}")))
            .collect();
        if !parallel_ids.is_empty() && !perpendicular_ids.is_empty() {
            let mut ids = parallel_ids;
            ids.extend(perpendicular_ids);
            conflicts.push(Conflict {
                category: ConflictCategory::Semantic,
                conflict_type: ConflictType::TypeMismatch,
                mcs: MinimalConflictSubgraph {
                    conflicting_constraints: ids.clone(),
                    conflict_type: ConflictType::TypeMismatch,
                },
                facts: ConflictFacts {
                    affected_entities: vec![pair.0.clone(), pair.1.clone()],
                    affected_constraints: ids.clone(),
                    message: format!(
                        "parallel and perpendicular relations conflict between {} and {}",
                        pair.0, pair.1
                    ),
                },
            });
        }

        // Multiple distance relations with different values on the same pair.
        let distances: Vec<(usize, f64)> = entries
            .iter()
            .filter_map(|(idx, rel)| match rel {
                SemanticRelation::DistanceFrom { value, .. } => Some((*idx, *value)),
                _ => None,
            })
            .collect();
        if distances.len() >= 2 {
            let unique_values: BTreeMap<u64, ()> = distances
                .iter()
                .map(|(_, value)| (value.to_bits(), ()))
                .collect();
            if unique_values.len() > 1 {
                let ids: Vec<ConstraintId> = distances
                    .iter()
                    .map(|(idx, _)| ConstraintId(format!("rel_{idx}")))
                    .collect();
                conflicts.push(Conflict {
                    category: ConflictCategory::Semantic,
                    conflict_type: ConflictType::TypeMismatch,
                    mcs: MinimalConflictSubgraph {
                        conflicting_constraints: ids.clone(),
                        conflict_type: ConflictType::TypeMismatch,
                    },
                    facts: ConflictFacts {
                        affected_entities: vec![pair.0.clone(), pair.1.clone()],
                        affected_constraints: ids.clone(),
                        message: format!(
                            "conflicting distance values between {} and {}: {}",
                            pair.0,
                            pair.1,
                            distances
                                .iter()
                                .map(|(_, v)| format!("{v}"))
                                .collect::<Vec<_>>()
                                .join(", ")
                        ),
                    },
                });
            }
        }
    }

    conflicts
}

fn relation_endpoints(rel: &SemanticRelation) -> (&EntityId, &EntityId) {
    match rel {
        SemanticRelation::CenteredIn {
            subject, reference, ..
        } => (subject, reference),
        SemanticRelation::Inside { subject, reference } => (subject, reference),
        SemanticRelation::Outside { subject, reference } => (subject, reference),
        SemanticRelation::ParallelTo { subject, reference } => (subject, reference),
        SemanticRelation::PerpendicularTo { subject, reference } => (subject, reference),
        SemanticRelation::TangentTo { subject, reference } => (subject, reference),
        SemanticRelation::SymmetricAbout {
            subject, reference, ..
        } => (subject, reference),
        SemanticRelation::AlignedWith { subject, reference } => (subject, reference),
        SemanticRelation::DistanceFrom {
            subject, reference, ..
        } => (subject, reference),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use scc_core::entity::{Entity, EntityKind};
    use scc_core::graph::DeclarationGraph;

    fn ir_from(dimensions: Vec<SemanticDimension>, relations: Vec<SemanticRelation>) -> SemanticIr {
        SemanticIr {
            entities: vec![
                Entity {
                    id: EntityId("a".into()),
                    kind: EntityKind::Circle,
                },
                Entity {
                    id: EntityId("b".into()),
                    kind: EntityKind::Circle,
                },
            ],
            relations,
            dimensions,
            arrays: vec![],
            anchors: vec![],
            graph: DeclarationGraph {
                nodes: vec![],
                edges: vec![],
            },
        }
    }

    #[test]
    fn conflicting_dimensions_are_detected() {
        let dims = vec![
            SemanticDimension::Diameter {
                target: EntityId("a".into()),
                value: 10.0,
            },
            SemanticDimension::Diameter {
                target: EntityId("a".into()),
                value: 20.0,
            },
        ];
        let ir = ir_from(dims, vec![]);
        let conflicts = extract_conflicts(&ir);
        assert_eq!(conflicts.len(), 1);
        assert_eq!(conflicts[0].conflict_type, ConflictType::TypeMismatch);
        assert_eq!(
            conflicts[0].mcs.conflicting_constraints,
            vec![ConstraintId("dim_0".into()), ConstraintId("dim_1".into())]
        );
        assert!(conflicts[0].facts.message.contains("diameter"));
    }

    #[test]
    fn identical_dimension_values_are_not_conflicting() {
        let dims = vec![
            SemanticDimension::Width {
                target: EntityId("a".into()),
                value: 50.0,
            },
            SemanticDimension::Width {
                target: EntityId("a".into()),
                value: 50.0,
            },
        ];
        let ir = ir_from(dims, vec![]);
        let conflicts = extract_conflicts(&ir);
        assert!(conflicts.is_empty());
    }

    #[test]
    fn inside_outside_same_pair_is_conflict() {
        let rels = vec![
            SemanticRelation::Inside {
                subject: EntityId("a".into()),
                reference: EntityId("b".into()),
            },
            SemanticRelation::Outside {
                subject: EntityId("a".into()),
                reference: EntityId("b".into()),
            },
        ];
        let ir = ir_from(vec![], rels);
        let conflicts = extract_conflicts(&ir);
        assert_eq!(conflicts.len(), 1);
        assert!(conflicts[0].facts.message.contains("inside and outside"));
        assert_eq!(
            conflicts[0].mcs.conflicting_constraints,
            vec![ConstraintId("rel_0".into()), ConstraintId("rel_1".into())]
        );
    }

    #[test]
    fn parallel_perpendicular_same_pair_is_conflict() {
        let rels = vec![
            SemanticRelation::ParallelTo {
                subject: EntityId("a".into()),
                reference: EntityId("b".into()),
            },
            SemanticRelation::PerpendicularTo {
                subject: EntityId("a".into()),
                reference: EntityId("b".into()),
            },
        ];
        let ir = ir_from(vec![], rels);
        let conflicts = extract_conflicts(&ir);
        assert_eq!(conflicts.len(), 1);
        assert!(conflicts[0]
            .facts
            .message
            .contains("parallel and perpendicular"));
    }

    #[test]
    fn different_distance_values_same_pair_is_conflict() {
        let rels = vec![
            SemanticRelation::DistanceFrom {
                subject: EntityId("a".into()),
                reference: EntityId("b".into()),
                value: 10.0,
            },
            SemanticRelation::DistanceFrom {
                subject: EntityId("a".into()),
                reference: EntityId("b".into()),
                value: 20.0,
            },
        ];
        let ir = ir_from(vec![], rels);
        let conflicts = extract_conflicts(&ir);
        assert_eq!(conflicts.len(), 1);
        assert!(conflicts[0].facts.message.contains("distance"));
    }

    #[test]
    fn identical_distance_values_are_not_conflicting() {
        let rels = vec![
            SemanticRelation::DistanceFrom {
                subject: EntityId("a".into()),
                reference: EntityId("b".into()),
                value: 10.0,
            },
            SemanticRelation::DistanceFrom {
                subject: EntityId("a".into()),
                reference: EntityId("b".into()),
                value: 10.0,
            },
        ];
        let ir = ir_from(vec![], rels);
        let conflicts = extract_conflicts(&ir);
        assert!(conflicts.is_empty());
    }

    #[test]
    fn pair_normalization_is_deterministic() {
        // Same logical pair, reversed subject/reference order.
        let rels = vec![
            SemanticRelation::Inside {
                subject: EntityId("a".into()),
                reference: EntityId("b".into()),
            },
            SemanticRelation::Outside {
                subject: EntityId("b".into()),
                reference: EntityId("a".into()),
            },
        ];
        let ir = ir_from(vec![], rels);
        let conflicts = extract_conflicts(&ir);
        assert_eq!(conflicts.len(), 1);
    }
}
