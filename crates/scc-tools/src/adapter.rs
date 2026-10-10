//! Tool adapter: `ToolCall → Vec<EngineeringCommand>`.
//!
//! The adapter is the only bridge from tool calls to the engineering command
//! protocol (ADR-019 Decision 1/2). It never touches an
//! [`scc_engineering::model::EngineeringModel`]; a composite tool emits a
//! sequence of commands that `CommandExecutor::apply_all` applies atomically.
//!
//! The tool layer never expands patterns into hole arrays and never exposes
//! SCC internals (Proposal/IR/GraphState).

use std::collections::BTreeMap;

use scc_core::authority::AuthorityLevel;
use scc_engineering::binding::{
    ExpectedParameterType, FeatureParameterReference, FeatureParameterSlot,
};
use scc_engineering::command::EngineeringCommand;
use scc_engineering::feature::{
    DrawingProperty, EngineeringConstraint, EngineeringFeature, HoleDefinition, HolePattern,
    ProjectionMethod,
};
use scc_engineering::model::ParameterOrigin;
use scc_engineering::parameter::ParameterValue;
use scc_engineering::{EngineeringConstraintId, FeatureId, ParameterId, PartId, Point2D};
use scc_engineering::{HoleExtent, HoleStartFace, SourcedLength};

use crate::call::ToolCall;
use crate::error::ToolError;
use crate::registry::ToolRegistry;
use crate::validator::validate_arguments;
use crate::value::ToolValue;

/// Adapt one tool call into the commands that express it.
pub fn adapt_tool_call(call: &ToolCall) -> Result<Vec<EngineeringCommand>, ToolError> {
    let registry = ToolRegistry::standard();
    let definition = registry
        .get(&call.name)
        .ok_or_else(|| ToolError::UnknownTool {
            name: call.name.0.clone(),
        })?;
    validate_arguments(definition, &call.arguments)?;

    let tool = call.name.0.as_str();
    let args = &call.arguments;
    match tool {
        "create_part" => adapt_create_part(args),
        "create_base_rectangle" => adapt_create_base_rectangle(args),
        "create_base_circle" => adapt_create_base_circle(args),
        "create_base_rounded_rectangle" => adapt_create_base_rounded_rectangle(args),
        "create_base_polygon" => adapt_create_base_polygon(args),
        "create_parametric_plate" => adapt_create_parametric_plate(args),
        "create_sketch" => adapt_create_sketch(args),
        "create_path" => adapt_create_path(args),
        "create_sweep" => adapt_create_sweep(args),
        "create_loft" => adapt_create_loft(args),
        "create_datum_axis" => adapt_create_datum_axis(args),
        "create_datum_plane" => adapt_create_datum_plane(args),
        "create_revolve" => adapt_create_revolve(args),
        "add_parametric_rectangular_cut" => adapt_parametric_rectangular_cut(args),
        "add_parametric_rectangular_add" => adapt_parametric_rectangular_add(args),
        "add_parametric_hole" => adapt_add_parametric_hole(args),
        "add_feature_pattern" => adapt_feature_pattern(args),
        "create_fillet" => adapt_create_fillet(args),
        "create_chamfer" => adapt_create_chamfer(args),
        "add_hole" => adapt_add_hole(args),
        "add_slot" => adapt_add_slot(args),
        "add_circular_hole_pattern" => adapt_circular_hole_pattern(args),
        "add_grid_hole_pattern" => adapt_grid_hole_pattern(args),
        "add_corner_hole_pattern" => adapt_corner_hole_pattern(args),
        "add_construction_circle" => adapt_construction_circle(args),
        "declare_parameter" => adapt_declare_parameter(args),
        "bind_parameter" => adapt_bind_parameter(args),
        "set_parameter" => adapt_set_parameter(args),
        "set_feature_parameter" => adapt_set_feature_parameter(args),
        "set_expression" => adapt_set_expression(args),
        "add_constraint" => adapt_add_constraint(args),
        "remove_constraint" => adapt_remove_constraint(args),
        "remove_feature" => adapt_remove_feature(args),
        "set_part_thickness" => adapt_set_part_thickness(args),
        "set_hole_extent" => adapt_set_hole_extent(args),
        "move_feature" => adapt_move_feature(args),
        "set_drawing_property" => adapt_set_drawing_property(args),
        other => Err(ToolError::UnknownTool { name: other.into() }),
    }
}

// ---- argument extractors (defensive: validation already ran) ----

fn get_string<'a>(
    args: &'a BTreeMap<String, ToolValue>,
    tool: &str,
    name: &str,
) -> Result<&'a str, ToolError> {
    match args.get(name) {
        Some(ToolValue::String(s)) => Ok(s),
        Some(other) => Err(ToolError::InvalidArgumentType {
            tool: tool.into(),
            argument: name.into(),
            expected: "string".into(),
            received: other.type_name().into(),
        }),
        None => Err(ToolError::MissingArgument {
            tool: tool.into(),
            argument: name.into(),
        }),
    }
}

fn get_number(
    args: &BTreeMap<String, ToolValue>,
    tool: &str,
    name: &str,
) -> Result<f64, ToolError> {
    match args.get(name) {
        Some(ToolValue::Number(n)) => Ok(*n),
        Some(ToolValue::Integer(i)) => Ok(*i as f64),
        Some(other) => Err(ToolError::InvalidArgumentType {
            tool: tool.into(),
            argument: name.into(),
            expected: "number".into(),
            received: other.type_name().into(),
        }),
        None => Err(ToolError::MissingArgument {
            tool: tool.into(),
            argument: name.into(),
        }),
    }
}

fn get_integer(
    args: &BTreeMap<String, ToolValue>,
    tool: &str,
    name: &str,
) -> Result<i64, ToolError> {
    match args.get(name) {
        Some(ToolValue::Integer(i)) => Ok(*i),
        Some(other) => Err(ToolError::InvalidArgumentType {
            tool: tool.into(),
            argument: name.into(),
            expected: "integer".into(),
            received: other.type_name().into(),
        }),
        None => Err(ToolError::MissingArgument {
            tool: tool.into(),
            argument: name.into(),
        }),
    }
}

fn missing(tool: &str, argument: &str) -> ToolError {
    ToolError::MissingArgument {
        tool: tool.into(),
        argument: argument.into(),
    }
}

// ---- the 26 tools ----

fn adapt_create_part(
    args: &BTreeMap<String, ToolValue>,
) -> Result<Vec<EngineeringCommand>, ToolError> {
    let id = get_string(args, "create_part", "id")?.to_string();
    let name = match args.get("name") {
        Some(ToolValue::String(n)) => n.clone(),
        _ => id.clone(),
    };
    Ok(vec![EngineeringCommand::CreatePart {
        id: PartId(id),
        name,
    }])
}

fn adapt_create_base_rectangle(
    args: &BTreeMap<String, ToolValue>,
) -> Result<Vec<EngineeringCommand>, ToolError> {
    let id = get_string(args, "create_base_rectangle", "id")?.to_string();
    let width = get_number(args, "create_base_rectangle", "width")?;
    let height = get_number(args, "create_base_rectangle", "height")?;
    Ok(vec![EngineeringCommand::AddFeature {
        feature: EngineeringFeature::BaseProfile {
            id: FeatureId(id.clone()),
            name: id.clone(),
            profile: scc_engineering::BaseProfile::Rectangle { width, height },
        },
    }])
}

/// Composite: `HOLE`-with-center semantics. `Hole` has no native position, so
/// the adapter emits the same center desugaring as the DSL:
/// `DeclareParameter {id}_center_x/y` + `AddFeature` + `BindFeatureParameter`.
fn adapt_create_base_circle(
    args: &BTreeMap<String, ToolValue>,
) -> Result<Vec<EngineeringCommand>, ToolError> {
    let tool = "create_base_circle";
    let id = get_string(args, tool, "id")?.to_string();
    let center_x = get_number(args, tool, "center_x")?;
    let center_y = get_number(args, tool, "center_y")?;
    let diameter = get_number(args, tool, "diameter")?;
    Ok(vec![EngineeringCommand::AddFeature {
        feature: EngineeringFeature::BaseProfile {
            id: FeatureId(id.clone()),
            name: id,
            profile: scc_engineering::BaseProfile::Circle {
                center: Point2D::new(center_x, center_y),
                diameter,
            },
        },
    }])
}

fn adapt_create_base_rounded_rectangle(
    args: &BTreeMap<String, ToolValue>,
) -> Result<Vec<EngineeringCommand>, ToolError> {
    let tool = "create_base_rounded_rectangle";
    let id = get_string(args, tool, "id")?.to_string();
    let center_x = get_number(args, tool, "center_x")?;
    let center_y = get_number(args, tool, "center_y")?;
    let width = get_number(args, tool, "width")?;
    let height = get_number(args, tool, "height")?;
    let corner_radius = get_number(args, tool, "corner_radius")?;
    Ok(vec![EngineeringCommand::AddFeature {
        feature: EngineeringFeature::BaseProfile {
            id: FeatureId(id.clone()),
            name: id,
            profile: scc_engineering::BaseProfile::RoundedRectangle {
                center: Point2D::new(center_x, center_y),
                width,
                height,
                corner_radius,
            },
        },
    }])
}

