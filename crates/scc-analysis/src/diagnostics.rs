use scc_core::conflict::{
    Conflict, ConflictCategory, ConflictFacts, ConflictType, MinimalConflictSubgraph,
};
use scc_core::ids::EntityId;

/// Accumulates analysis diagnostics without changing builder signatures.
///
/// Builder and checker functions in `scc-analysis` take `&mut DiagnosticCollector`
/// and emit conflicts as they are discovered. The collector is then converted into
/// the final `Vec<Conflict>` by `analyze()`.
#[derive(Debug, Default)]
pub struct DiagnosticCollector {
    conflicts: Vec<Conflict>,
}

impl DiagnosticCollector {
    /// Creates an empty collector.
    pub fn new() -> Self {
        DiagnosticCollector {
            conflicts: Vec::new(),
        }
    }

    /// Records a structural/semantic conflict.
    pub fn emit(
        &mut self,
        category: ConflictCategory,
        conflict_type: ConflictType,
        message: String,
        affected_entities: Vec<EntityId>,
    ) {
        self.conflicts.push(Conflict {
            category,
            conflict_type,
            mcs: MinimalConflictSubgraph {
                conflicting_constraints: vec![],
                conflict_type,
            },
            facts: ConflictFacts {
                affected_entities,
                affected_constraints: vec![],
                message,
            },
        });
    }

    /// Returns true if no diagnostics have been emitted.
    pub fn is_empty(&self) -> bool {
        self.conflicts.is_empty()
    }

    /// Returns the number of emitted diagnostics.
    pub fn len(&self) -> usize {
        self.conflicts.len()
    }

    /// Consumes the collector and returns the accumulated conflicts.
    pub fn into_conflicts(self) -> Vec<Conflict> {
        self.conflicts
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_collector_is_empty() {
        let diag = DiagnosticCollector::new();
        assert!(diag.is_empty());
        assert_eq!(diag.len(), 0);
    }

    #[test]
    fn emitted_conflict_is_accumulated() {
        let mut diag = DiagnosticCollector::new();
        diag.emit(
            ConflictCategory::Semantic,
            ConflictType::TypeMismatch,
            "unsupported relation kind: foo".into(),
            vec![EntityId("a".into()), EntityId("b".into())],
        );

        assert!(!diag.is_empty());
        assert_eq!(diag.len(), 1);

        let conflicts = diag.into_conflicts();
        assert_eq!(conflicts.len(), 1);
        assert_eq!(conflicts[0].category, ConflictCategory::Semantic);
        assert_eq!(conflicts[0].conflict_type, ConflictType::TypeMismatch);
        assert_eq!(conflicts[0].facts.message, "unsupported relation kind: foo");
        assert_eq!(
            conflicts[0].facts.affected_entities,
            vec![EntityId("a".into()), EntityId("b".into())]
        );
        assert!(conflicts[0].facts.affected_constraints.is_empty());
    }

    #[test]
    fn multiple_emits_are_accumulated() {
        let mut diag = DiagnosticCollector::new();
        diag.emit(
            ConflictCategory::Structural,
            ConflictType::MissingReference,
            "missing".into(),
            vec![EntityId("x".into())],
        );
        diag.emit(
            ConflictCategory::Semantic,
            ConflictType::TypeMismatch,
            "mismatch".into(),
            vec![EntityId("y".into())],
        );

        assert_eq!(diag.len(), 2);
        let conflicts = diag.into_conflicts();
        assert_eq!(conflicts[0].conflict_type, ConflictType::MissingReference);
        assert_eq!(conflicts[1].conflict_type, ConflictType::TypeMismatch);
    }
}
