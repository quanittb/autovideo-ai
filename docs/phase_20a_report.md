# Báo Cáo Nghiệm Thu Hoàn Thành Phase 20A: Google Flow Browser Driver & Gemini Prompt Refinement

**Dự án**: AutoVideo AI  
**Phase**: Phase 20A — Google Flow Browser Driver & Gemini Prompt Refinement  
**Ngày hoàn thành**: 21/08/2026  
**Zero-Fake Policy & Cost Expenditure**: $0.00 tiêu thụ API có phí, 0 Flow credit thật, 0 Replicate prediction, 0 fake provider response.

---

## 1. Mục Tiêu & Phạm Vi Triển Khai (Phase 20A)

Triển khai hoàn chỉnh hệ thống **Google Flow Browser Driver & Gemini Prompt Refinement** theo tiêu chuẩn 19 điểm Behavioral & Acceptance Corrections:

1. **Wire Rust -> Real Node Playwright Sidecar Process**:
   - Giao thức chuẩn line-delimited JSON-RPC qua `stdin`/`stdout` giữa tiến trình Rust và tiến trình con Node Playwright (`src-tauri/sidecars/flow-playwright/dist/index.js`).
   - Quản lý lifecycle đầy đủ: spawn, call RPC với request ID, timeout, stderr sanitization, graceful close, và process crash detection.
   - Mock mode kích hoạt trình duyệt Chromium thật điều khiển bởi Playwright để tương tác trực tiếp với `MockFlowServer` cục bộ.

2. **Real Mock Playwright Chromium E2E Verification**:
   - Test `test_phase20a_38_real_mock_playwright_chromium_e2e` khởi động `MockFlowServer` cục bộ, tạo profile Chromium tạm thời, spawn Node sidecar, khởi chạy Chromium thật, điều hướng tới Mock Flow, tải tệp MP4 tổng hợp, điền prompt, click Generate thật đúng 1 lần (xác thực qua counter), poll tiến độ, tải video đầu ra và xác thực tệp tải về thành công.

3. **Fail-Closed Flow UI Adapter**:
   - Bắt lỗi nghiêm ngặt khi thiếu selector: `FLOW_UI_CHANGED: prompt textarea missing`, `FLOW_UI_CHANGED: file input missing`, `FLOW_UI_CHANGED: generate button missing/disabled`.
   - Lưu trữ bằng chứng sinh video ngữ nghĩa (`localSubmissionAttemptId`, page fingerprint, post-click state transition, `submittedAt`).
   - Loại bỏ cờ stealth `--disable-blink-features=AutomationControlled`.
   - `checkAuthStatus` chỉ trả về `READY`, `LOGIN_REQUIRED`, `UNKNOWN` (UNKNOWN kích hoạt fail-closed).

4. **Crash-Safe Pre-Click State Machine & Ambiguous Recovery**:
   - Trước khi thực hiện bất kỳ submit nào: kiểm tra `child.submission_state`.
   - `NEVER_ATTEMPTED`: ghi `ATTEMPT_PERSISTED` xuống đĩa -> thực hiện submit đúng 1 lần.
   - `ATTEMPT_PERSISTED` / `AMBIGUOUS`: **ZERO automatic resubmit**, chuyển trạng thái `GENERATION_AMBIGUOUS` yêu cầu người dùng xác nhận hoặc đối soát UI.
   - `PROVEN_SUBMITTED`: **ZERO resubmit**, tiếp tục poll bằng `submission_evidence` đã lưu.
   - `PROVEN_COMPLETED`: tái sử dụng artifact đã qua validation, **ZERO resubmit**.
   - Test `test_phase20a_34_restart_recovery_zero_additional_generate_clicks` xác thực số lần click Generate giữ nguyên đúng 0 lần click mới sau sự cố.

5. **Cross-Instance Atomic Profile File Lock**:
   - Cơ chế khóa file `.session.lock` đa tiến trình/đa instance sử dụng `create_new(true)`.
   - Instance thứ 2 cố truy cập profile đang chạy lập tức nhận lỗi `PROFILE_IN_USE`.

6. **Source Media & Path Confinement**:
   - Backend chuẩn hóa và kiểm tra đường dẫn canonical `candidate.starts_with(canonical_root)`.
   - Lệnh `start_flow_generation` ràng buộc source video từ thư mục media của dự án `<project>/media/`, loại bỏ ô nhập đường dẫn tự do trên giao diện.

7. **Fail-Closed SecretStore & Gemini Header Authentication**:
   - Lưu trữ Gemini API key vào OS Credential Manager (Windows Credential Manager / macOS Keychain / Linux Secret Service).
   - Gọi Gemini API qua header `x-goog-api-key` (không đưa key vào URL query parameter).
   - Sanitize toàn bộ lỗi mạng từ Gemini, không bao giờ serialize authorization header hoặc api key ra log hay UI.

8. **Sanitized Public DTO Snapshots**:
   - `FlowProfileSnapshot`: `{ profileId, name, status, isLocked, createdAt, updatedAt }` (loại bỏ `profile_dir`).
   - `FlowJobSnapshot`: `{ ... finalOutputReady: bool ... }` (loại bỏ `final_output_path` đường dẫn thô).

9. **Flow Credit Policy Authority**:
   - `OMNI_EDIT_UPLOADED_VIDEO_CREDITS_PER_GENERATION = 40` (40 credits / segment generation, 0 automatic retries).

---

## 2. Bảng Trạng Thái Nghiệm Thu & Acceptance Flags

