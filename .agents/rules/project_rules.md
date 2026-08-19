---
trigger: always_on
---

# Project Guidelines & Default Context

Bạn đang phát triển repository:
https://github.com/quanittb/autovideo-ai

Base revision ban đầu:
`main @ 2ca89fa405eef9e52a8e72046a91bc8e8231f099`

## Quy tắc bắt buộc:

1. **Đọc code thực tế trước khi sửa**: Không coi tài liệu hoặc phase report cũ là bằng chứng rằng chức năng đã hoạt động.
2. **Giữ nguyên zero-fake policy**:
   - Unit/integration test được phép mock HTTP.
   - Runtime và live acceptance tuyệt đối không tạo fake output, fake provider status hoặc fake cost.
   - Thiếu API key phải trả về trạng thái BLOCKED rõ ràng.
3. **Không hard-code** đường dẫn máy cá nhân, API token, model version hoặc giá trong UI.
4. **Bảo mật API token**: API token chỉ được sử dụng ở Rust backend hoặc backend proxy; không đưa token vào frontend bundle hoặc log.
5. **Khai báo Capability chính xác**: Mọi provider phải khai báo capability đúng với request thực tế nó hỗ trợ.
6. **Kiểm soát chi phí**: Mọi tác vụ trả phí phải có cost estimate trước khi submit và budget guard phía backend.
7. **Phạm vi thay đổi phẫu thuật**: Không sửa ngoài phạm vi phase nếu không thực sự cần thiết.
8. **Bảo đảm Test Coverage**: Bổ sung unit test, integration test và failure-path test cho code mới.
9. **Chạy kiểm tra chất lượng trước khi hoàn thành**:
   - `npm run build` / `pnpm build`
   - `cargo fmt --check`
   - `cargo check`
   - `cargo test`
   - Các Python test suite liên quan được phát hiện trong repository
10. **Báo cáo chuẩn chỉnh**: Tạo `docs/phase_<number>_report.md` (hoặc `docs/phase_<name>_report.md`) gồm:
   - Thay đổi đã thực hiện
   - Kiến trúc
   - File thay đổi
   - Test đã chạy và kết quả thật
   - Chi phí phát sinh từ live test
   - Hạn chế còn lại
11. **Tính trung thực tiêu chí**: Nếu acceptance criteria chưa đạt, không tuyên bố phase hoàn thành.
12. **Dừng đúng lúc**: Hoàn thành phase hiện tại rồi dừng, không tự động làm phase tiếp theo.
