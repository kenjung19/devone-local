# สถาปัตยกรรม DEVONE Local

คง Tauri 2 + React + TypeScript + Rust core และ SQLite เดิม Windows x64 เป็น operational target; macOS/Linux มี boundary และรายงาน unsupported สำหรับ OS integration ที่ยังไม่มี implementation

## โครงสร้าง

| ส่วน | หน้าที่ |
|---|---|
| src/contracts.ts / bridge.ts | Typed IPC และ Tauri commands |
| src/App.tsx / components | Sites, Installed/Available Runtimes, Databases, Logs, Settings, Setup, PHP form, provisioning |
| src-tauri/src/app | Orchestration, snapshot, dependency reconciliation, controller lock, shared filesystem watcher |
| catalog / assets/runtime-catalog.json | Bundled/local/remote sources, schema/revision/integrity, last-good cache |
| runtime | Registry, import/install/remove guards, exact version validation, install progress, candidate PHP config |
| database / provision | MySQL lifecycle, persistent data, protocol health, project grants and credential references |
| dns / server | Owned loopback UDP/TCP .test authoritative resolver |
| platform / wildcard | Windows NRPT fixed-scope elevated helper and system resolution health |
| platform/windows.rs | Job Objects, native system executable resolution, current-user CA verification, DPAPI, listener PID ownership |
| process | Owned Child supervisor, health transitions, bounded recovery, graceful connected-socket shutdown |
| ports / storage / config | Port reservations, SQLite migrations, single DEVONE_HOME provider |
| projects / sites / webserver / tls / tools | Discovery/adapters, bindings, Caddy reload/HTTPS, CA, scoped terminal |

## State และ migrations

SQLite ใช้ WAL/foreign keys และ transaction migrations 001→002; version ใหม่กว่าที่รองรับถูกปฏิเสธ ไม่เปลี่ยน migration 001 ของ state เดิม

| Table | หน้าที่ |
|---|---|
| sites | unique path/hostname, present/issues, framework/document root |
| site_runtime_overrides | logical exact version ต่อ site/kind |
| runtime_catalog / runtime_installations | artifact metadata และ installed registry |
| database_instances | persistent data path และ initialize state ต่อ MySQL version |
| project_databases | site→exact instance, database/user, encrypted credential reference, pending/ready |
| port_allocations | unique owner/port |
| process_state | PID/status/timestamp; stale PID ไม่ถูก adopt |
| settings | defaults, autostart, setup completion, catalog source/revision/cache, CA fingerprint |
| certificates / tools | certificate metadata และ future tool foundation |

Catalog cache/registry/revision commit ใน transaction เดียว ไม่ refresh remote จาก network ทุก startup การ refresh เป็น action explicit แต่ state ใช้ last-good cache ได้

## DNS และ privilege

Normal app owns 127.0.0.1:53 UDP/TCP listeners ไม่มี Windows service Resolver ตอบเฉพาะชื่อใต้ .test เป็น A 127.0.0.1 และ AAAA no-data; ชื่ออื่น REFUSED ไม่มี upstream hijack

Windows helper รับเพียง `--devone-setup-dns` หรือ `--devone-remove-dns` ก่อนเปิด Home/แอป ใช้ native registry API แก้เพียง:

`HKLM\SYSTEM\CurrentControlSet\Services\Dnscache\Parameters\DnsPolicyConfig\{9073AE66-0413-46F2-9F35-D0E001000001}`

Values: Name REG_MULTI_SZ [.test], GenericDNSServers 127.0.0.1, ConfigOptions 8, Version 2, DisplayName/Comment ownership marker ตรวจ local/GPO namespace conflicts ไม่เปลี่ยน adapter DNS servers ไม่ใช้ arbitrary path/command จากผู้ใช้ Remove เฉพาะ owned policy Parent ที่ไม่ elevated flush resolver cache หลัง helper จบ Helper ไม่เขียน files ใน Home

Readiness ตรวจ owned live resolver UDP/TCP, policy contents และ Windows DnsQuery(no hosts/cache) ใน bounded background refresh ไม่ใช้ cached SQLite success แทน OS state Hosts fallback เป็น action explicit เท่านั้น Discovery ไม่เขียน privileged state

Policy คงอยู่เมื่อปิดแอป ดังนั้น .test จะ resolve ไม่ได้จนเปิดแอปใหม่ ถอนผ่าน Settings ได้ ไม่มีผลต่อชื่อ namespace อื่น

## Runtime/config/database isolation

