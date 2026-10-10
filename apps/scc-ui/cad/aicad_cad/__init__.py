"""Deterministic AICAD solid builders."""

__all__ = ["BuildResult", "build_feature_graph", "build_gearbox", "StepDeliveryError", "build_and_export_step"]


def __getattr__(name):
    if name == "BuildResult":
        from .model import BuildResult
        return BuildResult
    if name == "build_feature_graph":
        from .feature_graph import build_feature_graph
        return build_feature_graph
    if name == "build_gearbox":
        from .gearbox import build_gearbox
        return build_gearbox
    if name == "StepDeliveryError":
        from .step_delivery import StepDeliveryError
        return StepDeliveryError
    if name == "build_and_export_step":
        from .step_delivery import build_and_export_step
        return build_and_export_step
    raise AttributeError(name)
