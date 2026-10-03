# DEVONE Local

DEVONE Local รุ่นสร้างใหม่: Tauri 2 + React + TypeScript + Vite และ Rust core ใช้ native binaries, SQLite และ Caddy รองรับ Windows x64 ก่อน ไม่มี Docker หรือ PowerShell ใน runtime-control architecture

## เริ่มใช้งาน

ติดตั้ง Rust stable พร้อม MSVC Build Tools และ WebView2 ตาม [Tauri prerequisites](https://v2.tauri.app/start/prerequisites/) และ Node.js 22.12+ / pnpm

```powershell
pnpm install --frozen-lockfile
pnpm desktop
```

หน้าจอ browser อย่างเดียว: `pnpm dev` (ไม่มี runtime data จำลอง)
สร้าง frontend: `pnpm build`
สร้าง desktop executable: `pnpm desktop:build` (ยังไม่สร้าง installer)
CLI core: `pnpm core snapshot`, `pnpm core start` (Ctrl+C หยุดบริการที่เป็นเจ้าของ)

DEVONE_HOME ค่าเริ่มต้น Windows: `%LOCALAPPDATA%/Devone`
กำหนด home แยกก่อน launch ได้:
```powershell
$env:DEVONE_HOME = 'D:\DevoneWorkspace'
pnpm desktop
```
ไม่แก้ global PATH ไม่เพิ่ม Windows service ไม่แก้ .env

## Workflow ทดสอบ Windows

1. เปิด DEVONE และตรวจ Settings > DEVONE_HOME
2. นำ PHP Windows x64 distribution ที่เชื่อถือได้และแตก ZIP แล้ว เข้า Runtimes > PHP > Import Runtime ระบุ exact version จาก php.exe --version และ paths php.exe / php-cgi.exe
3. นำ Caddy Windows x64 เข้า Runtimes > Caddy > Import Runtime ระบุ exact version และ caddy.exe
4. นำ MySQL Windows x64 เข้า Runtimes > MySQL > Import Runtime ระบุ exact version และ bin/mysqld.exe / bin/mysqladmin.exe การนำเข้าเป็นการ COPY ไม่ย้ายต้นฉบับ
5. รุ่นแรกที่นำเข้าแต่ละประเภทเป็น default ตรวจหรือเปลี่ยน Set default ได้
6. วางโฟลเดอร์ `demo` ใน `DEVONE_HOME/www` และสร้าง index.php: `<?php echo PHP_VERSION;` ไซต์ปรากฏเองเป็น demo.test
7. Laravel detection ต้องพบ artisan และ public/index.php; document root จะเป็น public
8. ตั้ง MySQL override หรือใช้ global default ใน Site detail แล้วกด Start All
9. Windows hosts setup ต้องใช้ elevated app session ครั้งที่ต้องเขียน managed block: ปิดแอปแล้วเปิด terminal แบบ Administrator รัน pnpm desktop โดยใช้ DEVONE_HOME เดิม กด Sync domains ไม่ต้องแก้ hosts รายไซต์ด้วยตนเอง
10. พอร์ต localhost 80/443 ต้องว่าง Caddy ใช้ TLS internal และติดตั้ง local CA trust ให้ Windows user ที่ launch แอป เมื่อเริ่มได้ เปิด https://demo.test
11. นำเข้า PHP/MySQL เวอร์ชันอื่น เลือก override ให้ไซต์หนึ่ง เปิดสองไซต์และตรวจ PHP_VERSION ต่างกัน ส่วน Databases แสดง instance/port แยก
12. เปิด Terminal ของแต่ละไซต์ แล้ว php -v ต้องตรงกับ site binding ไม่เปลี่ยน PATH นอก terminal นั้น
13. เพิ่ม/ย้ายโฟลเดอร์ใน www: watcher reconcile อัตโนมัติ การย้ายออกเก็บ site record เป็น missing ไม่ลบข้อมูล
14. Stop All / Start All / Restart ตรวจ Logs และ process PID หยุด process ที่ DEVONE เป็นเจ้าของเท่านั้น
15. ปิดแล้วเปิด DEVONE: override/default/port และ MySQL data ยังคงอยู่ ถ้าปิดขณะ active จะ restore dependencies เมื่อเปิดใหม่

MySQL ใหม่ initialize-insecure: root ไม่มี password สำหรับ local development และ bind เฉพาะ 127.0.0.1 ไม่มี credential UI ในรอบนี้ MySQL X protocol ถูกปิดเพื่อไม่ชนพอร์ตระหว่างเวอร์ชัน อย่าใช้ instance นี้เป็น production

## Runtime catalog

ไม่มีเวอร์ชัน hardcode ในแอป catalog อ่านจาก DEVONE_HOME/config/runtime-catalog.json เป็น array ของ manifest อัปเดตได้โดยไม่ release แอป แล้วกด Reload catalog ตัวอย่าง `examples/runtime-manifest.json`

Install ใช้ HTTPS ZIP + SHA-256 บังคับ, จำกัด archive/expanded size, ปฏิเสธ traversal และ symlink, validate binary --version ก่อน register ZIP ต้องมี binary paths ตรง manifest; nested top-level directory ต้องระบุใน paths ให้ถูก PHP/MySQL distribution อาจต้องมี Microsoft Visual C++ Redistributable ที่เข้ากับรุ่นนั้น

Import/validate รัน binary health check หลัง user เลือก runtime อย่างชัดเจน ไม่รัน project scripts

## ตรวจสอบ

```powershell
pnpm lint
pnpm typecheck
pnpm test
pnpm build
cargo fmt --manifest-path src-tauri/Cargo.toml --check
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
cargo test --manifest-path src-tauri/Cargo.toml
```

Native integration test ใช้ binaries จริงแต่ไม่แก้ hosts หรือ Windows trust store ต้องมี localhost 80/443 ว่าง รายละเอียด `docs/architecture.md`

## ขอบเขต

Node และฐานข้อมูลชนิดอื่นยังไม่รัน macOS/Linux มี adapter boundary และ home strategy แต่ desktop/runtime support ยัง unavailable การติดตั้งครั้งแรกยังต้อง import native runtimes หรือจัด catalog เอง รายละเอียดสิ่งที่ทำแล้ว/ยังไม่ครบและความเสี่ยงอยู่ใน `docs/architecture.md`

ผลตรวจสอบจริงและข้อจำกัดของ QA: [docs/verification.md](docs/verification.md)
