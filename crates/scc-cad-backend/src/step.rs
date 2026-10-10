//! Transport types for validated, ephemeral STEP delivery.

use serde::{Deserialize, Serialize};

use crate::{SolidBackendFailure, SolidBuildResult, SolidOperationResult, SolidOperationStatus};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StepArtifact {
    pub extension: String,
    pub size_bytes: u64,
    pub sha256: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StepSolidWitness {
    pub valid: bool,
    pub solid_count: u32,
    pub bbox: crate::SolidResultBounds,
    pub volume_mm3: f64,
    pub surface_area_mm2: f64,
    pub center_of_mass: [f64; 3],
    pub edge_count: u32,
    pub face_count: u32,
    pub vertex_count: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StepMaterialProbe {
    pub label: String,
    pub point: [f64; 3],
    pub source_material: bool,
    pub reimported_material: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StepMaterialWitness {
    pub probe_count: u32,
    pub all_match: bool,
    pub probes: Vec<StepMaterialProbe>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StepEquivalence {
    pub bbox_match: bool,
    pub volume_match: bool,
    pub surface_area_match: bool,
    pub center_of_mass_match: bool,
    pub topology_match: bool,
    pub feature_geometry_match: bool,
    pub material_witness_match: bool,
    pub all_match: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct StepDeliveryTimings {
    pub manufacturing_lowering_ms: f64,
    pub manufacturing_build_ms: f64,
    pub step_serialization_ms: f64,
    pub step_reimport_ms: f64,
    pub equivalence_validation_ms: f64,
    pub total_ms: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StepDeliveryResult {
    pub schema_version: u32,
    pub format: String,
    pub unit: String,
    pub coordinate_frame: String,
    pub artifact: StepArtifact,
    pub source_solid: StepSolidWitness,
    pub reimported_solid: StepSolidWitness,
    pub equivalence: StepEquivalence,
    pub material_witness: StepMaterialWitness,
    pub operation_results: Vec<SolidOperationResult>,
    pub body_source: crate::SolidBodyResultSource,
    pub timings_ms: StepDeliveryTimings,
}

/// The exact bytes that passed STEP validation. The Rust adapter owns the
/// temporary file, hashes these bytes after validation, and returns this same
/// buffer to its caller; no second export is possible through this type.
#[derive(Debug, Clone, PartialEq)]
pub struct ValidatedStepArtifact {
    pub bytes: Vec<u8>,
    pub sha256: String,
    pub size_bytes: u64,
    pub witness: StepDeliveryResult,
}

#[derive(Debug, Deserialize)]
pub(crate) struct StepWireResponse {
    pub ok: bool,
    #[serde(default)]
    pub schema_version: Option<u32>,
    #[serde(default)]
    pub format: Option<String>,
    #[serde(default)]
    pub unit: Option<String>,
    #[serde(default)]
    pub coordinate_frame: Option<String>,
    #[serde(default)]
    pub artifact: Option<StepArtifact>,
    #[serde(default)]
    pub source_solid: Option<StepSolidWitness>,
    #[serde(default)]
    pub reimported_solid: Option<StepSolidWitness>,
    #[serde(default)]
    pub equivalence: Option<StepEquivalence>,
    #[serde(default)]
    pub material_witness: Option<StepMaterialWitness>,
    #[serde(default)]
    pub operation_results: Option<Vec<SolidOperationResult>>,
    #[serde(default)]
    pub body_source: Option<crate::SolidBodyResultSource>,
    #[serde(default)]
    pub timings_ms: StepDeliveryTimings,
    #[serde(default)]
    pub error: Option<SolidBackendFailure>,
}

impl StepWireResponse {
    pub(crate) fn into_result(self) -> Result<StepDeliveryResult, String> {
        Ok(StepDeliveryResult {
            schema_version: required(self.schema_version, "schema_version")?,
            format: required(self.format, "format")?,
            unit: required(self.unit, "unit")?,
            coordinate_frame: required(self.coordinate_frame, "coordinate_frame")?,
            artifact: required(self.artifact, "artifact")?,
            source_solid: required(self.source_solid, "source_solid")?,
            reimported_solid: required(self.reimported_solid, "reimported_solid")?,
            equivalence: required(self.equivalence, "equivalence")?,
            material_witness: required(self.material_witness, "material_witness")?,
            operation_results: required(self.operation_results, "operation_results")?,
            body_source: required(self.body_source, "body_source")?,
            timings_ms: self.timings_ms,
        })
    }
}

fn required<T>(value: Option<T>, field: &str) -> Result<T, String> {
    value.ok_or_else(|| format!("missing {field}"))
}

pub(crate) fn validate_step_result(
    spec: &scc_manufacturing::SolidBuildSpec,
    result: &StepDeliveryResult,
) -> Result<(), String> {
    validate_common_step_result(result, true)?;
    if result.operation_results.len() != spec.operations.len() {
        return Err(
            "STEP operation provenance cardinality does not match the manufacturing spec".into(),
        );
    }
    let expected_ids: Vec<(String, String)> = spec
        .operations
        .keys()
        .map(|id| {
            (
                crate::operation_id_wire(id),
                crate::source_id_wire(spec.operation_source(*id)),
            )
        })
        .collect();
    let actual_ids: Vec<(String, String)> = result
        .operation_results
        .iter()
        .map(|operation| (operation.operation_id.clone(), operation.source_id.clone()))
        .collect();
    if expected_ids != actual_ids {
        return Err("STEP operation provenance was not preserved".into());
    }
    if result.body_source.feature != crate::source_string(&spec.source_map.body.feature)
        || result.body_source.entity != crate::source_string(&spec.source_map.body.entity)
    {
        return Err("STEP body provenance was not preserved".into());
    }
    Ok(())
}

pub(crate) fn validate_parametric_step_result(
    spec: &crate::ParametricRebuildRequest,
    result: &StepDeliveryResult,
) -> Result<(), String> {
    // Parametric Project v2 compares exact geometric measurements and
    // analytic feature descriptors. Kernel edge/face cardinalities are
    // retained as diagnostics, not treated as EngineeringModel identity.
    validate_common_step_result(result, false)?;
    if result.operation_results.len() != spec.operations.len() {
        return Err("STEP operation provenance cardinality does not match the parametric manufacturing spec".into());
    }
    let expected_ids: Vec<(String, String)> = spec
        .operations
        .iter()
        .map(|operation| {
            (
                operation.feature_id().to_owned(),
                operation.source_feature_id().to_owned(),
            )
        })
        .collect();
    let actual_ids: Vec<(String, String)> = result
        .operation_results
        .iter()
        .map(|operation| (operation.operation_id.clone(), operation.source_id.clone()))
        .collect();
    if expected_ids != actual_ids
        || result
            .operation_results
            .iter()
            .any(|operation| operation.status != SolidOperationStatus::Applied)
    {
        return Err("STEP operation provenance/status was not preserved".into());
    }
    let first_add = spec
        .operations
        .iter()
        .find(|operation| match operation {
            crate::ParametricRebuildOperation::Extrude(operation) => {
                operation.operation == crate::ParametricBooleanOperation::Add
            }
            crate::ParametricRebuildOperation::Revolve(operation) => {
                operation.operation == crate::ParametricBooleanOperation::Add
            }
            crate::ParametricRebuildOperation::Sweep(operation) => {
                operation.operation == crate::ParametricBooleanOperation::Add
            }
            crate::ParametricRebuildOperation::Loft(operation) => {
                operation.operation == crate::ParametricBooleanOperation::Add
            }
            crate::ParametricRebuildOperation::Countersink(_)
            | crate::ParametricRebuildOperation::Fillet { .. }
            | crate::ParametricRebuildOperation::Chamfer { .. } => false,
        })
        .ok_or_else(|| "parametric STEP input has no ADD body source".to_owned())?;
    let body_source = first_add.source_feature_id();
    if result.body_source.feature != body_source || result.body_source.entity != body_source {
        return Err("parametric STEP body provenance was not preserved".into());
    }
    Ok(())
}

fn validate_common_step_result(
    result: &StepDeliveryResult,
    require_topology_match: bool,
) -> Result<(), String> {
    if result.schema_version != scc_manufacturing::SOLID_BUILD_SPEC_SCHEMA_VERSION
        || result.format != "step"
        || result.unit != "mm"
        || result.coordinate_frame != "PartLocal"
        || result.artifact.extension != ".step"
        || result.artifact.size_bytes == 0
        || result.artifact.sha256.len() != 64
        || !result
            .artifact
            .sha256
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit())
        || !valid_witness(&result.source_solid)
        || !valid_witness(&result.reimported_solid)
        || !valid_timings(&result.timings_ms)
        || !result.equivalence.bbox_match
        || !result.equivalence.volume_match
        || !result.equivalence.surface_area_match
        || !result.equivalence.center_of_mass_match
        || (require_topology_match && !result.equivalence.topology_match)
        || !result.equivalence.feature_geometry_match
        || !result.equivalence.material_witness_match
        || !result.equivalence.all_match
        || !result.material_witness.all_match
        || result.material_witness.probe_count as usize != result.material_witness.probes.len()
        || result
            .material_witness
            .probes
            .iter()
            .any(|probe| probe.source_material != probe.reimported_material)
    {
        return Err("STEP result failed artifact, witness, or equivalence checks".into());
    }
    Ok(())
}

fn valid_timings(timings: &StepDeliveryTimings) -> bool {
    [
        timings.manufacturing_lowering_ms,
        timings.manufacturing_build_ms,
        timings.step_serialization_ms,
        timings.step_reimport_ms,
        timings.equivalence_validation_ms,
        timings.total_ms,
    ]
    .iter()
    .all(|value| value.is_finite() && *value >= 0.0)
}

fn valid_witness(witness: &StepSolidWitness) -> bool {
    witness.valid
        && witness.solid_count == 1
        && witness.volume_mm3.is_finite()
        && witness.volume_mm3 > 0.0
        && witness.surface_area_mm2.is_finite()
        && witness.surface_area_mm2 > 0.0
        && witness.center_of_mass.iter().all(|value| value.is_finite())
        && witness.edge_count > 0
        && witness.face_count > 0
        && witness.vertex_count > 0
        && witness.bbox.x_min.is_finite()
        && witness.bbox.x_max.is_finite()
        && witness.bbox.y_min.is_finite()
        && witness.bbox.y_max.is_finite()
        && witness.bbox.z_min.is_finite()
        && witness.bbox.z_max.is_finite()
}

// Keep the import visible to rustdoc users of the module without exposing a
// second transport shape; the actual solid result remains the Phase 10.2 DTO.
#[allow(dead_code)]
fn _phase_10_2_type_marker(_: Option<SolidBuildResult>) {}