fn adapt_create_base_polygon(
    args: &BTreeMap<String, ToolValue>,
) -> Result<Vec<EngineeringCommand>, ToolError> {
    let tool = "create_base_polygon";
    let id = get_string(args, tool, "id")?.to_string();
    let value = args.get("points").ok_or_else(|| missing(tool, "points"))?;
    let ToolValue::Array(points) = value else {
        return Err(ToolError::InvalidArgumentType {
            tool: tool.into(),
            argument: "points".into(),
            expected: "array".into(),
            received: value.type_name().into(),
        });
    };
    if points.len() < 3 {
        return Err(ToolError::InvalidArgumentValue {
            tool: tool.into(),
            argument: "points".into(),
            message: "polygon requires at least three points".into(),
        });
    }
    let mut vertices = Vec::with_capacity(points.len());
    for (index, point) in points.iter().enumerate() {
        let ToolValue::Array(coordinates) = point else {
            return Err(ToolError::InvalidArgumentType {
                tool: tool.into(),
                argument: format!("points[{index}]"),
                expected: "array of two numbers".into(),
                received: point.type_name().into(),
            });
        };
        if coordinates.len() != 2 {
            return Err(ToolError::InvalidArgumentValue {
                tool: tool.into(),
                argument: format!("points[{index}]"),
                message: "each polygon point must contain exactly [x, y]".into(),
            });
        }
        let x = polygon_coordinate(tool, index, "x", &coordinates[0])?;
        let y = polygon_coordinate(tool, index, "y", &coordinates[1])?;
        vertices.push(Point2D::new(x, y));
    }
    let vertices = scc_engineering::profile::canonicalize_polygon(&vertices).map_err(|error| {
        ToolError::InvalidArgumentValue {
            tool: tool.into(),
            argument: "points".into(),
            message: error.to_string(),
        }
    })?;
    let profile = scc_engineering::BaseProfile::Polygon { vertices };
    Ok(vec![EngineeringCommand::AddFeature {
        feature: EngineeringFeature::BaseProfile {
            id: FeatureId(id.clone()),
            name: id,
            profile,
        },
    }])
}

fn polygon_coordinate(
    tool: &str,
    index: usize,
    axis: &str,
    value: &ToolValue,
) -> Result<f64, ToolError> {
    let number = match value {
        ToolValue::Number(value) => *value,
        ToolValue::Integer(value) => *value as f64,
        _ => {
            return Err(ToolError::InvalidArgumentType {
                tool: tool.into(),
                argument: format!("points[{index}].{axis}"),
                expected: "number".into(),
                received: value.type_name().into(),
            })
        }
    };
    if !number.is_finite() {
        return Err(ToolError::InvalidArgumentValue {
            tool: tool.into(),
            argument: format!("points[{index}].{axis}"),
            message: "coordinate must be finite".into(),
        });
    }
    Ok(number)
}

fn adapt_add_slot(
    args: &BTreeMap<String, ToolValue>,
) -> Result<Vec<EngineeringCommand>, ToolError> {
    let tool = "add_slot";
    let id = get_string(args, tool, "id")?.to_string();
    Ok(vec![EngineeringCommand::AddFeature {
        feature: EngineeringFeature::Slot {
            id: FeatureId(id),
            center: Point2D::new(
                get_number(args, tool, "center_x")?,
                get_number(args, tool, "center_y")?,
            ),
            length: get_number(args, tool, "overall_length")?,
            width: get_number(args, tool, "width")?,
            orientation_degrees: get_number(args, tool, "orientation_degrees")?,
        },
    }])
}

fn adapt_create_parametric_plate(
    args: &BTreeMap<String, ToolValue>,
) -> Result<Vec<EngineeringCommand>, ToolError> {
    let tool = "create_parametric_plate";
    let id = get_string(args, tool, "id")?.to_owned();
    let sketch_id = derived_feature_id(tool, &id, "_sketch")?;
    let extrude_id = derived_feature_id(tool, &id, "_extrude")?;
    let profile = scc_engineering::SketchProfile {
        outer: scc_engineering::SketchLoop::rectangle(
            &format!("{id}-edge"),
            Point2D::new(0.0, 0.0),
            get_number(args, tool, "width")?,
            get_number(args, tool, "height")?,
        )
        .map_err(|error| ToolError::InvalidArgumentValue {
            tool: tool.into(),
            argument: "width/height".into(),
            message: error.to_string(),
        })?,
        inner_loops: Vec::new(),
    };
    Ok(vec![
        EngineeringCommand::AddFeature {
            feature: EngineeringFeature::Parametric {
                id: FeatureId(sketch_id.clone()),
                name: format!("{id} sketch"),
                feature: scc_engineering::ParametricFeature::Sketch {
                    support: scc_engineering::PlaneReference::Principal {
                        plane: scc_engineering::PrincipalPlane::XY,
                    },
                    profile,
                },
                suppressed: false,
            },
        },
        EngineeringCommand::AddFeature {
            feature: EngineeringFeature::Parametric {
                id: FeatureId(extrude_id),
                name: format!("{id} base"),
                feature: scc_engineering::ParametricFeature::Extrude {
                    profile: FeatureId(sketch_id),
                    operation: scc_engineering::ExtrudeOperation::Add,
                    direction: scc_engineering::ExtrudeDirection::Normal { reversed: false },
                    extent: scc_engineering::ExtrudeExtent::Distance {
                        length: get_number(args, tool, "thickness")?,
                    },
                },
                suppressed: false,
            },
        },
    ])
}

fn validate_object_fields(
    value: &serde_json::Value,
    allowed: &[&str],
    path: &str,
) -> Result<(), String> {
    if let Some(object) = value.as_object() {
        if let Some(field) = object
            .keys()
            .find(|field| !allowed.contains(&field.as_str()))
        {
            return Err(format!("{path} contains unsupported field {field}"));
        }
    }
    Ok(())
}

fn validate_sketch_point_fields(value: &serde_json::Value, path: &str) -> Result<(), String> {
    validate_object_fields(value, &["x", "y"], path)
}

fn validate_sketch_loop_fields(value: &serde_json::Value, path: &str) -> Result<(), String> {
    validate_object_fields(value, &["entities"], path)?;
    let Some(entities) = value.get("entities").and_then(serde_json::Value::as_array) else {
        return Ok(());
    };
    for (index, entity) in entities.iter().enumerate() {
        let entity_path = format!("{path}.entities[{index}]");
        validate_object_fields(entity, &["id", "geometry"], &entity_path)?;
        let Some(geometry) = entity.get("geometry") else {
            continue;
        };
        let geometry_path = format!("{entity_path}.geometry");
        match geometry.get("kind").and_then(serde_json::Value::as_str) {
            Some("line") => {
                validate_object_fields(geometry, &["kind", "start", "end"], &geometry_path)?;
                if let Some(point) = geometry.get("start") {
                    validate_sketch_point_fields(point, &format!("{geometry_path}.start"))?;
                }
                if let Some(point) = geometry.get("end") {
                    validate_sketch_point_fields(point, &format!("{geometry_path}.end"))?;
                }
            }
            Some("arc") => {
                validate_object_fields(
                    geometry,
                    &["kind", "start", "end", "center", "radius", "direction"],
                    &geometry_path,
                )?;
                for name in ["start", "end", "center"] {
                    if let Some(point) = geometry.get(name) {
                        validate_sketch_point_fields(point, &format!("{geometry_path}.{name}"))?;
                    }
                }
            }
            Some("circle") => {
                validate_object_fields(geometry, &["kind", "center", "radius"], &geometry_path)?;
                if let Some(point) = geometry.get("center") {
                    validate_sketch_point_fields(point, &format!("{geometry_path}.center"))?;
                }
            }
            _ => {}
        }
    }
    Ok(())
}

fn validate_sketch_profile_fields(value: &serde_json::Value) -> Result<(), String> {
    validate_object_fields(value, &["outer", "inner_loops"], "profile")?;
    if let Some(outer) = value.get("outer") {
        validate_sketch_loop_fields(outer, "profile.outer")?;
    }
    if let Some(inner_loops) = value
        .get("inner_loops")
        .and_then(serde_json::Value::as_array)
    {
        for (index, inner) in inner_loops.iter().enumerate() {
            validate_sketch_loop_fields(inner, &format!("profile.inner_loops[{index}]"))?;
        }
    }
    Ok(())
}

