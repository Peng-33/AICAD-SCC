"""Independent analytic measurements from a reimported STEP solid.

This generic test utility observes the delivered STEP bytes through CadQuery/OCP;
it does not call AICAD's source-shape builder or rely on transient face indices.
"""

from __future__ import annotations

import json
import sys
from pathlib import Path

import cadquery as cq
from OCP.BRepAdaptor import BRepAdaptor_Surface
from OCP.GeomAbs import GeomAbs_Cone, GeomAbs_Cylinder, GeomAbs_Plane, GeomAbs_Torus


def point(value: object) -> list[float]:
    return [float(value.X()), float(value.Y()), float(value.Z())]


def observe(path: Path, probe_points: list[list[float]] | None = None) -> dict[str, object]:
    imported = cq.importers.importStep(str(path))
    solids = imported.solids().vals()
    if len(solids) != 1:
        raise ValueError(f"expected one reimported solid, got {len(solids)}")
    solid = solids[0]
    bounds = solid.BoundingBox()
    surfaces: list[dict[str, object]] = []
    for face in solid.Faces():
        adaptor = BRepAdaptor_Surface(face.wrapped, True)
        kind = adaptor.GetType()
        box = face.BoundingBox()
        record: dict[str, object] = {
            "bbox": [float(box.xmin), float(box.xmax), float(box.ymin), float(box.ymax), float(box.zmin), float(box.zmax)],
        }
        if kind == GeomAbs_Cylinder:
            cylinder = adaptor.Cylinder()
            record.update({
                "kind": "cylinder",
                "radius": float(cylinder.Radius()),
                "axis_origin": point(cylinder.Axis().Location()),
                "axis_direction": point(cylinder.Axis().Direction()),
                "u_range": [float(adaptor.FirstUParameter()), float(adaptor.LastUParameter())],
            })
        elif kind == GeomAbs_Cone:
            cone = adaptor.Cone()
            record.update({
                "kind": "cone",
                "reference_radius": float(cone.RefRadius()),
                "semi_angle_radians": float(cone.SemiAngle()),
                "axis_origin": point(cone.Axis().Location()),
                "axis_direction": point(cone.Axis().Direction()),
                "u_range": [float(adaptor.FirstUParameter()), float(adaptor.LastUParameter())],
            })
        elif kind == GeomAbs_Torus:
            torus = adaptor.Torus()
            record.update({
                "kind": "torus",
                "major_radius": float(torus.MajorRadius()),
                "minor_radius": float(torus.MinorRadius()),
                "axis_origin": point(torus.Axis().Location()),
                "axis_direction": point(torus.Axis().Direction()),
            })
        elif kind == GeomAbs_Plane:
            plane = adaptor.Plane()
            record.update({
                "kind": "plane",
                "plane_origin": point(plane.Location()),
                "plane_normal": point(plane.Axis().Direction()),
            })
        else:
            continue
        surfaces.append(record)
    material_probes = []
    for coordinates in probe_points or []:
        if len(coordinates) != 3:
            raise ValueError("material probe must have three coordinates")
        material_probes.append({
            "point": coordinates,
            "is_inside": bool(solid.isInside(cq.Vector(*coordinates), 1e-7)),
        })
    return {
        "source": "cadquery_ocp_reimported_step",
        "valid": bool(solid.isValid()),
        "material_probes": material_probes,
        "solid_count": len(solids),
        "bbox": [float(bounds.xmin), float(bounds.xmax), float(bounds.ymin), float(bounds.ymax), float(bounds.zmin), float(bounds.zmax)],
        "volume_mm3": float(solid.Volume()),
        "surfaces": surfaces,
    }


if __name__ == "__main__":
    probes = [[float(value) for value in argument.split(",")] for argument in sys.argv[2:]]
    print(json.dumps(observe(Path(sys.argv[1]), probes), separators=(",", ":")))
