# Báo Cáo Nghiệm Thu Hoàn Thành Phase 20A: Google Flow Browser Driver & Gemini Prompt Refinement

**Dự án**: AutoVideo AI  
**Phase**: Phase 20A — Google Flow Browser Driver & Gemini Prompt Refinement  
**Ngày hoàn thành**: 24/08/2026  
**Zero-Fake Policy & Cost Expenditure**: $0.00 tiêu thụ API có phí, 0 Flow credit thật, 0 Replicate prediction, 0 fake provider response.

---

## 1. Mục Tiêu & Phạm Vi Triển Khai (Phase 20A)

Triển khai hoàn chỉnh hệ thống **Google Flow Browser Driver & Gemini Prompt Refinement** cùng kiến trúc tách biệt trình duyệt đăng nhập thủ công và tự động hóa:

1. **Ranh Giới Bảo Mật Tuyệt Đối Của Tài Khoản Google (Strict Google Login Security Boundary)**:
   - Tuyệt đối không tự động hóa quy trình đăng nhập Google Account dưới bất kỳ hình thức nào (không autofill, không CDP, không Playwright, không WebView, không bypass stealth).
   - Tách biệt hoàn toàn 2 runtime trình duyệt:
     - **MANUAL_LOGIN_BROWSER**: Trình duyệt Google Chrome tiêu chuẩn đã cài đặt trên máy tính người dùng (`SystemChromeLauncher`), chạy hoàn toàn độc lập, **không Playwright, không CDP, không WebDriver, không cờ điều khiển từ xa**.
     - **FLOW_AUTOMATION_BROWSER**: Playwright Node Sidecar sử dụng Google Chrome Stable (`channel: 'chrome'`), chỉ chạy cho mục đích tự động hóa tác vụ trên Google Flow và kiểm tra trạng thái đăng nhập riêng biệt.

2. **Khởi Chạy Google Chrome Chuẩn Hệ Thống (`SystemChromeLauncher`)**:
   - Tự động phát hiện Google Chrome cài đặt trên Windows (`ProgramFiles`, `ProgramFiles(x86)`, `LOCALAPPDATA`, `where chrome.exe`), macOS, và Linux. Nếu không tìm thấy, trả về lỗi ngữ nghĩa `CHROME_NOT_INSTALLED`.
   - Tham số khởi chạy tối giản và an toàn:
     `chrome.exe --user-data-dir="<appData>/flow_profiles/<profileId>" --no-first-run --no-default-browser-check https://labs.google/fx/tools/flow`
   - Quản lý vòng đời tiến trình Chrome qua `ManualLoginBrowserSession` và `ManualChromeProcess`, hỗ trợ phát hiện người dùng tự đóng cửa sổ Chrome và giải phóng `.session.lock`.

3. **Quy Trình Đăng Nhập Thủ Công & Xác Thực Tách Biệt (Two-Step Login & Verification Flow)**:
   - **Bước 1 — Mở Chrome**: Người dùng nhấn `Open Chrome for Login` -> AutoVideo mở Chrome chuẩn hệ thống, hiển thị banner bảo mật, trạng thái chuyển `Chrome Open (Manual Login)`, khóa profile `isLocked: true`.
   - **Bước 2 — Đóng Chrome**: Người dùng đăng nhập thủ công trên Chrome và đóng cửa sổ trình duyệt (hoặc bấm `Close Login Browser`) -> Chrome thoát hoàn toàn, khóa profile được giải phóng, trạng thái chuyển `UNVERIFIED`.
   - **Bước 3 — Xác Thực Phiên**: Người dùng bấm `Verify Login` -> AutoVideo khởi chạy một instance Playwright tạm thời ở chế độ chỉ quan sát (`channel: 'chrome'`), kiểm tra URL và trạng thái UI của Flow:
     - Nếu phát hiện redirect sang `accounts.google.com` hoặc trang đăng nhập -> trả về ngay `LOGIN_REQUIRED`.
     - Nếu phát hiện trang Flow đã sẵn sàng -> trả về `READY`.
     - Nếu UI thay đổi hoặc thiếu selector -> trả về `UNKNOWN` (fail-closed).
     - Nếu người dùng vẫn đang mở Chrome thủ công -> chặn ngay với lỗi `LOGIN_BROWSER_STILL_OPEN`.