fn adapt_create_sketch(
    args: &BTreeMap<String, ToolValue>,
) -> Result<Vec<EngineeringCommand>, ToolError> {
    let tool = "create_sketch";
    let id = get_string(args, tool, "id")?.to_owned();
    let support = if let Some(support_value) = args.get("support") {
        if !matches!(support_value, ToolValue::Object(_)) {
            return Err(ToolError::InvalidArgumentType {
                tool: tool.into(),
                argument: "support".into(),
                expected: "typed principal or Datum PlaneReference object".into(),
                received: support_value.type_name().into(),
            });
        }
        let json = serde_json::to_value(support_value).map_err(|error| {
            ToolError::InvalidArgumentValue {
                tool: tool.into(),
                argument: "support".into(),
                message: error.to_string(),
            }
        })?;
        let reference: scc_engineering::PlaneReference =
            serde_json::from_value(json).map_err(|error| ToolError::InvalidArgumentValue {
                tool: tool.into(),
                argument: "support".into(),
                message: format!("invalid PlaneReference: {error}"),
            })?;
        if !matches!(
            reference,
            scc_engineering::PlaneReference::Principal { .. }
                | scc_engineering::PlaneReference::Datum { .. }
        ) {
            return Err(ToolError::InvalidArgumentValue {
                tool: tool.into(),
                argument: "support".into(),
                message: "Sketch support must be a principal plane or an earlier DatumPlane".into(),
            });
        }
        reference
    } else {
        let plane = match get_string(args, tool, "plane")? {
            "xy" => scc_engineering::PrincipalPlane::XY,
            "xz" => scc_engineering::PrincipalPlane::XZ,
            "yz" => scc_engineering::PrincipalPlane::YZ,
            _ => {
                return Err(ToolError::InvalidArgumentValue {
                    tool: tool.into(),
                    argument: "plane".into(),
                    message: "unsupported principal plane".into(),
                })
            }
        };
        scc_engineering::PlaneReference::Principal { plane }
    };
    let value = args
        .get("profile")
        .ok_or_else(|| missing(tool, "profile"))?;
    if !matches!(value, ToolValue::Object(_)) {
        return Err(ToolError::InvalidArgumentType {
            tool: tool.into(),
            argument: "profile".into(),
            expected: "typed SketchProfile object".into(),
            received: value.type_name().into(),
        });
    }
    let json = serde_json::to_value(value).map_err(|error| ToolError::InvalidArgumentValue {
        tool: tool.into(),
        argument: "profile".into(),
        message: error.to_string(),
    })?;
    validate_sketch_profile_fields(&json).map_err(|message| ToolError::InvalidArgumentValue {
        tool: tool.into(),
        argument: "profile".into(),
        message,
    })?;
    let profile: scc_engineering::SketchProfile =
        serde_json::from_value(json).map_err(|error| ToolError::InvalidArgumentValue {
            tool: tool.into(),
            argument: "profile".into(),
            message: format!("invalid SketchProfile: {error}"),
        })?;
    profile
        .validate()
        .map_err(|error| ToolError::InvalidArgumentValue {
            tool: tool.into(),
            argument: "profile".into(),
            message: error.to_string(),
        })?;
    let feature_id = FeatureId(id.clone());
    Ok(vec![EngineeringCommand::AddFeature {
        feature: EngineeringFeature::Parametric {
            id: feature_id,
            name: format!("Sketch {id}"),
            feature: scc_engineering::ParametricFeature::Sketch { support, profile },
            suppressed: false,
        },
    }])
}

fn adapt_create_path(
    args: &BTreeMap<String, ToolValue>,
) -> Result<Vec<EngineeringCommand>, ToolError> {
    let tool = "create_path";
    let id = get_string(args, tool, "id")?.to_owned();
    let support_value = args
        .get("support")
        .ok_or_else(|| missing(tool, "support"))?;
    if !matches!(support_value, ToolValue::Object(_)) {
        return Err(ToolError::InvalidArgumentType {
            tool: tool.into(),
            argument: "support".into(),
            expected: "typed PlaneReference object".into(),
            received: support_value.type_name().into(),
        });
    }
    let support_json =
        serde_json::to_value(support_value).map_err(|error| ToolError::InvalidArgumentValue {
            tool: tool.into(),
            argument: "support".into(),
            message: error.to_string(),
        })?;
    let support: scc_engineering::PlaneReference =
        serde_json::from_value(support_json).map_err(|error| ToolError::InvalidArgumentValue {
            tool: tool.into(),
            argument: "support".into(),
            message: format!("invalid PlaneReference: {error}"),
        })?;
    let segments_value = args
        .get("segments")
        .ok_or_else(|| missing(tool, "segments"))?;
    let segments_json =
        serde_json::to_value(segments_value).map_err(|error| ToolError::InvalidArgumentValue {
            tool: tool.into(),
            argument: "segments".into(),
            message: error.to_string(),
        })?;
    let segments: Vec<scc_engineering::SketchEntity> = serde_json::from_value(segments_json)
        .map_err(|error| ToolError::InvalidArgumentValue {
            tool: tool.into(),
            argument: "segments".into(),
            message: format!("invalid Path segment list: {error}"),
        })?;
    let feature_id = FeatureId(id.clone());
    Ok(vec![EngineeringCommand::AddFeature {
        feature: EngineeringFeature::Parametric {
            id: feature_id,
            name: format!("Path {id}"),
            feature: scc_engineering::ParametricFeature::Path { support, segments },
            suppressed: false,
        },
    }])
}

fn adapt_create_sweep(
    args: &BTreeMap<String, ToolValue>,
) -> Result<Vec<EngineeringCommand>, ToolError> {
    use scc_engineering::{ParametricFeature, SweepDirection, SweepOperation, SweepOrientation};
    let tool = "create_sweep";
    let id = get_string(args, tool, "id")?.to_owned();
    let profile = FeatureId(get_string(args, tool, "profile_id")?.to_owned());
    let path = FeatureId(get_string(args, tool, "path_id")?.to_owned());
    let operation = match get_string(args, tool, "operation")? {
        "add" => SweepOperation::Add,
        "cut" => SweepOperation::Cut,
        _ => {
            return Err(ToolError::InvalidArgumentValue {
                tool: tool.into(),
                argument: "operation".into(),
                message: "operation must be add or cut".into(),
            })
        }
    };
    let direction = match get_string(args, tool, "direction")? {
        "forward" => SweepDirection::Forward,
        "reverse" => SweepDirection::Reverse,
        _ => {
            return Err(ToolError::InvalidArgumentValue {
                tool: tool.into(),
                argument: "direction".into(),
                message: "direction must be forward or reverse".into(),
            })
        }
    };
    let orientation = match get_string(args, tool, "orientation")? {
        "fixed_binormal" => SweepOrientation::FixedBinormal,
        _ => {
            return Err(ToolError::InvalidArgumentValue {
                tool: tool.into(),
                argument: "orientation".into(),
                message: "orientation must be fixed_binormal".into(),
            })
        }
    };
    let anchor_value = args
        .get("profile_origin")
        .ok_or_else(|| missing(tool, "profile_origin"))?;
    if !matches!(anchor_value, ToolValue::Object(_)) {
        return Err(ToolError::InvalidArgumentType {
            tool: tool.into(),
            argument: "profile_origin".into(),
            expected: "Sketch-local Point2D object".into(),
            received: anchor_value.type_name().into(),
        });
    }
    let anchor_json =
        serde_json::to_value(anchor_value).map_err(|error| ToolError::InvalidArgumentValue {
            tool: tool.into(),
            argument: "profile_origin".into(),
            message: error.to_string(),
        })?;
    let profile_origin: Point2D =
        serde_json::from_value(anchor_json).map_err(|error| ToolError::InvalidArgumentValue {
            tool: tool.into(),
            argument: "profile_origin".into(),
            message: format!("invalid Point2D: {error}"),
        })?;
    let feature_id = FeatureId(id.clone());
    Ok(vec![EngineeringCommand::AddFeature {
        feature: EngineeringFeature::Parametric {
            id: feature_id,
            name: format!("Sweep {id}"),
            feature: ParametricFeature::Sweep {
                profile,
                path,
                operation,
                direction,
                profile_origin,
                orientation,
            },
            suppressed: false,
        },
    }])
}

fn adapt_create_loft(
    args: &BTreeMap<String, ToolValue>,
) -> Result<Vec<EngineeringCommand>, ToolError> {
    use scc_engineering::{LoftMode, LoftOperation, LoftOrientationPolicy, ParametricFeature};
    let tool = "create_loft";
    let id = get_string(args, tool, "id")?.to_owned();
    let sections_value = args
        .get("section_ids")
        .ok_or_else(|| missing(tool, "section_ids"))?;
    let ToolValue::Array(section_values) = sections_value else {
        return Err(ToolError::InvalidArgumentType {
            tool: tool.into(),
            argument: "section_ids".into(),
            expected: "ordered array of earlier Sketch feature IDs".into(),
            received: sections_value.type_name().into(),
        });
    };
    let mut sections = Vec::with_capacity(section_values.len());
    for value in section_values {
        let ToolValue::String(section_id) = value else {
            return Err(ToolError::InvalidArgumentType {
                tool: tool.into(),
                argument: "section_ids".into(),
                expected: "array of strings naming existing Sketch features".into(),
                received: value.type_name().into(),
            });
        };
        sections.push(FeatureId(section_id.clone()));
    }
    let operation = match get_string(args, tool, "operation")? {
        "add" => LoftOperation::Add,
        "cut" => LoftOperation::Cut,
        _ => {
            return Err(ToolError::InvalidArgumentValue {
                tool: tool.into(),
                argument: "operation".into(),
                message: "operation must be add or cut".into(),
            })
        }
    };
    let mode = match get_string(args, tool, "mode")? {
        "ruled" => LoftMode::Ruled,
        _ => {
            return Err(ToolError::InvalidArgumentValue {
                tool: tool.into(),
                argument: "mode".into(),
                message: "only ruled mode is supported".into(),
            })
        }
    };
    let orientation_policy = match get_string(args, tool, "orientation_policy")? {
        "aligned" => LoftOrientationPolicy::Aligned,
        _ => {
            return Err(ToolError::InvalidArgumentValue {
                tool: tool.into(),
                argument: "orientation_policy".into(),
                message: "only aligned section orientation is supported".into(),
            })
        }
    };
    Ok(vec![EngineeringCommand::AddFeature {
        feature: EngineeringFeature::Parametric {
            id: FeatureId(id.clone()),
            name: format!("Loft {id}"),
            feature: ParametricFeature::Loft {
                sections,
                operation,
                mode,
                orientation_policy,
            },
            suppressed: false,
        },
    }])
}

