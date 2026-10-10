//! Parameter dependency graph.
//!
//! The graph is **derived** from [`crate::model::EngineeringModel`] parameters
//! (ADR-016 Decision 4): it is computed on demand and is never a second source
//! of truth. It supports dependency collection, deterministic topological
//! ordering, cycle detection (iterative — no recursion, so no stack overflow),
//! and "affected" (transitive dependent) queries.

use std::collections::{BTreeMap, BTreeSet};

use crate::error::EngineeringError;
use crate::model::Parameter;
use crate::parameter::ParameterValue;
use crate::ParameterId;

/// Directed dependency graph over parameters: `edges[id]` is the set of
/// parameters that `id`'s expression references (the parameters it depends on).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ParameterDependencyGraph {
    edges: BTreeMap<ParameterId, BTreeSet<ParameterId>>,
}

impl ParameterDependencyGraph {
    /// Build the graph from a parameter map. Every parameter whose value is an
    /// [`ParameterValue::Expression`] contributes its referenced parameters;
    /// all other parameters have no outgoing edges.
    pub fn from_parameters(parameters: &BTreeMap<ParameterId, Parameter>) -> Self {
        let mut edges = BTreeMap::new();
        for (id, parameter) in parameters {
            let dependencies = match &parameter.value {
                ParameterValue::Expression(expression) => expression.referenced_parameters(),
                _ => BTreeSet::new(),
            };
            edges.insert(id.clone(), dependencies);
        }
        Self { edges }
    }

    /// The parameters `id` depends on directly.
    pub fn depends_on(&self, id: &ParameterId) -> BTreeSet<ParameterId> {
        self.edges.get(id).cloned().unwrap_or_default()
    }

    /// The parameters that depend on `id` directly.
    pub fn dependents(&self, id: &ParameterId) -> BTreeSet<ParameterId> {
        let mut result = BTreeSet::new();
        for (node, dependencies) in &self.edges {
            if dependencies.contains(id) {
                result.insert(node.clone());
            }
        }
        result
    }

    /// All parameters transitively affected by changing `id` (direct and
    /// indirect dependents). `id` itself is not included.
    pub fn affected(&self, id: &ParameterId) -> BTreeSet<ParameterId> {
        let mut result = BTreeSet::new();
        let mut stack: Vec<ParameterId> = self.dependents(id).into_iter().collect();
        while let Some(node) = stack.pop() {
            if result.insert(node.clone()) {
                for dependent in self.dependents(&node) {
                    stack.push(dependent);
                }
            }
        }
        result
    }

    /// All parameters `id` transitively depends on (direct and indirect
    /// dependencies). `id` itself is not included.
    pub fn transitive_dependencies(&self, id: &ParameterId) -> BTreeSet<ParameterId> {
        let mut result = BTreeSet::new();
        let mut stack: Vec<ParameterId> = self.depends_on(id).into_iter().collect();
        while let Some(node) = stack.pop() {
            if result.insert(node.clone()) {
                for dependency in self.depends_on(&node) {
                    stack.push(dependency);
                }
            }
        }
        result
    }

    /// Replace the direct dependencies of `id`. Used to check a *would-be*
    /// expression for cycles before it is committed (validate-before-write).
    pub fn set_dependencies(&mut self, id: ParameterId, dependencies: BTreeSet<ParameterId>) {
        self.edges.insert(id, dependencies);
    }

    /// Deterministic topological order (Kahn's algorithm). Errors with the
    /// first detected cycle when the graph is not a DAG.
    pub fn topological_order(&self) -> Result<Vec<ParameterId>, EngineeringError> {
        self.topological_order_restricted(&self.edges.keys().cloned().collect())
    }

