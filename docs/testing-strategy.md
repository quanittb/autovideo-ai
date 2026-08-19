# AutoVideo AI — Testing Strategy & Tiered Execution

## 1. Test Architecture Overview

To prevent expensive multi-gigabyte disk reads or hours-long neural inference from blocking routine builds and CI/CD pipelines, tests are segregated into 5 tiers:

| Test Tier | Scope | Target Execution Time | GPU Required? | Network Required? | Invocation Command |
|---|---|---|---|---|---|
| **A. Unit Tests** | Component math, blends, parsing, data structures | < 50 ms | No | No | `cargo test --lib` |
| **B. Contract Tests** | Provider interfaces, routing logic, budget limits, error codes | < 100 ms | No | No | `cargo test test_phase12` |
| **C. Fast Regression** | Hardware classification, manifest checks, state transitions | < 500 ms | No | No | `cargo test test_phase11 test_phase10 test_phase9` |
| **D. Local Smoke Test** | Real 1–4 frame neural generation on local GPU | < 30 sec | Yes | No | `cargo test test_local_smoke` |
| **E. Production Acceptance** | Full end-to-end video artifact verification | Explicit Manual Run | Yes | Optional | `python src-tauri/scripts/phase12_acceptance.py` |

## 2. Hard Timeouts

All long-running operations enforce strict cancellation and watchdog timeouts:
- `LOCAL_SMOKE_TIMEOUT`: 60 seconds
- `CLOUD_REQUEST_TIMEOUT`: 120 seconds
- `JOB_STAGE_TIMEOUT`: 300 seconds

## 3. Fast Regression Verification Command

```powershell
# Fast build and lint check
cargo fmt -- --check
cargo check --all-targets

# Fast unit & contract test suite (all 599 tests in < 2 seconds)
cargo test --test-threads=1

# Frontend build
npm run build
```