fn adapt_create_datum_plane(
    args: &BTreeMap<String, ToolValue>,
) -> Result<Vec<EngineeringCommand>, ToolError> {
    let tool = "create_datum_plane";
    let id = get_string(args, tool, "id")?.to_owned();
    let base_value = args.get("base").ok_or_else(|| missing(tool, "base"))?;
    if !matches!(base_value, ToolValue::Object(_)) {
        return Err(ToolError::InvalidArgumentType {
            tool: tool.into(),
            argument: "base".into(),
            expected: "typed principal or earlier Datum PlaneReference".into(),
            received: base_value.type_name().into(),
        });
    }
    let json =
        serde_json::to_value(base_value).map_err(|error| ToolError::InvalidArgumentValue {
            tool: tool.into(),
            argument: "base".into(),
            message: error.to_string(),
        })?;
    let base: scc_engineering::PlaneReference =
        serde_json::from_value(json).map_err(|error| ToolError::InvalidArgumentValue {
            tool: tool.into(),
            argument: "base".into(),
            message: format!("invalid PlaneReference: {error}"),
        })?;
    if !matches!(
        base,
        scc_engineering::PlaneReference::Principal { .. }
            | scc_engineering::PlaneReference::Datum { .. }
    ) {
        return Err(ToolError::InvalidArgumentValue {
            tool: tool.into(),
            argument: "base".into(),
            message: "DatumPlane base must be a principal plane or earlier DatumPlane".into(),
        });
    }
    let distance = get_number(args, tool, "distance")?;
    let feature_id = FeatureId(id.clone());
    Ok(vec![EngineeringCommand::AddFeature {
        feature: EngineeringFeature::Parametric {
            id: feature_id,
            name: format!("DatumPlane {id}"),
            feature: scc_engineering::ParametricFeature::DatumPlane {
                definition: scc_engineering::DatumPlaneDefinition::Offset { base, distance },
            },
            suppressed: false,
        },
    }])
}

fn adapt_create_datum_axis(
    args: &BTreeMap<String, ToolValue>,
) -> Result<Vec<EngineeringCommand>, ToolError> {
    let tool = "create_datum_axis";
    let id = get_string(args, tool, "id")?.to_owned();
    let value = args
        .get("definition")
        .ok_or_else(|| missing(tool, "definition"))?;
    if !matches!(value, ToolValue::Object(_)) {
        return Err(ToolError::InvalidArgumentType {
            tool: tool.into(),
            argument: "definition".into(),
            expected: "typed DatumAxisDefinition object".into(),
            received: value.type_name().into(),
        });
    }
    let json = serde_json::to_value(value).map_err(|error| ToolError::InvalidArgumentValue {
        tool: tool.into(),
        argument: "definition".into(),
        message: error.to_string(),
    })?;
    let definition: scc_engineering::DatumAxisDefinition =
        serde_json::from_value(json).map_err(|error| ToolError::InvalidArgumentValue {
            tool: tool.into(),
            argument: "definition".into(),
            message: format!("invalid DatumAxisDefinition: {error}"),
        })?;
    let feature_id = FeatureId(id.clone());
    Ok(vec![EngineeringCommand::AddFeature {
        feature: EngineeringFeature::Parametric {
            id: feature_id,
            name: format!("DatumAxis {id}"),
            feature: scc_engineering::ParametricFeature::DatumAxis { definition },
            suppressed: false,
        },
    }])
}

fn adapt_create_revolve(
    args: &BTreeMap<String, ToolValue>,
) -> Result<Vec<EngineeringCommand>, ToolError> {
    use scc_engineering::{
        AxisReference, ParametricFeature, RevolveAngle, RevolveOperation, RevolveOrientation,
    };
    let tool = "create_revolve";
    let id = get_string(args, tool, "id")?.to_owned();
    let profile = FeatureId(get_string(args, tool, "profile_id")?.to_owned());
    let axis = FeatureId(get_string(args, tool, "axis_feature_id")?.to_owned());
    let operation = match get_string(args, tool, "operation")? {
        "add" => RevolveOperation::Add,
        "cut" => RevolveOperation::Cut,
        _ => {
            return Err(ToolError::InvalidArgumentValue {
                tool: tool.into(),
                argument: "operation".into(),
                message: "operation must be add or cut".into(),
            })
        }
    };
    let angle = match get_string(args, tool, "angle_mode")? {
        "full_360" => RevolveAngle::Full360,
        "partial" => RevolveAngle::Partial {
            degrees: get_number(args, tool, "degrees")?,
        },
        _ => {
            return Err(ToolError::InvalidArgumentValue {
                tool: tool.into(),
                argument: "angle_mode".into(),
                message: "angle_mode must be full_360 or partial".into(),
            })
        }
    };
    let orientation = match get_string(args, tool, "orientation")? {
        "forward" => RevolveOrientation::Forward,
        "reverse" => RevolveOrientation::Reverse,
        _ => {
            return Err(ToolError::InvalidArgumentValue {
                tool: tool.into(),
                argument: "orientation".into(),
                message: "orientation must be forward or reverse".into(),
            })
        }
    };
    let feature_id = FeatureId(id.clone());
    Ok(vec![EngineeringCommand::AddFeature {
        feature: EngineeringFeature::Parametric {
            id: feature_id,
            name: format!("Revolve {id}"),
            feature: ParametricFeature::Revolve {
                profile,
                axis: AxisReference::DatumAxis { feature: axis },
                operation,
                angle,
                orientation,
            },
            suppressed: false,
        },
    }])
}

fn adapt_parametric_rectangular_add(
    args: &BTreeMap<String, ToolValue>,
) -> Result<Vec<EngineeringCommand>, ToolError> {
    use scc_engineering::{
        DatumPlaneDefinition, ExtrudeDirection, ExtrudeExtent, ExtrudeOperation, ParametricFeature,
        PlaneReference, PrincipalPlane, SketchLoop, SketchProfile,
    };
    let tool = "add_parametric_rectangular_add";
    let id = get_string(args, tool, "id")?.to_owned();
    let sketch_id = derived_feature_id(tool, &id, "_sketch")?;
    let plane = match get_string(args, tool, "plane")? {
        "x_y" => PrincipalPlane::XY,
        "x_z" => PrincipalPlane::XZ,
        "y_z" => PrincipalPlane::YZ,
        _ => {
            return Err(ToolError::InvalidArgumentValue {
                tool: tool.into(),
                argument: "plane".into(),
                message: "unsupported principal plane".into(),
            })
        }
    };
    let offset = get_number(args, tool, "support_offset")?;
    let mut commands = Vec::new();
    let support = if offset.abs() <= f64::EPSILON {
        PlaneReference::Principal { plane }
    } else {
        let datum_id = derived_feature_id(tool, &id, "_support")?;
        commands.push(EngineeringCommand::AddFeature {
            feature: EngineeringFeature::Parametric {
                id: FeatureId(datum_id.clone()),
                name: format!("{id} support"),
                feature: ParametricFeature::DatumPlane {
                    definition: DatumPlaneDefinition::Offset {
                        base: PlaneReference::Principal { plane },
                        distance: offset,
                    },
                },
                suppressed: false,
            },
        });
        PlaneReference::Datum {
            feature: FeatureId(datum_id),
        }
    };
    let profile = SketchProfile {
        outer: SketchLoop::rectangle(
            &format!("{id}-edge"),
            Point2D::new(
                get_number(args, tool, "center_u")?,
                get_number(args, tool, "center_v")?,
            ),
            get_number(args, tool, "width")?,
            get_number(args, tool, "height")?,
        )
        .map_err(|error| ToolError::InvalidArgumentValue {
            tool: tool.into(),
            argument: "width/height".into(),
            message: error.to_string(),
        })?,
        inner_loops: Vec::new(),
    };
    commands.push(EngineeringCommand::AddFeature {
        feature: EngineeringFeature::Parametric {
            id: FeatureId(sketch_id.clone()),
            name: format!("{id} sketch"),
            feature: ParametricFeature::Sketch {
                support: support.clone(),
                profile,
            },
            suppressed: false,
        },
    });
    commands.push(EngineeringCommand::AddFeature {
        feature: EngineeringFeature::Parametric {
            id: FeatureId(id.clone()),
            name: id,
            feature: ParametricFeature::Extrude {
                profile: FeatureId(sketch_id),
                operation: ExtrudeOperation::Add,
                direction: ExtrudeDirection::Normal { reversed: false },
                extent: ExtrudeExtent::Distance {
                    length: get_number(args, tool, "depth")?,
                },
            },
            suppressed: false,
        },
    });
    Ok(commands)
}

