# Báo Cáo Nghiệm Thu Hoàn Thành Phase 20A: Google Flow Browser Driver & Gemini Prompt Refinement

**Dự án**: AutoVideo AI  
**Phase**: Phase 20A — Google Flow Browser Driver & Gemini Prompt Refinement  
**Ngày hoàn thành**: 21/08/2026  
**Zero-Fake Policy & Cost Expenditure**: $0.00 tiêu thụ API có phí, 0 Flow credit thật, 0 Replicate prediction, 0 fake provider response.

---

## 1. Mục Tiêu & Phạm Vi Triển Khai (Phase 20A)

Triển khai hoàn chỉnh hệ thống **Google Flow Browser Driver & Gemini Prompt Refinement** và gói runtime hotfix:

1. **Wire Rust -> Real Node Playwright Sidecar Process**:
   - Giao thức chuẩn line-delimited JSON-RPC qua `stdin`/`stdout` giữa tiến trình Rust và tiến trình con Node Playwright (`src-tauri/sidecars/flow-playwright/dist/index.js`).
   - Quản lý lifecycle đầy đủ: spawn, call RPC với request ID, timeout, stderr sanitization, graceful close, và process crash detection.
   - Mock mode kích hoạt trình duyệt Chromium thật điều khiển bởi Playwright để tương tác trực tiếp với `MockFlowServer` cục bộ.

2. **Persistent Login Browser Session Lifecycle (FlowBrowserSessionManager)**:
   - `FlowBrowserSessionManager` sở hữu trực tiếp các `PlaywrightSidecarProcess` qua cấu trúc khóa 2 cấp: `Mutex<HashMap<ProfileId, Arc<tokio::sync::Mutex<FlowBrowserSession>>>>`. Outer lock cho map lookup/insert/remove, inner lock cho các thao tác RPC browser bất đồng bộ.
   - Khi `open_flow_profile_browser()` được gọi: profile lock được xác lập, tiến trình Chromium headed được giữ sống liên tục trong session manager (không bị tự động drop/kill khi hàm trả về).
   - Nút `[ Close Browser ]` cho phép người dùng chủ động đóng Chromium và giải phóng profile lock.
   - Phân biệt rõ `isLocked` (profile bị khóa do session login hoặc background generation worker) và `browserSessionOpen` (phiên Chromium login đang mở).
   - Clean shutdown handler đảm bảo toàn bộ session Chromium đang mở được giải phóng sạch sẽ khi đóng ứng dụng.

3. **Gemini Credential Lifecycle & Granular Diagnostic Engine**:
   - `GeminiCredentialManager` lưu trữ trạng thái shared state trong ứng dụng, quản lý khóa bảo mật thông qua OS Credential Manager (Windows Credential Manager / macOS Keychain / Linux Secret Service).
   - Khởi tạo ban đầu: `stored = true/false` và `verification_status = UNVERIFIED`.
   - Hàm kiểm tra `test_gemini_api_key()` thực hiện probe HTTP thực tế tới `models.get (gemini-3.5-flash)` để cập nhật trạng thái `VALID` hoặc phân tích mã lỗi chi tiết.
   - Bảng ánh xạ mã lỗi Google Generative AI đầy đủ:
     - 400 (`API_KEY_INVALID`, `INVALID_ARGUMENT`) -> `INVALID_KEY`
     - 400 (yêu cầu không hợp lệ khác) -> `BAD_REQUEST`
     - 401 -> `INVALID_KEY`
     - 403 (`PERMISSION_DENIED`) -> `FORBIDDEN`
     - 404 (`NOT_FOUND`) -> `MODEL_UNAVAILABLE`
     - 429 (`RESOURCE_EXHAUSTED`) -> `RATE_LIMITED`
     - 5xx (`INTERNAL`, `UNAVAILABLE`) -> `PROVIDER_TEMPORARY_FAILURE`
     - Timeout -> `TIMEOUT`
     - Lỗi mạng/kết nối -> `NETWORK_ERROR`
   - Bảo mật tuyệt đối: toàn bộ chuỗi lỗi và log đều được lọc và che dấu tự động (`[REDACTED_API_KEY]`), không để rò rỉ token API key.
   - Khi kiểm tra thất bại: khóa đã lưu vẫn được bảo toàn nguyên vẹn trong OS Credential Manager (`stored: true`), không xóa nhầm credential.

