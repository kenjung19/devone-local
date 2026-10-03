# สถาปัตยกรรมและสถานะ DEVONE Local

## โครงสร้าง

```text
devone-local/
├── .cargo/config.toml           # จำกัด compile concurrency
├── package.json / pnpm-lock.yaml
├── src/
│   ├── contracts.ts             # typed IPC request/response
│   ├── bridge.ts                # Tauri invoke
│   ├── presentation.ts / presentation.test.ts
│   ├── App.tsx                  # Sites/Runtimes/Databases/Logs/Settings
│   ├── components/              # Badge, SiteDetail, RuntimeDialogs, Logs
│   ├── main.tsx
│   └── style.css
├── src-tauri/
│   ├── Cargo.toml / Cargo.lock / build.rs
│   ├── tauri.conf.json / capabilities/
│   ├── migrations/001_foundation.sql
│   ├── tests/                   # windows_workflow, process_lifecycle, native Rust fixture
│   ├── icons/                   # source SVG + desktop icons
│   └── src/
│       ├── app/                 # application orchestration + snapshot
│       ├── core/                # errors, runtime references, service state
│       ├── config/              # DEVONE_HOME provider
│       ├── projects/            # adapters, discovery, safe hostnames
│       ├── sites/               # site model
│       ├── catalog/             # replaceable manifests
│       ├── runtime/             # import/install/validate/default/remove/PHP config
│       ├── process/             # owned child lifecycle, bounded commands/logging
│       ├── database/            # DatabaseEngine + MySQL
│       ├── ports/               # persisted allocations, reservations, health
│       ├── storage/             # SQLite migrations
│       ├── dns/                 # DnsProvider
│       ├── tls/                 # TlsProvider, Caddy CA
│       ├── webserver/           # WebServer + Caddy generation/validate/reload
│       ├── tools/               # per-site terminal environment
│       ├── platform/
│       │   ├── windows.rs       # Job Objects, hosts, trust, launch
│       │   ├── macos.rs         # future adapter, unsupported operations
│       │   └── linux.rs         # future adapter, unsupported operations
│       ├── ipc.rs
│       ├── main.rs
│       └── bin/core.rs
├── examples/runtime-manifest.json
└── docs/
```

## SQLite schema / migrations

Migration 001 รัน transaction และใช้ PRAGMA user_version ปฏิเสธฐานข้อมูลจากรุ่นใหม่กว่า เปิด foreign_keys และ WAL

| Table | หน้าที่ |
|---|---|
| sites | project path/hostname unique, adapter/document root, present/issue, timestamps |
| site_runtime_overrides | (site_id, kind) unique และ logical version reference |
| runtime_catalog | manifest JSON จาก catalog ภายนอก |
| runtime_installations | (kind, version) unique, relative install path, manifest |
| database_instances | runtime reference, relative data path, initialized flag |
| port_allocations | owner unique และ port unique |
| process_state | service key, pid/status/timestamp; ไม่ adopt PID ที่ค้าง |
| settings | defaults, autostart |
| certificates | CA path และสถานะ trust ที่บันทึกไว้ |
| tools | extension foundation ยังไม่มี tool manager UI |

project database content อยู่ใน DEVONE_HOME/database/mysql/<exact-version> ไม่เก็บใน SQLite

## Platform / isolation

Shared models ใช้ logical runtime references ไม่เก็บ absolute Windows executable paths Site project/document root เป็น filesystem paths ตาม platform Home เป็น path provider เดียว Runtime categories ไม่จำกัด PHP/MySQL มี Node/Other สำหรับอนาคต

Windows adapter ใช้ native process API, Job Object KILL_ON_JOB_CLOSE, cmd.exe terminal โดยไม่ใช้ shell script เป็น orchestration แก้ hosts เฉพาะ DEVONE managed block และสำรองครั้งแรก local CA trust ใช้ certutil current-user ไม่แก้ global PATH

macOS/Linux stub คืน unavailable สำหรับ OS integration ไม่อ้างว่า support แล้ว

## Runtime / process architecture

