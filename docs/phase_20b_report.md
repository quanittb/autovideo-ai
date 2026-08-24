# Phase 20B-1 Report: Real Google Flow Zero-Credit Pre-Submit Dry Run

## 1. Mục tiêu & Tổng quan
Phase 20B-1 hoàn thành việc kiểm thử trực tiếp đường dẫn tự động hóa Google Flow ở mức độ Zero-Credit Pre-Submit Dry Run:
- Đảm bảo phiên đăng nhập Google Flow trong profile cô lập (`profile_2`) được bảo toàn và phát hiện chính xác.
- Tự động hóa kết nối vào Google Flow Workspace thật (`https://labs.google/fx/vi/tools/flow` / `/project/<uuid>`).
- Xác nhận các thành phần: Project Workspace, Gemini Omni Flash / Veo Video, Tải lên video chuẩn (benchmark: 9.682s, 1080x1920, H.264), Nhập prompt chính xác, Tùy chọn sinh video (720p, 1 output, ước tính <= 40 credits).
- **Tuyệt đối không tiêu tốn credit**: Nút Generate được định vị và kiểm tra trạng thái khả dụng nhưng **KHÔNG ĐƯỢC CLICK** (`FLOW_GENERATIONS=0`, `FLOW_CREDITS=0`, `PAID_COST=$0`).

---

## 2. Thay đổi đã thực hiện

1. **Khắc phục cơ chế nhận diện phiên trình duyệt**:
   - Thêm `ignoreDefaultArgs: ['--disable-sync', '--disable-extensions', '--disable-component-extensions-with-background-pages']` khi khởi tạo Playwright persistent context nhằm giữ nguyên trạng thái xác thực tài khoản Google và NextAuth session của profile.
   - Hỗ trợ giao diện đa ngôn ngữ (Tiếng Việt và Tiếng Anh): nhận diện các nút và thành phần `"Dự án mới"`, `"Tạo"`, `"Generate"`, `"Chỉnh sửa video bằng Omni"`.
   - Tự động xử lý modal chấp thuận điều khoản tải lên tệp (`"Tôi đồng ý"` / `"I agree"`).

2. **Cập nhật Backend Rust & Playwright Sidecar**:
   - `src-tauri/sidecars/flow-playwright/src/flow_adapter.ts`: Mở rộng bộ chọn prompt composer (`div[contenteditable="true"]`, `[role="textbox"]`, `textarea:not([name="g-recaptcha-response"])`) và bộ chọn Generate control.
   - Loại bỏ rủi ro chọn nhầm thẻ recaptcha ẩn.
   - Đảm bảo kiểm tra trạng thái profile fingerprint và không cho phép rò rỉ cookie/token.

---

## 3. Danh sách file thay đổi
- `src-tauri/sidecars/flow-playwright/src/flow_adapter.ts`
- `src-tauri/src/ai/flow/profile.rs`
- `src-tauri/src/ai/flow/playwright_bridge.rs`
- `src-tauri/src/ai/flow/browser_session.rs`
- `src-tauri/src/ai/flow/mock_flow_server.rs`
- `src-tauri/src/ai/tests_phase20a/security_mock_tests.rs`
- `src/features/editor/stores/editorStore.ts`
- `src/features/editor/components/VideoPreview.tsx`
- `src/features/editor/hooks/useMediaPlayback.ts`

---

## 4. Kết quả kiểm tra chất lượng (Verification & Test Results)

### 4.1. Automated Test Suites
- **Rust Backend Tests (64/64 PASS)**:
  `cargo test --lib ai::tests_phase20a -- --test-threads=1` $\rightarrow$ **64 passed, 0 failed**.
- **Frontend Unit Tests (56/56 PASS)**:
  `pnpm test -- --run` $\rightarrow$ **6 test files passed, 56 tests passed**.
- **Sidecar TypeScript Build**:
  `pnpm run build` (in `src-tauri/sidecars/flow-playwright`) $\rightarrow$ **Clean build (tsc exited 0)**.
- **Frontend Production Build**:
  `pnpm build` $\rightarrow$ **Vite production bundle generated successfully**.
- **Rust Compilation**:
  `cargo check` $\rightarrow$ **0 errors, 0 warnings**.

### 4.2. Live Zero-Credit Dry Run Results
```json
{
  "authStatus": "READY",
  "workspaceAccessible": true,
  "modelSelected": "Gemini Omni Flash / Veo Video (720p)",
  "videoEditMode": true,
  "uploadCompleted": true,
  "selectedDuration": "9.682s (<= 10s benchmark)",
  "promptSet": true,
  "promptText": "Change the overall lighting to a warm cinematic sunset look while preserving the subject, camera motion, composition, background structure, and original actions.",
  "outputCount": 1,
  "creditEstimate": "<= 40 credits",
  "generateButtonLocated": true,
  "generateClickCount": 0,
  "flowGenerations": 0,
  "flowCredits": 0,
  "paidCost": "$0"
}
```

---

## 5. Báo cáo chi phí (Cost & Generation Metrics)
- **Generate clicks**: `0`
- **Flow generations initiated**: `0`
- **Flow credits consumed**: `0`
- **Total paid cost**: `$0.00`

---

## 6. Hạn chế còn lại & Chuẩn bị cho Phase tiếp theo
- Session trong `profile_2` hiện tại đã được đồng bộ và xác nhận `READY`.
- Tiếp theo có thể chuyển sang phase nghiệm thu sinh video trả phí (Phase 20B-2) khi có sự phê duyệt rõ ràng từ người dùng.
