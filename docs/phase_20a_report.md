# Báo Cáo Nghiệm Thu Hoàn Thành Phase 20A: Google Flow Browser Driver & Gemini Prompt Refinement

**Dự án**: AutoVideo AI  
**Phase**: Phase 20A — Google Flow Browser Driver & Gemini Prompt Refinement  
**Ngày hoàn thành**: 21/08/2026  
**Zero-Fake Policy & Cost Expenditure**: $0.00 tiêu thụ API có phí, 0 Flow credit thật, 0 Replicate prediction, 0 fake provider response.

---

## 1. Mục Tiêu & Phạm Vi Triển Khai (Phase 20A)

Triển khai hoàn chỉnh hệ thống **Google Flow Browser Driver & Gemini Prompt Refinement** theo tiêu chuẩn 12 điểm Delta Correction và Final Acceptance Micro-Fix:
1. **Flow Credit Policy Authority**:
   - `OMNI_EDIT_UPLOADED_VIDEO_CREDITS_PER_GENERATION = 40` (Gemini Omni Flash Edit Uploaded Video).
   - Mode authority phân định rõ ràng `FlowGenerationMode::OmniVideoGenerate` (20 credits) vs `FlowGenerationMode::OmniEditUploadedVideo` (40 credits).
   - Công thức dự toán chính xác: `estimatedCredits = segmentCount * outputsPerGeneration * creditsPerGeneration` (ví dụ: 5 segments = 200 credits).
2. **10-Second Video Edit Limit & Largest Legal Split**:
   - Phân đoạn video chính xác theo frame CFR (tối đa $\le 10.0s$ mỗi segment), bảo toàn luồng âm thanh gốc AAC trong các input segment tải lên Flow.
   - Mux âm thanh gốc duy nhất 1 lần khi ghép nối đầu ra cuối cùng, loại bỏ toàn bộ child audio sinh bởi Flow để bảo đảm tính toàn vẹn âm thanh gốc.
3. **OS-Protected SecretStore Credential Security**:
   - Lưu trữ Gemini API key an toàn qua OS Credential Manager (Windows Credential Manager / macOS Keychain / Linux Secret Service) qua crate `keyring`.
   - Cơ chế fallback `GEMINI_API_KEY` chỉ dành cho môi trường DEV. Tuyệt đối không ghi file plaintext lên đĩa hoặc rò rỉ token vào log, DTO, Tauri events hay manifests.
4. **Crash-Safe Pre-Click State Machine & Ambiguous Recovery**:
   - Chuyển trạng thái `AttemptPersisted` xuống đĩa trước khi bấm nút Generate trên browser.
   - Chuyển `GenerationAmbiguous` nếu xảy ra sự cố trong cửa sổ gửi lệnh; tuân thủ nghiêm ngặt quy tắc **ZERO automatic resubmit**.
5. **Node/TypeScript Playwright Sidecar**:
   - Sidecar độc lập tại `src-tauri/sidecars/flow-playwright` biên dịch độc lập qua `npm run build` (tsc) với 0 lỗi.
   - Xử lý 10 kịch bản mock: `READY`, `LOGGED_OUT`, `CREDITS_REQUIRED`, `GENERATION_PENDING`, `GENERATION_COMPLETE`, `GENERATION_ERROR`, `UI_CHANGED`, `CORRUPT_DOWNLOAD`, `WRONG_DOWNLOAD`, `DELAY_AFTER_GENERATE_CLICK`.
6. **Frontend Studio & Prompt UX Suite**:
   - Giao diện React `FlowGenPanel` tích hợp `FlowPromptEditor`, `FlowProfileSelector`, `FlowJobProgress`.
   - 12 hành vi UX cốt lõi (debounce, double-click guard, stale response protection, undo stack, provenance transition) được kiểm thử toàn diện với 100% test pass.

---

## 2. Bảng Trạng Thái Nghiệm Thu & Acceptance Flags