| Tiêu chí / Acceptance Flag | Giá trị công bố | Ghi chú |
| :--- | :---: | :--- |
| **MOCK_PLAYWRIGHT_CHROMIUM_VERIFIED** | **YES** | Đã xác thực qua Real Sidecar + Real Playwright + Real Chromium + Local Mock Server |
| **FLOW_REAL_BROWSER_VERIFIED** | **NO** | Chưa kết nối live Google Flow (tuân thủ Phase 20A mock scope) |
| **FLOW_REAL_GENERATION_VERIFIED** | **NO** | Chưa sinh video thật trên Google Flow |
| **PREVIEW_RUNTIME_VERIFIED** | **NO** | Kế thừa từ Phase 19 |
| **SEGMENT_BOUNDARY_VISUAL_QUALITY** | **NOT LIVE VERIFIED** | Kế thừa từ Phase 19 |
| **FLOW_SEGMENT_BOUNDARY_VISUAL_QUALITY** | **NOT LIVE VERIFIED** | Chưa nghiệm thu chất lượng hình ảnh ghép nối live |
| **FLOW_GENERATIONS** | **0** | Zero generation thật |
| **FLOW_CREDITS** | **0** | Zero credit thật tiêu hao |
| **REPLICATE_PREDICTIONS** | **0** | Zero prediction phát sinh |
| **PAID_COST** | **\$0.00** | Zero dollar chi phí phát sinh |

---

## 3. Kết Quả Kiểm Tra Chất Lượng (Quality Gates)

### 3.1. Node/TypeScript Sidecar Build Gate
```text
cd src-tauri/sidecars/flow-playwright
npm run build
> flow-playwright-sidecar@0.1.0 build
> tsc
Exit code: 0
```
**Kết quả**: ✅ **0 lỗi TypeScript, sidecar build thành công độc lập.**

### 3.2. Frontend Production Build (`npm run build`)
```text
> autovideo-ai@0.1.0 build
> tsc && vite build
vite v7.3.6 building client environment for production...
✓ 1865 modules transformed.
dist/index.html                   0.49 kB │ gzip:   0.31 kB
dist/assets/index-C8VDbSTs.css   94.71 kB │ gzip:  13.06 kB
dist/assets/window-DlQUgftK.js   13.92 kB │ gzip:   3.43 kB
dist/assets/index-JklCOE5h.js   495.35 kB │ gzip: 124.64 kB
✓ built in 6.07s
```
**Kết quả**: ✅ **0 lỗi TypeScript, 0 lỗi Vite bundle.**

### 3.3. Frontend Vitest Tests (`npm test -- --run`)
```text
 ✓ src/stores/__tests__/cloudJobStore.test.ts (12 tests) 6ms
 ✓ src/stores/__tests__/segmentedCloudJobStore.test.ts (8 tests) 6ms
 ✓ src/stores/__tests__/flowJobStore.test.ts (4 tests) 5ms
 ✓ src/features/flow/__tests__/flowPromptUx.test.ts (12 tests) 10ms

 Test Files  4 passed (4)
      Tests  36 passed (36)
   Duration  340ms
```
**Kết quả**: ✅ **4 test files, 36/36 tests PASSED (100%).**

### 3.4. Rust Formatting (`cargo fmt -- --check`)
```text
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
Exit code: 0
```
**Kết quả**: ✅ **Formatting chuẩn 100%.**

### 3.5. Rust Type Check (`cargo check --all-targets`)
```text
cargo check --all-targets --manifest-path src-tauri/Cargo.toml
Finished `dev` profile in 10.03s
Exit code: 0
```
**Kết quả**: ✅ **0 compile errors, 0 warnings.**

### 3.6. Rust Test Regression Suites (Serial Execution)
- `cargo test --lib -- tests_phase20a`: **44/44 passed (100%)**
  - Bao gồm `test_phase20a_38_real_mock_playwright_chromium_e2e` với real Chromium execution.
  - Bao gồm `test_phase20a_34_restart_recovery_zero_additional_generate_clicks` với crash window assertion.
  - Bao gồm `test_phase20a_28_same_profile_concurrency_lock` cross-instance file lock.
- `cargo test --lib -- tests_phase19`: **29/29 passed (100%)**
- `cargo test --lib -- tests_phase18`: **13/13 passed (100%)**
- `cargo test --lib -- tests_phase17`: **56/56 passed (100%)**
- `cargo test --lib -- tests_phase16`: **39/39 passed (100%)**
- `cargo test --lib -- tests_phase15`: **38/38 passed (100%)**
- `cargo test --lib -- test_phase14`: **10/10 passed (100%)**
- `cargo test --lib -- test_cloud`: **6/6 passed (100%)**

**Tổng cộng Rust tests**: ✅ **235/235 tests PASSED (100%), 0 failures, 0 regressions.**

---

## 4. Hạn Chế Còn Lại & Khuyến Nghị Tiếp Theo

1. Chưa thực hiện live authentication hoặc live video generation trên hạ tầng Google Flow thật.
2. Quá trình ghép nối video và chất lượng hình ảnh ở ranh giới segment (`FLOW_SEGMENT_BOUNDARY_VISUAL_QUALITY`) sẽ được kiểm chứng khi kết nối live Google Flow trong các phase tiếp theo.
3. Hoàn tất toàn bộ Phase 20A và **DỪNG LẠI**, tuyệt đối không bắt đầu Phase 20B khi chưa có chỉ thị.
