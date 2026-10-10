use crate::error::{AnalysisError, AnalysisErrors};
use scc_core::array::{ArrayNode, PatternNode};
use scc_core::conflict::{
    Conflict, ConflictCategory, ConflictFacts, ConflictType, MinimalConflictSubgraph,
};
use scc_core::entity::{Entity, EntityKind};
use scc_core::graph::{DeclarationGraph, EdgeRelation, GraphEdge, GraphNode, NodeKind};
use scc_core::ids::{ArrayId, EntityId};
use scc_core::semantic_ir::{SemanticAnchor, SemanticDimension, SemanticIr, SemanticRelation};
use scc_proposal::schema::{Proposal, ProposalArray};
use std::collections::{BTreeMap, BTreeSet};

fn map_entity_kind(kind: &str) -> Option<EntityKind> {
    match kind {
        "point" => Some(EntityKind::Point),
        "line" => Some(EntityKind::Line),
        "circle" => Some(EntityKind::Circle),
        "arc" => Some(EntityKind::Arc),
        "rectangle" => Some(EntityKind::Rectangle),
        "polygon" => Some(EntityKind::Polygon),
        _ => None,
    }
}

fn build_entities(proposal: &Proposal) -> Result<Vec<Entity>, Conflict> {
    let mut entities = Vec::new();
    for pe in &proposal.entities {
        let kind = map_entity_kind(&pe.kind).ok_or_else(|| Conflict {
            category: ConflictCategory::Structural,
            conflict_type: ConflictType::InvalidEntity,
            mcs: MinimalConflictSubgraph {
                conflicting_constraints: vec![],
                conflict_type: ConflictType::InvalidEntity,
            },
            facts: ConflictFacts {
                affected_entities: vec![EntityId(pe.id.clone())],
                affected_constraints: vec![],
                message: format!("unknown entity kind: {}", pe.kind),
            },
        })?;
        entities.push(Entity {
            id: EntityId(pe.id.clone()),
            kind,
        });
    }
    Ok(entities)
}

fn build_arrays(proposal: &Proposal) -> Result<Vec<ArrayNode>, Conflict> {
    let mut arrays = Vec::new();
    for pa in &proposal.arrays {
        let pattern = match pa.kind.as_str() {
            "linear" => PatternNode::Linear {
                count: pa.count,
                spacing: pa.spacing.unwrap_or(0.0),
                direction: pa.direction.clone(),
            },
            "circular" => PatternNode::Circular {
                count: pa.count,
                radius: pa.radius.unwrap_or(0.0),
                center: None,
            },
            "grid" => PatternNode::Grid {
                rows: pa.rows.unwrap_or(1),
                cols: pa.cols.unwrap_or(1),
                row_spacing: pa.row_spacing.unwrap_or(0.0),
                col_spacing: pa.col_spacing.unwrap_or(0.0),
            },
            _ => {
                return Err(Conflict {
                    category: ConflictCategory::Structural,
                    conflict_type: ConflictType::InvalidEntity,
                    mcs: MinimalConflictSubgraph {
                        conflicting_constraints: vec![],
                        conflict_type: ConflictType::InvalidEntity,
                    },
                    facts: ConflictFacts {
                        affected_entities: vec![EntityId(pa.id.clone())],
                        affected_constraints: vec![],
                        message: format!("unknown array kind: {}", pa.kind),
                    },
                })
            }
        };
        arrays.push(ArrayNode {
            id: ArrayId(pa.id.clone()),
            pattern,
            element: EntityId(pa.element.clone()),
        });
    }
    Ok(arrays)
}

fn build_relations(proposal: &Proposal) -> Vec<SemanticRelation> {
    let mut relations = Vec::new();
    for pr in &proposal.relations {
        let subject = EntityId(pr.subject.clone());
        let reference = EntityId(pr.reference.clone());
        let rel = match pr.kind.as_str() {
            "centered_in" => SemanticRelation::CenteredIn {
                subject,
                reference,
                axis: pr.axis.clone(),
            },
            "inside" => SemanticRelation::Inside { subject, reference },
            "outside" => SemanticRelation::Outside { subject, reference },
            "parallel_to" => SemanticRelation::ParallelTo { subject, reference },
            "perpendicular_to" => SemanticRelation::PerpendicularTo { subject, reference },
            "tangent_to" => SemanticRelation::TangentTo { subject, reference },
            "symmetric_about" => SemanticRelation::SymmetricAbout {
                subject,
                reference,
                axis: pr.axis.clone(),
            },
            "aligned_with" => SemanticRelation::AlignedWith { subject, reference },
            "distance" => SemanticRelation::DistanceFrom {
                subject,
                reference,
                value: pr.value.unwrap_or(0.0),
            },
            _ => {
                continue;
            }
        };
        relations.push(rel);
    }
    relations
}

fn build_dimensions(proposal: &Proposal) -> Vec<SemanticDimension> {
    let mut dims = Vec::new();
    for pd in &proposal.dimensions {
        let target = EntityId(pd.target.clone());
        let dim = match pd.kind.as_str() {
            "diameter" => SemanticDimension::Diameter {
                target,
                value: pd.value,
            },
            "radius" => SemanticDimension::Radius {
                target,
                value: pd.value,
            },
            "width" => SemanticDimension::Width {
                target,
                value: pd.value,
            },
            "height" => SemanticDimension::Height {
                target,
                value: pd.value,
            },
            "length" => SemanticDimension::Length {
                target,
                value: pd.value,
            },
            "offset" => SemanticDimension::Offset {
                target,
                value: pd.value,
            },
            "x" => SemanticDimension::X {
                target,
                value: pd.value,
            },
            "y" => SemanticDimension::Y {
                target,
                value: pd.value,
            },
            _ => {
                continue;
            }
        };
        dims.push(dim);
    }
    dims
}

/// Append profile endpoint dimensions after semantic conflict extraction.
///
/// The original SCC semantic dimension enum is intentionally kept wire
/// compatible. Profile dimensions are accepted and validated by this module,
/// then represented in the existing coordinate streams for the lowering
/// stage. Appending them after the legacy MCS pass prevents endpoint pairs
/// from being mistaken for conflicting duplicate position declarations.
fn append_profile_dimensions(proposal: &Proposal, semantic_ir: &mut SemanticIr) {
    for dimension in &proposal.dimensions {
        let target = EntityId(dimension.target.clone());
        match dimension.kind.as_str() {
            "x1" | "x2" => semantic_ir.dimensions.push(SemanticDimension::X {
                target,
                value: dimension.value,
            }),
            "y1" | "y2" => semantic_ir.dimensions.push(SemanticDimension::Y {
                target,
                value: dimension.value,
            }),
            "start_angle" | "end_angle" => semantic_ir.dimensions.push(SemanticDimension::Offset {
                target,
                value: dimension.value,
            }),
            _ => {}
        }
    }
}

