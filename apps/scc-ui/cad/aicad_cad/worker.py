import argparse
import json
import re
from pathlib import Path

import cadquery as cq

from .feature_graph import build_feature_graph
from .gearbox import build_gearbox


def _artifacts(base_name: str, source_suffix: str = "caddoc.json") -> dict[str, tuple[str, str]]:
    return {
        "source": (f"{base_name}.{source_suffix}", "application/json"),
        "brep": (f"{base_name}.brep", "application/octet-stream"),
        "stl": (f"{base_name}.stl", "model/stl"),
        "glb": (f"{base_name}.glb", "model/gltf-binary"),
        "svg": (f"{base_name}.svg", "image/svg+xml"),
    }


def _safe_name(value: str) -> str:
    slug = re.sub(r"[^a-z0-9]+", "-", value.lower()).strip("-")
    return slug[:64] or "cad-document"


def _coverage(spec: dict) -> dict:
    requirements = spec.get("requirements", [])
    if spec.get("kind") != "feature_graph_v1":
        return {"status": "legacy_untracked", "total": 0, "complete": 0, "open": 0}
    dispositions = {}
    for requirement in requirements:
        disposition = requirement["disposition"]
        dispositions[disposition] = dispositions.get(disposition, 0) + 1
    open_count = sum(dispositions.get(key, 0) for key in ("assumption", "unresolved", "unsupported"))
    return {
        "status": "complete" if open_count == 0 else "partial",
        "total": len(requirements),
        "complete": len(requirements) - open_count,
        "open": open_count,
        **dispositions,
        "by_disposition": dispositions,
    }


def _parameter_evidence(spec: dict) -> dict:
    if spec.get("kind") != "feature_graph_v1":
        return {"status": "legacy_untracked", "total": 0}
    ids_by_source = {"user": [], "derived": [], "assumption": []}
    derived_values = {}
    for parameter_id, definition in spec.get("parameters", {}).items():
        source = definition["source"]
        ids_by_source[source].append(parameter_id)
        if source == "derived":
            derived_values[parameter_id] = definition["value"]
    return {
        "status": "tracked",
        "total": sum(len(ids) for ids in ids_by_source.values()),
        **{source: len(ids) for source, ids in ids_by_source.items()},
        "ids_by_source": ids_by_source,
        "derived_values": derived_values,
    }


def _manufacturing_evidence(spec: dict) -> dict:
    if spec.get("kind") != "feature_graph_v1":
        return {"status": "legacy_untracked", "total": 0, "ids": []}
    categories = {}
    ids = []
    for category, semantics in spec.get("manufacturing", {}).items():
        category_ids = [semantic["id"] for semantic in semantics]
        categories[category] = {"count": len(category_ids), "ids": category_ids}
        ids.extend(category_ids)
    return {
        "status": "tracked",
        "total": len(ids),
        "ids": ids,
        "categories": categories,
    }


def build_document(spec: dict, output_dir: Path) -> dict:
    output_dir.mkdir(parents=True, exist_ok=True)
    kind = spec.get("kind")
    if kind == "precision_gearbox_housing_v1":
        result = build_gearbox(spec)
        base_name = "gearbox-housing"
        source_suffix = "cadspec.json"
    elif kind == "feature_graph_v1":
        result = build_feature_graph(spec)
        base_name = _safe_name(spec.get("name", "cad-document"))
        source_suffix = "caddoc.json"
    else:
        raise ValueError("Unsupported CadSpec kind")

    artifacts_definition = _artifacts(base_name, source_suffix)
    shape = result.shape.val()
    if not shape.isValid() or not shape.Solids():
        raise ValueError("OCCT produced an invalid or empty solid")
    # Capture exact B-Rep bounds before STL/GLB meshing attaches a deflected
    # triangulation that can expand later bounding-box queries by ~0.01 mm.
    bounds = shape.BoundingBox()

    source_path = output_dir / artifacts_definition["source"][0]
    source_path.write_text(json.dumps(spec, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    shape.exportBrep(str(output_dir / artifacts_definition["brep"][0]))
    cq.exporters.export(
        result.shape,
        str(output_dir / artifacts_definition["stl"][0]),
        tolerance=0.1,
        angularTolerance=0.1,
    )
    assembly = cq.Assembly(name=base_name)
    assembly.add(result.shape, name="part", color=cq.Color(0.45, 0.55, 0.65))
    assembly.export(str(output_dir / artifacts_definition["glb"][0]))
    cq.exporters.export(
        result.shape,
        str(output_dir / artifacts_definition["svg"][0]),
        opt={
            "width": 960,
            "height": 720,
            "projectionDir": (1, -1, 0.75),
            "showAxes": False,
            "showHidden": False,
            "strokeWidth": 0.6,
        },
    )

    artifacts = {
        name: {
            "filename": filename,
            "content_type": content_type,
            "bytes": (output_dir / filename).stat().st_size,
        }
        for name, (filename, content_type) in artifacts_definition.items()
    }
    return {
        "status": "built",
        "document_kind": kind,
        "source_hash": spec.get("source_hash"),
        "solid_valid": True,
        "solid_count": len(shape.Solids()),
        "volume_mm3": shape.Volume(),
        "bounding_box_mm": {
            "x": round(bounds.xlen, 6),
            "y": round(bounds.ylen, 6),
            "z": round(bounds.zlen, 6),
        },
        "feature_ids": list(result.feature_ids),
        "feature_records": list(result.feature_records),
        "coverage": _coverage(spec),
        "parameter_evidence": _parameter_evidence(spec),
        "manufacturing_evidence": _manufacturing_evidence(spec),
        "artifacts": artifacts,
    }


def main() -> None:
    parser = argparse.ArgumentParser(description="Build a bounded AICAD CadSpec")
    parser.add_argument("--input", required=True)
    parser.add_argument("--output", required=True)
    args = parser.parse_args()
    spec = json.loads(Path(args.input).read_text(encoding="utf-8"))
    print(json.dumps(build_document(spec, Path(args.output)), ensure_ascii=False, separators=(",", ":")))


if __name__ == "__main__":
    main()
