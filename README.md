# DEVONE Local

DEVONE Local ใช้ Tauri 2 + React + TypeScript + Vite และ Rust core เดิม รัน PHP/MySQL/Caddy native binaries แยก exact version บน Windows x64 ไม่ใช้ Docker หรือ PowerShell เป็น runtime orchestrator

## เริ่มใช้งาน

Toolchain ที่ล็อกไว้: Node **24.21.0 LTS**, pnpm **12.8.1**, Rust **1.99.0** พร้อม MSVC Build Tools และ WebView2 ตาม [Tauri prerequisites](https://v2.tauri.app/start/prerequisites/) ใช้ `.node-version`, `packageManager` และ `rust-toolchain.toml` ไม่อัปเกรดเวอร์ชันอัตโนมัติ

```powershell
pnpm install --frozen-lockfile
pnpm desktop
```

เปิดแอปด้วยบัญชีผู้ใช้ปกติ First-run wizard ตรวจ Home, wildcard DNS, runtimes และ CA จริง เลือกติดตั้งเวอร์ชันจาก bundled catalog หรือนำเข้า native distribution ที่เชื่อถือได้ รุ่นแรกเป็น default ตั้งค่า Windows DNS ผ่าน helper ที่แสดง UAC ครั้งเดียว จากนั้นสร้าง Caddy CA และติดตั้ง trust สำหรับผู้ใช้ปัจจุบัน Retry แต่ละขั้นตอนได้ หรือข้ามขั้นที่ยังไม่พร้อม แล้วเปิด wizard อีกครั้งจาก Settings

MySQL initialization เป็นขั้นที่เลือกทำได้ใน wizard/Databases เมื่อมีไซต์ที่ใช้ instance นั้น แอปจะ initialize เฉพาะ data directory ใหม่ ห้าม reinitialize/upgrade data เดิมอัตโนมัติ

DEVONE_HOME ค่าเริ่มต้น: `%LOCALAPPDATA%/Devone` เปลี่ยนก่อนเปิดแอปได้:

```powershell
$env:DEVONE_HOME = 'D:\DevoneWorkspace'
pnpm desktop
```

## ใช้งานโปรเจกต์

1. วางโฟลเดอร์ `demo` ใน `DEVONE_HOME/www` มี `index.php` แล้วไซต์จะปรากฏเป็น `demo.test` Laravel ต้องพบ `artisan` และ `public/index.php`
2. เลือก global defaults หรือ site override จาก runtime ที่ติดตั้งแล้ว กด Start All เมื่อ environment active โฟลเดอร์ใหม่จะถูก discover และ route อัตโนมัติ
3. Wildcard DNS ตอบ `*.test → 127.0.0.1` เท่านั้น ใช้ UDP/TCP 53 และ Caddy ใช้ 80/443 บน loopback ตรวจพอร์ตชนกันผ่าน Setup/Logs
4. PHP Configure เป็น form ของ directives และ extensions ที่มีไฟล์จริง ตรวจ `php -v`, `--ini`, `-m` ก่อนบันทึกแล้ว restart เฉพาะ version pool Original php.ini ไม่ถูกแก้
5. Databases จัดการ initialize/start/stop/restart/validate ของแต่ละ exact MySQL version ข้อมูลอยู่ `database/mysql/<version>` และ ports คงอยู่หลังเปิดแอปใหม่
6. Site detail > Project database สร้าง database/user เฉพาะโปรเจกต์ ใช้ database-specific grants และ credential reference ที่เข้ารหัส Windows DPAPI ปุ่มแสดงรหัสผ่านเป็น action ชัดเจน ไม่แก้ `.env`
7. Terminal เปิดในโฟลเดอร์ไซต์ โดย PHP/MySQL PATH และ ini context ใช้เฉพาะ terminal นั้น
8. Site detail > Logs รวม core/Caddy/site/PHP/MySQL ที่เกี่ยวข้อง พร้อม recent errors Stop All และปิดแอปหยุดเฉพาะ process ที่ DEVONE เป็นเจ้าของ

MySQL administrator สำหรับ development instance ใหม่ยังเป็น root password ว่างบน loopback User ของโปรเจกต์ใช้รหัสผ่านสุ่มและไม่มี global grants การเปลี่ยน MySQL version ของ database ที่ provision แล้วต้องย้ายข้อมูลด้วยตนเอง แอปไม่ migrate ให้อัตโนมัติ

## DNS และ HTTPS

แอปเป็นเจ้าของ loopback DNS resolver เฉพาะช่วงที่เปิด DEVONE ไม่เพิ่ม Windows service ไม่เปลี่ยน DNS servers ของ network adapter Windows NRPT policy มี namespace เดียว `.test` จึงไม่ส่ง DNS ชื่ออื่นเข้า DEVONE Settings > Remove owned DNS policy ใช้ helper ถอนเฉพาะ policy ของ DEVONE

เมื่อปิดแอป `.test` จะไม่ resolve ผ่าน policy นี้จนเปิดแอปใหม่ การถอน policy ไม่ลบ hosts entries ที่โปรแกรมอื่นสร้างไว้ หากมี policy ที่ขัดแย้ง แอปปฏิเสธการเขียนทับ ตรวจ OS resolution แยกจาก registry และ UDP/TCP health

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

`pnpm dev` แสดง frontend โดยไม่มีข้อมูล runtime จำลอง `pnpm desktop:build` สร้าง desktop executable (ยังไม่สร้าง installer) `pnpm core start` ใช้ Ctrl+C หยุด owned services ไม่มี Node execution, database engine อื่น หรือ runtime support macOS/Linux ใน Phase 1
