# รายงาน Phase 1 continuation — 3 ตุลาคม 2026

1. **ฐานเดิมและขอบเขต** ต่อจาก main ของ `kenjung19/devone-local` โดยคง Tauri/React/Rust core, runtime registry, SQLite, platform adapters และ native process model รอบนี้ไม่มี commit/push อัตโนมัติ ไม่มี Phase 2 execution

2. **Exact toolchain** Node 24.21.0 LTS (official release 2026-09-07), pnpm 12.8.1, Rust stable 1.99.0 (official channel 2026-10-01) มี .node-version, engines, packageManager, rust-toolchain.toml และ lockfiles ตรวจ Node ZIP SHA-256 จาก official SHASUMS ข้อมูลที่ resolve ก่อนแก้ dependency อยู่ toolchain-versions.json

3. **Frontend packages** React/React DOM 19.3.0, Tauri API/CLI 2.12.1, Vite 8.3.2, plugin-react 6.1.1, TypeScript compiler 7.0.2, TypeScript 6 API alias 6.0.2, typescript-eslint 8.71.0, ESLint 10.12.0, eslint/js 10.0.1, Vitest 5.0.3, Prettier 3.9.9, globals 17.13.0, types/node 26.6.4, types/react และ react-dom 19.3.0 ไม่มี latest/ranges ใน direct packages ยืนยัน tsc --version เป็น 7.0.2 API alias 6 ใช้สำหรับ tooling compatibility ตาม [Microsoft](https://devblogs.microsoft.com/typescript/announcing-typescript-7-0/#running-side-by-side-with-typescript-6.0) ข้อยกเว้น release age จำกัด exact eslint@10.12.0 และ ignore@7.0.12 ตามเวอร์ชัน stable ที่ resolve ในรอบนี้ ไม่ปิด policy ทั้งระบบ

4. **Rust packages** Direct crates pin exact: tauri 2.12.1, tauri-build 2.7.1, serde 1.0.229, serde_json 1.0.151, rusqlite 0.40.2, thiserror 2.0.21, notify 8.2.0, ctrlc 3.5.2, tracing 0.1.44, tracing-subscriber 0.3.23, uuid 1.27.0, sha2 0.11.0, reqwest 0.13.5, zip 8.6.0, windows-sys 0.61.2, tempfile 3.27.0 เพิ่ม winreg 0.56.0 สำหรับ scoped policy, mysql 28.0.3 สำหรับ protocol/provisioning, zeroize 1.9.0 สำหรับ transient credentials ใช้ Cargo.lock และ upgrade เฉพาะ explicit maintenance

5. **DNS** Resolver เป็น Rust owned UDP/TCP listener บน 127.0.0.1:53 ตอบ A สำหรับ .test และ AAAA เป็น no-data ปฏิเสธชื่ออื่น ไม่มี forwarding DNS ทั่วระบบ NRPT helper รับ fixed setup/remove argument เท่านั้น เขียน fixed owned registry key ผ่าน native API ตรวจ policy conflicts และ ownership ไม่รับ user path/commands หลักการ scoped DNS อ้างอิง [Microsoft NRPT](https://learn.microsoft.com/en-us/openspecs/windows_protocols/ms-gpnrpt/0fb6a915-3dcc-439b-bace-674100c97a25) app ไม่ต้อง elevated ปกติ Hosts เป็น explicit fallback Lifecycle หยุด resolver เมื่อแอปปิด Health ตรวจ UDP/TCP และ Windows DnsQuery แยกกัน

6. **Setup** Wizard ตรวจ actual Home/default binaries, resolver/policy/system lookup, CA และ current-user certificate store Retry แต่ละ action, import/install, MySQL initialize, prepare CA, trust, finish/explicit skip และ reopen Settings ไม่ต้องอ่าน README ก่อนใช้งาน สถานะไม่ถูกทำให้ ready จากปุ่ม finish/skip

7. **Catalog** Bundled/local/remote source enum; schema version/revision; platform filter; duplicate/path/source/checksum validation; metadata SHA pin สำหรับ remote HTTPS; last-good JSON cache และ registry atomically commit ใน SQLite ไม่มี executable scripts ใน metadata ไม่มี production remote feed URL ที่ผู้ใช้ยังไม่ได้กำหนด ชุด bundled PHP ใช้ official releases metadata Caddy 2.11.7 ใช้ verified official asset MySQL 8.4.11 LTS ดาวน์โหลด Oracle CDN ตรวจ vendor MD5 แล้ว pin SHA-256 a492371d687d2bab088b0062581144a0044b8964baefdf4faa579292b423d25c

8. **Runtime UI** แยก installed/available; available ต้องตรง platform; install มี actual bytes/total และ phase downloading/verifying/extracting/validating/complete/failed; backend errors แสดงใน dialog; version selection ใช้ installed registry; removal guards เดิมยังคงอยู่ ไม่มี automatic runtime upgrades

9. **PHP** Form เจ็ด directives, prefill saved config, physical extension toggles, zend_extension สำหรับ opcache/xdebug, reject absent files/unsafe values; candidate scan directory ตรวจ -v/--ini/-m และ startup warnings ก่อน atomic write managed ini/JSON Original php.ini คงเดิม Restart เฉพาะ version pool การเปิด terminal ใช้ managed context

10. **MySQL** Initialize/start/stop/restart/validate/log/status ต่อ exact version Persistent data/port และ loopback/X protocol isolation ไม่ reinitialize data ที่มีอยู่ Partial initialization แสดง recovery error/log ชัดเจน Windows MySQL monitor/server อาจมี PID ต่างกัน จึงตรวจว่า listener อยู่ใน owned Job Object ตาม [MySQL server options](https://dev.mysql.com/doc/refman/8.4/en/server-options.html) Windows MySQL 8.0.12+ รับ --no-monitor ผ่าน CLI เพื่อให้ DEVONE จัดการ recovery และยังตรวจ owned Job membership ของ listener Startup ตรวจ binary version และ protocol SELECT 1 การเปลี่ยน global default ไม่ย้าย provisioned project database binding

11. **Project database** Migration 002 เพิ่ม project_databases reference/status/ownership สร้าง database และ local user สุ่ม Grant เฉพาะ database นั้น (escape underscore grant patterns) ปฏิเสธ existing unmanaged names Retry pending provisioning และ encrypted credential reference เดิมได้ Windows DPAPI current-user storage, transient password zeroization, no secret CLI argv/logs ไม่แก้ .env ไม่มี credential rotation/migration UI ในรอบนี้

12. **Reliability/startup** Snapshot health transitions, owned Child + Job Objects, unexpected-exit log, สาม restart attempts/backoff 1/2/4 วินาทีต่อ owned service cycle, stop ยกเลิก pending retries และไม่ adopt stale PID Caddy reload validate ก่อน commit, watcher lifecycle รวมเป็น module และ new-folder event เริ่ม dependencies/route โดยไม่ restart แอป ไม่เริ่ม installed MySQL ที่ไม่ได้ใช้ใน autostart

13. **Logs/site/terminal/platform** Site detail มี resolved source/default/override, DNS, HTTPS, Open Site/Folder/Terminal/Logs; site logs filter core/Caddy/PHP/MySQL/access และ recent errors Windows binary roles/system executable resolution อยู่ platform adapter Terminal PATH เป็น child-local Environment Home/certificate/privilege/credential adapters พร้อมจุดขยาย macOS/Linux แต่ยังรายงาน unsupported สำหรับ execution/integration

14. **Tests และผลจริง** ดู verification.md สำหรับผล command suite ล่าสุด Unit/native tests ครอบคลุม resolver wildcard/refusal/UDP/TCP/conflicts/drop, setup skip/reopen, catalog cache/schema/revision/platform, PHP validation, DPAPI, supervisor capture/timeout/recovery/stale PID Native workflow ขยาย actual HTTPS สอง PHP, SQL สอง MySQL, config preservation/restart isolation, project credential/grants persistence, scoped terminal PATH, watcher discovery และ Caddy PID UI server-render tests ตรวจ setup incomplete/error, PHP prefill/extensions และ installed-only site selector

15. **ข้อจำกัด/ความเสี่ยงและ manual checks** ต้องตรวจ UAC install/remove NRPT, OS resolver readiness, actual CA trust removal/reinstall และ native GUI clicks บนเครื่อง Windows ที่ยอมรับ dialog จริง Automated native suite ไม่แก้ machine policy/trust ไม่มี release installer/signing/tray/Composer manager/macOS/Linux support Root bootstrap password ยังว่างบน loopback การส่งต่อ port reservation ก่อน child bind มี race สั้น Resolver DNS .test ใช้ได้เฉพาะช่วงเปิดแอป Catalog feed deployment ไม่อยู่ใน scope ไม่มีการอ้างว่า UI/UAC manual checks ผ่านหากไม่ได้ตรวจจริง

## คำสั่ง verify native fixture

ดู environment fixture paths ใน architecture.md แล้วรัน:

```powershell
cargo test --manifest-path src-tauri/Cargo.toml --test windows_workflow -- --ignored --nocapture
```

หาก toolchain/cache อยู่ workspace เพื่อจำกัดพื้นที่ C ใช้ `scripts/workspace-env.ps1` ก่อน commands ไฟล์นี้ตั้งเฉพาะ environment ของ terminal ปัจจุบันและไม่อัปเกรด dependencies

## คำสั่ง pnpm ที่ใช้

```powershell
pnpm install --frozen-lockfile
pnpm lint
pnpm typecheck
pnpm test
pnpm build
pnpm desktop
pnpm desktop:build
```

ใช้ Node/pnpm เวอร์ชันที่ pin; การตรวจสอบรอบนี้ dot-source `./scripts/workspace-env.ps1` ก่อนเพื่อใช้ toolchain/cache ใน workspace คำสั่ง desktop และ desktop:build เป็นคำสั่งสำหรับใช้งาน ไม่ใช่การอ้างว่า release installer/manual UI QA ผ่านแล้ว

## ผล verification สุดท้าย

Frontend lint/typecheck/build ผ่าน; Vitest 7 tests ผ่าน Rust fmt/clippy ผ่าน; cargo test 21 unit + 5 process tests ผ่าน Native Windows workflow แบบ explicit --ignored ผ่าน 1 test (96.38s) รวม autostart เฉพาะ dependencies และ persistence รายละเอียด/manual checks อยู่ใน verification.md การ commit/push ทำหลังผู้ใช้สั่งโดยชัดเจน