4. **Real Mock Playwright Chromium E2E Verification**:
   - Test `test_phase20a_38_real_mock_playwright_chromium_e2e` khởi động `MockFlowServer` cục bộ, tạo profile Chromium tạm thời, spawn Node sidecar, khởi chạy Chromium thật, điều hướng tới Mock Flow, tải tệp MP4 tổng hợp, điền prompt, click Generate thật đúng 1 lần (xác thực qua counter), poll tiến độ, tải video đầu ra và xác thực tệp tải về thành công.

5. **Fail-Closed Flow UI Adapter**:
   - Bắt lỗi nghiêm ngặt khi thiếu selector: `FLOW_UI_CHANGED: prompt textarea missing`, `FLOW_UI_CHANGED: file input missing`, `FLOW_UI_CHANGED: generate button missing/disabled`.
   - Lưu trữ bằng chứng sinh video ngữ nghĩa (`localSubmissionAttemptId`, page fingerprint, post-click state transition, `submittedAt`).
   - Loại bỏ cờ stealth `--disable-blink-features=AutomationControlled`.
   - `checkAuthStatus` chỉ trả về `READY`, `LOGIN_REQUIRED`, `UNKNOWN` (UNKNOWN kích hoạt fail-closed).

6. **Crash-Safe Pre-Click State Machine & Ambiguous Recovery**:
   - Trước khi thực hiện bất kỳ submit nào: kiểm tra `child.submission_state`.
   - `NEVER_ATTEMPTED`: ghi `ATTEMPT_PERSISTED` xuống đĩa -> thực hiện submit đúng 1 lần.
   - `ATTEMPT_PERSISTED` / `AMBIGUOUS`: **ZERO automatic resubmit**, chuyển trạng thái `GENERATION_AMBIGUOUS` yêu cầu người dùng xác nhận hoặc đối soát UI.
   - `PROVEN_SUBMITTED`: **ZERO resubmit**, tiếp tục poll bằng `submission_evidence` đã lưu.
   - `PROVEN_COMPLETED`: tái sử dụng artifact đã qua validation, **ZERO resubmit**.

7. **Cross-Instance Atomic Profile File Lock**:
   - Cơ chế khóa file `.session.lock` đa tiến trình/đa instance sử dụng `create_new(true)`.
   - Instance thứ 2 cố truy cập profile đang chạy lập tức nhận lỗi `PROFILE_IN_USE`.

8. **Source Media & Path Confinement**:
   - Backend chuẩn hóa và kiểm tra đường dẫn canonical `candidate.starts_with(canonical_root)`.
   - Lệnh `start_flow_generation` ràng buộc source video từ thư mục media của dự án `<project>/media/`, loại bỏ ô nhập đường dẫn tự do trên giao diện.

9. **Flow Credit Policy Authority**:
   - `OMNI_EDIT_UPLOADED_VIDEO_CREDITS_PER_GENERATION = 40` (40 credits / segment generation, 0 automatic retries).

---

## 2. Bảng Trạng Thái Nghiệm Thu & Acceptance Flags

| Tiêu chí / Acceptance Flag | Giá trị công bố | Ghi chú |
| :--- | :---: | :--- |
| **MOCK_PLAYWRIGHT_CHROMIUM_VERIFIED** | **YES** | Đã xác thực qua Real Sidecar + Real Playwright + Real Chromium + Local Mock Server |
| **LOGIN_BROWSER_SESSION_PERSISTENCE_VERIFIED** | **YES** | Phiên Chromium login sống bền bỉ, không bị tự động tắt khi IPC call hoàn thành |
| **PROFILE_AUTH_REFRESH_RELOAD_CONSISTENCY** | **YES** | Refresh READY -> reload profiles vẫn READY; đóng browser -> status trở về UNKNOWN |
| **APP_SHUTDOWN_SESSION_CLEANUP_WIRED** | **YES** | Hook `handle_app_shutdown` được kích hoạt trên `ExitRequested`/`Exit`, đóng sạch sẽ toàn bộ Chromium và giải phóng lock |
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
dist/assets/index-BpOSgi2F.css   95.57 kB │ gzip:  13.14 kB
dist/assets/window-D1PylawR.js   13.92 kB │ gzip:   3.43 kB
dist/assets/index-B3nAcdmx.js   499.96 kB │ gzip: 125.47 kB
✓ built in 7.03s
```
**Kết quả**: ✅ **0 lỗi TypeScript, 0 lỗi Vite bundle.**

### 3.3. Frontend Vitest Tests (`npm test -- --run`)
```text
 ✓ src/stores/__tests__/segmentedCloudJobStore.test.ts (8 tests) 8ms
 ✓ src/stores/__tests__/cloudJobStore.test.ts (12 tests) 9ms
 ✓ src/stores/__tests__/flowJobStore.test.ts (6 tests) 9ms
 ✓ src/features/flow/__tests__/flowPromptUx.test.ts (12 tests) 13ms

 Test Files  4 passed (4)
      Tests  38 passed (38)
   Duration  431ms
