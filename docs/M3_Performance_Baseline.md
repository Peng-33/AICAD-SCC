# SCC v0.3.0 M3 Performance and Convergence Baseline

## Purpose

This document records the first reproducible local baseline for the deterministic direct Geometry solver. It is evidence for regression comparison, not a frozen CI time threshold. Correctness and convergence assertions remain enforced by ordinary tests; wall-clock thresholds should only be adopted after comparable Windows and Linux CI samples exist.

## Reproduction

```bash
cargo test --release -p scc-geometry \
  --test performance_baseline_tests -- --ignored --nocapture --test-threads=1
```

The benchmark tests are ignored during normal test runs. Each builds its GraphState before starting the timer and measures only `DeterministicGeometrySolver::solve`.

## Baseline environment

- Date: 2026-08-09
- OS: Fedora Linux 44, kernel `7.1.5-201.fc44.x86_64`
- Architecture: x86_64
- CPU: 13th Gen Intel Core i9-13980HX, 24 cores / 32 threads
- Rust: `rustc 1.96.0 (ac68faa20 2026-05-25)`
- Cargo profile: `release`, one test thread

## Results

| Case | Model size | Elapsed | Convergence evidence |
|---|---:|---:|---|
| Explicit Point construction | 10,000 entities | 14,316 µs | `fully_constrained`, `exact`, 0 remaining DoF |
| Linear array expansion | 10,000 derived entities | 3,630 µs | `fully_constrained`, `exact`, 0 remaining DoF, 2 evaluated constraints |

The first array measurement used a linear duplicate-ID scan and took 81,349 µs. Replacing it with deterministic `BTreeSet` membership reduced the measured solve to 3,630 µs while preserving output order and error behavior.

## Interpretation

- The direct solver produces exact, zero-DoF reports for both baseline cases.
- The results establish an order-of-magnitude reference for this host; they are not portable latency guarantees.
- A future CI threshold should use repeated samples, report median and tail latency, and separate platform baselines.
- Any material slowdown should first be reproduced with the same release command and model size before being classified as a regression.
