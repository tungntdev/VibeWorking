# AI Office 3D — Văn phòng AI

Ứng dụng desktop độc lập viết bằng **Rust (Tauri 2)**. Mở ra là một văn phòng 3D (mô hình Kenney): mỗi nhân viên AI ngồi một bàn, có chức danh, cấp độ (Cơ bản → Chuyên gia), kỹ năng và kết nối AI riêng. Bạn là **Quản lý** giao nhiệm vụ; **Điều phối viên** (AI hoặc người được giao vai trò Điều phối) phân rã nhiệm vụ và giao cho từng đầu mối; nhân viên đi tới bàn điều phối nhận việc, về chỗ làm, rồi điều phối viên tổng hợp báo cáo.

## Tính năng

| Nhóm | Chi tiết |
|---|---|
| Đăng nhập | Lần đầu tạo tài khoản Quản lý. Mật khẩu băm Argon2, khoá tạm 60 giây sau 5 lần sai, phiên hết hạn sau 12 giờ không dùng. |
| Phân quyền 3 cấp | **Quản lý**: toàn quyền. **Điều phối**: giao nhiệm vụ, phân công đầu việc cho nhân viên cụ thể, duyệt và chạy. **Quan sát**: chỉ xem. |
| Tuyển dụng bằng AI | Viết mô tả công việc → AI đề xuất danh sách ứng viên (tên, chức danh, cấp độ, kỹ năng, lời nhắc hệ thống, nhân vật 3D, lý do). Chọn người để tuyển. Không có AI thì dùng bộ chọn theo từ khoá. |
| Thư viện vị trí mẫu | 20 vị trí từ cơ bản (thư ký, CSKH, nhập liệu) tới chuyên gia (kiến trúc sư giải pháp, an toàn thông tin, chiến lược). |
| Nhân sự | Thêm, sửa hồ sơ, đổi kết nối AI/model, đổi nhân vật 3D, cho thôi việc. |
| Giao nhiệm vụ | 3 chế độ: **Tự động** (AI phân công và chạy luôn), **AI đề xuất, duyệt** (sửa phân công rồi mới chạy), **Phân công tay** (tự chọn đầu mối). Đầu việc cùng "bước" chạy song song, bước sau đọc được kết quả bước trước. Có huỷ, chạy lại phần lỗi. |
| Giao việc nhanh | Bấm vào nhân viên trong cảnh 3D → nhập yêu cầu → giao trực tiếp. |
| Kết nối AI | Claude Code CLI, OpenAI Codex CLI, Gemini CLI, Qwen CLI, Claude API, OpenAI API (và mọi dịch vụ tương thích: OpenRouter, DeepSeek, Groq, LM Studio), Gemini API, Ollama cục bộ, chế độ Mô phỏng. Thêm được **bất kỳ CLI nào** bằng mẫu tham số. |
| Văn phòng 3D | Xoay/phóng to, bấm chọn nhân viên; nhân viên đi nhận việc, gõ việc (màn hình sáng, vòng sáng dưới chân), mừng khi xong, lắc đầu khi lỗi, thỉnh thoảng đi lấy cà phê. Bảng tin trên tường hiển thị số liệu. Văn phòng tự mở thêm dãy bàn khi tuyển thêm người. |
| Nhật ký | Mọi thao tác (giao việc, phân công, hoàn thành, lỗi, nhân sự) được ghi lại. |

## Cài đặt

### macOS

File cài đặt: `dist/AI Office 3D_1.0.0_universal.dmg` (chạy trên cả Mac chip Apple và Mac Intel, macOS 11 trở lên).

1. Mở tệp `.dmg`, kéo **AI Office 3D** vào **Applications**.
2. Lần mở đầu tiên trên máy khác, macOS có thể chặn vì ứng dụng chưa được Apple công chứng (notarize). Vào **Cài đặt hệ thống → Quyền riêng tư & Bảo mật**, kéo xuống và bấm **Vẫn mở**. Nếu macOS báo ứng dụng "bị hỏng", chạy lệnh sau trong Terminal rồi mở lại:

```bash
xattr -dr com.apple.quarantine "/Applications/AI Office 3D.app"
```

Dữ liệu lưu tại `~/Library/Application Support/vn.vibeworking.aioffice/` (`office.db` và thư mục `workspaces/` của từng nhân viên). Sao lưu thư mục này là giữ được toàn bộ văn phòng.

