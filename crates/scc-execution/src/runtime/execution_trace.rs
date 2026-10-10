use scc_core::ids::SnapshotId;
use scc_core::runtime::{ExecutionTrace, TraceEvent};

pub struct ExecutionTraceRecorder;

impl ExecutionTraceRecorder {
    pub fn record_overlay_events(trace: &mut ExecutionTrace, events: Vec<(SnapshotId, String)>) {
        for (snapshot, summary) in events {
            trace.events.push(TraceEvent {
                snapshot,
                command_summary: summary,
            });
        }
    }
}
