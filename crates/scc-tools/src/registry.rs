//! Tool registry: deterministic registration, lookup, and listing.

use std::collections::BTreeMap;

use crate::definition::{ToolDefinition, ToolName, ToolParameter};

/// A deterministic registry of tool definitions.
#[derive(Debug, Clone, Default)]
pub struct ToolRegistry {
    tools: BTreeMap<ToolName, ToolDefinition>,
}

impl ToolRegistry {
    /// An empty registry.
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a tool definition. Duplicate tool names are rejected
    /// (`SCC-TOOL-007`); "last definition wins" is forbidden.
    pub fn register(&mut self, definition: ToolDefinition) -> Result<(), crate::error::ToolError> {
        if self.tools.contains_key(&definition.name) {
            return Err(crate::error::ToolError::UnsupportedTool {
                tool: definition.name.0.clone(),
            });
        }
        self.tools.insert(definition.name.clone(), definition);
        Ok(())
    }

    /// Look up a tool definition by name.
    pub fn get(&self, name: &ToolName) -> Option<&ToolDefinition> {
        self.tools.get(name)
    }

    /// Whether the registry contains the named tool.
    pub fn contains(&self, name: &ToolName) -> bool {
        self.tools.contains_key(name)
    }

    /// Every registered tool in stable `BTreeMap` order (sorted by name).
    pub fn list(&self) -> Vec<&ToolDefinition> {
        self.tools.values().collect()
    }

    /// The standard provider-neutral AICAD Tool set.
    pub fn standard() -> Self {
        let mut registry = Self::new();
        for definition in standard_definitions() {
            registry
                .register(definition)
                .expect("standard tool names are unique");
        }
        registry
    }
}

const V2_HOLE_PARAMETER_SLOTS: &[&str] = &[
    "diameter",
    "blind_depth",
    "counterbore_diameter",
    "counterbore_depth",
    "countersink_mouth_diameter",
    "countersink_included_angle_degrees",
];

const SLOTS: &[&str] = &[
    "width",
    "height",
    "center_x",
    "center_y",
    "diameter",
    "blind_depth",
    "counterbore_diameter",
    "counterbore_depth",
    "countersink_mouth_diameter",
    "countersink_included_angle_degrees",
    "radius",
    "length",
    "count",
    "pcd",
    "start_angle",
    "rows",
    "columns",
    "spacing_x",
    "spacing_y",
    "offset_x",
    "offset_y",
    "orientation",
];

const CONSTRAINT_TYPES: &[&str] = &["symmetric", "aligned", "distance", "tangent", "fixed"];

const DRAWING_PROPERTIES: &[&str] = &[
    "scale",
    "show_centerlines",
    "show_construction_geometry",
    "projection_method",
];