fn adapt_parametric_rectangular_cut(
    args: &BTreeMap<String, ToolValue>,
) -> Result<Vec<EngineeringCommand>, ToolError> {
    use scc_engineering::{
        DatumPlaneDefinition, ExtrudeDirection, ExtrudeExtent, ExtrudeOperation, ParametricFeature,
        PlaneReference, PrincipalPlane, SketchLoop, SketchProfile,
    };
    let tool = "add_parametric_rectangular_cut";
    let id = get_string(args, tool, "id")?.to_owned();
    let sketch_id = derived_feature_id(tool, &id, "_sketch")?;
    let plane = match get_string(args, tool, "plane")? {
        "x_y" => PrincipalPlane::XY,
        "x_z" => PrincipalPlane::XZ,
        "y_z" => PrincipalPlane::YZ,
        _ => {
            return Err(ToolError::InvalidArgumentValue {
                tool: tool.into(),
                argument: "plane".into(),
                message: "unsupported principal plane".into(),
            })
        }
    };
    let offset = get_number(args, tool, "support_offset")?;
    let mut commands = Vec::new();
    let support = if offset.abs() <= f64::EPSILON {
        PlaneReference::Principal { plane }
    } else {
        let datum_id = derived_feature_id(tool, &id, "_support")?;
        commands.push(EngineeringCommand::AddFeature {
            feature: EngineeringFeature::Parametric {
                id: FeatureId(datum_id.clone()),
                name: format!("{id} support"),
                feature: ParametricFeature::DatumPlane {
                    definition: DatumPlaneDefinition::Offset {
                        base: PlaneReference::Principal { plane },
                        distance: offset,
                    },
                },
                suppressed: false,
            },
        });
        PlaneReference::Datum {
            feature: FeatureId(datum_id),
        }
    };
    let profile = SketchProfile {
        outer: SketchLoop::rectangle(
            &format!("{id}-edge"),
            Point2D::new(
                get_number(args, tool, "center_u")?,
                get_number(args, tool, "center_v")?,
            ),
            get_number(args, tool, "width")?,
            get_number(args, tool, "height")?,
        )
        .map_err(|error| ToolError::InvalidArgumentValue {
            tool: tool.into(),
            argument: "width/height".into(),
            message: error.to_string(),
        })?,
        inner_loops: Vec::new(),
    };
    commands.push(EngineeringCommand::AddFeature {
        feature: EngineeringFeature::Parametric {
            id: FeatureId(sketch_id.clone()),
            name: format!("{id} sketch"),
            feature: ParametricFeature::Sketch {
                support: support.clone(),
                profile,
            },
            suppressed: false,
        },
    });
    let extent = match get_string(args, tool, "termination")? {
        "through_all" => ExtrudeExtent::ThroughAll,
        "blind" => ExtrudeExtent::Distance {
            length: get_number(args, tool, "depth")?,
        },
        _ => {
            return Err(ToolError::InvalidArgumentValue {
                tool: tool.into(),
                argument: "termination".into(),
                message: "unsupported CUT extent".into(),
            })
        }
    };
    commands.push(EngineeringCommand::AddFeature {
        feature: EngineeringFeature::Parametric {
            id: FeatureId(id.clone()),
            name: id,
            feature: ParametricFeature::Extrude {
                profile: FeatureId(sketch_id),
                operation: ExtrudeOperation::Cut,
                direction: ExtrudeDirection::Normal {
                    reversed: get_bool(args, tool, "reversed")?,
                },
                extent,
            },
            suppressed: false,
        },
    });
    Ok(commands)
}

fn adapt_add_parametric_hole(
    args: &BTreeMap<String, ToolValue>,
) -> Result<Vec<EngineeringCommand>, ToolError> {
    use scc_engineering::{
        HoleKind, HoleTermination, ParametricFeature, PlaneReference, PrincipalPlane, Vector3D,
    };
    let tool = "add_parametric_hole";
    let id = get_string(args, tool, "id")?.to_owned();
    let mode = get_string(args, tool, "support_mode")?;
    let mut commands = Vec::new();
    let support = match mode {
        "principal" => {
            let plane = match optional_string(args, tool, "plane")?.unwrap_or("x_y") {
                "x_y" => PrincipalPlane::XY,
                "x_z" => PrincipalPlane::XZ,
                "y_z" => PrincipalPlane::YZ,
                _ => {
                    return Err(ToolError::InvalidArgumentValue {
                        tool: tool.into(),
                        argument: "plane".into(),
                        message: "unsupported principal plane".into(),
                    })
                }
            };
            let offset = optional_number(args, tool, "support_offset")?.unwrap_or(0.0);
            if offset.abs() <= f64::EPSILON {
                PlaneReference::Principal { plane }
            } else {
                let support_id = derived_feature_id(tool, &id, "_support")?;
                commands.push(EngineeringCommand::AddFeature {
                    feature: EngineeringFeature::Parametric {
                        id: FeatureId(support_id.clone()),
                        name: format!("{id} support"),
                        feature: ParametricFeature::DatumPlane {
                            definition: scc_engineering::DatumPlaneDefinition::Offset {
                                base: PlaneReference::Principal { plane },
                                distance: offset,
                            },
                        },
                        suppressed: false,
                    },
                });
                PlaneReference::Datum {
                    feature: FeatureId(support_id),
                }
            }
        }
        "datum" => PlaneReference::Datum {
            feature: FeatureId(get_string(args, tool, "support_feature_id")?.to_owned()),
        },
        "planar_face" => {
            let selector = match get_string(args, tool, "face_selector")? {
                "top" => scc_engineering::PlanarFaceSelector::Top,
                "bottom" => scc_engineering::PlanarFaceSelector::Bottom,
                "positive_x" => scc_engineering::PlanarFaceSelector::PositiveX,
                "negative_x" => scc_engineering::PlanarFaceSelector::NegativeX,
                "positive_y" => scc_engineering::PlanarFaceSelector::PositiveY,
                "negative_y" => scc_engineering::PlanarFaceSelector::NegativeY,
                _ => {
                    return Err(ToolError::InvalidArgumentValue {
                        tool: tool.into(),
                        argument: "face_selector".into(),
                        message: "unsupported face selector".into(),
                    })
                }
            };
            let normal = match selector {
                scc_engineering::PlanarFaceSelector::Top => [0.0, 0.0, 1.0],
                scc_engineering::PlanarFaceSelector::Bottom => [0.0, 0.0, -1.0],
                scc_engineering::PlanarFaceSelector::PositiveX => [1.0, 0.0, 0.0],
                scc_engineering::PlanarFaceSelector::NegativeX => [-1.0, 0.0, 0.0],
                scc_engineering::PlanarFaceSelector::PositiveY => [0.0, 1.0, 0.0],
                scc_engineering::PlanarFaceSelector::NegativeY => [0.0, -1.0, 0.0],
            };
            PlaneReference::PlanarFace {
                source_feature_id: FeatureId(
                    get_string(args, tool, "support_feature_id")?.to_owned(),
                ),
                selector,
                qualifier: scc_engineering::PlanarFaceQualifier {
                    normal: Vector3D {
                        x: normal[0],
                        y: normal[1],
                        z: normal[2],
                    },
                    plane_offset: get_number(args, tool, "face_offset")?,
                    tolerance: optional_number(args, tool, "face_tolerance")?.unwrap_or(1.0e-6),
                },
            }
        }
        _ => {
            return Err(ToolError::InvalidArgumentValue {
                tool: tool.into(),
                argument: "support_mode".into(),
                message: "unsupported support mode".into(),
            })
        }
    };
    let termination = match get_string(args, tool, "termination")? {
        "through_all" => HoleTermination::ThroughAll,
        "blind" => HoleTermination::Blind {
            depth: get_number(args, tool, "depth")?,
        },
        _ => {
            return Err(ToolError::InvalidArgumentValue {
                tool: tool.into(),
                argument: "termination".into(),
                message: "unsupported termination".into(),
            })
        }
    };
    let diameter = get_number(args, tool, "diameter")?;
    let kind = match get_string(args, tool, "hole_kind")? {
        "simple" => HoleKind::Simple,
        "counterbore" => HoleKind::Counterbore {
            diameter: get_number(args, tool, "counterbore_diameter")?,
            depth: get_number(args, tool, "counterbore_depth")?,
        },
        "countersink" => HoleKind::Countersink {
            diameter: get_number(args, tool, "countersink_diameter")?,
            included_angle_degrees: get_number(args, tool, "countersink_angle_degrees")?,
        },
        _ => {
            return Err(ToolError::InvalidArgumentValue {
                tool: tool.into(),
                argument: "hole_kind".into(),
                message: "unsupported hole kind".into(),
            })
        }
    };
    commands.push(EngineeringCommand::AddFeature {
        feature: EngineeringFeature::Parametric {
            id: FeatureId(id.clone()),
            name: id,
            feature: ParametricFeature::Hole {
                support,
                position: Point2D::new(
                    get_number(args, tool, "center_u")?,
                    get_number(args, tool, "center_v")?,
                ),
                diameter,
                reversed: get_bool(args, tool, "reversed")?,
                termination,
                kind,
            },
            suppressed: false,
        },
    });
    Ok(commands)
}

