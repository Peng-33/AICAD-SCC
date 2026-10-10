from dataclasses import dataclass

import cadquery as cq


@dataclass(frozen=True)
class BuildResult:
    shape: cq.Workplane
    feature_ids: tuple[str, ...]
    feature_records: tuple[dict, ...] = ()
