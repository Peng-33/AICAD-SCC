//! Rust-side transport adapter for the stateless CadQuery backend.
//!
//! This crate serializes the frozen `SolidBuildSpec`, invokes a configured
//! worker without a shell, parses the structured result, and checks
//! provenance/cardinality. It performs no geometry calculation.

use std::ffi::OsString;
use std::fs;
use std::io::{Read, Write};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use sha2::{Digest, Sha256};

use scc_manufacturing::{SolidBuildSpec, SolidOperationId, SOLID_BUILD_SPEC_SCHEMA_VERSION};
use serde::{Deserialize, Serialize};

pub mod parametric;
pub mod step;
pub use parametric::{
    ParametricArcDirection, ParametricBooleanOperation, ParametricCountersinkOperation,
    ParametricCurve, ParametricEdgeCurveKind, ParametricEdgeQualifier, ParametricEdgeReference,
    ParametricEdgeSelector, ParametricExtent, ParametricExtrudeOperation, ParametricFaceReference,
    ParametricGeneratedFaceQualifier, ParametricGeneratedFaceReference,
    ParametricGeneratedFaceSelector, ParametricLoftMode, ParametricLoftOperation,
    ParametricLoftOrientationPolicy, ParametricLoftSection, ParametricOperationTiming,
    ParametricProfileEdgeSide, ParametricProfileEndpoint, ParametricProfilePlane,
    ParametricRebuildOperation, ParametricRebuildRequest, ParametricRebuildResult,
    ParametricRevolveAngle, ParametricRevolveOperation, ParametricRevolveOrientation,
    ParametricSectionAxis, ParametricSectionFrame, ParametricSectionGeometry,
    ParametricSectionPlane, ParametricSectionViewSide, ParametricSemanticSurfaceKind,
    ParametricSweepDirection, ParametricSweepOperation, ParametricSweepOrientation,
    PARAMETRIC_REBUILD_SCHEMA_VERSION,
};
pub use step::{StepDeliveryResult, StepDeliveryTimings, ValidatedStepArtifact};

