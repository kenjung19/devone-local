# ผลตรวจสอบ — 3 ตุลาคม 2026

สภาพแวดล้อม Windows x64, Node 22.22.3, Rust 1.93.0, pnpm 10.33.0

| คำสั่ง | ผล |
|---|---|
| pnpm lint | ผ่าน ไม่มี warning |
| pnpm typecheck | ผ่าน strict TypeScript |
| pnpm test | ผ่าน 2 tests |
| pnpm build | ผ่าน frontend production build |
| cargo fmt --manifest-path src-tauri/Cargo.toml --check | ผ่าน |
| cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings | ผ่าน |
| cargo test --manifest-path src-tauri/Cargo.toml | ผ่าน 10 core unit + 2 native process tests; native workflow ignored ใน default suite |
| native workflow (explicit --ignored) | ผ่าน 1 test ด้วย fixture PHP 8.3.28/8.5.1, MySQL 5.7.39/8.4.3, Caddy 2.11.7 |
| pnpm core install caddy 2.11.7 | ผ่าน local catalog, HTTPS download, SHA-256 และ binary validation |
| core snapshot / validate | discovery พบ demo.test, legacy.test และ Laravel fixture laravel-demo.test; Caddy installed/default และรายงาน version จริง |

Desktop binary และ typed Tauri commands compile/linked สำเร็จจาก cargo test รวมทุก target ไม่ได้ทดสอบ release installer หรือ UI ผ่านการคลิกจริง เพราะ session ไม่มี browser/native UI surface ให้ตรวจ

Native workflow ใช้ temporary DEVONE_HOME บน D: ไม่แก้ hosts และไม่ติดตั้ง root CA ใน Windows ใช้ CA เฉพาะ test HTTP client ตรวจ:
- PHP ต่างเวอร์ชันเสิร์ฟ HTTPS concurrently
- MySQL สอง engine instances concurrently
- Laravel document root public
- persisted site overrides และ global-default inheritance
- Caddy reload PID เดิมหลัง discovery
- SQL value 42 อยู่หลัง stop/reopen/restart
- stop all หยุด owned services
- ป้องกัน controller ซ้ำและ runtime removal ที่ยัง referenced

ข้อจำกัดของการทดสอบ: ยังไม่ยืนยัน Windows hosts/trust installation ผ่าน system integration จริง หรือ per-site terminal ด้วย GUI คู่มือ manual test อยู่ README.md

ระหว่างทดสอบพบ resource limits และแก้ขั้นตอน:
- linker memory: จำกัด Cargo jobs=2 และ debug symbols=0 ใน dev/test
- temporary data บน C: ไม่พอ: ใช้ DEVONE_TEST_ROOT บน D: แทน โดยไม่ลบข้อมูลอื่น
- sandbox ไม่อนุญาต bind localhost 80/443: native workflow ใช้ approved unsandboxed test
- TypeScript 7 ไม่มี compiler API ให้ ESLint: ใช้ official TypeScript 6 API alias ควบคู่ native TypeScript 7 compiler ตาม [Microsoft](https://devblogs.microsoft.com/typescript/announcing-typescript-7-0/#running-side-by-side-with-typescript-6.0)

Release package/installer, wildcard resolver, UAC helper, MySQL credentials UI และ polished extension UI ยังไม่ได้ทำ รายละเอียด architecture/schema/tree/risks อยู่ architecture.md
