use std::collections::BTreeMap;

use scc_core::ids::EntityId;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SolveStatus {
    FullyConstrained,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SolveMethod {
    DeterministicDirect,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConvergenceStatus {
    Exact,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DegreesOfFreedomReport {
    pub remaining: u32,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub entities: Vec<EntityId>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GeometrySolveReport {
    pub status: SolveStatus,
    pub method: SolveMethod,
    pub convergence: ConvergenceStatus,
    pub evaluated_constraints: usize,
    pub degrees_of_freedom: DegreesOfFreedomReport,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Point2 {
    pub x: f64,
    pub y: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum GeometryPrimitive {
    Point {
        position: Point2,
    },
    Line {
        start: Point2,
        end: Point2,
    },
    Circle {
        center: Point2,
        radius: f64,
    },
    Arc {
        center: Point2,
        radius: f64,
        start_angle: f64,
        end_angle: f64,
    },
    Rectangle {
        origin: Point2,
        width: f64,
        height: f64,
    },
    Polygon {
        vertices: Vec<Point2>,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GeometryEntity {
    pub id: EntityId,
    pub primitive: GeometryPrimitive,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GeometryModel {
    pub source_root: u64,
    pub source_step: u64,
    pub solve_report: GeometrySolveReport,
    pub entities: BTreeMap<EntityId, GeometryEntity>,
}

impl GeometryModel {
    pub fn new(source_root: u64, source_step: u64, evaluated_constraints: usize) -> Self {
        Self {
            source_root,
            source_step,
            solve_report: GeometrySolveReport {
                status: SolveStatus::FullyConstrained,
                method: SolveMethod::DeterministicDirect,
                convergence: ConvergenceStatus::Exact,
                evaluated_constraints,
                degrees_of_freedom: DegreesOfFreedomReport {
                    remaining: 0,
                    entities: Vec::new(),
                },
            },
            entities: BTreeMap::new(),
        }
    }
}
