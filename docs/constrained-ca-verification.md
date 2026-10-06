# ผลตรวจ DEVONE constrained CA — 2026-10-06

## Step 0: ผ่านก่อนเริ่ม implementation

- Catalog ใช้ Caddy 2.11.7 ตรวจทั้ง source ของเวอร์ชันนั้นและ executable จริง:
  custom local root ผ่าน validate และ Caddy สร้าง intermediate ที่ตรวจ signature
  ย้อนกลับไปยัง root ที่ป้อนให้ได้ การต่ออายุ intermediate ใช้ root/key เดียวกัน
  ตาม [source Caddy 2.11.7](https://github.com/caddyserver/caddy/blob/v2.11.7/modules/caddypki/ca.go)
- Windows CertGetCertificateChain ใช้ exclusive in-memory root store จริง:
  foo.test ผ่าน (0x0), example.com ถูกปฏิเสธ (0x4000) โดยไม่เปลี่ยน CurrentUser\Root
  เทสต์ใน repo ใช้ parameter ของ production generator และทดสอบ site.test,
  example.com, IPv4 และ IPv6 ผ่าน intermediate อีกครั้ง
- คำขอเดิมระบุว่า Edge/Chrome ใช้ Windows chain API แต่ browser รุ่นปัจจุบันใช้
  verifier ของตนเอง:
  [Microsoft Edge certificate verifier](https://learn.microsoft.com/en-us/deployedge/microsoft-edge-security-cert-verification)
  และ [Chromium Windows local-root configuration](https://github.com/chromium/chromium/blob/main/net/cert/internal/trust_store_win.cc)
  เปิด enforce anchor constraints ข้อมูลนี้ไม่ใช่ผลทดสอบ browser GUI
- ตรวจเส้นทาง issuance แล้วไม่พบ managed certificate สำหรับ localhost/IP;
  host เหล่านั้นที่ใช้ probe เป็น HTTP จึงไม่ต้องขยาย name constraints

## สิ่งที่ทำ

- สร้าง root ECDSA P-256 ด้วย rcgen =0.14.10/ring, CN มี UUID,
  อายุประมาณสิบปี, critical CA pathLen 1, signing key usage และ critical
  name constraints อนุญาต DNS test และห้าม IP ทุกช่วง
- เก็บคู่ PEM ที่ certs/devone-ca/root.{crt,key}; protected staging directory,
  atomic certificate write และ create_new สำหรับ key ไม่ overwrite คู่เดิม
  ตรวจ matching key และ constraints ก่อน reuse และปฏิเสธ links/junctions
- Protected DACL ของ directory/key ให้ full control เฉพาะ current user SID
  และ SYSTEM การจำกัดนี้ไม่ป้องกัน process ที่ทำงานเป็นผู้ใช้เดียวกัน
- Setup/main Caddyfile ใช้ custom root พร้อม quote paths, คง storage และ
  skip_install_trust; intermediate/leaf เดิมย้ายไป backup ก่อนสร้างใหม่
- ปุ่ม Upgrade HTTPS certificate authority เป็น explicit action ใน Settings,
  Sites และ Setup แจ้งสถานะแบบไม่เป็น dialog; ไม่ migrate เพียงเพราะเปิดแอป
- Journal เก็บ phase และ fingerprints เพื่อ retry หลัง interruption;
  record ownership ก่อน OS mutation และถอนเฉพาะ exact owned legacy root
  หลังยืนยัน trust ใหม่แล้ว ยกเลิก trust จะ rollback กลับชุดเดิมและคง pending
- Recreate สร้าง constrained root ใหม่และสำรอง material เดิม

## ความต่างจากลำดับในคำขอ

- ย้าย pki/leaf cache ไป backup **ก่อน** reload/trust ใหม่ เพื่อไม่ให้ Caddy
  reuse intermediate หรือ leaf ที่เซ็นด้วย root เดิม เมื่อ cancel จะคืน cache
  เดิม หลัง upgrade สำเร็จเก็บ backup ไว้ ไม่ลบ
- คงชื่อ CaddyTls และ certificate id caddy-local เพื่อไม่เปลี่ยน schema
  ระหว่าง legacy pending ca_path ยังเลือก legacy เพื่อให้ใช้งานต่อได้;
  fresh install และ activated upgrade ใช้ root ใหม่
- ถ้า environment หยุดอยู่จะไม่เริ่ม site services จากการ upgrade:
  rewrite/validate Setup ทันที และ generate main Caddyfile ก่อนเริ่ม serve site
  ครั้งถัดไป ถ้า environment กำลังทำงานจะ rewrite/reload ทั้งสองทันที
- การตรวจ chain ใช้ temporary exclusive memory store จึงไม่ติดตั้ง root
  เข้า trust store จริง หน้าต่าง confirmation และ browser ต้องตรวจด้วยมือ

## ผลตรวจอัตโนมัติ

| คำสั่ง/การตรวจ | ผล |
| --- | --- |
| pnpm typecheck | ผ่าน |
| pnpm lint | ผ่าน |
| pnpm test | ผ่าน 48 เทสต์ |
| cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings | ผ่าน |
| cargo test --manifest-path src-tauri/Cargo.toml --lib | ผ่าน 59 เทสต์ |
| cargo fmt --manifest-path src-tauri/Cargo.toml -- --check | ผ่าน |
| git diff --check | ผ่าน |
| Native release_acceptance::two_php_versions_config_and_scoped_terminal | ผ่าน; PHP/Caddy จริงและ HTTPS |
| cargo test --manifest-path src-tauri/Cargo.toml --features process-fixture --test process_lifecycle | ผ่าน 9 เทสต์ |

Migration tests ครอบคลุม fresh, happy path, cancel, partial success หลัง trust,
crash ก่อน trust และ resume, fingerprint mismatch ก่อน/หลัง trust และการคง
unrelated root ไว้ เทสต์ Windows chain ไม่ใช้ ignore และรันผ่านบนเครื่องนี้

## ไฟล์ที่เปลี่ยนในงาน CA

- Dependencies: src-tauri/Cargo.toml, src-tauri/Cargo.lock
- Backend: src-tauri/src/tls/authority.rs,
  src-tauri/src/tls/authority/windows_tests.rs, src-tauri/src/tls/upgrade.rs,
  src-tauri/src/tls/mod.rs, src-tauri/src/platform/ca_security.rs,
  src-tauri/src/platform/mod.rs, src-tauri/src/setup.rs,
  src-tauri/src/webserver/mod.rs, src-tauri/src/app/mod.rs, src-tauri/src/ipc.rs
- Frontend: src/contracts.ts, src/components/CaUpgradeNotice.tsx,
  src/components/ProductSettings.tsx, src/components/SitesOverview.tsx,
  src/components/Setup.tsx, src/components/workflows.test.tsx
- Native fixtures: src-tauri/tests/phase2.rs, src-tauri/tests/phase3.rs,
  src-tauri/tests/product_smoke.rs, src-tauri/tests/windows_workflow.rs
- Docs: README.md, docs/windows-hardening.md,
  docs/architecture-and-verification.md, docs/windows-authorization-readiness.md,
  scripts/acceptance/README.md และรายงานนี้

## Bug review รอบก่อนที่ยังอยู่ใน working tree

| ข้อ | สถานะ |
| --- | --- |
| 1 TCP sockets สืบทอด non-blocking | Fixed: reset accepted DNS/activation stream ก่อน timeout; มี delayed/fragment tests |
| 2 mysql client-side commands ใน restore | Skipped: binary-mode ที่มีอยู่ปิด system/tee/source ใน batch แล้ว; commands=OFF ถูก binary-mode override จึงไม่เพิ่ม flag ที่ไม่มีผล |
| 3 recovery หลัง start ไม่สำเร็จ | Fixed: arm เมื่อ healthy แล้วเท่านั้น; manual Start และ automatic reconcile ไม่ติด pending recovery |
| 4 stop_all หยุดเมื่อเจอ error | Fixed: ลองทุก service/รวบรวม errors; clear recovery และพยายาม update state แม้ termination error |
| 5 recursive watcher/issues ซ้ำ | Fixed: watch www แบบ non-recursive; periodic marker reconcile และ issue de-duplication |

ไฟล์เพิ่มเติมของรอบนั้น: src-tauri/src/app/watcher.rs,
src-tauri/src/process/mod.rs, src-tauri/src/dns/server.rs,
src-tauri/src/desktop_instance.rs, src-tauri/tests/fixtures/process.rs,
src-tauri/tests/process_lifecycle.rs รวม app/mod.rs ที่ทั้งสองงานแก้

## ตรวจด้วยมือบนเครื่องที่มี legacy CA

1. ใช้ disposable legacy Home ที่มี recorded fingerprint ตรงกับ root ที่ trust
   และตรวจว่า .test เดิมใช้งานได้ จด Root entry เดิมไว้
2. เปิด Settings / HTTPS ตรวจ notice ใน Sites/Setup ด้วย การเปิดหน้าต้องไม่
   แสดง trust prompt เอง จากนั้นกด Upgrade อย่างชัดเจน
3. ยกเลิก prompt ครั้งแรก: site และ trust เดิมต้องยังใช้ได้ ปุ่ม upgrade
   ต้องยังอยู่ และมี backup โดยไม่มี unrelated certificate ถูกถอน
4. กดอีกครั้งและอนุมัติ: .test ต้องได้ intermediate/leaf ใหม่ใต้ constrained
   DEVONE root และถอนเฉพาะ exact owned legacy entry ตรวจ backup เดิมยังอยู่
5. Restart และตรวจ HTTPS ผ่าน browser ตรวจ critical constraints ด้วย
   certificate viewer; ทดสอบ example.com ที่ออกจาก CA นี้ต้องถูกปฏิเสธ
6. อีก disposable Home: interrupt หลัง trust ใหม่แต่ก่อนถอนเดิม แล้ว restart
   และกด upgrade ซ้ำให้จบ อีกชุดเปลี่ยน old fingerprint ให้ mismatch:
   ต้องถูกปฏิเสธโดยไม่ถอน certificate ใด

ยังไม่ได้ทดสอบ trust confirmation/browser GUI หรือสร้าง installer ใหม่ในรอบนี้
และยังไม่ได้ commit/push
