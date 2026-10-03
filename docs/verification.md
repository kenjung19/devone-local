# ผลตรวจสอบ — 3 ตุลาคม 2026

Windows x64; Node 24.21.0 LTS, pnpm 12.8.1, Rust 1.99.0 ตรวจเวอร์ชันจริงและใช้ toolchain/cache บน D: ผ่าน `scripts/workspace-env.ps1` รายการ dependency exact pins อยู่ใน [รายงานรอบพัฒนา](phase1-continuation.md) และ [ข้อมูลที่ resolve](toolchain-versions.json)

| คำสั่ง | ผล |
|---|---|
| pnpm lint | ผ่าน ไม่มี warning |
| pnpm typecheck | ผ่าน compiler TypeScript 7.0.2 |
| pnpm test | ผ่าน 7 tests ใน 3 files |
| pnpm build | ผ่าน Vite production build |
| cargo fmt --manifest-path src-tauri/Cargo.toml --check | ผ่าน |
| cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings | ผ่าน |
| cargo test --manifest-path src-tauri/Cargo.toml | ผ่าน 21 unit + 5 process tests; native workflow ignored ใน default suite |
| native workflow explicit --ignored | ผ่าน 1 test กับโค้ดสุดท้าย (96.38s) |

Desktop binary และ typed Tauri commands compile/link ใน all-targets suite แต่ไม่ได้ทดสอบคลิก native GUI หรือ release installer ใน session นี้

## Coverage

- Resolver wildcard/refusal/UDP/TCP, listener conflicts และ shutdown
- Setup skip/reopen, actual readiness state; catalog schema/revision/platform/cache failure preservation
- Archive SHA-256 corruption rejection, unsafe paths/config values, managed PHP validation
- Windows DPAPI roundtrip และ PID ownership ของ TCP listener
- MySQL initialize guard ไม่ adopt/erase existing data
- SQLite migration persistence/idempotence, site/default overrides, runtime removal guards และ port reuse/conflicts
- Process stdout/stderr, bounded timeout/restart, stale PID safety และ graceful stop ที่ไม่ส่งคำสั่งให้ foreign listener
- React server-render tests: setup errors/incomplete state, PHP prefill/physical extension checkboxes และ installed-only site selector
- Toolchain tests: frontend exact pins/compiler/API aliases, Rust direct pins และ bundled artifact metadata

Native fixture suite ใช้ PHP 8.3.28/8.5.1, MySQL 5.7.39/8.4.3, Caddy 2.11.7 ตรวจ actual HTTPS หลาย PHP พร้อมกัน, Laravel public root, physical curl enable/disable, memory_limit และ original ini preservation, isolated version restart, SQL สอง instances, project credentials/restricted grants, default change ไม่ย้าย database, scoped terminal PATH, watcher discovery/Caddy PID, autostart เฉพาะ dependencies และ SQL value 42/credentials ที่คงอยู่หลังเปิดใหม่

Native suite ใช้ temporary DEVONE_HOME บน D: ไม่แก้ Windows NRPT/hosts หรือ system CA store; HTTP client resolve ชื่อเองและ trust CA เฉพาะ test ดู fixture variables ใน [architecture](architecture.md)

## คำสั่งรันซ้ำ

```powershell
# ใช้บรรทัดนี้เฉพาะเมื่อมี workspace toolchain/cache ที่สร้างไว้
. ./scripts/workspace-env.ps1
pnpm install --frozen-lockfile
pnpm lint
pnpm typecheck
pnpm test
pnpm build
cargo fmt --manifest-path src-tauri/Cargo.toml --check
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
cargo test --manifest-path src-tauri/Cargo.toml
# ตั้ง fixture variables ตาม architecture ก่อน
cargo test --manifest-path src-tauri/Cargo.toml --test windows_workflow -- --ignored --nocapture
```

## ส่วนที่ยังต้องตรวจจริง

1. เปิด desktop โดยไม่ elevated และเดิน Setup; ยืนยัน UAC เกิดเฉพาะ fixed DNS helper พร้อม retry/cancel/install/remove
2. ตรวจ Windows OS DNS ด้วยชื่อ .test ใหม่ และตรวจ unrelated namespaces หลัง setup/remove; ทดสอบ UDP/TCP 53 conflict
3. ตรวจ CA trust ใน current-user store, remove/reinstall และ browser HTTPS จริง
4. คลิก install/import/progress/error, PHP extension form, MySQL actions, credentials reveal, site logs/Open Site/Folder/Terminal และ folder watcher ใน GUI
5. Release packaging/signing/installer ยังไม่ได้ verify; macOS/Linux execution และ Phase 2 ยังไม่รองรับ

## ประเด็นที่พบและแก้ระหว่าง verification

TypeScript 7 compiler ไม่มี API สำหรับ ESLint จึงใช้ official TypeScript 6 API alias คู่กับ compiler 7 ไม่ downgrade compiler Fixture PHP 8.5.1 ต้องใช้ PHPRC สำหรับ single-action CLI validation จึงยืนยัน --ini/-m โหลด managed ini จริง Windows MySQL monitor/server อาจมี PID ต่างกัน จึงตรวจ listener membership ใน owned Job Object แทนการเชื่อเฉพาะ parent PID ตาม [MySQL documentation](https://dev.mysql.com/doc/refman/8.4/en/server-options.html)

ระหว่าง rerun พบ Windows Access denied ระหว่าง import fixture บางรอบ เพิ่ม error context ของ copy/validation/move แล้วและมีรอบที่ผ่านครบ ไม่ยืนยันสาเหตุว่าเป็น antivirus โดยไม่มีหลักฐาน ผู้ใช้สามารถ retry หลังตรวจ path/สิทธิ์และไฟล์ที่ถูกล็อกได้

ใช้ Cargo jobs=2/debug=0 ตาม resource limits เดิม และ temp/cache บน D: ไม่ลบข้อมูลผู้ใช้หรือหยุด MySQL ของ Laragon
