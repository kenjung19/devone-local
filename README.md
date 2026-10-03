# DEVONE Local

DEVONE Local ใช้ Tauri 2 + React + TypeScript + Vite และ Rust core เดิม รัน PHP/MySQL/Caddy native binaries แยก exact version บน Windows x64 ไม่ใช้ Docker หรือ PowerShell เป็น runtime orchestrator

## ติดตั้งและใช้งานบน Windows

สำหรับ Windows 10/11 x64 ให้เปิด `DEVONE Local_0.1.0_x64-setup.exe` แล้วเปิด DEVONE Local จาก Start Menu ไม่ต้องติดตั้ง Git, Rust, Node, pnpm หรือใช้ terminal สำหรับการใช้งานปกติ Installer ใช้ NSIS แบบ current-user และติดตั้ง WebView2 ถ้ายังไม่มี (ต้องมี internet สำหรับ bootstrap และดาวน์โหลด runtimes)

Setup wizard เลือก Caddy/PHP/MySQL จาก bundled catalog หรือนำเข้า distribution ที่เชื่อถือได้ ตั้ง wildcard DNS ผ่าน UAC helper เฉพาะ operation และเตรียม/trust CA สำหรับผู้ใช้ปัจจุบัน เลือกติดตั้งเฉพาะเวอร์ชันที่ต้องการ ไม่ต้องสร้าง manifest เอง

ข้อมูลอยู่ `%LOCALAPPDATA%/Devone` แยกจาก application binaries: `www`, `database`, `runtimes`, `config`, SQLite, credentials, `certs`, `logs`, `cache`, `backups` Upgrade/reinstall และ uninstall ปกติเก็บ DEVONE Home ไว้ ไม่ลบโปรเจกต์หรือฐานข้อมูล

ปิดหน้าต่าง = ซ่อนไป tray; watcher/DNS/services ทำงานต่อ เลือก **Quit DEVONE Local** เพื่อหยุด owned services เมนู tray เปิดแอป, Start/Stop/Restart, Sites และ Open www ได้ Settings แยก Start with Windows จาก Start environment automatically การเปิดแอปซ้ำสำหรับ Home เดิมจะเปิดหน้าต่างเดิม

ถอน DNS/trust ผ่าน Settings ก่อน uninstall หากต้องการเลิกใช้งาน integration แอปไม่ถอน DNS policy ทุกครั้งที่ Quit และ installer ไม่ลบ trust/policy เงียบ ๆ

## สำหรับผู้พัฒนา