### Windows

Build trên một máy Windows 10/11 (xem "Build trên Windows" bên dưới) → được file `AI Office 3D_1.0.0_x64-setup.exe`. Chạy file này để cài; trình cài đặt tự tải WebView2 nếu máy chưa có. Dữ liệu lưu tại `%APPDATA%\vn.vibeworking.aioffice\`.

Khác biệt trên Windows: chức năng "Quên mật khẩu → xác thực bằng máy tính" (Touch ID) chỉ có trên macOS; nhập mẫu từ `.doc/.rtf/.odt` cần lưu lại thành `.docx` trước; nút "Cài Node.js" dùng `winget`; đăng nhập CLI mở cửa sổ Command Prompt.

### Build lại từ mã nguồn

Cần Rust (`rustup`) và Node.js (chỉ để chạy Tauri CLI).

macOS — bản universal (chip Apple + Intel), ra `.app` và `.dmg`:

```bash
rustup target add x86_64-apple-darwin
```

```bash
npx @tauri-apps/cli@2 build --target universal-apple-darwin --bundles app,dmg
```

Kết quả: `src-tauri/target/universal-apple-darwin/release/bundle/dmg/`. Ứng dụng được ký ad-hoc (`signingIdentity: "-"` trong `tauri.conf.json`); muốn phát hành không bị Gatekeeper chặn cần tài khoản Apple Developer để ký Developer ID và công chứng.

### Build trên Windows

Trên máy Windows 10/11 (64-bit), cài một lần:

1. **Visual Studio Build Tools** (chọn "Desktop development with C++"): <https://visualstudio.microsoft.com/visual-cpp-build-tools/>
2. **Rust**: tải `rustup-init.exe` từ <https://rustup.rs> và cài mặc định (toolchain `stable-x86_64-pc-windows-msvc`).
3. **Node.js LTS**: <https://nodejs.org> (hoặc `winget install OpenJS.NodeJS.LTS`).
4. WebView2 đã có sẵn trên Windows 10 (bản cập nhật mới)/11.

Chép cả thư mục dự án sang máy Windows (không cần thư mục `src-tauri/target`), mở PowerShell trong thư mục dự án và chạy:

```powershell
npx @tauri-apps/cli@2 build --bundles nsis
```

File cài đặt nằm ở `src-tauri\target\release\bundle\nsis\AI Office 3D_1.0.0_x64-setup.exe`. Muốn thêm gói `.msi`, dùng `--bundles nsis,msi` (cần bật tính năng VBSCRIPT của Windows cho WiX).

Kiểm thử backend trên Windows: các bài test dùng `/bin/sh` chỉ chạy được trên macOS/Linux, nên trên Windows chỉ cần build.

Chạy chế độ phát triển (tự nạp lại giao diện):

```bash
npx @tauri-apps/cli@2 dev
```

Kiểm thử backend:

```bash
cd src-tauri && cargo test
```

Xem trước giao diện trong trình duyệt với dữ liệu giả lập (không cần Rust):

```bash
node tools/serve.mjs
```

## Phong cách văn phòng & nhân vật

- 5 phong cách: **Hiện đại**, **Gỗ ấm cúng**, **Công nghệ (tối)** (neon, màn hình phát sáng), **Startup sáng tạo**, **Hành chính trang nghiêm**. Chọn ngay ở màn hình khởi tạo (xem trước trực tiếp phía sau) hoặc đổi trong **Cài đặt**. Mỗi phong cách đổi màu sàn, tường, gỗ, vải, ánh sáng và đồ trang trí (`ui/js/themes.js`).
- 36 nhân vật 3D: 18 Kenney Mini Characters + 18 Kenney Blocky Characters, chiều cao được chuẩn hoá để ngồi vừa ghế.

## Di chuyển trong văn phòng

- Nhân viên chỉ rời bàn khi **nhận việc** (đi tới bàn điều phối) hoặc **trao đổi công việc** (khi đầu việc ở bước sau cần kết quả của đồng nghiệp ở bước trước, người đó sang bàn đồng nghiệp lấy kết quả). Không còn đi lại ngẫu nhiên.
- Đường đi tìm bằng A* trên lưới 15 cm dựng từ hộp bao của đồ đạc (bàn, ghế, tủ, sofa…), có làm mượt đường.
- Tránh va chạm giữa người với người: giữ khoảng cách tối thiểu, người đứng yên luôn được nhường, hai người cùng đi thì một người dừng chờ; chờ lâu thì tự tìm đường vòng. Trước bàn điều phối có các vị trí đứng riêng để không chồng lên nhau.

## Giao việc kèm tệp & thư mục (như project/cowork)

- Kéo thả tệp hoặc thư mục vào cửa sổ: nếu đang mở hộp thoại giao việc (hoặc thẻ nhân viên) thì tệp được thêm vào đó; nếu chưa mở gì thì tự mở hộp thoại **Giao nhiệm vụ** kèm tệp. Có nút **Chọn thư mục** / **Thêm tệp**.
- **Thư mục** → thành thư mục làm việc: AI dạng CLI chạy ngay trong thư mục đó (đọc tệp trực tiếp). Bật **Cho phép AI tạo/sửa tệp** thì ứng dụng thêm cờ tương ứng: Claude `--permission-mode acceptEdits`, Codex `--sandbox workspace-write`, Gemini `--approval-mode auto_edit` (sửa được ở mục Kết nối AI). Mặc định chỉ đọc.
- **Tệp** → chép vào `missions/nhiem-vu-<id>/tep-dinh-kem/`; nội dung văn bản (≤150 KB mỗi tệp) được trích vào yêu cầu để cả AI dạng API cũng đọc được; với Claude/Gemini CLI thư mục này còn được cấp quyền đọc (`--add-dir` / `--include-directories`).
- AI dạng API không truy cập ổ đĩa: nhận cây thư mục rút gọn và nội dung vài tệp chính (README, .md…).
- Trang chi tiết nhiệm vụ có nút **Mở** từng tệp/thư mục trong Finder và **Thư mục của nhiệm vụ**.

## Xuất tệp & làm việc với thư mục

- **Xuất theo yêu cầu**: trong trang nhiệm vụ, bấm **Xuất báo cáo** (báo cáo tổng hợp) hoặc **Xuất tệp** ở kết quả từng bước → chọn Word (.docx), PDF, Excel (.xlsx), CSV, HTML, Markdown, TXT. Mặc định lưu vào thư mục kết quả của nhiệm vụ rồi tự mở; tích "Chọn nơi lưu" để lưu chỗ khác.
  - Word: tiêu đề, đậm/nghiêng, danh sách, **bảng thật** rộng hết trang, phông Times New Roman 13.
  - PDF: dùng phông Arial của máy, đúng dấu tiếng Việt, có bảng kẻ khung.
  - Excel: mỗi bảng Markdown thành một trang tính; số liệu kiểu "1.250", "320,5", "12%" được đổi thành số để tính toán.
- **Nhân viên tự xuất tệp**: chỉ cần nhắn "xuất file Word/Excel/PDF…". Mọi AI (kể cả AI dạng API) được dặn đóng gói nội dung tệp giữa `<<<TỆP: ten.docx>>>` … `<<<HẾT TỆP>>>`; ứng dụng tạo tệp thật đúng định dạng, thay khối đó bằng dòng "📄 Đã tạo tệp …".
- **Tệp đã tạo/sửa**: trước và sau mỗi bước, ứng dụng chụp trạng thái thư mục làm việc và liệt kê tệp mới tạo / đã sửa / đã xuất (bỏ qua .git, node_modules…). Mục **Tệp kết quả** gom toàn bộ, có nút Mở / Hiện trong Finder / Mở thư mục kết quả.
- **Tạo thư mục mới** khi giao việc: tạo thư mục (mặc định `~/Documents/AI Office/<tên>`) làm thư mục làm việc và bật quyền tạo/sửa tệp — nhân viên lưu sản phẩm thẳng vào đó.
- Nơi lưu tệp do ứng dụng tạo: thư mục làm việc (nếu có và được phép sửa), nếu không thì `missions/nhiem-vu-<id>/ket-qua/` trong thư mục dữ liệu.

## Quên mật khẩu

Ở màn hình đăng nhập bấm **Quên mật khẩu?** → nhập tên đăng nhập → xác thực bằng **Touch ID hoặc mật khẩu đăng nhập máy tính** (hộp thoại hệ thống macOS, LocalAuthentication). Thành công thì có thể **đăng nhập luôn** hoặc **đặt mật khẩu mới**. Tài khoản đã bị Quản lý khoá thì không mở được bằng cách này. Mọi lần dùng đều được ghi vào Nhật ký.

## Trợ lý thiết lập lần đầu

Quản lý đăng nhập lần đầu sẽ gặp trợ lý 3 bước:

1. **Kết nối AI** — ứng dụng quét máy (Node.js, npm, Homebrew, từng CLI, API key, Ollama). CLI còn thiếu có nút **Cài đặt** (cài qua `npm install -g`; nếu thư mục npm cần quyền quản trị thì tự cài vào `~/.npm-global`; Claude Code dùng được trình cài chính thức khi máy chưa có Node.js; máy chưa có Node.js mà có Homebrew thì có nút **Cài Node.js**). Nút **Đăng nhập** mở Terminal chạy lệnh đăng nhập của CLI, **Thử gọi AI** gửi một câu hỏi rất ngắn để chắc chắn đã đăng nhập, API thì nhập khoá ngay tại chỗ.
2. **Mô tả công việc** — tên văn phòng, mô tả việc cần làm, số nhân viên.
3. **Chọn nhân viên** — AI đề xuất ứng viên, mỗi người đã được gán sẵn AI + model kèm lý do; đổi được trước khi tuyển.

Mở lại bất cứ lúc nào: **Kết nối AI → Trợ lý thiết lập**.

## Model & mức suy luận của từng nhân viên

Hồ sơ nhân viên có bộ chọn **AI · Model · Mức suy luận**. Danh sách model lấy trực tiếp từ kết nối khi được (Gemini CLI `--list-models`, Claude/OpenAI/Gemini API `/models`), không thì là danh sách gợi ý; vẫn gõ tay được tên model bất kỳ.

| Mức suy luận | Claude Code CLI | Claude API | Codex CLI / OpenAI API | Gemini API | Gemini CLI |
|---|---|---|---|---|---|
| Thấp → Tối đa | `--effort low…max` | `output_config.effort` | `model_reasoning_effort` / `reasoning_effort` (tối đa `high`) | `thinkingBudget` (2.5) hoặc `thinkingLevel` (3.x) | không có tham số → ghi yêu cầu vào lời nhắc |

Tra cứu web cho từng bước: Claude Code `--allowedTools WebSearch,WebFetch`, Claude API công cụ `web_search`, Gemini API `google_search`, Gemini CLI có sẵn công cụ tìm kiếm. Các tham số này sửa được ở mục Kết nối AI (`effort_args`, `web_args`).

## Quy trình nhiều AI (nhiều hãng trong một chuỗi việc)

Mục **Quy trình** cho phép dựng chuỗi bước, mỗi bước chọn người làm (hoặc tự chọn theo từ khoá vai trò) và **AI riêng của bước** (hãng, model, mức suy luận, tra cứu web) — khác với AI thường ngày của nhân viên. Có sẵn 3 mẫu:

- **Nghiên cứu → Tổng hợp → Kiểm chứng**: Gemini 2.5 Pro tra cứu (suy luận cao, web) → Claude Opus viết tổng hợp → GPT/Codex kiểm tra chéo (suy luận rất cao).
- **Lập trình → Review chéo → Kiểm thử**: Claude viết mã → Codex review → Gemini viết kiểm thử và tài liệu.
- **Dịch → Hiệu đính**: Gemini dịch → Claude hiệu đính.

Khi giao việc chọn **Theo quy trình** → ứng dụng tạo các đầu việc theo từng bước, chạy ngay hoặc chờ duyệt. Bước nào dùng AI chưa sẵn sàng trên máy sẽ tạm dùng AI của nhân viên và được ghi rõ trong nhận định. Mỗi đầu việc (kể cả khi phân công tay) cũng có thể đặt AI riêng.

## Theo dõi, hỏi đáp và nhắn tiếp

- **Bảng tên trên bàn**: họ tên + chức danh in trên bảng tên gắn mặt trước bàn (bấm vào để mở hồ sơ). Trên đầu nhân vật chỉ còn trạng thái và bóng thoại khi có việc.
- **Xem việc đang làm**: bấm vào nhân viên → thẻ hiện việc đang làm (nhiệm vụ, bước x/y, thời gian đã làm), chuỗi các bước của nhiệm vụ, chỉ dẫn được giao, **nhật ký tiến trình trực tiếp** (đọc tệp nào, tìm kiếm gì, đang viết phần nào…), việc đang xếp hàng và việc vừa xong. Claude Code / Gemini CLI chạy với `--output-format stream-json` để ứng dụng đọc từng sự kiện; nhật ký được lưu lại theo từng đầu việc.
- **Nhân viên hỏi lại**: khi thiếu thông tin bắt buộc, AI trả lời bằng dòng `[[HỎI]] …`. Nhân viên đứng lên, ra trước bàn, quay về phía bạn với bóng thoại câu hỏi; góc phải dưới hiện thẻ câu hỏi. Trả lời ở thẻ nhân viên hoặc trong trang nhiệm vụ → câu trả lời được ghi vào chỉ dẫn và nhiệm vụ tự chạy tiếp (trạng thái "Chờ bạn trả lời" trong lúc chờ).
- **Nhắn tiếp**: nhiệm vụ đã xong (hoặc lỗi/huỷ) có ô "Nhắn tiếp / yêu cầu chỉnh sửa" — chọn người làm (mặc định người làm bước cuối), ứng dụng thêm bước `↪ …` đọc toàn bộ kết quả trước đó và cập nhật báo cáo bằng kết quả mới nhất.

## Báo cáo khi hoàn thành

- Khi nhiệm vụ xong, **người trình báo cáo** (điều phối viên nếu nhiều người cùng làm, hoặc người làm bước cuối) đứng lên, ra khoảng trống phía trước phòng, quay về phía bạn với bóng thoại "📋 Báo cáo đã xong!" — bấm vào để mở báo cáo.
- Góc trên bên phải hiện **thẻ hoàn thành**: tóm tắt báo cáo, số tệp kết quả, nút *Xem báo cáo đầy đủ*, *Mở tệp* (nếu đã tự xuất theo mẫu), *Xuất tệp…*, *Nhắn tiếp*. Nhiệm vụ lỗi hiện thẻ đỏ.
- Đồng thời gửi **thông báo của macOS** (lần đầu macOS hỏi quyền thông báo).

## Trí nhớ của nhân viên

- Mỗi nhân viên có **trí nhớ riêng**, ngoài ra có **quy định chung của văn phòng** áp dụng cho mọi người. Trí nhớ được đưa vào mọi việc sau, kể cả nhiệm vụ khác.
- AI tự ghi nhớ khi bạn yêu cầu chỉnh sửa có tính lâu dài (VD "lần sau luôn để đơn vị triệu đồng") bằng dòng `[[GHI NHỚ]] …` / `[[GHI NHỚ CHUNG]] …`; ứng dụng báo "🧠 … đã ghi nhớ".
- Bấm vào nhân viên → mục **🧠 Trí nhớ**: xem, thêm tay, tạm tắt hoặc xoá từng điều.

## Mẫu tài liệu & báo cáo

- Mục **Mẫu tài liệu** ở thanh bên: tạo mẫu (Markdown: tiêu đề, bảng, danh sách), hoặc **nhập từ tệp Word/Markdown** (.docx, .md, .txt; .doc/.rtf/.odt qua `textutil` của macOS), hoặc kéo thả tệp Word vào khi đang soạn mẫu.
- Mỗi mẫu có mô tả, từ khoá và định dạng tự xuất (Word, PDF, Excel…). Khi giao việc chọn *Mẫu kết quả*: **tự chọn theo từ khoá**, không dùng mẫu, hoặc chọn một mẫu cụ thể (nút "Giao việc theo mẫu").
- Bước cuối và bản tổng hợp được viết **đúng cấu trúc mẫu**; xong việc ứng dụng tự xuất tệp theo định dạng của mẫu vào thư mục kết quả.

## Bảng vừa khung hình

- Bảng trong báo cáo cuộn ngang bên trong khung; nút ⤢ mở bảng toàn màn hình (Esc để đóng). Khi thẻ nhân viên đang mở, khung nhiệm vụ tự thu hẹp để không bị che.

## Tối ưu AI cho từng vị trí

Khi có nhiều AI sẵn sàng, bộ tối ưu (`src-tauri/src/optimizer.rs`) chọn AI và model cho từng nhân viên:

- Gắn nhãn nhóm việc từ chức danh/kỹ năng (lập trình, kiểm thử, hạ tầng, bảo mật, số liệu, soạn thảo, dịch thuật, nghiên cứu, truyền thông, thiết kế, pháp lý, y tế, hành chính, điều phối).
- Mỗi họ AI có hồ sơ thế mạnh: Claude mạnh lập trình/soạn thảo/lập luận/pháp lý; OpenAI (Codex) mạnh lập trình/hạ tầng/kiểm thử/số liệu; Gemini mạnh nghiên cứu/dịch thuật/truyền thông; AI cục bộ (Ollama) hợp việc hành chính cơ bản.
- Theo cấp độ: Chuyên gia dùng model mạnh nhất (Claude `opus`, Gemini `pro`), Trung cấp/Chuyên nghiệp dùng `sonnet`, Cơ bản dùng model nhẹ (`haiku`, `flash`) hoặc AI cục bộ để tiết kiệm.
- Khi các AI ngang điểm thì chia đều để không dồn tải vào một AI.

Áp dụng cho nhân viên đang có: **Kết nối AI → Tối ưu AI cho nhân viên** (xem bảng đề xuất, chọn dòng muốn đổi rồi bấm Áp dụng).

## Kết nối AI

Vào **Kết nối AI** trong ứng dụng:

- **CLI** (khuyên dùng nếu đã cài và đăng nhập): ứng dụng gọi lệnh trong thư mục làm việc riêng của từng nhân viên. Mẫu mặc định:
  - Claude Code: `claude -p {prompt} --output-format text {model_args}` (model: `--model {model}`)
  - Codex: `codex exec --skip-git-repo-check --output-last-message {output_file} {model_args} {prompt}`
  - Gemini: `gemini -p {prompt} {model_args}`
  - Biến thay thế: `{prompt}`, `{model}`, `{model_args}`, `{output_file}`, `{workdir}`. Có tuỳ chọn gửi yêu cầu qua stdin.
- **API**: nhập API key. Claude API mặc định dùng `claude-opus-5-5` và bật cơ chế dự phòng phía máy chủ (`fallbacks: "default"`) khi model từ chối yêu cầu. Model của OpenAI/Gemini là giá trị gợi ý, sửa theo tài khoản của bạn.
- **Mô phỏng**: trả lời mẫu, dùng để thử văn phòng khi chưa có AI.

Khi chạy lần đầu, ứng dụng tự dò CLI có trên máy theo thứ tự Claude → Codex → Gemini để làm kết nối cho Điều phối viên; không có thì dùng Mô phỏng. Ứng dụng mở từ Finder lấy PATH từ shell đăng nhập của bạn; nếu vừa cài CLI mới, bấm **Quét lại PATH**.

Lưu ý bảo mật: API key lưu trong `office.db` trên máy (không gửi đi đâu ngoài nhà cung cấp tương ứng) và không bao giờ trả về giao diện. Chỉ Quản lý được thêm/sửa kết nối, vì kết nối CLI chạy chương trình trên máy.

## Cấu trúc mã nguồn

```
src-tauri/src/
  db.rs         SQLite: lược đồ, truy vấn
  auth.rs       Argon2, phiên, khoá tạm, vai trò
  providers.rs  Gọi CLI / Anthropic / OpenAI / Gemini / mô phỏng, huỷ và giới hạn thời gian
  templates.rs  Thư viện vị trí mẫu, bộ chọn ứng viên ngoại tuyến
  memory.rs     Trí nhớ nhân viên, thư viện mẫu tài liệu, nhập mẫu từ Word
  export.rs     Xuất Word/PDF/Excel/CSV/HTML, tệp do AI tạo
  engine.rs     Tuyển dụng, lập kế hoạch, phân công, thực thi theo bước, tổng hợp
  commands.rs   Các lệnh Tauri (kiểm tra phiên + quyền ở mọi lệnh)
  state.rs      Trạng thái dùng chung, phát sự kiện thời gian thực
ui/
  index.html, css/app.css
  js/app.js       Giao diện quản lý
  js/office3d.js  Cảnh 3D (Three.js)
  js/md.js        Hiển thị Markdown an toàn (không tạo liên kết/ảnh)
  js/demo.js      Backend giả lập cho bản xem trước trên trình duyệt
  assets/         Mô hình Kenney (CC0)
```

## Ghi công

Mô hình 3D: [Kenney](https://kenney.nl) — Furniture Kit và Mini Characters, giấy phép CC0. Thư viện 3D: three.js (MIT).