    /// Deterministic topological order restricted to `subset`, considering only
    /// edges whose endpoints are both inside `subset`. Errors on a cycle.
    pub fn topological_order_restricted(
        &self,
        subset: &BTreeSet<ParameterId>,
    ) -> Result<Vec<ParameterId>, EngineeringError> {
        let mut indegree: BTreeMap<ParameterId, usize> =
            subset.iter().map(|id| (id.clone(), 0)).collect();
        for node in subset {
            // Indegree of `node` = the number of parameters it depends on
            // (its predecessors), restricted to the subset.
            for dependency in self.depends_on(node) {
                if subset.contains(&dependency) {
                    *indegree.get_mut(node).expect("subset node") += 1;
                }
            }
        }

        let mut ready: BTreeSet<ParameterId> = indegree
            .iter()
            .filter(|(_, degree)| **degree == 0)
            .map(|(id, _)| id.clone())
            .collect();
        let mut order = Vec::with_capacity(subset.len());
        while let Some(node) = ready.pop_first() {
            order.push(node.clone());
            // A node's *dependents* (successors) have one pending prerequisite
            // satisfied when this node is removed.
            for dependent in self.dependents(&node) {
                if subset.contains(&dependent) {
                    let degree = indegree.get_mut(&dependent).expect("subset node");
                    *degree -= 1;
                    if *degree == 0 {
                        ready.insert(dependent);
                    }
                }
            }
        }

        if order.len() != subset.len() {
            let cycle = self.find_cycle().unwrap_or_default();
            return Err(EngineeringError::ExpressionCycle {
                cycle: cycle.iter().map(|id| id.0.clone()).collect(),
            });
        }
        Ok(order)
    }

    /// Return one deterministic cycle as a path `a → b → c → a` if the graph
    /// contains a cycle, using an iterative white/gray/black DFS (no recursion).
    pub fn find_cycle(&self) -> Option<Vec<ParameterId>> {
        const WHITE: u8 = 0;
        const GRAY: u8 = 1;
        const BLACK: u8 = 2;

        let mut color: BTreeMap<ParameterId, u8> = BTreeMap::new();
        let nodes: Vec<ParameterId> = self.edges.keys().cloned().collect();

        for start in &nodes {
            if color.get(start).copied().unwrap_or(WHITE) == BLACK {
                continue;
            }

            // Explicit recursion emulation. `path` is the current gray stack;
            // `pending[level]` holds the not-yet-visited children of
            // `path[level]`, aligned by index.
            let mut path: Vec<ParameterId> = vec![start.clone()];
            let mut pending: Vec<Vec<ParameterId>> =
                vec![self.depends_on(start).into_iter().collect()];
            color.insert(start.clone(), GRAY);

            'search: while !path.is_empty() {
                let level = path.len() - 1;
                let mut descended = false;

                while let Some(child) = pending[level].pop() {
                    match color.get(&child).copied().unwrap_or(WHITE) {
                        BLACK => continue,
                        GRAY => {
                            // Back edge: child is already on the current path.
                            let position = path
                                .iter()
                                .position(|node| node == &child)
                                .expect("gray node on path");
                            let mut cycle: Vec<ParameterId> = path[position..].to_vec();
                            cycle.push(child);
                            return Some(cycle);
                        }
                        _ => {
                            path.push(child.clone());
                            pending.push(self.depends_on(&child).into_iter().collect());
                            color.insert(child.clone(), GRAY);
                            descended = true;
                            break;
                        }
                    }
                }

                if descended {
                    continue 'search;
                }

                // No unvisited children left: this node is finished (black).
                let finished = path.pop().expect("path non-empty");
                pending.pop();
                color.insert(finished, BLACK);
            }
        }

        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use scc_core::authority::AuthorityLevel;

    use crate::expression::Expression;
    use crate::model::{Parameter, ParameterOrigin};

    fn param(id: &str, expression: Option<Expression>) -> Parameter {
        let value = match expression {
            Some(expression) => ParameterValue::Expression(expression),
            None => ParameterValue::Unresolved,
        };
        Parameter::new(
            ParameterId(id.into()),
            value,
            ParameterOrigin::Derived,
            AuthorityLevel::L0,
        )
    }

    fn graph(params: &[(&str, Option<Expression>)]) -> ParameterDependencyGraph {
        let mut map = BTreeMap::new();
        for (id, expression) in params {
            let p = param(id, expression.clone());
            map.insert(p.id.clone(), p);
        }
        ParameterDependencyGraph::from_parameters(&map)
    }

    fn reference(id: &str) -> Expression {
        Expression::Reference {
            parameter: ParameterId(id.into()),
        }
    }