Manifest: runtime category, exact version, platform+architecture key, binary roles, HTTPS source, SHA-256, metadata
Registry: list installed/available, import, download ZIP, validate, remove guard, default/override resolution
PHP: shared FastCGI child ต่อ exact version, dynamic port, per-runtime generated ini/directives/extensions
MySQL: DatabaseEngine trait, shared instance ต่อ exact version, own cnf/data/log/port, initialize once ไม่ upgrade data อัตโนมัติ
Caddy: generated routes จาก sites + PHP allocated ports, validate before safe reload, loopback binding, TLS internal, dedicated storage under certs
Supervisor: owns Child handles, stdout/stderr files, TCP readiness, unexpected exit status, bounded health wait, graceful Caddy/MySQL shutdown แล้ว fallback kill, Windows Job Objects กัน orphan
Home lock: มี controller เดียวต่อ DEVONE_HOME ไม่ adopt/kill arbitrary stale PIDs
Start All: เริ่ม dependencies ของไซต์ present ที่มี PHP binding รวมถึง MySQL bindings และ Caddy ไม่เริ่ม installed versions ที่ไม่ถูกใช้

## Features ที่มี code รองรับ

startup scan + recursive filesystem watcher (register เฉพาะ direct child directories), Plain PHP/Laravel adapters, sanitized .test domains, conflict/reserved checks, missing records retained, custom runtime import, catalog ZIP install, per-site binding persistence, concurrent version pools/instances, terminal environment, generated config, start/stop/restart, health/log dashboard

## Phase 1 ที่ยังไม่ครบ

- wildcard DNS resolver/service: ตอนนี้ใช้ hosts managed block แยก OS ต้อง elevation เมื่อต้องเพิ่มรายการ
- privilege helper/UAC setup wizard: ยังไม่มี ต้อง launch elevated เอง
- curated remote runtime catalog: ยังไม่มี feed อัตโนมัติ ผู้ใช้จัด local catalog หรือ import
- MySQL credential provisioning/rotation และ database/user ต่อไซต์: ยังไม่มี ใช้ shared local instance
- extension availability/dependency validation และ polished extension toggle UI: model + JSON config มีแล้ว
- multiple workers ต่อ PHP pool / automatic crash backoff restart: ยังไม่มี
- authenticated protocol health: PHP/MySQL health เป็น TCP readiness ส่วน Caddy เป็น admin endpoint port
- generated IPC schema/codegen: types ฝั่ง TS และ Rust ต้องปรับร่วมกัน
- installer, tray, signed binaries, native dialog picker และ visual desktop QA ยังต้องเพิ่ม
- Tool Manager มี boundary/table และ terminal context ยังไม่มี Composer installation
- macOS/Linux runtime execution ไม่รองรับ และงาน out-of-scope ไม่ได้ทำ

## ความเสี่ยง/ข้อจำกัด

- การปล่อย port reservation ก่อน native child bind มีช่วง race สั้น หากโปรเซสอื่นแย่งพอร์ต start จะต้องรายงานล้มเหลว
- CA trusted flag เป็นสถานะหลังคำสั่งสำเร็จ ไม่ได้ตรวจ external trust removal ทุกครั้ง
- hosts block เป็น machine integration ขั้นแรก ไม่ใช่ wildcard *.test; ไม่ลบ unmanaged entries
- generated config marker ตรวจเพื่อไม่ overwrite user config; managed ini ที่แก้ด้วยมืออาจถูก regenerate ให้แก้ผ่าน directives model
- PHP distribution compatibility/VC runtime/extensions และ MySQL compatibility กับ CPU/Windows ต้องทดสอบจริงตามเวอร์ชัน
- MySQL initialization ล้มเหลวอาจทิ้ง directory; แอปปฏิเสธ reinitialize หากมีข้อมูล ต้องตรวจเอง ไม่ลบอัตโนมัติ
- changing global default เป็น explicit user action และเปลี่ยนเฉพาะ inherited bindings; overrides ไม่เปลี่ยน
- readiness ไม่ได้ยืนยัน route ทั้งหมดเสมอ integration test จึงตรวจ actual HTTPS content
- ไม่มี custom runtime execution ใน project โดยอัตโนมัติ binary runtime ที่ import ต้องเชื่อถือได้

## Native Windows integration test

ใช้ trusted fixture dirs ที่มี exact versions ตาม test (เป็น test fixture versions ไม่ใช่ catalog constants):

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

สร้าง temporary home, copy runtime fixtures, ตรวจ HTTPS โดยใช้ Caddy root CA เฉพาะ test client และ resolve DNS เฉพาะ client ไม่ install trust/แก้ hosts ตรวจ two PHP/two MySQL concurrently, actual SQL persistence, Laravel document root, Caddy PID stable หลัง reload, stop/restart และ guard runtime removal