const DEFAULT_TIMEOUT: Duration = Duration::from_secs(60);
const DEFAULT_MAX_OUTPUT_BYTES: usize = 4 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SolidResultBounds {
    pub x_min: f64,
    pub x_max: f64,
    pub y_min: f64,
    pub y_max: f64,
    pub z_min: f64,
    pub z_max: f64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SolidOperationStatus {
    Applied,
    NoMaterialChange,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SolidOperationResult {
    pub operation_id: String,
    pub source_id: String,
    pub status: SolidOperationStatus,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SolidBodyResultSource {
    pub feature: String,
    pub entity: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SolidBuildResult {
    pub schema_version: u32,
    pub valid: bool,
    pub body_count: u32,
    pub solid_count: u32,
    pub operation_count: u32,
    pub bbox: SolidResultBounds,
    pub volume_mm3: f64,
    pub operation_results: Vec<SolidOperationResult>,
    pub body_source: SolidBodyResultSource,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SolidBackendFailure {
    pub stage: String,
    pub code: String,
    pub message: String,
    #[serde(default)]
    pub path: Option<String>,
    #[serde(default)]
    pub operation_id: Option<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum CadBackendError {
    #[error("failed to serialize SolidBuildSpec: {0}")]
    Serialize(#[from] serde_json::Error),
    #[error("failed to start CAD backend: {0}")]
    Start(#[source] std::io::Error),
    #[error("CAD backend timed out after {0} seconds")]
    Timeout(u64),
    #[error("CAD backend exited unsuccessfully: {diagnostics}")]
    Process { diagnostics: String },
    #[error("CAD backend response exceeded {0} bytes")]
    ResponseTooLarge(usize),
    #[error("CAD backend returned invalid JSON: {0}")]
    Protocol(#[source] serde_json::Error),
    #[error("CAD backend rejected the request: {0:?}")]
    Backend(SolidBackendFailure),
    #[error("CAD backend returned an invalid result: {0}")]
    InvalidResult(String),
}

/// One formal STEP authority accepts the legacy SpatialModel snapshot or a
/// derived EngineeringModel v2 operation plan. This is dispatch metadata, not
/// another editable model or exporter.
#[derive(Debug, Clone, Copy)]
pub enum FormalManufacturingInput<'a> {
    ProjectV1(&'a SolidBuildSpec),
    ProjectV2(&'a ParametricRebuildRequest),
}

#[derive(Debug, Clone)]
pub struct CadBackendClient {
    program: PathBuf,
    args: Vec<OsString>,
    timeout: Duration,
    max_output_bytes: usize,
}

impl CadBackendClient {
    pub fn new(program: impl Into<PathBuf>) -> Self {
        Self {
            program: program.into(),
            args: Vec::new(),
            timeout: DEFAULT_TIMEOUT,
            max_output_bytes: DEFAULT_MAX_OUTPUT_BYTES,
        }
    }

    pub fn with_args<I, S>(mut self, args: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<OsString>,
    {
        self.args = args.into_iter().map(Into::into).collect();
        self
    }

    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    pub fn with_max_output_bytes(mut self, max_output_bytes: usize) -> Self {
        self.max_output_bytes = max_output_bytes;
        self
    }

    pub fn execute_solid_backend(
        &self,
        spec: &SolidBuildSpec,
    ) -> Result<SolidBuildResult, CadBackendError> {
        let input = serde_json::to_vec(spec)?;
        let output = self.invoke_worker(&input)?;
        let wire: WireResponse =
            serde_json::from_slice(&output).map_err(CadBackendError::Protocol)?;
        if !wire.ok {
            return Err(CadBackendError::Backend(wire.error.ok_or_else(|| {
                CadBackendError::InvalidResult("failed response omitted error".into())
            })?));
        }
        let result = wire.into_result()?;
        validate_result_against_spec(spec, &result)?;
        Ok(result)
    }

    pub fn execute_step_delivery_backend(
        &self,
        spec: &SolidBuildSpec,
    ) -> Result<StepDeliveryResult, CadBackendError> {
        let input = serde_json::to_vec(spec)?;
        let output = self.invoke_worker(&input)?;
        let wire: step::StepWireResponse =
            serde_json::from_slice(&output).map_err(CadBackendError::Protocol)?;
        if !wire.ok {
            return Err(CadBackendError::Backend(wire.error.ok_or_else(|| {
                CadBackendError::InvalidResult("failed response omitted error".into())
            })?));
        }
        let result = wire.into_result().map_err(CadBackendError::InvalidResult)?;
        step::validate_step_result(spec, &result).map_err(CadBackendError::InvalidResult)?;
        Ok(result)
    }

    /// Execute the real worker using the one formal STEP authority and return
    /// the exact bytes that passed reimport/equivalence validation.
    pub fn execute_step_delivery_artifact(
        &self,
        spec: &SolidBuildSpec,
    ) -> Result<ValidatedStepArtifact, CadBackendError> {
        self.execute_formal_step_delivery_artifact(FormalManufacturingInput::ProjectV1(spec))
    }

    /// Dispatch the immutable Project v1 or Project v2 manufacturing input
    /// through the same worker module, exporter, reimport validator, and
    /// validated-byte identity gate.
    pub fn execute_formal_step_delivery_artifact(
        &self,
        input: FormalManufacturingInput<'_>,
    ) -> Result<ValidatedStepArtifact, CadBackendError> {
        let request_bytes = match input {
            FormalManufacturingInput::ProjectV1(spec) => serde_json::to_vec(spec)?,
            FormalManufacturingInput::ProjectV2(spec) => {
                spec.validate()?;
                serde_json::to_vec(&serde_json::json!({
                    "manufacturing_kind": "project_v2",
                    "spec": spec,
                }))?
            }
        };
        let artifact = self.execute_step_delivery_input(&request_bytes)?;
        match input {
            FormalManufacturingInput::ProjectV1(spec) => {
                step::validate_step_result(spec, &artifact.witness)
                    .map_err(CadBackendError::InvalidResult)?;
            }
            FormalManufacturingInput::ProjectV2(spec) => {
                step::validate_parametric_step_result(spec, &artifact.witness)
                    .map_err(CadBackendError::InvalidResult)?;
            }
        }
        Ok(artifact)
    }

    fn execute_step_delivery_input(
        &self,
        input: &[u8],
    ) -> Result<ValidatedStepArtifact, CadBackendError> {
        let temp_dir = tempfile::tempdir().map_err(CadBackendError::Start)?;
        let artifact_path = temp_dir.path().join("validated.step");
        let extra_args = [
            OsString::from("--artifact-path"),
            artifact_path.as_os_str().to_owned(),
        ];
        let output = self.invoke_worker_with_extra_args(input, &extra_args)?;
        let wire: step::StepWireResponse =
            serde_json::from_slice(&output).map_err(CadBackendError::Protocol)?;
        if !wire.ok {
            return Err(CadBackendError::Backend(wire.error.ok_or_else(|| {
                CadBackendError::InvalidResult("failed response omitted error".into())
            })?));
        }
        let result = wire.into_result().map_err(CadBackendError::InvalidResult)?;
        let metadata = fs::symlink_metadata(&artifact_path).map_err(CadBackendError::Start)?;
        if !metadata.file_type().is_file()
            || metadata.file_type().is_symlink()
            || artifact_path.extension().and_then(|value| value.to_str()) != Some("step")
        {
            return Err(CadBackendError::InvalidResult(
                "worker artifact was not a regular .step file".into(),
            ));
        }
        let bytes = fs::read(&artifact_path).map_err(CadBackendError::Start)?;
        let size_bytes = bytes.len() as u64;
        let sha256 = sha256_hex(&bytes);
        if size_bytes != result.artifact.size_bytes || sha256 != result.artifact.sha256 {
            return Err(CadBackendError::InvalidResult(
                "validated and delivered artifact identity differs".into(),
            ));
        }
        Ok(ValidatedStepArtifact {
            bytes,
            sha256,
            size_bytes,
            witness: result,
        })
    }

    fn invoke_worker(&self, input: &[u8]) -> Result<Vec<u8>, CadBackendError> {
        self.invoke_worker_with_extra_args(input, &[])
    }

    fn invoke_worker_with_extra_args(
        &self,
        input: &[u8],
        extra_args: &[OsString],
    ) -> Result<Vec<u8>, CadBackendError> {
        let mut child = Command::new(&self.program)
            .args(&self.args)
            .args(extra_args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(CadBackendError::Start)?;
        let stdout = child.stdout.take().ok_or_else(|| {
            CadBackendError::InvalidResult("backend stdout was unavailable".into())
        })?;
        let stderr = child.stderr.take().ok_or_else(|| {
            CadBackendError::InvalidResult("backend stderr was unavailable".into())
        })?;
        // Drain both pipes while the worker runs. Waiting for exit before
        // reading stdout deadlocks when a valid B-Rep evidence response fills
        // the OS pipe buffer; the worker then cannot exit and we report a
        // misleading timeout even though CAD calculation has completed.
        let stdout_limit = self.max_output_bytes;
        let stdout_reader = thread::spawn(move || read_capped(stdout, stdout_limit));
        let stderr_reader = thread::spawn(move || read_capped(stderr, 64 * 1024));
        let write_result = child
            .stdin
            .take()
            .ok_or_else(|| CadBackendError::InvalidResult("backend stdin was unavailable".into()))?
            .write_all(input);
        if let Err(error) = write_result {
            let _ = child.kill();
            let _ = child.wait();
            let _ = stdout_reader.join();
            let _ = stderr_reader.join();
            return Err(CadBackendError::Start(error));
        }

        let started = Instant::now();
        let status = loop {
            match child.try_wait() {
                Ok(Some(status)) => break status,
                Ok(None) => {}
                Err(error) => {
                    let _ = child.kill();
                    let _ = child.wait();
                    let _ = stdout_reader.join();
                    let _ = stderr_reader.join();
                    return Err(CadBackendError::Start(error));
                }
            }
            if started.elapsed() >= self.timeout {
                let _ = child.kill();
                let _ = child.wait();
                let _ = stdout_reader.join();
                let _ = stderr_reader.join();
                return Err(CadBackendError::Timeout(self.timeout.as_secs()));
            }
            thread::sleep(Duration::from_millis(5));
        };

        let (stdout, stdout_too_large) = stdout_reader
            .join()
            .map_err(|_| CadBackendError::InvalidResult("backend stdout reader panicked".into()))?
            .map_err(CadBackendError::Start)?;
        let (stderr, _) = stderr_reader
            .join()
            .map_err(|_| CadBackendError::InvalidResult("backend stderr reader panicked".into()))?
            .map_err(CadBackendError::Start)?;
        if stdout_too_large {
            return Err(CadBackendError::ResponseTooLarge(self.max_output_bytes));
        }
        if !status.success() {
            return Err(CadBackendError::Process {
                diagnostics: bounded_diagnostics(&stderr),
            });
        }
        Ok(stdout)
    }
}

pub fn execute_solid_backend(
    program: impl Into<PathBuf>,
    args: impl IntoIterator<Item = impl Into<OsString>>,
    spec: &SolidBuildSpec,
) -> Result<SolidBuildResult, CadBackendError> {
    CadBackendClient::new(program)
        .with_args(args)
        .execute_solid_backend(spec)
}

pub fn execute_step_delivery_backend(
    program: impl Into<PathBuf>,
    args: impl IntoIterator<Item = impl Into<OsString>>,
    spec: &SolidBuildSpec,
) -> Result<StepDeliveryResult, CadBackendError> {
    CadBackendClient::new(program)
        .with_args(args)
        .execute_step_delivery_backend(spec)
}

pub fn execute_step_delivery_artifact(
    program: impl Into<PathBuf>,
    args: impl IntoIterator<Item = impl Into<OsString>>,
    spec: &SolidBuildSpec,
) -> Result<ValidatedStepArtifact, CadBackendError> {
    CadBackendClient::new(program)
        .with_args(args)
        .execute_step_delivery_artifact(spec)
}

#[derive(Debug, Deserialize)]
struct WireResponse {
    ok: bool,
    #[serde(default)]
    schema_version: Option<u32>,
    #[serde(default)]
    valid: Option<bool>,
    #[serde(default)]
    body_count: Option<u32>,
    #[serde(default)]
    solid_count: Option<u32>,
    #[serde(default)]
    operation_count: Option<u32>,
    #[serde(default)]
    bbox: Option<SolidResultBounds>,
    #[serde(default)]
    volume_mm3: Option<f64>,
    #[serde(default)]
    operation_results: Option<Vec<SolidOperationResult>>,
    #[serde(default)]
    body_source: Option<SolidBodyResultSource>,
    #[serde(default)]
    error: Option<SolidBackendFailure>,
}

impl WireResponse {
    fn into_result(self) -> Result<SolidBuildResult, CadBackendError> {
        let result = SolidBuildResult {
            schema_version: required(self.schema_version, "schema_version")?,
            valid: required(self.valid, "valid")?,
            body_count: required(self.body_count, "body_count")?,
            solid_count: required(self.solid_count, "solid_count")?,
            operation_count: required(self.operation_count, "operation_count")?,
            bbox: required(self.bbox, "bbox")?,
            volume_mm3: required(self.volume_mm3, "volume_mm3")?,
            operation_results: required(self.operation_results, "operation_results")?,
            body_source: required(self.body_source, "body_source")?,
        };
        Ok(result)
    }
}

fn required<T>(value: Option<T>, field: &str) -> Result<T, CadBackendError> {
    value.ok_or_else(|| CadBackendError::InvalidResult(format!("missing {field}")))
}

fn validate_result_against_spec(
    spec: &SolidBuildSpec,
    result: &SolidBuildResult,
) -> Result<(), CadBackendError> {
    if result.schema_version != SOLID_BUILD_SPEC_SCHEMA_VERSION
        || !result.valid
        || result.body_count != 1
        || result.solid_count != 1
        || result.operation_count != spec.operations.len() as u32
        || !result.volume_mm3.is_finite()
        || result.volume_mm3 <= 0.0
        || result.operation_results.len() != spec.operations.len()
    {
        return Err(CadBackendError::InvalidResult(
            "result failed validity/cardinality checks".into(),
        ));
    }
    let expected_ids: Vec<(String, String)> = spec
        .operations
        .keys()
        .map(|id| {
            (
                operation_id_wire(id),
                source_id_wire(spec.operation_source(*id)),
            )
        })
        .collect();
    let actual_ids: Vec<(String, String)> = result
        .operation_results
        .iter()
        .map(|operation| (operation.operation_id.clone(), operation.source_id.clone()))
        .collect();
    if expected_ids != actual_ids {
        return Err(CadBackendError::InvalidResult(
            "operation IDs or provenance were not preserved".into(),
        ));
    }
    if result.body_source.feature != source_string(&spec.source_map.body.feature)
        || result.body_source.entity != source_string(&spec.source_map.body.entity)
    {
        return Err(CadBackendError::InvalidResult(
            "body provenance was not preserved".into(),
        ));
    }
    if !result.bbox.x_min.is_finite()
        || !result.bbox.x_max.is_finite()
        || !result.bbox.y_min.is_finite()
        || !result.bbox.y_max.is_finite()
        || !result.bbox.z_min.is_finite()
        || !result.bbox.z_max.is_finite()
    {
        return Err(CadBackendError::InvalidResult(
            "bbox contains non-finite values".into(),
        ));
    }
    Ok(())
}

pub(crate) fn operation_id_wire(id: &SolidOperationId) -> String {
    serde_json::to_value(id)
        .ok()
        .and_then(|value| value.as_u64().map(|number| number.to_string()))
        .unwrap_or_default()
}

pub(crate) fn source_id_wire(id: Option<&scc_spatial::SpatialCutId>) -> String {
    id.and_then(|value| serde_json::to_value(value).ok())
        .and_then(|value| value.as_str().map(str::to_owned))
        .unwrap_or_default()
}

pub(crate) fn source_string<T: Serialize>(value: &T) -> String {
    serde_json::to_value(value)
        .ok()
        .and_then(|value| value.as_str().map(str::to_owned))
        .unwrap_or_default()
}

fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn read_capped(mut reader: impl Read, limit: usize) -> std::io::Result<(Vec<u8>, bool)> {
    let mut captured = Vec::with_capacity(limit.min(16 * 1024));
    let mut buffer = [0_u8; 8192];
    let mut exceeded = false;
    loop {
        let count = reader.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        let remaining = limit.saturating_sub(captured.len());
        let keep = count.min(remaining);
        captured.extend_from_slice(&buffer[..keep]);
        exceeded |= keep < count;
    }
    Ok((captured, exceeded))
}

fn bounded_diagnostics(stderr: &[u8]) -> String {
    String::from_utf8_lossy(stderr)
        .replace('\n', " ")
        .chars()
        .take(8_000)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[cfg(unix)]
    #[test]
    fn worker_output_larger_than_pipe_capacity_is_drained_before_timeout() {
        let client = CadBackendClient::new("/bin/sh")
            .with_args(["-c", "head -c 131072 /dev/zero"])
            .with_timeout(Duration::from_secs(3));
        let bytes = client.invoke_worker(b"").unwrap();
        assert_eq!(bytes.len(), 131_072);
    }

    #[cfg(unix)]
    #[test]
    fn worker_timeout_kills_child_and_returns_no_response_bytes() {
        let client = CadBackendClient::new("/bin/sh")
            .with_args(["-c", "exec sleep 5"])
            .with_timeout(Duration::from_millis(50));
        let started = Instant::now();
        let error = client.invoke_worker(b"").unwrap_err();
        assert!(matches!(error, CadBackendError::Timeout(_)));
        assert!(started.elapsed() < Duration::from_secs(2));
    }

    #[cfg(unix)]
    #[test]
    fn interrupted_worker_discards_partial_step_like_stdout() {
        let client = CadBackendClient::new("/bin/sh")
            .with_args(["-c", "printf 'ISO-10303-21;\n'; kill -TERM $$"]);
        let error = client.invoke_worker(b"").unwrap_err();
        assert!(matches!(error, CadBackendError::Process { .. }));
    }

    #[test]
    fn wire_success_is_strictly_decoded() {
        let wire: WireResponse = serde_json::from_value(json!({
            "ok": true,
            "schema_version": 1,
            "valid": true,
            "body_count": 1,
            "solid_count": 1,
            "operation_count": 0,
            "bbox": {"x_min": 0.0, "x_max": 120.0, "y_min": 0.0, "y_max": 80.0, "z_min": 0.0, "z_max": 10.0},
            "volume_mm3": 96000.0,
            "operation_results": [],
            "body_source": {"feature": "plate", "entity": "plate"}
        })).unwrap();
        let result = wire.into_result().unwrap();
        assert_eq!(result.operation_count, 0);
        assert_eq!(result.volume_mm3, 96000.0);
    }

    #[test]
    fn error_wire_requires_structured_error() {
        let wire: WireResponse = serde_json::from_value(json!({"ok": false})).unwrap();
        assert!(wire.error.is_none());
    }

    #[test]
    fn step_wire_success_decodes_artifact_and_witnesses() {
        let wire: step::StepWireResponse = serde_json::from_value(json!({
            "ok": true,
            "schema_version": 1,
            "format": "step",
            "unit": "mm",
            "coordinate_frame": "PartLocal",
            "artifact": {"extension": ".step", "size_bytes": 12, "sha256": "a".repeat(64)},
            "source_solid": {
                "valid": true, "solid_count": 1,
                "bbox": {"x_min": 0.0, "x_max": 1.0, "y_min": 0.0, "y_max": 1.0, "z_min": 0.0, "z_max": 1.0},
                "volume_mm3": 1.0,
                "surface_area_mm2": 6.0,
                "center_of_mass": [0.5, 0.5, 0.5],
                "edge_count": 12, "face_count": 6, "vertex_count": 8
            },
            "reimported_solid": {
                "valid": true, "solid_count": 1,
                "bbox": {"x_min": 0.0, "x_max": 1.0, "y_min": 0.0, "y_max": 1.0, "z_min": 0.0, "z_max": 1.0},
                "volume_mm3": 1.0,
                "surface_area_mm2": 6.0,
                "center_of_mass": [0.5, 0.5, 0.5],
                "edge_count": 12, "face_count": 6, "vertex_count": 8
            },
            "equivalence": {"bbox_match": true, "volume_match": true, "surface_area_match": true, "center_of_mass_match": true, "topology_match": true, "feature_geometry_match": true, "material_witness_match": true, "all_match": true},
            "material_witness": {"probe_count": 1, "all_match": true, "probes": [{
                "label": "body", "point": [0.5, 0.5, 0.5], "source_material": true, "reimported_material": true
            }]},
            "operation_results": [],
            "body_source": {"feature": "plate", "entity": "plate"}
        })).unwrap();
        let result = wire.into_result().unwrap();
        assert_eq!(result.artifact.extension, ".step");
        assert!(result.equivalence.all_match);
        assert_eq!(result.material_witness.probe_count, 1);
    }
}