```
**Kết quả**: ✅ **4 test files, 38/38 tests PASSED (100%).**

### 3.4. Rust Formatting (`cargo fmt --check`)
```text
cargo fmt --check
Exit code: 0
```
**Kết quả**: ✅ **Formatting chuẩn 100%.**

### 3.5. Rust Type Check (`cargo check`)
```text
cargo check
Exit code: 0
```
**Kết quả**: ✅ **0 compile errors, 0 warnings.**

### 3.6. Rust Test Regression Suites (Serial Execution)
- `cargo test --lib -- tests_phase20a --test-threads=1`: **59/59 passed (100%)**
  - `test_phase20a_01` -> `test_phase20a_12`: Prompt optimization provenance, sanitization, single active request & undo tests.
  - `test_phase20a_13` -> `test_phase20a_19`: Flow manifest freeze, submitted prompt hash, credit calculation & log secrecy.
  - `test_phase20a_20` -> `test_phase20a_27`: Legal segmentation, fractional CFR frames, audio mux & corruption rejection.
  - `test_phase20a_28` -> `test_phase20a_44`: Security mock tests, real Playwright Chromium e2e, crash safety invariant & zero click resubmit.
  - `test_phase20a_45_browser_session_persistence_and_bounded_alive`: Session sống liên tục qua IPC, không bị drop/kill.
  - `test_phase20a_46_same_session_auth_refresh`: Refresh auth trên cùng 1 phiên Chromium đang mở.
  - `test_phase20a_47_browser_already_open_guard`: Ngăn chặn mở trùng phiên trên cùng profile (`BROWSER_ALREADY_OPEN`).
  - `test_phase20a_48_explicit_browser_close_and_lock_release`: Đóng Chromium và giải phóng `.session.lock`.
  - `test_phase20a_49_session_manager_shutdown_cleanup`: `close_all()` giải phóng toàn bộ Chromium khi tắt ứng dụng.
  - `test_phase20a_50_profile_locked_by_worker_not_browser_open`: Phân biệt rõ `isLocked` và `browser_session_open`.
  - `test_phase20a_51_gemini_unverified_on_store`: Lưu khóa mới có trạng thái ban đầu `UNVERIFIED`.
  - `test_phase20a_52_gemini_mock_validation_success_valid`: Phản hồi 200 từ Mock Gemini cập nhật `VALID`.
  - `test_phase20a_53_gemini_mock_validation_error_statuses`: Ánh xạ chính xác các mã lỗi 400/401/403/404/429/500/timeout/network.
  - `test_phase20a_54_failed_verification_preserves_stored_key`: Lỗi xác thực không làm mất khóa đã lưu.
  - `test_phase20a_55_get_gemini_status_retains_valid_in_session`: Giữ trạng thái `VALID` trong phiên ứng dụng.
  - `test_phase20a_56_app_restart_resets_to_unverified`: Khởi động lại ứng dụng đặt lại trạng thái `UNVERIFIED`.
  - `test_phase20a_57_zero_credential_leakage_in_diagnostics`: Che dấu API key trong toàn bộ chuỗi chẩn đoán.
  - `test_phase20a_58_profile_auth_refresh_reload_consistency`: Live auth status được bảo toàn nhất quán qua các lần list/load profiles.
  - `test_phase20a_59_production_app_shutdown_lifecycle_callback_cleans_sessions`: Callback shutdown của app gọi `close_all()` giải phóng lock và tiến trình.
- Workspace regression suite (`cargo test --workspace -- --test-threads=1`): **849/849 passed (100%)**

**Tổng cộng Rust tests**: ✅ **849/849 tests PASSED (100%), 0 failures, 0 regressions.**

---

## 4. Hạn Chế Còn Lại & Khuyến Nghị Tiếp Theo

1. Chưa thực hiện live authentication hoặc live video generation trên hạ tầng Google Flow thật.
2. Quá trình ghép nối video và chất lượng hình ảnh ở ranh giới segment (`FLOW_SEGMENT_BOUNDARY_VISUAL_QUALITY`) sẽ được kiểm chứng khi kết nối live Google Flow trong các phase tiếp theo.
3. Hoàn tất toàn bộ Phase 20A và **DỪNG LẠI**, tuyệt đối không bắt đầu Phase 20B khi chưa có chỉ thị.