| Tiêu chí / Acceptance Flag | Giá trị công bố | Ghi chú |
| :--- | :---: | :--- |
| **OMNI_EDIT_UPLOADED_VIDEO_CREDITS_PER_GENERATION** | **40** | Chuẩn contract Flow Video Edit |
| **MOCK_PLAYWRIGHT_CHROMIUM_VERIFIED** | **YES** | Đã xác thực qua mock server & sidecar |
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
dist/index.html                   0.49 kB │ gzip:   0.32 kB
dist/assets/index-B59G3199.css   93.23 kB │ gzip:  12.92 kB
dist/assets/window-Ajdedgbu.js   13.92 kB │ gzip:   3.43 kB
dist/assets/index-C6It6p1Z.js   489.71 kB │ gzip: 123.25 kB
✓ built in 5.86s
```
**Kết quả**: ✅ **0 lỗi TypeScript, 0 lỗi Vite bundle.**

### 3.3. Frontend Vitest Tests (`npm test -- --run`)
```text
 ✓ src/stores/__tests__/cloudJobStore.test.ts (12 tests) 8ms
 ✓ src/stores/__tests__/segmentedCloudJobStore.test.ts (8 tests) 6ms
 ✓ src/stores/__tests__/flowJobStore.test.ts (4 tests) 8ms
 ✓ src/features/flow/__tests__/flowPromptUx.test.ts (12 tests) 11ms

 Test Files  4 passed (4)
      Tests  36 passed (36)
   Duration  363ms
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
Finished `dev` profile in 36.97s
Exit code: 0
```
**Kết quả**: ✅ **0 compile errors, 0 warnings.**

### 3.6. Rust Test Regression Suites (Serial Execution)
- `cargo test --lib -- tests_phase20a`: **44/44 passed (100%)**
- `cargo test --lib -- tests_phase19`: **29/29 passed (100%)**
- `cargo test --lib -- tests_phase18`: **13/13 passed (100%)**
- `cargo test --lib -- tests_phase17`: **56/56 passed (100%)**
- `cargo test --lib -- tests_phase16`: **39/39 passed (100%)**
- `cargo test --lib -- tests_phase15`: **38/38 passed (100%)**
- `cargo test --lib -- test_phase14`: **10/10 passed (100%)**
- `cargo test --lib -- test_cloud`: **6/6 passed (100%)**

**Tổng cộng Rust tests**: ✅ **235/235 tests PASSED (100%), 0 failures, 0 regressions.**

---

## 4. Danh Sách Tệp Đã Thay Đổi / Bổ Sung Trong Phase 20A

### Backend (`src-tauri`):
- `src-tauri/Cargo.toml`: Thêm dependency `keyring = "3"`.
- `src-tauri/src/ai/flow/mod.rs`: Re-export các kiểu dữ liệu và policy authority.
- `src-tauri/src/ai/flow/capability.rs`: `FlowCapabilityPolicy` (40 credits / gen), `FlowGenerationMode`.
- `src-tauri/src/ai/flow/prompt_optimizer.rs`: Gemini client và `SecretStore` kết nối OS credential store.
- `src-tauri/src/ai/flow/manifest.rs`: Schema 21-state machine và DTOs.
- `src-tauri/src/ai/flow/profile.rs`: Chrome profile manager và session concurrency lock.
- `src-tauri/src/ai/flow/segment.rs`: Largest legal 10s video segmenter và AAC source audio preservation.
- `src-tauri/src/ai/flow/playwright_bridge.rs`: RPC bridge và URL/path validation.
- `src-tauri/src/ai/flow/mock_flow_server.rs`: Mock HTTP server hỗ trợ 10 scenarios.
- `src-tauri/src/ai/flow/output_validator.rs`: Video probe và duration drift validator.
- `src-tauri/src/ai/flow/stitcher.rs`: Segment concat và single audio muxer.
- `src-tauri/src/ai/flow/store.rs`: Atomic CAS manifest storage.
- `src-tauri/src/ai/flow/orchestrator.rs`: Crash-safe orchestrator worker.
- `src-tauri/src/commands/mod.rs` & `src-tauri/src/lib.rs`: 9 Flow Tauri IPC commands.
- `src-tauri/src/ai/tests_phase20a/*`: 4 sub-modules test suite (44 tests).

### Sidecar (`flow-playwright`):
- `src-tauri/sidecars/flow-playwright/package.json`
- `src-tauri/sidecars/flow-playwright/tsconfig.json`
- `src-tauri/sidecars/flow-playwright/src/flow_adapter.ts`
- `src-tauri/sidecars/flow-playwright/src/bridge.ts`
- `src-tauri/sidecars/flow-playwright/src/index.ts`

### Frontend (`src`):
- `src/types/index.ts`: Bổ sung tab `'flow'`.
- `src/lib/ipc.ts`: Flow API IPC contracts.
- `src/features/flow/usePromptOptimization.ts`: Prompt hook và undo stack.
- `src/stores/flowJobStore.ts`: Zustand store.
- `src/features/flow/FlowPromptEditor.tsx`: Editor UI.
- `src/features/flow/FlowProfileSelector.tsx`: Profile selector UI.
- `src/features/flow/FlowJobProgress.tsx`: 21-state progress visualizer.
- `src/features/flow/FlowGenPanel.tsx`: Flow Gen Studio Panel (cập nhật 40 credits).
- `src/components/layout/Sidebar.tsx` & `src/app/App.tsx`: Navigation & routing.
- `src/stores/__tests__/flowJobStore.test.ts`: Store unit tests.
- `src/features/flow/__tests__/flowPromptUx.test.ts`: 12 UX behavioral tests.

---

## 5. Hạn Chế Còn Lại & Khuyến Nghị Tiếp Theo

1. Chưa thực hiện live authentication hoặc live video generation trên hạ tầng Google Flow thật.
2. Quá trình ghép nối video và chất lượng hình ảnh ở ranh giới segment (`FLOW_SEGMENT_BOUNDARY_VISUAL_QUALITY`) sẽ cần được kiểm chứng khi thực hiện các phase live tiếp theo.
3. Hoàn tất toàn bộ Phase 20A và **DỪNG LẠI**, không bắt đầu Phase 20B khi chưa có chỉ thị.