4. **Gemini Model Optimization & Capability Policy Authority**:
   - Cập nhật model mặc định sang **`gemini-3.5-flash-lite`** (mô hình tối ưu chi phí thấp, phản hồi nhanh cho tác vụ prompt refinement).
   - Khởi tạo chính sách năng lực phiên bản `PromptOptimizationCapabilityPolicy` (`policy_version: "1.0"`, `max_output_tokens: 800`, `timeout_sec: 10`, `allow_paid_fallback: false`).
   - Loại bỏ cấu hình sampling không tương thích (`temperature: 0.7`) trong `generationConfig`.
   - Quản lý khóa API key an toàn qua OS Credential Manager, che dấu thông tin nhạy cảm (`[REDACTED_API_KEY]`) trong toàn bộ chẩn đoán và log.

5. **Real Mock Playwright Chromium E2E Verification**:
   - Test `test_phase20a_38_real_mock_playwright_chromium_e2e` khởi động `MockFlowServer` cục bộ, spawn Node sidecar, khởi chạy Chromium thật, điều hướng tới Mock Flow, tải tệp MP4 tổng hợp, điền prompt, click Generate thật đúng 1 lần (xác thực qua counter), poll tiến độ, tải video đầu ra và xác thực tệp tải về thành công.

6. **Crash-Safe Pre-Click State Machine & Ambiguous Recovery**:
   - Trước khi thực hiện bất kỳ submit nào: kiểm tra `child.submission_state`.
   - `NEVER_ATTEMPTED`: ghi `ATTEMPT_PERSISTED` xuống đĩa -> thực hiện submit đúng 1 lần.
   - `ATTEMPT_PERSISTED` / `AMBIGUOUS`: **ZERO automatic resubmit**, chuyển trạng thái `GENERATION_AMBIGUOUS` yêu cầu người dùng xác nhận hoặc đối soát UI.
   - `PROVEN_SUBMITTED`: **ZERO resubmit**, tiếp tục poll bằng `submission_evidence` đã lưu.
   - `PROVEN_COMPLETED`: tái sử dụng artifact đã qua validation, **ZERO resubmit**.

7. **Cross-Instance Atomic Profile File Lock & Concurrency Guard**:
   - Cơ chế khóa file `.session.lock` đa tiến trình/đa instance sử dụng `create_new(true)`.
   - Chặn tuyệt đối việc mở đồng thời manual Chrome và Playwright automation trên cùng một profile.

---

## 2. Bảng Trạng Thái Nghiệm Thu & Acceptance Flags