    #[test]
    fn graph_collects_expression_dependencies() {
        let g = graph(&[
            ("a", None),
            (
                "b",
                Some(Expression::divide_parameter_by_constant(
                    ParameterId("a".into()),
                    2.0,
                )),
            ),
            (
                "c",
                Some(Expression::Add {
                    left: Box::new(reference("a")),
                    right: Box::new(reference("b")),
                }),
            ),
        ]);
        assert_eq!(g.depends_on(&ParameterId("a".into())), BTreeSet::new());
        assert_eq!(
            g.depends_on(&ParameterId("b".into())),
            BTreeSet::from([ParameterId("a".into())])
        );
        assert_eq!(
            g.depends_on(&ParameterId("c".into())),
            BTreeSet::from([ParameterId("a".into()), ParameterId("b".into())])
        );
    }

    #[test]
    fn topological_order_is_deterministic_and_respects_dependencies() {
        // c depends on b depends on a; d independent.
        let g = graph(&[
            ("d", None),
            ("a", None),
            ("b", Some(reference("a"))),
            (
                "c",
                Some(Expression::Add {
                    left: Box::new(reference("b")),
                    right: Box::new(reference("a")),
                }),
            ),
        ]);
        let order = g.topological_order().unwrap();
        assert_eq!(order.len(), 4);
        let pos = |id: &str| order.iter().position(|p| p.0 == id).unwrap();
        assert!(pos("a") < pos("b"));
        assert!(pos("a") < pos("c"));
        assert!(pos("b") < pos("c"));
    }

    #[test]
    fn detects_direct_and_indirect_cycles() {
        // a -> b -> a
        let g = graph(&[("a", Some(reference("b"))), ("b", Some(reference("a")))]);
        let cycle = g.find_cycle().unwrap();
        assert_eq!(cycle.len(), 3);
        assert_eq!(cycle.first(), cycle.last(), "cycle must close on itself");
        assert!(g.topological_order().is_err());

        // a -> b -> c -> a
        let g = graph(&[
            ("a", Some(reference("b"))),
            ("b", Some(reference("c"))),
            ("c", Some(reference("a"))),
        ]);
        assert!(g.find_cycle().is_some());
        assert!(matches!(
            g.topological_order().unwrap_err(),
            EngineeringError::ExpressionCycle { .. }
        ));
    }

    #[test]
    fn self_cycle_is_detected() {
        let g = graph(&[("a", Some(reference("a")))]);
        assert!(g.find_cycle().is_some());
    }

    #[test]
    fn acyclic_graph_has_no_cycle() {
        let g = graph(&[
            ("a", None),
            ("b", Some(reference("a"))),
            (
                "c",
                Some(Expression::Add {
                    left: Box::new(reference("a")),
                    right: Box::new(reference("b")),
                }),
            ),
        ]);
        assert!(g.find_cycle().is_none());
    }

    #[test]
    fn affected_reports_transitive_dependents() {
        let g = graph(&[
            ("a", None),
            ("b", Some(reference("a"))),
            ("c", Some(reference("b"))),
            ("d", None),
        ]);
        let affected = g.affected(&ParameterId("a".into()));
        assert_eq!(
            affected,
            BTreeSet::from([ParameterId("b".into()), ParameterId("c".into())])
        );
        assert!(g.affected(&ParameterId("d".into())).is_empty());
    }

    #[test]
    fn transitive_dependencies_walk_upstream() {
        let g = graph(&[
            ("a", None),
            ("b", Some(reference("a"))),
            ("c", Some(reference("b"))),
        ]);
        assert_eq!(
            g.transitive_dependencies(&ParameterId("c".into())),
            BTreeSet::from([ParameterId("a".into()), ParameterId("b".into())])
        );
    }

    #[test]
    fn restricted_order_and_cycle_check_via_set_dependencies() {
        // a -> b currently; then b -> a (would-be) must be rejected.
        let mut g = graph(&[("a", None), ("b", Some(reference("a")))]);
        assert!(g.find_cycle().is_none());

        g.set_dependencies(
            ParameterId("a".into()),
            BTreeSet::from([ParameterId("b".into())]),
        );
        assert!(g.find_cycle().is_some());
        assert!(g.topological_order().is_err());
    }
}