fn build_anchors(proposal: &Proposal) -> Vec<SemanticAnchor> {
    let mut anchors = Vec::new();
    for pa in &proposal.anchors {
        let id = EntityId(pa.id.clone());
        let anchor = match pa.kind.as_str() {
            "origin_point" => SemanticAnchor::OriginPoint { id: id.clone() },
            "fixed_center" => SemanticAnchor::FixedCenter {
                id: id.clone(),
                entity: match pa.entity.as_ref() {
                    Some(entity) => EntityId(entity.clone()),
                    None => continue,
                },
            },
            "reference_edge" => SemanticAnchor::ReferenceEdge {
                id: id.clone(),
                entity: match pa.entity.as_ref() {
                    Some(entity) => EntityId(entity.clone()),
                    None => continue,
                },
            },
            "datum_line" => SemanticAnchor::DatumLine {
                id: id.clone(),
                entity: match pa.entity.as_ref() {
                    Some(entity) => EntityId(entity.clone()),
                    None => continue,
                },
            },
            _ => continue,
        };
        anchors.push(anchor);
    }
    anchors
}

fn build_graph(
    entities: &[Entity],
    arrays: &[ArrayNode],
    relations: &[SemanticRelation],
) -> DeclarationGraph {
    let mut nodes = Vec::new();
    let mut edges = Vec::new();

    for e in entities {
        nodes.push(GraphNode {
            id: e.id.0.clone(),
            node_kind: NodeKind::Entity,
        });
    }

    for a in arrays {
        nodes.push(GraphNode {
            id: a.id.0.clone(),
            node_kind: NodeKind::Array,
        });
        edges.push(GraphEdge {
            from: a.id.0.clone(),
            to: a.element.0.clone(),
            relation: EdgeRelation::Ownership,
        });
    }

    for rel in relations {
        let (subject, reference) = relation_endpoints(rel);
        edges.push(GraphEdge {
            from: subject.0.clone(),
            to: reference.0.clone(),
            relation: EdgeRelation::Dependency,
        });
    }

    DeclarationGraph { nodes, edges }
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

fn supported_relation_kind(kind: &str) -> bool {
    matches!(
        kind,
        "centered_in"
            | "inside"
            | "outside"
            | "parallel_to"
            | "perpendicular_to"
            | "tangent_to"
            | "symmetric_about"
            | "aligned_with"
            | "distance"
    )
}

fn supported_dimension_kind(kind: &str) -> bool {
    matches!(
        kind,
        "diameter"
            | "radius"
            | "width"
            | "height"
            | "length"
            | "offset"
            | "x"
            | "y"
            | "x1"
            | "y1"
            | "x2"
            | "y2"
            | "start_angle"
            | "end_angle"
    )
}

fn supported_array_kind(kind: &str) -> bool {
    matches!(kind, "linear" | "circular" | "grid")
}

fn invalid_array_shape(array: &ProposalArray) -> Option<String> {
    match array.kind.as_str() {
        "linear" => {
            if array.count == 0 {
                return Some("linear array count must be greater than zero".into());
            }
            if array.spacing.is_none_or(|spacing| spacing < 0.0) {
                return Some("linear array spacing must be present and non-negative".into());
            }
            if !matches!(array.direction.as_deref(), None | Some("x" | "y")) {
                return Some("linear array direction must be x or y".into());
            }
            None
        }
        "circular" => {
            if array.count == 0 {
                return Some("circular array count must be greater than zero".into());
            }
            if array.radius.is_none_or(|radius| radius <= 0.0) {
                return Some("circular array radius must be present and greater than zero".into());
            }
            None
        }
        "grid" => {
            let Some(rows) = array.rows else {
                return Some("grid array rows must be present".into());
            };
            let Some(cols) = array.cols else {
                return Some("grid array cols must be present".into());
            };
            if rows == 0 || cols == 0 {
                return Some("grid array rows and cols must be greater than zero".into());
            }
            if array.row_spacing.is_none_or(|spacing| spacing < 0.0)
                || array.col_spacing.is_none_or(|spacing| spacing < 0.0)
            {
                return Some(
                    "grid array row and column spacing must be present and non-negative".into(),
                );
            }
            if rows.checked_mul(cols) != Some(array.count) {
                return Some("grid array count must equal rows multiplied by cols".into());
            }
            None
        }
        _ => None,
    }
}

fn supported_anchor_kind(kind: &str) -> bool {
    matches!(
        kind,
        "origin_point" | "fixed_center" | "reference_edge" | "datum_line"
    )
}

fn collect_analysis_errors(proposal: &Proposal, semantic_ir: &SemanticIr) -> Vec<AnalysisError> {
    let mut errors = Vec::new();
    let entity_ids: BTreeSet<String> = semantic_ir
        .entities
        .iter()
        .map(|e| e.id.0.clone())
        .collect();

    // Unsupported kinds (treated as type mismatches at the proposal level).
    for (index, relation) in proposal.relations.iter().enumerate() {
        if !supported_relation_kind(&relation.kind) {
            errors.push(AnalysisError::TypeMismatch {
                entity_ids: vec![
                    EntityId(relation.subject.clone()),
                    EntityId(relation.reference.clone()),
                ],
                path: format!("/relations/{}", index),
                message: format!("unsupported relation kind: {}", relation.kind),
            });
        }
    }
    for (index, dimension) in proposal.dimensions.iter().enumerate() {
        if !supported_dimension_kind(&dimension.kind) {
            errors.push(AnalysisError::TypeMismatch {
                entity_ids: vec![EntityId(dimension.target.clone())],
                path: format!("/dimensions/{}", index),
                message: format!("unsupported dimension kind: {}", dimension.kind),
            });
        }
    }
    for (index, array) in proposal.arrays.iter().enumerate() {
        if !supported_array_kind(&array.kind) {
            errors.push(AnalysisError::TypeMismatch {
                entity_ids: vec![EntityId(array.id.clone()), EntityId(array.element.clone())],
                path: format!("/arrays/{}", index),
                message: format!("unsupported array kind: {}", array.kind),
            });
        } else if let Some(message) = invalid_array_shape(array) {
            errors.push(AnalysisError::TypeMismatch {
                entity_ids: vec![EntityId(array.id.clone()), EntityId(array.element.clone())],
                path: format!("/arrays/{}", index),
                message,
            });
        }
    }
    for (index, anchor) in proposal.anchors.iter().enumerate() {
        if !supported_anchor_kind(&anchor.kind) {
            errors.push(AnalysisError::TypeMismatch {
                entity_ids: vec![EntityId(anchor.id.clone())],
                path: format!("/anchors/{}", index),
                message: format!("unsupported anchor kind: {}", anchor.kind),
            });
        }
        if matches!(
            anchor.kind.as_str(),
            "fixed_center" | "reference_edge" | "datum_line"
        ) && anchor.entity.is_none()
        {
            errors.push(AnalysisError::InvalidAnchorReference {
                anchor_id: EntityId(anchor.id.clone()),
                entity_id: EntityId::default(),
                index,
            });
        }
    }

    // Missing references.
    for (index, relation) in proposal.relations.iter().enumerate() {
        if !entity_ids.contains(&relation.subject) {
            errors.push(AnalysisError::MissingEntityReference {
                entity_id: EntityId(relation.subject.clone()),
                path: format!("/relations/{}/subject", index),
            });
        }
        if !entity_ids.contains(&relation.reference) {
            errors.push(AnalysisError::MissingEntityReference {
                entity_id: EntityId(relation.reference.clone()),
                path: format!("/relations/{}/reference", index),
            });
        }
    }
    for (index, dimension) in proposal.dimensions.iter().enumerate() {
        if !entity_ids.contains(&dimension.target) {
            errors.push(AnalysisError::MissingEntityReference {
                entity_id: EntityId(dimension.target.clone()),
                path: format!("/dimensions/{}/target", index),
            });
        }
    }

    // X/Y position dimensions are only meaningful on positionable entity kinds
    // (Point, Circle, Arc, Rectangle). Line/Polygon positions come from their
    // endpoints/vertices, so accepting x/y on them would silently drop data.
    for (index, dimension) in proposal.dimensions.iter().enumerate() {
        if matches!(dimension.kind.as_str(), "x" | "y") {
            if let Some(entity) = semantic_ir
                .entities
                .iter()
                .find(|e| e.id.0 == dimension.target)
            {
                if matches!(entity.kind, EntityKind::Line | EntityKind::Polygon) {
                    errors.push(AnalysisError::TypeMismatch {
                        entity_ids: vec![EntityId(dimension.target.clone())],
                        path: format!("/dimensions/{}", index),
                        message: format!(
                            "x/y position dimension is not meaningful for a {} entity",
                            format!("{:?}", entity.kind).to_lowercase()
                        ),
                    });
                }
            }
        }
    }
    for (index, array) in proposal.arrays.iter().enumerate() {
        if !entity_ids.contains(&array.element) {
            errors.push(AnalysisError::InvalidArrayReference {
                array_id: ArrayId(array.id.clone()),
                element_id: EntityId(array.element.clone()),
                index,
            });
        }
    }
    for (index, anchor) in proposal.anchors.iter().enumerate() {
        if let Some(entity) = &anchor.entity {
            if !entity_ids.contains(entity) {
                errors.push(AnalysisError::InvalidAnchorReference {
                    anchor_id: EntityId(anchor.id.clone()),
                    entity_id: EntityId(entity.clone()),
                    index,
                });
            }
        }
    }

    // Cycles.
    errors.extend(detect_cycle_errors(&semantic_ir.graph));

    // Type compatibility.
    errors.extend(check_type_errors(
        proposal,
        &semantic_ir.relations,
        &semantic_ir.entities,
    ));

    errors
}

fn detect_cycle_errors(graph: &DeclarationGraph) -> Vec<AnalysisError> {
    let mut adj: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for edge in &graph.edges {
        adj.entry(edge.from.clone())
            .or_default()
            .push(edge.to.clone());
    }

    let mut visited = BTreeSet::new();
    let mut on_stack = BTreeSet::new();
    let mut cycles: Vec<Vec<String>> = Vec::new();

    fn dfs(
        node: &str,
        adj: &BTreeMap<String, Vec<String>>,
        visited: &mut BTreeSet<String>,
        on_stack: &mut BTreeSet<String>,
        path: &mut Vec<String>,
        cycles: &mut Vec<Vec<String>>,
    ) {
        visited.insert(node.to_string());
        on_stack.insert(node.to_string());
        path.push(node.to_string());

        if let Some(neighbors) = adj.get(node) {
            for next in neighbors {
                if !visited.contains(next) {
                    dfs(next, adj, visited, on_stack, path, cycles);
                } else if on_stack.contains(next) {
                    if let Some(start) = path.iter().position(|n| n == next) {
                        cycles.push(path[start..].to_vec());
                    }
                }
            }
        }

        path.pop();
        on_stack.remove(node);
    }

    for node in &graph.nodes {
        if !visited.contains(&node.id) {
            dfs(
                &node.id,
                &adj,
                &mut visited,
                &mut on_stack,
                &mut Vec::new(),
                &mut cycles,
            );
        }
    }

    cycles
        .into_iter()
        .map(|path| AnalysisError::CircularDependency { path })
        .collect()
}

fn check_type_errors(
    proposal: &Proposal,
    relations: &[SemanticRelation],
    entities: &[Entity],
) -> Vec<AnalysisError> {
    let entity_kinds: BTreeMap<&str, &EntityKind> = entities
        .iter()
        .map(|e| (e.id.0.as_str(), &e.kind))
        .collect();
    let mut errors = Vec::new();

    let source_relations = proposal
        .relations
        .iter()
        .enumerate()
        .filter(|(_, relation)| supported_relation_kind(&relation.kind));

    for ((index, _), rel) in source_relations.zip(relations.iter()) {
        let (subject, reference) = relation_endpoints(rel);
        match rel {
            SemanticRelation::CenteredIn { .. } => {
                if let Some(EntityKind::Point) = entity_kinds.get(reference.0.as_str()) {
                    errors.push(AnalysisError::TypeMismatch {
                        entity_ids: vec![subject.clone(), reference.clone()],
                        path: format!("/relations/{}", index),
                        message: format!("cannot center {} inside a point", subject.0),
                    });
                }
            }
            SemanticRelation::Inside { .. } => {
                if entity_kinds
                    .get(reference.0.as_str())
                    .is_some_and(|k| matches!(k, EntityKind::Point | EntityKind::Line))
                {
                    errors.push(AnalysisError::TypeMismatch {
                        entity_ids: vec![subject.clone(), reference.clone()],
                        path: format!("/relations/{}", index),
                        message: format!(
                            "cannot place {} inside a {} (no interior)",
                            subject.0,
                            entity_kinds
                                .get(reference.0.as_str())
                                .map(|k| format!("{:?}", k).to_lowercase())
                                .unwrap_or_else(|| "missing".into())
                        ),
                    });
                }
            }
            SemanticRelation::Outside { .. } => {
                if entity_kinds
                    .get(reference.0.as_str())
                    .is_some_and(|k| matches!(k, EntityKind::Point | EntityKind::Line))
                {
                    errors.push(AnalysisError::TypeMismatch {
                        entity_ids: vec![subject.clone(), reference.clone()],
                        path: format!("/relations/{}", index),
                        message: format!(
                            "cannot place {} outside a {} (no interior)",
                            subject.0,
                            entity_kinds
                                .get(reference.0.as_str())
                                .map(|k| format!("{:?}", k).to_lowercase())
                                .unwrap_or_else(|| "missing".into())
                        ),
                    });
                }
            }
            SemanticRelation::ParallelTo { .. } => {
                let subject_kind = entity_kinds.get(subject.0.as_str());
                let reference_kind = entity_kinds.get(reference.0.as_str());
                let is_invalid = |kind: &&EntityKind| {
                    matches!(
                        kind,
                        EntityKind::Point
                            | EntityKind::Circle
                            | EntityKind::Arc
                            | EntityKind::Rectangle
                            | EntityKind::Polygon
                    )
                };
                if subject_kind.is_some_and(is_invalid) || reference_kind.is_some_and(is_invalid) {
                    errors.push(AnalysisError::TypeMismatch {
                        entity_ids: vec![subject.clone(), reference.clone()],
                        path: format!("/relations/{}", index),
                        message: format!(
                            "parallel relation requires a line entity, got {} and {}",
                            subject.0, reference.0
                        ),
                    });
                }
            }
            SemanticRelation::PerpendicularTo { .. } => {
                let subject_kind = entity_kinds.get(subject.0.as_str());
                let reference_kind = entity_kinds.get(reference.0.as_str());
                let is_invalid = |kind: &&EntityKind| {
                    matches!(
                        kind,
                        EntityKind::Point
                            | EntityKind::Circle
                            | EntityKind::Arc
                            | EntityKind::Rectangle
                            | EntityKind::Polygon
                    )
                };
                if subject_kind.is_some_and(is_invalid) || reference_kind.is_some_and(is_invalid) {
                    errors.push(AnalysisError::TypeMismatch {
                        entity_ids: vec![subject.clone(), reference.clone()],
                        path: format!("/relations/{}", index),
                        message: format!(
                            "perpendicular relation requires a line entity, got {} and {}",
                            subject.0, reference.0
                        ),
                    });
                }
            }
            SemanticRelation::TangentTo { .. } => {
                let subject_kind = entity_kinds.get(subject.0.as_str());
                let reference_kind = entity_kinds.get(reference.0.as_str());
                let is_curved =
                    |kind: &&EntityKind| matches!(kind, EntityKind::Circle | EntityKind::Arc);
                let has_curved_endpoint =
                    subject_kind.is_some_and(is_curved) || reference_kind.is_some_and(is_curved);
                if !has_curved_endpoint {
                    errors.push(AnalysisError::TypeMismatch {
                        entity_ids: vec![subject.clone(), reference.clone()],
                        path: format!("/relations/{}", index),
                        message: format!(
                            "tangent relation requires at least one circle or arc, got {} and {}",
                            subject.0, reference.0
                        ),
                    });
                }
            }
            SemanticRelation::SymmetricAbout { axis, .. } => {
                if axis.is_none()
                    && entity_kinds
                        .get(reference.0.as_str())
                        .is_some_and(|k| matches!(k, EntityKind::Point))
                {
                    errors.push(AnalysisError::TypeMismatch {
                        entity_ids: vec![subject.clone(), reference.clone()],
                        path: format!("/relations/{}", index),
                        message: format!(
                            "cannot mirror {} about a point without an explicit axis",
                            subject.0
                        ),
                    });
                }
            }
            SemanticRelation::AlignedWith { .. } => {
                let subject_kind = entity_kinds.get(subject.0.as_str());
                let reference_kind = entity_kinds.get(reference.0.as_str());
                let is_not_line = |kind: &&EntityKind| !matches!(kind, EntityKind::Line);
                if subject_kind.is_some_and(is_not_line) || reference_kind.is_some_and(is_not_line)
                {
                    errors.push(AnalysisError::TypeMismatch {
                        entity_ids: vec![subject.clone(), reference.clone()],
                        path: format!("/relations/{}", index),
                        message: format!(
                            "alignment relation requires line entities, got {} and {}",
                            subject.0, reference.0
                        ),
                    });
                }
            }
            SemanticRelation::DistanceFrom { .. } => {
                // Distance is meaningful between any two entities; no type restriction.
            }
        }
    }

    errors
}

pub struct AnalysisResult {
    pub semantic_ir: Option<SemanticIr>,
    pub conflicts: Vec<Conflict>,
}

/// Successful outcome of a detailed semantic analysis.
#[derive(Debug)]
pub struct AnalysisOutcome {
    pub semantic_ir: SemanticIr,
    pub conflicts: Vec<Conflict>,
}

fn analyze_core(proposal: &Proposal) -> (Option<SemanticIr>, Vec<AnalysisError>) {
    let entities = match build_entities(proposal) {
        Ok(e) => e,
        Err(c) => {
            return (
                None,
                vec![AnalysisError::InvalidDeclarationGraph {
                    message: c.facts.message,
                }],
            )
        }
    };

    let arrays = match build_arrays(proposal) {
        Ok(a) => a,
        Err(c) => {
            return (
                None,
                vec![AnalysisError::InvalidDeclarationGraph {
                    message: c.facts.message,
                }],
            )
        }
    };

    let relations = build_relations(proposal);
    let dimensions = build_dimensions(proposal);
    let anchors = build_anchors(proposal);
    let graph = build_graph(&entities, &arrays, &relations);

    let semantic_ir = SemanticIr {
        entities,
        relations,
        dimensions,
        arrays,
        anchors,
        graph,
    };

    let errors = collect_analysis_errors(proposal, &semantic_ir);
    (Some(semantic_ir), errors)
}

/// Analyze a proposal with structured diagnostics.
///
/// Returns [`AnalysisErrors`] when the proposal cannot be safely analyzed or
/// an analysis invariant fails. Business-level conflicts are returned as
/// `AnalysisOutcome::conflicts` and are never converted into system errors.
pub fn analyze_detailed(proposal: &Proposal) -> Result<AnalysisOutcome, AnalysisErrors> {
    let (semantic_ir, errors) = analyze_core(proposal);
    if !errors.is_empty() {
        return Err(AnalysisErrors { errors });
    }
    let semantic_ir = semantic_ir.ok_or_else(|| AnalysisErrors {
        errors: vec![AnalysisError::InvalidDeclarationGraph {
            message: "semantic_ir must be present when there are no errors".into(),
        }],
    })?;
    let conflicts = crate::mcs::extract_conflicts(&semantic_ir);
    let mut semantic_ir = semantic_ir;
    if conflicts.is_empty() {
        append_profile_dimensions(proposal, &mut semantic_ir);
    }
    Ok(AnalysisOutcome {
        semantic_ir,
        conflicts,
    })
}

/// Legacy v0.1.0 analysis API.
///
/// Converts structured [`AnalysisError`]s into [`Conflict`] values so that
/// existing callers continue to see the same shape of result. Business-level
/// conflicts from the minimal-conflict-subgraph extractor are still included.
pub fn analyze(proposal: &Proposal) -> AnalysisResult {
    let (mut semantic_ir, errors) = analyze_core(proposal);
    let mut conflicts: Vec<Conflict> = errors.into_iter().map(|e| e.into_conflict()).collect();
    if let Some(ir) = &semantic_ir {
        conflicts.extend(crate::mcs::extract_conflicts(ir));
    }
    if conflicts.is_empty() {
        if let Some(ir) = &mut semantic_ir {
            append_profile_dimensions(proposal, ir);
        }
        AnalysisResult {
            semantic_ir,
            conflicts: vec![],
        }
    } else {
        AnalysisResult {
            semantic_ir: None,
            conflicts,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use scc_core::ids::ConstraintId;
    use scc_proposal::schema::*;

    fn simple_proposal() -> Proposal {
        Proposal {
            version: "1.0".into(),
            entities: vec![
                ProposalEntity {
                    id: "rect_1".into(),
                    kind: "rectangle".into(),
                },
                ProposalEntity {
                    id: "circle_1".into(),
                    kind: "circle".into(),
                },
            ],
            relations: vec![ProposalRelation {
                kind: "centered_in".into(),
                subject: "circle_1".into(),
                reference: "rect_1".into(),
                axis: Some("both".into()),
                value: None,
            }],
            dimensions: vec![
                ProposalDimension {
                    target: "circle_1".into(),
                    kind: "diameter".into(),
                    value: 20.0,
                },
                ProposalDimension {
                    target: "rect_1".into(),
                    kind: "width".into(),
                    value: 100.0,
                },
            ],
            arrays: vec![],
            anchors: vec![ProposalAnchor {
                id: "origin".into(),
                kind: "origin_point".into(),
                entity: None,
            }],
            metadata: serde_json::Value::Null,
        }
    }

    #[test]
    fn valid_proposal_produces_semantic_ir() {
        let result = analyze(&simple_proposal());
        assert!(result.conflicts.is_empty());
        assert!(result.semantic_ir.is_some());
        let ir = result.semantic_ir.unwrap();
        assert_eq!(ir.entities.len(), 2);
        assert_eq!(ir.relations.len(), 1);
    }

    #[test]
    fn missing_reference_detected() {
        let mut p = simple_proposal();
        p.relations.push(ProposalRelation {
            kind: "parallel_to".into(),
            subject: "circle_1".into(),
            reference: "nonexistent".into(),
            axis: None,
            value: None,
        });
        let result = analyze(&p);
        assert!(!result.conflicts.is_empty());
        assert!(result
            .conflicts
            .iter()
            .any(|c| c.conflict_type == ConflictType::MissingReference));
    }

    #[test]
    fn circular_dependency_detected() {
        let p = Proposal {
            version: "1.0".into(),
            entities: vec![
                ProposalEntity {
                    id: "a".into(),
                    kind: "circle".into(),
                },
                ProposalEntity {
                    id: "b".into(),
                    kind: "circle".into(),
                },
            ],
            relations: vec![
                ProposalRelation {
                    kind: "centered_in".into(),
                    subject: "a".into(),
                    reference: "b".into(),
                    axis: None,
                    value: None,
                },
                ProposalRelation {
                    kind: "centered_in".into(),
                    subject: "b".into(),
                    reference: "a".into(),
                    axis: None,
                    value: None,
                },
            ],
            dimensions: vec![],
            arrays: vec![],
            anchors: vec![],
            metadata: serde_json::Value::Null,
        };
        let result = analyze(&p);
        assert!(!result.conflicts.is_empty());
        assert!(result
            .conflicts
            .iter()
            .any(|c| c.conflict_type == ConflictType::CircularDependency));
    }

    #[test]
    fn type_mismatch_detected() {
        let p = Proposal {
            version: "1.0".into(),
            entities: vec![
                ProposalEntity {
                    id: "p1".into(),
                    kind: "circle".into(),
                },
                ProposalEntity {
                    id: "p2".into(),
                    kind: "point".into(),
                },
            ],
            relations: vec![ProposalRelation {
                kind: "centered_in".into(),
                subject: "p1".into(),
                reference: "p2".into(),
                axis: None,
                value: None,
            }],
            dimensions: vec![],
            arrays: vec![],
            anchors: vec![],
            metadata: serde_json::Value::Null,
        };
        let result = analyze(&p);
        assert!(result
            .conflicts
            .iter()
            .any(|c| c.conflict_type == ConflictType::TypeMismatch));
    }

    #[test]
    fn inside_outside_invalid_container_rejected() {
        for kind in ["inside", "outside"] {
            for container_kind in ["point", "line"] {
                let p = Proposal {
                    version: "1.0".into(),
                    entities: vec![
                        ProposalEntity {
                            id: "circle_1".into(),
                            kind: "circle".into(),
                        },
                        ProposalEntity {
                            id: "container".into(),
                            kind: container_kind.into(),
                        },
                    ],
                    relations: vec![ProposalRelation {
                        kind: kind.into(),
                        subject: "circle_1".into(),
                        reference: "container".into(),
                        axis: None,
                        value: None,
                    }],
                    dimensions: vec![],
                    arrays: vec![],
                    anchors: vec![],
                    metadata: serde_json::Value::Null,
                };
                let result = analyze(&p);
                assert!(
                    result.conflicts.iter().any(|c| {
                        c.conflict_type == ConflictType::TypeMismatch
                            && c.facts.message.contains("no interior")
                    }),
                    "{} relation referencing a {} must produce a type mismatch conflict",
                    kind,
                    container_kind
                );
            }
        }
    }

    #[test]
    fn parallel_perpendicular_require_line_entities() {
        let make_proposal = |kind: &str, subject: &str, reference: &str| Proposal {
            version: "1.0".into(),
            entities: vec![
                ProposalEntity {
                    id: "point_1".into(),
                    kind: "point".into(),
                },
                ProposalEntity {
                    id: "circle_1".into(),
                    kind: "circle".into(),
                },
                ProposalEntity {
                    id: "line_1".into(),
                    kind: "line".into(),
                },
                ProposalEntity {
                    id: "line_2".into(),
                    kind: "line".into(),
                },
                ProposalEntity {
                    id: "rect_1".into(),
                    kind: "rectangle".into(),
                },
                ProposalEntity {
                    id: "poly_1".into(),
                    kind: "polygon".into(),
                },
            ],
            relations: vec![ProposalRelation {
                kind: kind.into(),
                subject: subject.into(),
                reference: reference.into(),
                axis: None,
                value: None,
            }],
            dimensions: vec![],
            arrays: vec![],
            anchors: vec![],
            metadata: serde_json::Value::Null,
        };

        for kind in ["parallel_to", "perpendicular_to"] {
            // Point as subject
            let p = make_proposal(kind, "point_1", "line_1");
            let result = analyze(&p);
            assert!(
                result
                    .conflicts
                    .iter()
                    .any(|c| c.conflict_type == ConflictType::TypeMismatch),
                "{} with point subject must be rejected",
                kind
            );

            // Circle as subject
            let p = make_proposal(kind, "circle_1", "line_1");
            let result = analyze(&p);
            assert!(
                result
                    .conflicts
                    .iter()
                    .any(|c| c.conflict_type == ConflictType::TypeMismatch),
                "{} with circle subject must be rejected",
                kind
            );

            // Rectangle as subject
            let p = make_proposal(kind, "rect_1", "line_1");
            let result = analyze(&p);
            assert!(
                result
                    .conflicts
                    .iter()
                    .any(|c| c.conflict_type == ConflictType::TypeMismatch),
                "{} with rectangle subject must be rejected",
                kind
            );

            // Polygon as reference
            let p = make_proposal(kind, "line_1", "poly_1");
            let result = analyze(&p);
            assert!(
                result
                    .conflicts
                    .iter()
                    .any(|c| c.conflict_type == ConflictType::TypeMismatch),
                "{} with polygon reference must be rejected",
                kind
            );

            // Line-Line: valid
            let p = make_proposal(kind, "line_1", "line_2");
            let result = analyze(&p);
            assert!(
                result.conflicts.is_empty(),
                "{} between two lines must be accepted",
                kind
            );
        }
    }

    #[test]
    fn tangent_requires_at_least_one_curved_entity() {
        let make_proposal = |subject: &str, reference: &str| Proposal {
            version: "1.0".into(),
            entities: vec![
                ProposalEntity {
                    id: "point_1".into(),
                    kind: "point".into(),
                },
                ProposalEntity {
                    id: "line_1".into(),
                    kind: "line".into(),
                },
                ProposalEntity {
                    id: "circle_1".into(),
                    kind: "circle".into(),
                },
            ],
            relations: vec![ProposalRelation {
                kind: "tangent_to".into(),
                subject: subject.into(),
                reference: reference.into(),
                axis: None,
                value: None,
            }],
            dimensions: vec![],
            arrays: vec![],
            anchors: vec![],
            metadata: serde_json::Value::Null,
        };

        // Point-Point: invalid
        let result = analyze(&make_proposal("point_1", "point_1"));
        assert!(result
            .conflicts
            .iter()
            .any(|c| c.conflict_type == ConflictType::TypeMismatch));

        // Point-Line: invalid
        let result = analyze(&make_proposal("point_1", "line_1"));
        assert!(result
            .conflicts
            .iter()
            .any(|c| c.conflict_type == ConflictType::TypeMismatch));

        // Line-Line: invalid
        let result = analyze(&make_proposal("line_1", "line_1"));
        assert!(result
            .conflicts
            .iter()
            .any(|c| c.conflict_type == ConflictType::TypeMismatch));

        // Circle-Line: valid
        let result = analyze(&make_proposal("circle_1", "line_1"));
        assert!(result.conflicts.is_empty());
    }

    #[test]
    fn symmetric_about_point_without_axis_rejected() {
        let p = Proposal {
            version: "1.0".into(),
            entities: vec![
                ProposalEntity {
                    id: "circle_1".into(),
                    kind: "circle".into(),
                },
                ProposalEntity {
                    id: "point_1".into(),
                    kind: "point".into(),
                },
            ],
            relations: vec![ProposalRelation {
                kind: "symmetric_about".into(),
                subject: "circle_1".into(),
                reference: "point_1".into(),
                axis: None,
                value: None,
            }],
            dimensions: vec![],
            arrays: vec![],
            anchors: vec![],
            metadata: serde_json::Value::Null,
        };
        let result = analyze(&p);
        assert!(result
            .conflicts
            .iter()
            .any(|c| c.conflict_type == ConflictType::TypeMismatch));
    }

    #[test]
    fn aligned_with_point_rejected() {
        let p = Proposal {
            version: "1.0".into(),
            entities: vec![
                ProposalEntity {
                    id: "line_1".into(),
                    kind: "line".into(),
                },
                ProposalEntity {
                    id: "point_1".into(),
                    kind: "point".into(),
                },
            ],
            relations: vec![ProposalRelation {
                kind: "aligned_with".into(),
                subject: "line_1".into(),
                reference: "point_1".into(),
                axis: None,
                value: None,
            }],
            dimensions: vec![],
            arrays: vec![],
            anchors: vec![],
            metadata: serde_json::Value::Null,
        };
        let result = analyze(&p);
        assert!(result
            .conflicts
            .iter()
            .any(|c| c.conflict_type == ConflictType::TypeMismatch));
    }

    #[test]
    fn aligned_with_circle_rejected() {
        let p = Proposal {
            version: "1.0".into(),
            entities: vec![
                ProposalEntity {
                    id: "line_1".into(),
                    kind: "line".into(),
                },
                ProposalEntity {
                    id: "circle_1".into(),
                    kind: "circle".into(),
                },
            ],
            relations: vec![ProposalRelation {
                kind: "aligned_with".into(),
                subject: "line_1".into(),
                reference: "circle_1".into(),
                axis: None,
                value: None,
            }],
            dimensions: vec![],
            arrays: vec![],
            anchors: vec![],
            metadata: serde_json::Value::Null,
        };
        let result = analyze(&p);
        assert!(result
            .conflicts
            .iter()
            .any(|c| c.conflict_type == ConflictType::TypeMismatch));
    }

    #[test]
    fn x_y_position_dimensions_are_supported_and_preserved() {
        let mut p = simple_proposal();
        p.dimensions.push(ProposalDimension {
            target: "circle_1".into(),
            kind: "x".into(),
            value: -45.0,
        });
        p.dimensions.push(ProposalDimension {
            target: "circle_1".into(),
            kind: "y".into(),
            value: -25.0,
        });
        let result = analyze_detailed(&p).unwrap();
        let dims = &result.semantic_ir.dimensions;
        assert!(dims
            .iter()
            .any(|d| matches!(d, SemanticDimension::X { value, .. } if *value == -45.0)));
        assert!(dims
            .iter()
            .any(|d| matches!(d, SemanticDimension::Y { value, .. } if *value == -25.0)));
    }

    #[test]
    fn x_y_position_dimensions_on_lines_or_polygons_are_rejected() {
        for kind in ["line", "polygon"] {
            let p = Proposal {
                version: "1.0".into(),
                entities: vec![ProposalEntity {
                    id: "shape".into(),
                    kind: kind.into(),
                }],
                relations: vec![],
                dimensions: vec![ProposalDimension {
                    target: "shape".into(),
                    kind: "x".into(),
                    value: 50.0,
                }],
                arrays: vec![],
                anchors: vec![],
                metadata: serde_json::Value::Null,
            };
            let errors = analyze_detailed(&p).unwrap_err();
            assert!(
                errors.errors.iter().any(|error| matches!(
                    error,
                    AnalysisError::TypeMismatch { path, message, .. }
                        if path == "/dimensions/0" && message.contains("not meaningful")
                )),
                "{kind} must reject x dimensions"
            );
        }
    }

    #[test]
    fn array_preserved_as_node() {
        let mut p = simple_proposal();
        p.arrays.push(ProposalArray {
            id: "hole_array".into(),
            kind: "linear".into(),
            count: 4,
            spacing: Some(20.0),
            element: "circle_1".into(),
            rows: None,
            cols: None,
            row_spacing: None,
            col_spacing: None,
            radius: None,
            direction: None,
        });
        let result = analyze(&p);
        assert!(result.conflicts.is_empty());
        let ir = result.semantic_ir.unwrap();
        assert_eq!(ir.arrays.len(), 1);
        assert!(ir
            .graph
            .nodes
            .iter()
            .any(|n| n.node_kind == NodeKind::Array));
    }

    #[test]
    fn invalid_array_shape_is_rejected_before_lowering() {
        let mut p = simple_proposal();
        p.arrays.push(ProposalArray {
            id: "bad_array".into(),
            kind: "linear".into(),
            count: 2,
            spacing: Some(10.0),
            element: "circle_1".into(),
            rows: None,
            cols: None,
            row_spacing: None,
            col_spacing: None,
            radius: None,
            direction: Some("diagonal".into()),
        });

        let errors = analyze_detailed(&p).unwrap_err();
        assert!(errors.errors.iter().any(|error| {
            matches!(
                error,
                AnalysisError::TypeMismatch { path, message, .. }
                    if path == "/arrays/0" && message.contains("direction")
            )
        }));
    }

    // ── P0-1 regression: distance value preservation ─────────────

    #[test]
    fn distance_value_preserved_in_semantic_ir() {
        let p = Proposal {
            version: "1.0".into(),
            entities: vec![
                ProposalEntity {
                    id: "a".into(),
                    kind: "circle".into(),
                },
                ProposalEntity {
                    id: "b".into(),
                    kind: "circle".into(),
                },
            ],
            relations: vec![ProposalRelation {
                kind: "distance".into(),
                subject: "a".into(),
                reference: "b".into(),
                axis: None,
                value: Some(42.5),
            }],
            dimensions: vec![],
            arrays: vec![],
            anchors: vec![],
            metadata: serde_json::Value::Null,
        };
        let result = analyze(&p);
        assert!(result.conflicts.is_empty());
        let ir = result.semantic_ir.unwrap();
        match &ir.relations[0] {
            SemanticRelation::DistanceFrom { value, .. } => {
                assert_eq!(*value, 42.5);
            }
            _ => panic!("expected DistanceFrom"),
        }
    }

    // NOTE: distance_without_value_defaults_to_zero was removed.
    // The previous behavior (defaulting a missing distance value to 0.0) was
    // identified as P0-3: silent data loss / intent corruption. A distance
    // relation without a value is now rejected by scc-proposal::validate,
    // so it never reaches semantic analysis. See:
    //   crates/scc-proposal/src/validate.rs::distance_relation_requires_value

    #[test]
    fn different_distance_values_produce_different_semantic_ir() {
        let make_proposal = |v: f64| Proposal {
            version: "1.0".into(),
            entities: vec![
                ProposalEntity {
                    id: "a".into(),
                    kind: "circle".into(),
                },
                ProposalEntity {
                    id: "b".into(),
                    kind: "circle".into(),
                },
            ],
            relations: vec![ProposalRelation {
                kind: "distance".into(),
                subject: "a".into(),
                reference: "b".into(),
                axis: None,
                value: Some(v),
            }],
            dimensions: vec![],
            arrays: vec![],
            anchors: vec![],
            metadata: serde_json::Value::Null,
        };

        let ir_a = analyze(&make_proposal(10.0)).semantic_ir.unwrap();
        let ir_b = analyze(&make_proposal(20.0)).semantic_ir.unwrap();
        let json_a = serde_json::to_string(&ir_a).unwrap();
        let json_b = serde_json::to_string(&ir_b).unwrap();
        assert_ne!(
            json_a, json_b,
            "different distance values must produce different output"
        );
    }

    #[test]
    fn distance_value_roundtrips_through_json_parse() {
        // Simulate end-to-end: JSON string → Proposal → analysis → SemanticIR
        let json_input = r#"{
            "version": "1.0",
            "entities": [
                { "id": "c1", "kind": "circle" },
                { "id": "c2", "kind": "circle" }
            ],
            "relations": [
                { "kind": "distance", "subject": "c1", "reference": "c2", "value": 77.0 }
            ],
            "dimensions": [],
            "arrays": [],
            "anchors": [],
            "metadata": {}
        }"#;
        let proposal: Proposal = serde_json::from_str(json_input).unwrap();
        let result = analyze(&proposal);
        let ir = result.semantic_ir.unwrap();
        match &ir.relations[0] {
            SemanticRelation::DistanceFrom { value, .. } => {
                assert_eq!(*value, 77.0);
            }
            _ => panic!("expected DistanceFrom"),
        }
    }

    // ── P0-2: unknown proposal kinds are reported, not silently dropped ─

    #[test]
    fn unknown_relation_kind_produces_conflict() {
        let mut p = simple_proposal();
        p.relations.push(ProposalRelation {
            kind: "unknown_relation".into(),
            subject: "rect_1".into(),
            reference: "circle_1".into(),
            axis: None,
            value: None,
        });
        let result = analyze(&p);
        assert!(
            result.semantic_ir.is_none(),
            "unknown relation kind must prevent semantic IR"
        );
        assert!(result.conflicts.iter().any(|c| {
            c.conflict_type == ConflictType::TypeMismatch
                && c.facts
                    .message
                    .contains("unsupported relation kind: unknown_relation")
        }));
    }

    #[test]
    fn type_error_paths_keep_original_proposal_indices() {
        let mut proposal = simple_proposal();
        proposal.entities.push(ProposalEntity {
            id: "line_1".into(),
            kind: "line".into(),
        });
        proposal.relations = vec![
            ProposalRelation {
                kind: "unsupported_relation".into(),
                subject: "rect_1".into(),
                reference: "line_1".into(),
                axis: None,
                value: None,
            },
            ProposalRelation {
                kind: "tangent_to".into(),
                subject: "rect_1".into(),
                reference: "line_1".into(),
                axis: None,
                value: None,
            },
        ];

        let errors = analyze_detailed(&proposal).unwrap_err().errors;
        assert!(errors.iter().any(|error| matches!(
            error,
            AnalysisError::TypeMismatch { path, message, .. }
                if path == "/relations/0" && message.contains("unsupported relation")
        )));
        assert!(errors.iter().any(|error| matches!(
            error,
            AnalysisError::TypeMismatch { path, message, .. }
                if path == "/relations/1" && message.contains("tangent relation")
        )));
        assert!(errors.iter().all(|error| match error {
            AnalysisError::TypeMismatch { path, .. } => !path.is_empty(),
            _ => true,
        }));
    }

    #[test]
    fn unknown_dimension_kind_produces_conflict() {
        let mut p = simple_proposal();
        p.dimensions.push(ProposalDimension {
            target: "circle_1".into(),
            kind: "unknown_dimension".into(),
            value: 0.0,
        });
        let result = analyze(&p);
        assert!(
            result.semantic_ir.is_none(),
            "unknown dimension kind must prevent semantic IR"
        );
        assert!(result.conflicts.iter().any(|c| {
            c.conflict_type == ConflictType::TypeMismatch
                && c.facts
                    .message
                    .contains("unsupported dimension kind: unknown_dimension")
        }));
    }

    #[test]
    fn diagnostic_collector_accumulates_builder_and_checker_diagnostics() {
        let mut p = simple_proposal();
        // Builder diagnostic: unknown relation kind
        p.relations.push(ProposalRelation {
            kind: "bad_relation".into(),
            subject: "rect_1".into(),
            reference: "circle_1".into(),
            axis: None,
            value: None,
        });
        // Builder diagnostic: unknown dimension kind
        p.dimensions.push(ProposalDimension {
            target: "circle_1".into(),
            kind: "bad_dimension".into(),
            value: 0.0,
        });
        // Checker diagnostic: missing reference
        p.relations.push(ProposalRelation {
            kind: "parallel_to".into(),
            subject: "rect_1".into(),
            reference: "missing_entity".into(),
            axis: None,
            value: None,
        });

        let result = analyze(&p);
        assert!(
            result.semantic_ir.is_none(),
            "accumulated diagnostics must prevent semantic IR"
        );
        assert_eq!(
            result.conflicts.len(),
            4,
            "expected diagnostics from two builders and two checkers"
        );
        assert!(result
            .conflicts
            .iter()
            .any(|c| c.facts.message.contains("bad_relation")));
        assert!(result
            .conflicts
            .iter()
            .any(|c| c.facts.message.contains("bad_dimension")));
        assert!(result
            .conflicts
            .iter()
            .any(|c| c.facts.message.contains("missing_entity")));
        assert!(result.conflicts.iter().any(|c| c
            .facts
            .message
            .contains("parallel relation requires a line entity")));
    }
    #[test]
    fn unknown_anchor_kind_produces_conflict() {
        let mut p = simple_proposal();
        p.anchors.push(ProposalAnchor {
            id: "anchor_1".into(),
            kind: "unknown_anchor".into(),
            entity: None,
        });
        let result = analyze(&p);
        assert!(
            result.semantic_ir.is_none(),
            "unknown anchor kind must prevent semantic IR"
        );
        assert!(result.conflicts.iter().any(|c| {
            c.conflict_type == ConflictType::TypeMismatch
                && c.facts
                    .message
                    .contains("unsupported anchor kind: unknown_anchor")
        }));
    }

    #[test]
    fn conflicting_dimensions_produce_mcs_conflict() {
        let mut p = simple_proposal();
        p.dimensions.push(ProposalDimension {
            target: "circle_1".into(),
            kind: "diameter".into(),
            value: 10.0,
        });
        p.dimensions.push(ProposalDimension {
            target: "circle_1".into(),
            kind: "diameter".into(),
            value: 20.0,
        });
        let result = analyze(&p);
        assert!(
            result.semantic_ir.is_none(),
            "conflicting dimensions must prevent semantic IR"
        );
        let conflict = result
            .conflicts
            .iter()
            .find(|c| {
                c.facts
                    .message
                    .contains("conflicting diameter values for entity circle_1")
            })
            .expect("expected an MCS conflict for diameter");
        assert_eq!(
            conflict.mcs.conflicting_constraints,
            vec![
                ConstraintId("dim_0".into()),
                ConstraintId("dim_2".into()),
                ConstraintId("dim_3".into())
            ]
        );
    }

    #[test]
    fn conflicting_relations_produce_mcs_conflict() {
        let mut p = simple_proposal();
        p.relations.push(ProposalRelation {
            kind: "inside".into(),
            subject: "rect_1".into(),
            reference: "circle_1".into(),
            axis: None,
            value: None,
        });
        p.relations.push(ProposalRelation {
            kind: "outside".into(),
            subject: "rect_1".into(),
            reference: "circle_1".into(),
            axis: None,
            value: None,
        });
        let result = analyze(&p);
        assert!(
            result.semantic_ir.is_none(),
            "conflicting relations must prevent semantic IR"
        );
        let conflict = result
            .conflicts
            .iter()
            .find(|c| {
                c.facts
                    .message
                    .contains("inside and outside relations conflict")
            })
            .expect("expected an MCS conflict for inside/outside");
        assert_eq!(
            conflict.mcs.conflicting_constraints,
            vec![ConstraintId("rel_1".into()), ConstraintId("rel_2".into())]
        );
    }
}