| Tiêu chí / Acceptance Flag | Giá trị công bố | Ghi chú |
| :--- | :---: | :--- |
| **GOOGLE_LOGIN_UNDER_PLAYWRIGHT** | **DISABLED** | Nghiêm cấm hoàn toàn tự động hóa đăng nhập Google qua Playwright/CDP |
| **MANUAL_LOGIN_BROWSER_IMPLEMENTED** | **YES** | Trình duyệt Google Chrome tiêu chuẩn được khởi chạy an toàn qua `SystemChromeLauncher` |
| **MANUAL_REAL_GOOGLE_LOGIN_VERIFIED** | **NO** | Chưa thực hiện đăng nhập tài khoản Google cá nhân thực tế |
| **PRODUCTION_PLAYWRIGHT_BROWSER** | **GOOGLE_CHROME_STABLE** | Playwright sidecar sử dụng Google Chrome Stable (`channel: 'chrome'`) |
| **MOCK_PLAYWRIGHT_CHROMIUM_VERIFIED** | **YES** | Đã xác thực qua Real Sidecar + Real Playwright + Real Chromium + Local Mock Server |
| **LOGIN_BROWSER_SESSION_PERSISTENCE_VERIFIED** | **YES** | Tiến trình manual Chrome sống bền bỉ theo vòng đời cửa sổ của người dùng |
| **AUTH_STATUS_SEMANTICS_PRESERVED** | **YES** | Ánh xạ kiểu ngữ nghĩa `FlowAuthStatus` (`READY`, `LOGIN_REQUIRED`, `UNKNOWN`, `FLOW_UI_CHANGED`, `FLOW_ELIGIBILITY_REQUIRED`) |
| **MANUAL_CHROME_PROFILE_LIVENESS_HARDENED** | **YES** | Theo dõi PID qua `--user-data-dir`, không mất lock khi có launcher process handoff |
| **TARGETED_PROFILE_PROCESS_CLEANUP_VERIFIED** | **YES** | Chỉ đóng tiến trình thuộc profile quản lý, không ảnh hưởng Chrome riêng của người dùng |
| **FLOW_AUTH_SESSION_REUSE_VERIFIED** | **NO** | Chưa kiểm thử tái sử dụng session live giữa Chrome thủ công và automation trên Google Flow thật |
| **PROFILE_AUTH_REFRESH_RELOAD_CONSISTENCY** | **YES** | Tách biệt trạng thái `manualBrowserOpen`, `isLocked`, và kết quả xác thực `READY`/`LOGIN_REQUIRED` |
| **APP_SHUTDOWN_SESSION_CLEANUP_WIRED** | **YES** | Hook `handle_app_shutdown` giải phóng toàn bộ tiến trình Chrome và file lock khi ứng dụng thoát |
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
dist/assets/window-Bvw5GUfd.js   13.92 kB │ gzip:   3.43 kB
dist/assets/index-DW5Lt5iE.js   500.40 kB │ gzip: 125.66 kB
✓ built in 9.23s
```
**Kết quả**: ✅ **0 lỗi TypeScript, 0 lỗi Vite bundle.**

### 3.3. Frontend Vitest Tests (`npm test -- --run`)
```text
 ✓ src/stores/__tests__/segmentedCloudJobStore.test.ts (8 tests) 8ms
 ✓ src/stores/__tests__/cloudJobStore.test.ts (12 tests) 9ms
 ✓ src/features/flow/__tests__/flowPromptUx.test.ts (12 tests) 13ms
 ✓ src/stores/__tests__/flowJobStore.test.ts (6 tests) 15ms

 Test Files  4 passed (4)
      Tests  38 passed (38)
   Duration  451ms
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
- `cargo test --lib -- tests_phase20a --test-threads=1`: **63/63 passed (100%)**
  - `test_phase20a_01` -> `test_phase20a_12`: Prompt optimization provenance, sanitization, single active request & undo tests với `gemini-3.5-flash-lite`.
  - `test_phase20a_13` -> `test_phase20a_19`: Flow manifest freeze, submitted prompt hash, credit calculation & log secrecy.
  - `test_phase20a_20` -> `test_phase20a_27`: Legal segmentation, fractional CFR frames, audio mux & corruption rejection.
  - `test_phase20a_28` -> `test_phase20a_44`: Security mock tests, real Playwright Chromium e2e, crash safety invariant & zero click resubmit.
  - `test_phase20a_45_manual_login_launcher_no_playwright_args_no_automation`: Xác thực tham số khởi chạy Chrome thủ công tuyệt đối không chứa cờ automation/CDP/debug.
  - `test_phase20a_46_verify_while_manual_chrome_running_blocked`: Khóa đồng thời — chặn xác thực Playwright khi Chrome thủ công đang mở (`LOGIN_BROWSER_STILL_OPEN`).
  - `test_phase20a_47_browser_already_open_guard`: Ngăn chặn mở trùng phiên trên cùng profile (`BROWSER_ALREADY_OPEN`).
  - `test_phase20a_48_explicit_browser_close_and_lock_release`: Đóng Chrome thủ công và giải phóng `.session.lock`.
  - `test_phase20a_49_session_manager_shutdown_cleanup`: `close_all()` giải phóng toàn bộ tiến trình khi tắt ứng dụng.
  - `test_phase20a_50_profile_locked_by_worker_not_browser_open`: Phân biệt rõ `isLocked` và `manual_browser_open`.
  - `test_phase20a_51_gemini_unverified_on_store`: Lưu khóa mới có trạng thái ban đầu `UNVERIFIED`.
  - `test_phase20a_52_gemini_mock_validation_success_valid`: Phản hồi 200 từ Mock Gemini cập nhật `VALID` cho `gemini-3.5-flash-lite`.
  - `test_phase20a_53_gemini_mock_validation_error_statuses`: Ánh xạ chính xác các mã lỗi 400/401/403/404/429/500/timeout/network.
  - `test_phase20a_54_failed_verification_preserves_stored_key`: Lỗi xác thực không làm mất khóa đã lưu.
  - `test_phase20a_55_get_gemini_status_retains_valid_in_session`: Giữ trạng thái `VALID` trong phiên ứng dụng.
  - `test_phase20a_56_app_restart_resets_to_unverified`: Khởi động lại ứng dụng đặt lại trạng thái `UNVERIFIED`.
  - `test_phase20a_57_zero_credential_leakage_in_diagnostics`: Che dấu API key trong toàn bộ chuỗi chẩn đoán.
  - `test_phase20a_58_profile_auth_refresh_reload_consistency`: Quản lý nhất quán trạng thái qua các bước mở Chrome -> đóng Chrome -> xác thực Playwright.
  - `test_phase20a_59_production_app_shutdown_lifecycle_callback_cleans_sessions`: Callback shutdown của app gọi `close_all()` giải phóng lock và tiến trình.
  - `test_phase20a_60_auth_status_typed_semantics_mapping`: Kiểm thử toàn diện enum `FlowAuthStatus` và parser không làm mất kiểu ngữ nghĩa.
  - `test_phase20a_61_mock_server_scenarios_auth_inspection`: Xác thực chính xác `READY`, `LOGIN_REQUIRED`, `FLOW_UI_CHANGED`, `FLOW_ELIGIBILITY_REQUIRED` qua MockFlowServer.
  - `test_phase20a_62_manual_chrome_profile_liveness_handoff_and_user_exit`: Kiểm thử vòng đời liveness handoff và tự giải phóng session khi người dùng đóng Chrome.
  - `test_phase20a_63_close_login_browser_only_targets_managed_profile_pids`: Xác thực close chỉ kết thúc PID thuộc profile chỉ định, tuyệt đối không diệt Chrome của người dùng.
- Workspace regression suite (`cargo test --workspace -- --test-threads=1`): **853/853 passed (100%)**

**Tổng cộng Rust tests**: ✅ **853/853 tests PASSED (100%), 0 failures, 0 regressions.**

---

## 4. Hạn Chế Còn Lại & Khuyến Nghị Tiếp Theo

1. Chưa thực hiện live authentication hoặc live video generation trên hạ tầng Google Flow thật.
2. Quá trình ghép nối video và chất lượng hình ảnh ở ranh giới segment (`FLOW_SEGMENT_BOUNDARY_VISUAL_QUALITY`) sẽ được kiểm chứng khi kết nối live Google Flow trong các phase tiếp theo.
3. Hoàn tất toàn bộ Phase 20A và **DỪNG LẠI**, tuyệt đối không bắt đầu Phase 20B khi chưa có chỉ thị.