fn standard_definitions() -> Vec<ToolDefinition> {
    use ToolParameter as P;
    vec![
        ToolDefinition {
            name: ToolName("create_part".into()),
            description: "Create the single part of the design.".into(),
            parameters: vec![
                P::string("id", true, "Stable part id."),
                P::string("name", false, "Optional display name (defaults to id)."),
            ],
        },
        ToolDefinition {
            name: ToolName("create_base_rectangle".into()),
            description: "Add the rectangular base profile of the part.".into(),
            parameters: vec![
                P::string("id", true, "Feature id of the base profile."),
                P::number("width", true, "Base profile width in mm.", Some(0.0)),
                P::number("height", true, "Base profile height in mm.", Some(0.0)),
            ],
        },
        ToolDefinition {
            name: ToolName("create_base_circle".into()),
            description: "Add a circular base profile of the part.".into(),
            parameters: vec![
                P::string("id", true, "Feature id of the base profile."),
                P::number("center_x", true, "Base profile center X in mm.", None),
                P::number("center_y", true, "Base profile center Y in mm.", None),
                P::number("diameter", true, "Outer base diameter in mm.", Some(0.0)),
            ],
        },
        ToolDefinition {
            name: ToolName("create_base_rounded_rectangle".into()),
            description: "Add a centered rounded rectangular base profile of the part.".into(),
            parameters: vec![
                P::string("id", true, "Feature id of the base profile."),
                P::number("center_x", true, "Base profile center X in mm.", None),
                P::number("center_y", true, "Base profile center Y in mm.", None),
                P::number("width", true, "Base profile width in mm.", Some(0.0)),
                P::number("height", true, "Base profile height in mm.", Some(0.0)),
                P::number("corner_radius", true, "Base corner radius in mm.", None),
            ],
        },
        ToolDefinition {
            name: ToolName("create_base_polygon".into()),
            description: "Add a polygon base profile from an ordered vertex list.".into(),
            parameters: vec![
                P::string("id", true, "Feature id of the base profile."),
                P::array("points", true, "Ordered polygon vertices as [[x, y], ...]."),
            ],
        },
        ToolDefinition {
            name: ToolName("create_parametric_plate".into()),
            description: "Create a centered rectangular base as an ordered EngineeringModel v2 Sketch and Extrude ADD history.".into(),
            parameters: vec![
                P::string("id", true, "Stable base feature id prefix."),
                P::number("width", true, "Plate width in mm.", Some(0.0)),
                P::number("height", true, "Plate height in mm.", Some(0.0)),
                P::number("thickness", true, "Plate thickness in mm.", Some(0.0)),
            ],
        },
        ToolDefinition {
            name: ToolName("create_sketch".into()),
            description: "Add a generic closed Sketch profile on a principal plane or earlier DatumPlane using the existing typed Line/Arc/Circle loop schema.".into(),
            parameters: vec![
                P::string("id", true, "Stable Sketch feature id."),
                P::string_enum("plane", false, "Legacy principal support plane when support is omitted.", &["xy", "xz", "yz"]),
                P::any("support", false, "Typed PlaneReference: principal plane or earlier DatumPlane."),
                P::any("profile", true, "Typed SketchProfile with outer loop and optional inner_loops."),
            ],
        },
        ToolDefinition {
            name: ToolName("create_path".into()),
            description: "Add a generic open Path feature using an explicit principal or DatumPlane support and ordered stable Line/Arc SketchEntity segments. Paths must be connected, C1-continuous, non-degenerate and non-self-intersecting; sharp corners and closed paths are unsupported.".into(),
            parameters: vec![
                P::string("id", true, "Stable Path feature id."),
                P::any("support", true, "Typed PlaneReference; currently Principal or an earlier DatumPlane."),
                P::array("segments", true, "Ordered Path segments as stable SketchEntity objects with Line or Arc geometry."),
            ],
        },
        ToolDefinition {
            name: ToolName("create_sweep".into()),
            description: "Add a generic fixed-section Sweep ADD/CUT referencing an earlier closed Sketch and Path. Profile origin and fixed-binormal orientation must match the directed Path endpoint tangent. No variable section, arbitrary twist, or raw CAD code.".into(),
            parameters: vec![
                P::string("id", true, "Stable Sweep feature id."),
                P::string("profile_id", true, "Earlier Sketch feature id."),
                P::string("path_id", true, "Earlier Path feature id."),
                P::string_enum("operation", true, "Material operation.", &["add", "cut"]),
                P::string_enum("direction", true, "Directed Path traversal.", &["forward", "reverse"]),
                P::any("profile_origin", true, "Sketch-local {x,y} point that lies inside the profile and maps onto the directed Path endpoint."),
                P::string_enum("orientation", true, "G7A fixed planar frame rule.", &["fixed_binormal"]),
            ],
        },
        ToolDefinition {
            name: ToolName("create_loft".into()),
            description: "Add a general EngineeringModel v2 ruled Loft ADD/CUT from two or more earlier Sketch references. Sections must be parallel, co-oriented, strictly ordered, single-outer-loop, and have explicit one-to-one curve correspondence; supports only the aligned frame policy.".into(),
            parameters: vec![
                P::string("id", true, "Stable Loft feature id."),
                P::array("section_ids", true, "Ordered existing Sketch feature IDs, from first to last section."),
                P::string_enum("operation", true, "Material operation.", &["add", "cut"]),
                P::string_enum("mode", true, "Explicit Loft geometry mode.", &["ruled"]),
                P::string_enum("orientation_policy", true, "Section orientation policy.", &["aligned"]),
            ],
        },
        ToolDefinition {
            name: ToolName("create_datum_plane".into()),
            description: "Add a generic offset DatumPlane from a principal plane or earlier DatumPlane. The finite distance is explicit in millimetres.".into(),
            parameters: vec![
                P::string("id", true, "Stable DatumPlane feature id."),
                P::any("base", true, "Typed principal or earlier Datum PlaneReference."),
                P::number("distance", true, "Signed offset from the base plane in millimetres.", None),
            ],
        },
        ToolDefinition {
            name: ToolName("create_datum_axis".into()),
            description: "Add a generic EngineeringModel v2 DatumAxis. Definition is {kind:principal,axis:x|y|z} or {kind:through_two_points,first:{x,y,z},second:{x,y,z}}.".into(),
            parameters: vec![
                P::string("id", true, "Stable DatumAxis feature id."),
                P::any("definition", true, "Typed DatumAxisDefinition: principal X/Y/Z or two finite datum points."),
            ],
        },
        ToolDefinition {
            name: ToolName("create_revolve".into()),
            description: "Add a generic Revolve ADD/CUT using an existing Sketch profile and earlier DatumAxis reference. No raw CAD code or topology indices.".into(),
            parameters: vec![
                P::string("id", true, "Stable Revolve feature id."),
                P::string("profile_id", true, "Earlier Sketch feature id."),
                P::string("axis_feature_id", true, "Earlier DatumAxis feature id."),
                P::string_enum("operation", true, "Material operation.", &["add", "cut"]),
                P::string_enum("angle_mode", true, "full_360 or partial angle.", &["full_360", "partial"]),
                P::number("degrees", false, "Partial angle in degrees, strictly between 0 and 360.", None),
                P::string_enum("orientation", true, "Rotation direction.", &["forward", "reverse"]),
            ],
        },
        ToolDefinition {
            name: ToolName("add_parametric_rectangular_add".into()),
            description: "Add a generic rectangular Sketch + Extrude ADD node on an XY/XZ/YZ plane or offset DatumPlane; use it for bosses and other valid additive prisms, not a fixture-specific feature.".into(),
            parameters: vec![
                P::string("id", true, "Stable Extrude ADD feature id."),
                P::string_enum("plane", true, "Principal support plane.", &["x_y", "x_z", "y_z"]),
                P::number("support_offset", true, "Offset along the support plane normal in mm.", None),
                P::number("center_u", true, "Profile center local U in mm.", None),
                P::number("center_v", true, "Profile center local V in mm.", None),
                P::number("width", true, "Rectangle width in mm.", Some(0.0)),
                P::number("height", true, "Rectangle height in mm.", Some(0.0)),
                P::number("depth", true, "Positive additive extrusion depth in mm.", Some(0.0)),
            ],
        },
        ToolDefinition {
            name: ToolName("add_parametric_rectangular_cut".into()),
            description: "Add an exact rectangular Sketch + Extrude CUT node that can be reused as a generic Pattern seed.".into(),
            parameters: vec![
                P::string("id", true, "Stable Extrude CUT feature id."),
                P::string_enum("plane", true, "Principal support plane.", &["x_y", "x_z", "y_z"]),
                P::number("support_offset", true, "Offset along the support plane normal in mm.", None),
                P::number("center_u", true, "Profile center local U in mm.", None),
                P::number("center_v", true, "Profile center local V in mm.", None),
                P::number("width", true, "Cut profile width in mm.", Some(0.0)),
                P::number("height", true, "Cut profile height in mm.", Some(0.0)),
                P::boolean("reversed", true, "Cut opposite the support plane normal when true."),
                P::string_enum("termination", true, "CUT extent.", &["through_all", "blind"]),
                P::number("depth", false, "Positive blind cut depth in mm.", Some(0.0)),
            ],
        },
        ToolDefinition {
            name: ToolName("add_parametric_hole".into()),
            description: "Add a typed EngineeringModel v2 Hole on a principal plane, datum plane, or uniquely qualified semantic planar face.".into(),
            parameters: vec![
                P::string("id", true, "Stable hole feature id."),
                P::string_enum("support_mode", true, "Support reference kind.", &["principal", "datum", "planar_face"]),
                P::string_enum("plane", false, "Principal plane used when support_mode is principal.", &["x_y", "x_z", "y_z"]),
                P::number("support_offset", false, "Offset along the principal plane normal in mm.", None),
                P::string("support_feature_id", false, "Earlier DatumPlane id, or source feature id for planar_face."),
                P::string_enum("face_selector", false, "Semantic planar face selector.", &["top", "bottom", "positive_x", "negative_x", "positive_y", "negative_y"]),
                P::number("face_offset", false, "Qualified signed plane offset in mm.", None),
                P::number("face_tolerance", false, "Face qualifier tolerance in mm (defaults to 0.000001).", Some(0.0)),
                P::number("center_u", true, "Hole center local U coordinate in mm." , None),
                P::number("center_v", true, "Hole center local V coordinate in mm.", None),
                P::number("diameter", true, "Main bore diameter in mm.", Some(0.0)),
                P::boolean("reversed", true, "Cut opposite the support plane normal when true."),
                P::string_enum("termination", true, "Hole termination.", &["through_all", "blind"]),
                P::number("depth", false, "Positive blind depth in mm." , Some(0.0)),
                P::string_enum("hole_kind", true, "Mechanical hole kind.", &["simple", "counterbore", "countersink"]),
                P::number("counterbore_diameter", false, "Counterbore diameter in mm.", Some(0.0)),
                P::number("counterbore_depth", false, "Counterbore depth in mm.", Some(0.0)),
                P::number("countersink_diameter", false, "Countersink mouth diameter in mm.", Some(0.0)),
                P::number("countersink_angle_degrees", false, "Countersink included angle in degrees.", Some(0.0)),
            ],
        },
        ToolDefinition {
            name: ToolName("create_fillet".into()),
            description: "Add a generic constant-radius Fillet Feature. edge_refs is a non-empty array of semantic objects {source_feature_id, selector:{kind:profile_boundary|pocket_entrance|hole_entrance|hole_exit|between_faces|convex|concave,...}, qualifier:{curve_kind,radius_mm,length_mm,midpoint,tolerance_mm}}. Use stable SketchEntityId or semantic face references; raw OCP edge indices are forbidden.".into(),
            parameters: vec![
                P::string("id", true, "Stable EngineeringModel v2 Fillet feature id."),
                P::number("radius", true, "Positive constant fillet radius in mm.", Some(0.0)),
                P::array("edge_refs", true, "Non-empty array of unique semantic EdgeReference objects; do not provide edge_index."),
            ],
        },
        ToolDefinition {
            name: ToolName("create_chamfer".into()),
            description: "Add a generic equal-distance Chamfer Feature. edge_refs uses semantic objects {source_feature_id, selector:{kind:profile_boundary|pocket_entrance|hole_entrance|hole_exit|between_faces|convex|concave,...}, qualifier:{curve_kind,radius_mm,length_mm,midpoint,tolerance_mm}}. A hole-mouth chamfer remains a Chamfer, never a Countersink. Raw OCP edge indices are forbidden.".into(),
            parameters: vec![
                P::string("id", true, "Stable EngineeringModel v2 Chamfer feature id."),
                P::number("distance", true, "Positive equal chamfer distance in mm.", Some(0.0)),
                P::array("edge_refs", true, "Non-empty array of unique semantic EdgeReference objects; do not provide edge_index."),
            ],
        },
        ToolDefinition {
            name: ToolName("add_feature_pattern".into()),
            description: "Add a stable EngineeringModel v2 Pattern node referencing an earlier Extrude CUT or Hole seed.".into(),
            parameters: vec![
                P::string("id", true, "Stable Pattern feature id."),
                P::string("seed_feature_id", true, "Earlier parametric Extrude CUT or Hole feature id."),
                P::string_enum("pattern_kind", true, "Pattern transform.", &["linear", "circular", "explicit_placement"]),
                P::integer("count", false, "Total instances including the seed for linear/circular patterns.", Some(2.0)),
                P::number("direction_x", false, "Linear unit direction X.", None),
                P::number("direction_y", false, "Linear unit direction Y.", None),
                P::number("direction_z", false, "Linear unit direction Z.", None),
                P::number("spacing", false, "Linear instance spacing in mm.", Some(0.0)),
                P::number("axis_x", false, "Circular pattern unit axis X.", None),
                P::number("axis_y", false, "Circular pattern unit axis Y.", None),
                P::number("axis_z", false, "Circular pattern unit axis Z.", None),
                P::number("center_x", false, "Circular pattern center X in mm.", None),
                P::number("center_y", false, "Circular pattern center Y in mm.", None),
                P::number("center_z", false, "Circular pattern center Z in mm.", None),
                P::number("radius", false, "Circular pattern radius in mm.", Some(0.0)),
                P::number("start_angle_degrees", false, "Seed's start angle in degrees.", None),
                P::number("angular_span_degrees", false, "Circular span in degrees (360 for full circle).", Some(0.0)),
                P::any("reference_plane", false, "Typed principal or DatumPlane reference for explicit placement."),
                P::array("placements", false, "Additional absolute local U/V anchor positions [{local_u_mm,local_v_mm}, ...]; the original seed remains instance zero."),
            ],
        },
        ToolDefinition {
            name: ToolName("add_hole".into()),
            description: "Add a single round hole at a center position.".into(),
            parameters: vec![
                P::string("id", true, "Feature id of the hole."),
                P::number("center_x", true, "Hole center X in mm.", None),
                P::number("center_y", true, "Hole center Y in mm.", None),
                P::number("diameter", true, "Hole diameter in mm.", Some(0.0)),
            ],
        },
        ToolDefinition {
            name: ToolName("add_slot".into()),
            description: "Add a through oblong slot (overall length, width, center, orientation)."
                .into(),
            parameters: vec![
                P::string("id", true, "Feature id of the slot."),
                P::number("center_x", true, "Slot center X in mm.", None),
                P::number("center_y", true, "Slot center Y in mm.", None),
                P::number(
                    "overall_length",
                    true,
                    "Overall end-to-end slot length in mm.",
                    Some(0.0),
                ),
                P::number("width", true, "Slot width in mm.", Some(0.0)),
                P::number(
                    "orientation_degrees",
                    true,
                    "Slot orientation in degrees.",
                    None,
                ),
            ],
        },
        ToolDefinition {
            name: ToolName("add_circular_hole_pattern".into()),
            description: "Add a circular hole pattern (center, PCD, count, diameter, start angle)."
                .into(),
            parameters: vec![
                P::string("id", true, "Feature id of the pattern."),
                P::number("center_x", true, "Pattern center X in mm.", None),
                P::number("center_y", true, "Pattern center Y in mm.", None),
                P::number("pcd", true, "Pitch circle diameter in mm.", Some(0.0)),
                P::integer("count", true, "Number of holes.", Some(1.0)),
                P::number("diameter", true, "Hole diameter in mm.", Some(0.0)),
                P::number("start_angle", true, "Start angle in degrees.", None),
            ],
        },
        ToolDefinition {
            name: ToolName("add_grid_hole_pattern".into()),
            description: "Add a rectangular grid hole pattern.".into(),
            parameters: vec![
                P::string("id", true, "Feature id of the pattern."),
                P::number("center_x", true, "Grid center X in mm.", None),
                P::number("center_y", true, "Grid center Y in mm.", None),
                P::integer("rows", true, "Number of rows.", Some(1.0)),
                P::integer("columns", true, "Number of columns.", Some(1.0)),
                P::number("spacing_x", true, "Column spacing in mm.", Some(0.0)),
                P::number("spacing_y", true, "Row spacing in mm.", Some(0.0)),
                P::number("diameter", true, "Hole diameter in mm.", Some(0.0)),
            ],
        },
        ToolDefinition {
            name: ToolName("add_corner_hole_pattern".into()),
            description: "Add a pattern of holes inset from each corner of the base profile."
                .into(),
            parameters: vec![
                P::string("id", true, "Feature id of the pattern."),
                P::string("base", true, "Id of the base profile feature."),
                P::number(
                    "offset_x",
                    true,
                    "Horizontal inset from the corner.",
                    Some(0.0),
                ),
                P::number(
                    "offset_y",
                    true,
                    "Vertical inset from the corner.",
                    Some(0.0),
                ),
                P::number("diameter", true, "Hole diameter in mm.", Some(0.0)),
            ],
        },
        ToolDefinition {
            name: ToolName("add_construction_circle".into()),
            description: "Add a reference-only construction circle.".into(),
            parameters: vec![
                P::string("id", true, "Feature id of the circle."),
                P::number("center_x", true, "Circle center X in mm.", None),
                P::number("center_y", true, "Circle center Y in mm.", None),
                P::number("diameter", true, "Circle diameter in mm.", Some(0.0)),
            ],
        },
        ToolDefinition {
            name: ToolName("declare_parameter".into()),
            description: "Declare a new user parameter with a numeric value.".into(),
            parameters: vec![
                P::string("id", true, "Parameter id."),
                P::number("value", true, "Initial numeric value.", None),
            ],
        },
        ToolDefinition {
            name: ToolName("bind_parameter".into()),
            description: "Bind one typed feature slot to a declared parameter.".into(),
            parameters: vec![
                P::string("feature", true, "Feature id."),
                P::string_enum("slot", true, "Typed feature field.", SLOTS),
                P::string("parameter", true, "Declared parameter id."),
            ],
        },
        ToolDefinition {
            name: ToolName("set_parameter".into()),
            description: "Set a parameter to a direct numeric value.".into(),
            parameters: vec![
                P::string("target", true, "Parameter id."),
                P::number("value", true, "Numeric value.", None),
            ],
        },
        ToolDefinition {
            name: ToolName("set_feature_parameter".into()),
            description: "Set one schema-backed feature parameter through its stable typed reference; edits rebuild the source model.".into(),
            parameters: vec![
                P::string("part_id", true, "Stable part id from EngineeringContext."),
                P::string("feature", true, "Stable existing Feature id; do not create a replacement Feature."),
                P::string_enum(
                    "slot",
                    true,
                    "Editable schema-backed Hole parameter slot for the current capability schema.",
                    V2_HOLE_PARAMETER_SLOTS,
                ),
                P::string("parameter", true, "Parameter id from the typed Workbench reference."),
                P::string_enum("expected_type", true, "Expected engineering value type.", &["length_mm", "angle_degrees"]),
                P::number("value", true, "New typed numeric parameter value in mm or degrees.", None),
            ],
        },
        ToolDefinition {
            name: ToolName("set_expression".into()),
            description: "Bind a parameter to a derived expression, e.g. \"plate_width / 8\"."
                .into(),
            parameters: vec![
                P::string("target", true, "Parameter id."),
                P::string("expression", true, "Expression over declared parameters."),
            ],
        },
        ToolDefinition {
            name: ToolName("add_constraint".into()),
            description: "Add a supported engineering constraint between features.".into(),
            parameters: vec![
                P::string("id", true, "Constraint id."),
                P::string_enum("type", true, "Constraint kind.", CONSTRAINT_TYPES),
                P::string(
                    "source",
                    false,
                    "Source feature (required except for fixed).",
                ),
                P::string("target", true, "Target feature."),
                P::number("value", false, "Value for a distance constraint.", None),
            ],
        },
        ToolDefinition {
            name: ToolName("remove_constraint".into()),
            description: "Remove an existing constraint by id.".into(),
            parameters: vec![P::string("target", true, "Constraint id.")],
        },
        ToolDefinition {
            name: ToolName("remove_feature".into()),
            description: "Remove a feature by id (rejected if still referenced).".into(),
            parameters: vec![P::string("target", true, "Feature id.")],
        },
        ToolDefinition {
            name: ToolName("move_feature".into()),
            description: "Move a position-capable feature to an explicit position.".into(),
            parameters: vec![
                P::string("target", true, "Feature id."),
                P::number("x", true, "Target X in mm.", None),
                P::number("y", true, "Target Y in mm.", None),
            ],
        },
        ToolDefinition {
            name: ToolName("set_part_thickness".into()),
            description: "Declare the part thickness required by spatial consumers.".into(),
            parameters: vec![P::number(
                "thickness",
                true,
                "Part thickness in mm.",
                Some(0.0),
            )],
        },
        ToolDefinition {
            name: ToolName("set_hole_extent".into()),
            description: "Declare whether a hole or hole pattern is through-all or blind.".into(),
            parameters: vec![
                P::string("target", true, "Hole or hole-pattern feature id."),
                P::string_enum(
                    "extent",
                    true,
                    "Hole extent.",
                    &["through_all", "blind_base", "blind_opposite"],
                ),
                P::number(
                    "depth",
                    false,
                    "Blind depth in mm; required for blind extents.",
                    Some(0.0),
                ),
            ],
        },
        ToolDefinition {
            name: ToolName("set_drawing_property".into()),
            description: "Set a drawing presentation property (data only, no drawing model)."
                .into(),
            parameters: vec![
                P::string_enum("property", true, "Drawing property.", DRAWING_PROPERTIES),
                P::any("value", true, "Property value (validated by the adapter)."),
            ],
        },
    ]
}