fn adapt_feature_pattern(
    args: &BTreeMap<String, ToolValue>,
) -> Result<Vec<EngineeringCommand>, ToolError> {
    use scc_engineering::{
        ParametricFeature, PatternDefinition, PatternPlacement, PlaneReference, Vector3D,
    };
    let tool = "add_feature_pattern";
    let id = get_string(args, tool, "id")?.to_owned();
    let seed_feature_id = FeatureId(get_string(args, tool, "seed_feature_id")?.to_owned());
    let pattern_kind = get_string(args, tool, "pattern_kind")?;
    let count = || -> Result<u32, ToolError> {
        u32::try_from(get_integer(args, tool, "count")?).map_err(|_| {
            ToolError::InvalidArgumentValue {
                tool: tool.into(),
                argument: "count".into(),
                message: "count must be a positive integer".into(),
            }
        })
    };
    let definition =
        match pattern_kind {
            "linear" => PatternDefinition::Linear {
                direction: Vector3D {
                    x: get_number(args, tool, "direction_x")?,
                    y: get_number(args, tool, "direction_y")?,
                    z: get_number(args, tool, "direction_z")?,
                },
                count: count()?,
                spacing: get_number(args, tool, "spacing")?,
            },
            "circular" => PatternDefinition::Circular {
                axis: Vector3D {
                    x: get_number(args, tool, "axis_x")?,
                    y: get_number(args, tool, "axis_y")?,
                    z: get_number(args, tool, "axis_z")?,
                },
                center: Vector3D {
                    x: get_number(args, tool, "center_x")?,
                    y: get_number(args, tool, "center_y")?,
                    z: get_number(args, tool, "center_z")?,
                },
                count: count()?,
                radius: get_number(args, tool, "radius")?,
                start_angle_degrees: get_number(args, tool, "start_angle_degrees")?,
                angular_span_degrees: get_number(args, tool, "angular_span_degrees")?,
            },
            "explicit_placement" => {
                let reference_value = args
                    .get("reference_plane")
                    .ok_or_else(|| missing(tool, "reference_plane"))?;
                if !matches!(reference_value, ToolValue::Object(_)) {
                    return Err(ToolError::InvalidArgumentType {
                        tool: tool.into(),
                        argument: "reference_plane".into(),
                        expected: "typed principal or DatumPlane reference object".into(),
                        received: reference_value.type_name().into(),
                    });
                }
                let reference_json = serde_json::to_value(reference_value).map_err(|error| {
                    ToolError::InvalidArgumentValue {
                        tool: tool.into(),
                        argument: "reference_plane".into(),
                        message: error.to_string(),
                    }
                })?;
                let reference_plane: PlaneReference = serde_json::from_value(reference_json)
                    .map_err(|error| ToolError::InvalidArgumentValue {
                        tool: tool.into(),
                        argument: "reference_plane".into(),
                        message: format!("invalid PlaneReference: {error}"),
                    })?;
                let placement_values = match args.get("placements") {
                    Some(ToolValue::Array(values)) => values,
                    Some(other) => {
                        return Err(ToolError::InvalidArgumentType {
                            tool: tool.into(),
                            argument: "placements".into(),
                            expected: "array of local U/V placement objects".into(),
                            received: other.type_name().into(),
                        })
                    }
                    None => return Err(missing(tool, "placements")),
                };
                if placement_values
                    .iter()
                    .any(|value| !matches!(value, ToolValue::Object(_)))
                {
                    return Err(ToolError::InvalidArgumentType {
                        tool: tool.into(),
                        argument: "placements".into(),
                        expected: "array of local U/V placement objects".into(),
                        received: "array containing a non-object".into(),
                    });
                }
                let placements_json = serde_json::to_value(placement_values).map_err(|error| {
                    ToolError::InvalidArgumentValue {
                        tool: tool.into(),
                        argument: "placements".into(),
                        message: error.to_string(),
                    }
                })?;
                let placements: Vec<PatternPlacement> = serde_json::from_value(placements_json)
                    .map_err(|error| ToolError::InvalidArgumentValue {
                        tool: tool.into(),
                        argument: "placements".into(),
                        message: format!("invalid PatternPlacement: {error}"),
                    })?;
                PatternDefinition::ExplicitPlacement {
                    reference_plane,
                    placements,
                }
            }
            _ => {
                return Err(ToolError::InvalidArgumentValue {
                    tool: tool.into(),
                    argument: "pattern_kind".into(),
                    message: "unsupported pattern kind".into(),
                })
            }
        };
    Ok(vec![EngineeringCommand::AddFeature {
        feature: EngineeringFeature::Parametric {
            id: FeatureId(id.clone()),
            name: id,
            feature: ParametricFeature::Pattern {
                seed_feature_id,
                definition,
            },
            suppressed: false,
        },
    }])
}

fn derived_feature_id(tool: &str, base: &str, suffix: &str) -> Result<String, ToolError> {
    let derived = format!("{base}{suffix}");
    if derived.len() > 64 {
        return Err(ToolError::InvalidArgumentValue {
            tool: tool.into(),
            argument: "id".into(),
            message: "base feature id is too long for deterministic Sketch/Datum IDs".into(),
        });
    }
    Ok(derived)
}

fn optional_string<'a>(
    args: &'a BTreeMap<String, ToolValue>,
    tool: &str,
    name: &str,
) -> Result<Option<&'a str>, ToolError> {
    match args.get(name) {
        Some(ToolValue::String(value)) => Ok(Some(value)),
        None => Ok(None),
        Some(other) => Err(ToolError::InvalidArgumentType {
            tool: tool.into(),
            argument: name.into(),
            expected: "string".into(),
            received: other.type_name().into(),
        }),
    }
}
fn optional_number(
    args: &BTreeMap<String, ToolValue>,
    tool: &str,
    name: &str,
) -> Result<Option<f64>, ToolError> {
    match args.get(name) {
        Some(ToolValue::Number(value)) => Ok(Some(*value)),
        Some(ToolValue::Integer(value)) => Ok(Some(*value as f64)),
        None => Ok(None),
        Some(other) => Err(ToolError::InvalidArgumentType {
            tool: tool.into(),
            argument: name.into(),
            expected: "number".into(),
            received: other.type_name().into(),
        }),
    }
}
fn get_bool(args: &BTreeMap<String, ToolValue>, tool: &str, name: &str) -> Result<bool, ToolError> {
    match args.get(name) {
        Some(ToolValue::Boolean(value)) => Ok(*value),
        Some(other) => Err(ToolError::InvalidArgumentType {
            tool: tool.into(),
            argument: name.into(),
            expected: "boolean".into(),
            received: other.type_name().into(),
        }),
        None => Err(ToolError::MissingArgument {
            tool: tool.into(),
            argument: name.into(),
        }),
    }
}

fn get_edge_refs(
    args: &BTreeMap<String, ToolValue>,
    tool: &str,
) -> Result<Vec<scc_engineering::parametric::EdgeReference>, ToolError> {
    let value = args
        .get("edge_refs")
        .ok_or_else(|| ToolError::MissingArgument {
            tool: tool.into(),
            argument: "edge_refs".into(),
        })?;
    let ToolValue::Array(values) = value else {
        return Err(ToolError::InvalidArgumentType {
            tool: tool.into(),
            argument: "edge_refs".into(),
            expected: "array of semantic edge reference objects".into(),
            received: value.type_name().into(),
        });
    };
    if values.is_empty() {
        return Err(ToolError::InvalidArgumentValue {
            tool: tool.into(),
            argument: "edge_refs".into(),
            message: "at least one semantic edge reference is required".into(),
        });
    }
    if values
        .iter()
        .any(|value| !matches!(value, ToolValue::Object(_)))
    {
        return Err(ToolError::InvalidArgumentType {
            tool: tool.into(),
            argument: "edge_refs".into(),
            expected: "array of semantic edge reference objects".into(),
            received: "array containing a non-object".into(),
        });
    }
    let value = serde_json::to_value(values).map_err(|error| ToolError::InvalidArgumentValue {
        tool: tool.into(),
        argument: "edge_refs".into(),
        message: error.to_string(),
    })?;
    serde_json::from_value(value).map_err(|error| ToolError::InvalidArgumentValue {
        tool: tool.into(),
        argument: "edge_refs".into(),
        message: format!("invalid semantic EdgeReference: {error}"),
    })
}

fn adapt_create_fillet(
    args: &BTreeMap<String, ToolValue>,
) -> Result<Vec<EngineeringCommand>, ToolError> {
    let tool = "create_fillet";
    let feature_id = FeatureId(get_string(args, tool, "id")?.into());
    let edge_refs = get_edge_refs(args, tool)?;
    let radius = get_number(args, tool, "radius")?;
    Ok(vec![EngineeringCommand::AddFeature {
        feature: EngineeringFeature::Parametric {
            id: feature_id.clone(),
            name: format!("Fillet {feature_id}"),
            feature: scc_engineering::parametric::ParametricFeature::Fillet { edge_refs, radius },
            suppressed: false,
        },
    }])
}

fn adapt_create_chamfer(
    args: &BTreeMap<String, ToolValue>,
) -> Result<Vec<EngineeringCommand>, ToolError> {
    let tool = "create_chamfer";
    let feature_id = FeatureId(get_string(args, tool, "id")?.into());
    let edge_refs = get_edge_refs(args, tool)?;
    let distance = get_number(args, tool, "distance")?;
    Ok(vec![EngineeringCommand::AddFeature {
        feature: EngineeringFeature::Parametric {
            id: feature_id.clone(),
            name: format!("Chamfer {feature_id}"),
            feature: scc_engineering::parametric::ParametricFeature::Chamfer {
                edge_refs,
                distance,
            },
            suppressed: false,
        },
    }])
}