Toolchain คง exact pins: Node **24.21.0 LTS**, pnpm **12.8.1**, Rust **1.99.0** พร้อม MSVC Build Tools และ WebView2 ตาม [Tauri prerequisites](https://v2.tauri.app/start/prerequisites/) ใช้ shell ที่ตั้งค่า toolchain ถูกต้อง ไม่ต้องใช้ PowerShell script

```text
pnpm install --frozen-lockfile
pnpm desktop
pnpm test
pnpm build
pnpm release:windows
```

`package.json` เป็นแหล่ง application version; Tauri อ่าน version จากไฟล์นี้ `pnpm version:check` ตรวจ Cargo package version ให้ตรงกัน ไม่มี auto-increment `scripts/workspace-env.ps1` เป็น optional cache setup สำหรับ workspace เครื่องทดสอบนี้เท่านั้น ไม่เป็น product dependency

Release command ใช้ Tauri NSIS bundler ตาม [เอกสาร Tauri](https://v2.tauri.app/distribute/windows-installer/) ผลอยู่ `src-tauri/target/x86_64-pc-windows-msvc/release/bundle/nsis/` พร้อม `release-artifacts.json` (version, architecture, size, SHA-256) ยังไม่ signing ดู [รายงาน desktop release](docs/windows-product.md) สำหรับผล build และ checklist ที่ยังต้องตรวจจริง

## ใช้งานโปรเจกต์

1. วางโฟลเดอร์ `demo` ใน `DEVONE_HOME/www` มี `index.php` แล้วไซต์จะปรากฏเป็น `demo.test` Laravel ต้องพบ `artisan` และ `public/index.php`
2. เลือก global defaults หรือ site override จาก runtime ที่ติดตั้งแล้ว กด Start All เมื่อ environment active โฟลเดอร์ใหม่จะถูก discover และ route อัตโนมัติ
3. Wildcard DNS ตอบ `*.test → 127.0.0.1` เท่านั้น ใช้ UDP/TCP 53 และ Caddy ใช้ 80/443 บน loopback ตรวจพอร์ตชนกันผ่าน Setup/Logs
4. PHP Configure เป็น form ของ directives และ extensions ที่มีไฟล์จริง ตรวจ `php -v`, `--ini`, `-m` ก่อนบันทึกแล้ว restart เฉพาะ version pool Original php.ini ไม่ถูกแก้
5. Databases จัดการ initialize/start/stop/restart/validate ของแต่ละ exact MySQL version ข้อมูลอยู่ `database/mysql/<version>` และ ports คงอยู่หลังเปิดแอปใหม่
6. Site detail > Project database สร้าง database/user เฉพาะโปรเจกต์ ใช้ database-specific grants และ credential reference ที่เข้ารหัส Windows DPAPI ปุ่มแสดงรหัสผ่านเป็น action ชัดเจน ไม่แก้ `.env`
7. Terminal เปิดในโฟลเดอร์ไซต์ โดย PHP/MySQL PATH และ ini context ใช้เฉพาะ terminal นั้น
8. Site detail > Logs รวม core/Caddy/site/PHP/MySQL ที่เกี่ยวข้อง พร้อม recent errors Stop All และ Quit หยุดเฉพาะ process ที่ DEVONE เป็นเจ้าของ

MySQL administrator สำหรับ development instance ใหม่ยังเป็น root password ว่างบน loopback User ของโปรเจกต์ใช้รหัสผ่านสุ่มและไม่มี global grants การเปลี่ยน MySQL version ของ database ที่ provision แล้วต้องย้ายข้อมูลด้วยตนเอง แอปไม่ migrate ให้อัตโนมัติ

## DNS และ HTTPS

แอปเป็นเจ้าของ loopback DNS resolver เฉพาะช่วงที่เปิด DEVONE ไม่เพิ่ม Windows service ไม่เปลี่ยน DNS servers ของ network adapter Windows NRPT policy มี namespace เดียว `.test` จึงไม่ส่ง DNS ชื่ออื่นเข้า DEVONE Settings > Remove owned DNS policy ใช้ helper ถอนเฉพาะ policy ของ DEVONE

เมื่อเลือก Quit `.test` จะไม่ resolve ผ่าน policy นี้จนเปิดแอปใหม่ การปิดหน้าต่างไป tray ยังคง resolver ทำงาน การถอน policy ไม่ลบ hosts entries ที่โปรแกรมอื่นสร้างไว้ หากมี policy ที่ขัดแย้ง แอปปฏิเสธการเขียนทับ ตรวจ OS resolution แยกจาก registry และ UDP/TCP health

Hosts fallback เป็น action explicit เท่านั้น ใช้ managed block เดิมและต้องมีสิทธิ์เขียน hosts ระบบ แอปไม่เรียกมันใน discovery ปกติ ค่าเริ่มต้นคือ wildcard DNS

Caddy ใช้ local CA ใน `certs/caddy` Trust ใช้ current-user store และตรวจ actual certificate context ใน Windows ไม่อาศัย SQLite trusted flag อย่างเดียว

## Runtime catalog

Bundled metadata อยู่ `src-tauri/assets/runtime-catalog.json` Local override อยู่ `DEVONE_HOME/config/runtime-catalog.json` ใช้ schema:

```json
{"schema_version":1,"revision":1,"manifests":[]}
```

รองรับ legacy local manifest array ด้วย Settings รองรับ remote HTTPS URL พร้อม metadata SHA-256 ที่ pin ไว้ ไม่ใช้ endpoint ที่แต่งขึ้น Catalog update ไม่ต้อง release แอปใหม่ Revision rollback/schema/integrity failure ไม่ทำลาย last-good registry/cache ซึ่งเก็บใน SQLite transaction เดียว Platform ที่ไม่ตรงถูกกรองออก

Install รับ HTTPS ZIP + SHA-256 เท่านั้น มี byte progress, checksum verification, extraction และ binary validation ไม่ execute คำสั่งจาก catalog `metadata.archive_root` รองรับ MySQL ZIP ที่มี top-level folder Import เป็นการ copy ไม่ย้ายต้นฉบับ Runtime removal ป้องกัน references/defaults/database/running instances

## ตรวจสอบและขอบเขต

```powershell
pnpm lint
pnpm typecheck
pnpm test
pnpm build
cargo fmt --manifest-path src-tauri/Cargo.toml --check
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
cargo test --manifest-path src-tauri/Cargo.toml
```

ดู [architecture](docs/architecture.md), [ผล verification](docs/verification.md) และ [รายงานรอบพัฒนา](docs/phase1-continuation.md) Native fixture test และคำสั่งอยู่ใน architecture

`pnpm dev` แสดง frontend โดยไม่มีข้อมูล runtime จำลอง `pnpm desktop:build` สร้าง desktop executable และ NSIS installer `pnpm core start` ใช้ Ctrl+C หยุด owned services ไม่มี Node execution, database engine อื่น หรือ runtime support macOS/Linux ใน Phase 1