Exact manifest platform/version/binary role ไม่เก็บ executable commands Install ใช้ HTTPS SHA-256, archive limits, path/symlink rejection, optional archive_root แล้ว validate binary ก่อน register ไม่อัปเกรด runtime อัตโนมัติ Node/Other metadata ขยายได้ แต่ execution ปฏิเสธใน Phase 1

PHP มีหนึ่ง FastCGI pool ต่อ exact version Managed ini copy ค่าต้นฉบับที่ไม่ override โดยไม่แก้ไฟล์ต้นฉบับ Extension directives สร้างจาก selected physical files Candidate validation ใช้ PHPRC + PHP_INI_SCAN_DIR ว่าง ตรวจ -v/--ini/-m ก่อน atomic replacement เปิด CLI terminal ใช้ context เดียวกัน ผลทดสอบ curl enable/disable และ actual memory_limit ช่วยตรวจการโหลด config

MySQL หนึ่ง shared instance ต่อ exact version ข้อมูลอยู่ database/mysql/<version> ไม่ upgrade/reinitialize nonempty data Persistent allocated port, no-defaults/cnf, 127.0.0.1, mysqlx off สำหรับรุ่น 8+ Project database pin exact instance แม้ global default เปลี่ยนไป สร้าง user สุ่ม password64hexและ grants เฉพาะ database ใช้ escaped underscore ไม่ให้ pattern grant ครอบ schema อื่น Secrets เข้ารหัส DPAPI current-user; SQLite เก็บ reference/status เท่านั้น Root bootstrap password ว่างใช้เฉพาะ administration ของ development instance ไม่มี automatic .env changes

## Supervisor และ startup

Controller เดียวต่อ Home Owned Child + Windows Job Object เท่านั้นที่ถูก terminate ไม่ adopt stale PID Listener health ตรวจ TCP owner PID/membership ใน owned Job Object จึงไม่ยอมรับ unrelated process ที่บังพอร์ต

Unexpected exit log และ restart สูงสุดสามครั้ง backoff 1/2/4 วินาทีต่อ cycle Explicit stop/restart ล้าง pending retry Health transitions persisted Graceful Caddy /stop และ MySQL SHUTDOWN เปิด connection แล้วตรวจ listener ownership ก่อนส่งคำสั่ง Socket เดิมไม่สลับไป server อื่น Fallback kill/wait เฉพาะ owned Child

Startup scan + persisted overrides/defaults/provisioned bindings เริ่มเฉพาะ dependencies ของ present valid sites เมื่อ autostart active Runtime binary validation/MySQL protocol health/Caddy config validation precede readiness New folder watcher debounce แล้ว reconcile routes/dependenciesโดยไม่ restart แอป Caddy reload คง PID เมื่อ config ผ่าน

## Native Windows verification

ใช้ fixture distributions ที่เชื่อถือได้ พอร์ต 80/443 ว่าง Native test ไม่เขียน machine DNS policy หรือ Windows trust store ใช้ Caddy root CA เฉพาะ test HTTP client:

```powershell
$env:DEVONE_TEST_ROOT = 'D:\runtime-test-workspace'
$env:DEVONE_FIXTURE_PHP_A = 'D:\runtime-fixtures\php-8.3.28'
$env:DEVONE_FIXTURE_PHP_B = 'D:\runtime-fixtures\php-8.5.1'
$env:DEVONE_FIXTURE_MYSQL_A = 'D:\runtime-fixtures\mysql-5.7.39'
$env:DEVONE_FIXTURE_MYSQL_B = 'D:\runtime-fixtures\mysql-8.4.3'
$env:DEVONE_FIXTURE_CADDY = 'D:\runtime-fixtures\caddy'
$env:DEVONE_FIXTURE_CADDY_VERSION = '2.11.7'
cargo test --manifest-path src-tauri/Cargo.toml --test windows_workflow -- --ignored --nocapture
```

Fixture versions เป็น verification baseline ไม่ใช่ catalog defaults รายงาน test results และข้อจำกัดอยู่ [verification](verification.md)

## ข้อจำกัดที่เหลือ

UAC/NRPT/CA trust install-remove และ GUI flow ต้อง manual test บน Windows จริง Session automated tests ไม่อ้างว่าผ่านจุดเหล่านี้ ไม่มี installer/signing/tray/native folder picker/Composer manager/credential rotation/database migration UI หรือ macOS/Linux execution Port reservation ก่อน native bind มี race สั้นที่ health ownership guard ตรวจจับ Root bootstrap ยัง password ว่างบน loopback Catalog remote endpoint ต้องกำหนด URL+metadata checksum โดย explicit action Runtime binary ที่ import/downloadต้องเชื่อถือได้