fn adapt_add_hole(
    args: &BTreeMap<String, ToolValue>,
) -> Result<Vec<EngineeringCommand>, ToolError> {
    let tool = "add_hole";
    let id = get_string(args, tool, "id")?.to_string();
    let center_x = get_number(args, tool, "center_x")?;
    let center_y = get_number(args, tool, "center_y")?;
    let diameter = get_number(args, tool, "diameter")?;
    Ok(center_desugar(
        id.clone(),
        center_x,
        center_y,
        EngineeringFeature::Hole {
            id: FeatureId(id),
            diameter,
        },
    ))
}

fn adapt_circular_hole_pattern(
    args: &BTreeMap<String, ToolValue>,
) -> Result<Vec<EngineeringCommand>, ToolError> {
    let tool = "add_circular_hole_pattern";
    let id = get_string(args, tool, "id")?.to_string();
    let center_x = get_number(args, tool, "center_x")?;
    let center_y = get_number(args, tool, "center_y")?;
    let pcd = get_number(args, tool, "pcd")?;
    let count = get_integer(args, tool, "count")? as u32;
    let diameter = get_number(args, tool, "diameter")?;
    let start_angle = get_number(args, tool, "start_angle")?;
    Ok(vec![EngineeringCommand::AddFeature {
        feature: EngineeringFeature::HolePattern {
            id: FeatureId(id),
            pattern: HolePattern::Circular {
                center: Point2D::new(center_x, center_y),
                pitch_circle_diameter: pcd,
                count,
                start_angle,
                hole: HoleDefinition { diameter },
            },
        },
    }])
}

/// Composite: `Grid` has no native center, so `center_x/y` desugar exactly like
/// the DSL's `GRID ... CENTER`.
fn adapt_grid_hole_pattern(
    args: &BTreeMap<String, ToolValue>,
) -> Result<Vec<EngineeringCommand>, ToolError> {
    let tool = "add_grid_hole_pattern";
    let id = get_string(args, tool, "id")?.to_string();
    let center_x = get_number(args, tool, "center_x")?;
    let center_y = get_number(args, tool, "center_y")?;
    let rows = get_integer(args, tool, "rows")? as u32;
    let columns = get_integer(args, tool, "columns")? as u32;
    let spacing_x = get_number(args, tool, "spacing_x")?;
    let spacing_y = get_number(args, tool, "spacing_y")?;
    let diameter = get_number(args, tool, "diameter")?;
    Ok(center_desugar(
        id.clone(),
        center_x,
        center_y,
        EngineeringFeature::HolePattern {
            id: FeatureId(id),
            pattern: HolePattern::Grid {
                rows,
                columns,
                spacing_x,
                spacing_y,
                hole: HoleDefinition { diameter },
            },
        },
    ))
}

fn adapt_corner_hole_pattern(
    args: &BTreeMap<String, ToolValue>,
) -> Result<Vec<EngineeringCommand>, ToolError> {
    let tool = "add_corner_hole_pattern";
    let id = get_string(args, tool, "id")?.to_string();
    // `base` is a declared reference; the corner lowering resolves the base
    // profile from the model (Phase 3 auto-finds it). Accepted for
    // traceability; validated against the model when it exists.
    let _base = get_string(args, tool, "base")?.to_string();
    let offset_x = get_number(args, tool, "offset_x")?;
    let offset_y = get_number(args, tool, "offset_y")?;
    let diameter = get_number(args, tool, "diameter")?;
    Ok(vec![EngineeringCommand::AddFeature {
        feature: EngineeringFeature::HolePattern {
            id: FeatureId(id),
            pattern: HolePattern::Corner {
                offset_x,
                offset_y,
                hole: HoleDefinition { diameter },
            },
        },
    }])
}

fn adapt_construction_circle(
    args: &BTreeMap<String, ToolValue>,
) -> Result<Vec<EngineeringCommand>, ToolError> {
    let tool = "add_construction_circle";
    let id = get_string(args, tool, "id")?.to_string();
    let center_x = get_number(args, tool, "center_x")?;
    let center_y = get_number(args, tool, "center_y")?;
    let diameter = get_number(args, tool, "diameter")?;
    Ok(center_desugar(
        id.clone(),
        center_x,
        center_y,
        EngineeringFeature::ConstructionCircle {
            id: FeatureId(id),
            diameter,
        },
    ))
}

fn adapt_declare_parameter(
    args: &BTreeMap<String, ToolValue>,
) -> Result<Vec<EngineeringCommand>, ToolError> {
    let tool = "declare_parameter";
    let id = ParameterId(get_string(args, tool, "id")?.to_string());
    let value = get_number(args, tool, "value")?;
    Ok(vec![EngineeringCommand::DeclareParameter {
        id,
        value: ParameterValue::Scalar(value),
        origin: ParameterOrigin::User,
        authority: AuthorityLevel::L0,
    }])
}

fn adapt_set_parameter(
    args: &BTreeMap<String, ToolValue>,
) -> Result<Vec<EngineeringCommand>, ToolError> {
    let tool = "set_parameter";
    let target = ParameterId(get_string(args, tool, "target")?.to_string());
    let value = get_number(args, tool, "value")?;
    Ok(vec![EngineeringCommand::SetParameter {
        target,
        value: ParameterValue::Scalar(value),
    }])
}

fn adapt_set_feature_parameter(
    args: &BTreeMap<String, ToolValue>,
) -> Result<Vec<EngineeringCommand>, ToolError> {
    let tool = "set_feature_parameter";
    let expected_type = match get_string(args, tool, "expected_type")? {
        "length_mm" => ExpectedParameterType::LengthMm,
        "angle_degrees" => ExpectedParameterType::AngleDegrees,
        value => {
            return Err(ToolError::InvalidArgumentValue {
                tool: tool.into(),
                argument: "expected_type".into(),
                message: format!("unsupported parameter value type {}", value),
            });
        }
    };
    let reference = FeatureParameterReference {
        part_id: PartId(get_string(args, tool, "part_id")?.into()),
        feature_id: FeatureId(get_string(args, tool, "feature")?.into()),
        slot: FeatureParameterSlot::from_name(get_string(args, tool, "slot")?).ok_or_else(
            || ToolError::InvalidArgumentValue {
                tool: tool.into(),
                argument: "slot".into(),
                message: "unknown typed feature parameter slot".into(),
            },
        )?,
        parameter_id: ParameterId(get_string(args, tool, "parameter")?.into()),
        expected_type,
    };
    Ok(vec![EngineeringCommand::SetFeatureParameter {
        target: reference,
        value: ParameterValue::Scalar(get_number(args, tool, "value")?),
    }])
}

fn adapt_set_expression(
    args: &BTreeMap<String, ToolValue>,
) -> Result<Vec<EngineeringCommand>, ToolError> {
    let tool = "set_expression";
    let target = ParameterId(get_string(args, tool, "target")?.to_string());
    let expression_text = get_string(args, tool, "expression")?.to_string();
    // Parse through the DSL expression parser (never eval).
    let dsl_expr = scc_dsl::parser::parse_expression(&expression_text).map_err(|e| {
        ToolError::ExpressionParseFailed {
            tool: tool.into(),
            message: e.to_string(),
        }
    })?;
    let expression =
        scc_dsl::command_builder::expression_to_engineering(&dsl_expr).map_err(|e| {
            ToolError::ExpressionParseFailed {
                tool: tool.into(),
                message: e.to_string(),
            }
        })?;
    Ok(vec![EngineeringCommand::SetExpression {
        target,
        expression,
    }])
}

fn adapt_bind_parameter(
    args: &BTreeMap<String, ToolValue>,
) -> Result<Vec<EngineeringCommand>, ToolError> {
    let tool = "bind_parameter";
    let feature = FeatureId(get_string(args, tool, "feature")?.to_string());
    let slot_name = get_string(args, tool, "slot")?;
    let slot = FeatureParameterSlot::from_name(slot_name).ok_or_else(|| {
        ToolError::InvalidArgumentValue {
            tool: tool.into(),
            argument: "slot".into(),
            message: format!("`{}` is not a valid feature parameter slot", slot_name),
        }
    })?;
    let parameter = ParameterId(get_string(args, tool, "parameter")?.to_string());
    Ok(vec![EngineeringCommand::BindFeatureParameter {
        feature,
        slot,
        parameter,
    }])
}

fn adapt_add_constraint(
    args: &BTreeMap<String, ToolValue>,
) -> Result<Vec<EngineeringCommand>, ToolError> {
    let tool = "add_constraint";
    let id = EngineeringConstraintId(get_string(args, tool, "id")?.to_string());
    let constraint_type = get_string(args, tool, "type")?;
    let target = FeatureId(get_string(args, tool, "target")?.to_string());
    let source = match args.get("source") {
        Some(ToolValue::String(s)) => Some(FeatureId(s.clone())),
        Some(_) => {
            return Err(ToolError::InvalidArgumentType {
                tool: tool.into(),
                argument: "source".into(),
                expected: "string".into(),
                received: "non-string".into(),
            });
        }
        None => None,
    };
    let value = match args.get("value") {
        Some(ToolValue::Number(n)) => Some(*n),
        Some(_) => {
            return Err(ToolError::InvalidArgumentType {
                tool: tool.into(),
                argument: "value".into(),
                expected: "number".into(),
                received: "non-number".into(),
            });
        }
        None => None,
    };

    let constraint = match constraint_type {
        "fixed" => EngineeringConstraint::Fixed { id, target },
        "symmetric" => EngineeringConstraint::Symmetric {
            id,
            source: source.ok_or_else(|| missing(tool, "source"))?,
            target,
        },
        "aligned" => EngineeringConstraint::Aligned {
            id,
            source: source.ok_or_else(|| missing(tool, "source"))?,
            target,
        },
        "tangent" => EngineeringConstraint::Tangent {
            id,
            source: source.ok_or_else(|| missing(tool, "source"))?,
            target,
        },
        "distance" => EngineeringConstraint::Distance {
            id,
            source: source.ok_or_else(|| missing(tool, "source"))?,
            target,
            value: value.ok_or_else(|| missing(tool, "value"))?,
        },
        other => {
            return Err(ToolError::InvalidArgumentValue {
                tool: tool.into(),
                argument: "type".into(),
                message: format!("`{}` is not a supported constraint type in v1", other),
            });
        }
    };
    Ok(vec![EngineeringCommand::AddConstraint { constraint }])
}

fn adapt_remove_constraint(
    args: &BTreeMap<String, ToolValue>,
) -> Result<Vec<EngineeringCommand>, ToolError> {
    let tool = "remove_constraint";
    let target = EngineeringConstraintId(get_string(args, tool, "target")?.to_string());
    Ok(vec![EngineeringCommand::RemoveConstraint { id: target }])
}

fn adapt_remove_feature(
    args: &BTreeMap<String, ToolValue>,
) -> Result<Vec<EngineeringCommand>, ToolError> {
    let tool = "remove_feature";
    let target = FeatureId(get_string(args, tool, "target")?.to_string());
    Ok(vec![EngineeringCommand::RemoveFeature { target }])
}

fn adapt_move_feature(
    args: &BTreeMap<String, ToolValue>,
) -> Result<Vec<EngineeringCommand>, ToolError> {
    let tool = "move_feature";
    let target = FeatureId(get_string(args, tool, "target")?.to_string());
    let x = get_number(args, tool, "x")?;
    let y = get_number(args, tool, "y")?;
    Ok(vec![EngineeringCommand::MoveFeature {
        target,
        position: Point2D::new(x, y),
    }])
}
fn adapt_set_part_thickness(
    args: &BTreeMap<String, ToolValue>,
) -> Result<Vec<EngineeringCommand>, ToolError> {
    let tool = "set_part_thickness";
    let thickness = get_number(args, tool, "thickness")?;
    Ok(vec![EngineeringCommand::SetPartThickness {
        source: Some(SourcedLength::Literal { value: thickness }),
    }])
}
fn adapt_set_hole_extent(
    args: &BTreeMap<String, ToolValue>,
) -> Result<Vec<EngineeringCommand>, ToolError> {
    let tool = "set_hole_extent";
    let target = FeatureId(get_string(args, tool, "target")?.to_string());
    let extent_name = get_string(args, tool, "extent")?;
    let extent = match extent_name {
        "through_all" => HoleExtent::ThroughAll,
        "blind_base" | "blind_opposite" => {
            let depth = get_number(args, tool, "depth")?;
            let start_face = if extent_name == "blind_base" {
                HoleStartFace::Base
            } else {
                HoleStartFace::Opposite
            };
            HoleExtent::Blind {
                start_face,
                depth: SourcedLength::Literal { value: depth },
            }
        }
        _ => {
            return Err(ToolError::InvalidArgumentValue {
                tool: tool.into(),
                argument: "extent".into(),
                message: "unsupported hole extent".into(),
            })
        }
    };
    Ok(vec![EngineeringCommand::SetHoleExtent {
        target,
        extent: Some(extent),
    }])
}
fn adapt_set_drawing_property(
    args: &BTreeMap<String, ToolValue>,
) -> Result<Vec<EngineeringCommand>, ToolError> {
    let tool = "set_drawing_property";
    let property = get_string(args, tool, "property")?;
    let value = args.get("value").ok_or_else(|| missing(tool, "value"))?;
    let drawing = match (property, value) {
        ("scale", ToolValue::Number(v)) => DrawingProperty::Scale { value: *v },
        ("scale", other) => {
            return Err(bad_drawing_value(tool, "scale requires a number", other));
        }
        ("show_centerlines", ToolValue::Boolean(b)) => {
            DrawingProperty::ShowCenterlines { enabled: *b }
        }
        ("show_centerlines", other) => {
            return Err(bad_drawing_value(
                tool,
                "show_centerlines requires a boolean",
                other,
            ));
        }
        ("show_construction_geometry", ToolValue::Boolean(b)) => {
            DrawingProperty::ShowConstructionGeometry { enabled: *b }
        }
        ("show_construction_geometry", other) => {
            return Err(bad_drawing_value(
                tool,
                "show_construction_geometry requires a boolean",
                other,
            ));
        }
        ("projection_method", ToolValue::String(s)) if s == "first_angle" => {
            DrawingProperty::ProjectionMethod {
                method: ProjectionMethod::FirstAngle,
            }
        }
        ("projection_method", ToolValue::String(s)) if s == "third_angle" => {
            DrawingProperty::ProjectionMethod {
                method: ProjectionMethod::ThirdAngle,
            }
        }
        ("projection_method", other) => {
            return Err(bad_drawing_value(
                tool,
                "projection_method requires \"first_angle\" or \"third_angle\"",
                other,
            ));
        }
        (other, _) => {
            return Err(ToolError::InvalidArgumentValue {
                tool: tool.into(),
                argument: "property".into(),
                message: format!("`{}` is not a supported drawing property", other),
            });
        }
    };
    Ok(vec![EngineeringCommand::SetDrawingProperty {
        property: drawing,
    }])
}

fn bad_drawing_value(tool: &str, message: &str, value: &ToolValue) -> ToolError {
    ToolError::InvalidArgumentValue {
        tool: tool.into(),
        argument: "value".into(),
        message: format!("{} (got {})", message, value.type_name()),
    }
}

/// Emit the center-desugar sequence for a position-less feature, mirroring the
/// DSL's `CENTER` handling so both front ends produce identical commands.
fn center_desugar(
    id: String,
    x: f64,
    y: f64,
    feature: EngineeringFeature,
) -> Vec<EngineeringCommand> {
    let cx = format!("{}_center_x", id);
    let cy = format!("{}_center_y", id);
    let mut commands = vec![
        EngineeringCommand::DeclareParameter {
            id: ParameterId(cx.clone()),
            value: ParameterValue::Scalar(x),
            origin: ParameterOrigin::User,
            authority: AuthorityLevel::L0,
        },
        EngineeringCommand::DeclareParameter {
            id: ParameterId(cy.clone()),
            value: ParameterValue::Scalar(y),
            origin: ParameterOrigin::User,
            authority: AuthorityLevel::L0,
        },
        EngineeringCommand::AddFeature { feature },
    ];
    commands.push(EngineeringCommand::BindFeatureParameter {
        feature: FeatureId(id.clone()),
        slot: FeatureParameterSlot::CenterX,
        parameter: ParameterId(cx),
    });
    commands.push(EngineeringCommand::BindFeatureParameter {
        feature: FeatureId(id),
        slot: FeatureParameterSlot::CenterY,
        parameter: ParameterId(cy),
    });
    commands
}

#[cfg(test)]
mod g6b_tests {
    use super::*;
    use crate::call::ToolCall;
    use crate::value::{array, num, str_value};

    #[test]
    fn explicit_pattern_tool_decodes_typed_plane_and_nonuniform_placements() {
        let object = |pairs: Vec<(&str, ToolValue)>| {
            ToolValue::Object(
                pairs
                    .into_iter()
                    .map(|(key, value)| (key.to_owned(), value))
                    .collect(),
            )
        };
        let mut args = BTreeMap::new();
        args.insert("id".into(), str_value("layout"));
        args.insert("seed_feature_id".into(), str_value("hole"));
        args.insert("pattern_kind".into(), str_value("explicit_placement"));
        args.insert(
            "reference_plane".into(),
            object(vec![
                ("kind", str_value("principal")),
                ("plane", str_value("y_z")),
            ]),
        );
        args.insert(
            "placements".into(),
            array(vec![
                object(vec![("local_u_mm", num(12.0)), ("local_v_mm", num(-7.0))]),
                object(vec![("local_u_mm", num(-21.0)), ("local_v_mm", num(9.0))]),
            ]),
        );
        let commands = adapt_tool_call(&ToolCall::new("add_feature_pattern", args)).unwrap();
        let [EngineeringCommand::AddFeature {
            feature:
                EngineeringFeature::Parametric {
                    feature:
                        scc_engineering::ParametricFeature::Pattern {
                            definition:
                                scc_engineering::PatternDefinition::ExplicitPlacement {
                                    reference_plane,
                                    placements,
                                },
                            ..
                        },
                    ..
                },
        }] = commands.as_slice()
        else {
            panic!("tool must produce the existing typed Pattern feature")
        };
        assert_eq!(
            *reference_plane,
            scc_engineering::PlaneReference::Principal {
                plane: scc_engineering::PrincipalPlane::YZ
            }
        );
        assert_eq!(
            placements
                .iter()
                .map(|p| (p.local_u_mm, p.local_v_mm))
                .collect::<Vec<_>>(),
            vec![(12.0, -7.0), (-21.0, 9.0)]
        );
    }
}
