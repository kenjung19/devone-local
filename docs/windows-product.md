# Windows desktop Phase 1 — รอบต่อจาก a76de88

## รายงาน 20 ประเด็น

1. **การแก้ไข** คง Rust core/runtime/SQLite เดิม เพิ่ม packaging, tray, activation, startup preferences, CA management และ product Settings/runtime UX
2. **Locked stack** Node 24.21.0 LTS, pnpm 12.8.1, Rust 1.99.0 และ dependencies exact versions เดิม ไม่มี broad upgrades เปลี่ยนเฉพาะเปิด feature tray-icon ของ Tauri 2.12.1 (crate ที่มีใน Cargo.lock อยู่แล้ว)
3. **Installer** Tauri supported NSIS currentUser, Windows 10/11 x64, ชื่อ DEVONE Local, version 0.1.0, multi-resolution ICO จาก SVG เดิม, Start Menu folder และ ARP/uninstaller จาก standard Tauri template WebView2 downloadBootstrapper ไม่เพิ่ม custom packaging architecture [Tauri Windows installer](https://v2.tauri.app/distribute/windows-installer/)
4. **Artifacts** `src-tauri/target/x86_64-pc-windows-msvc/release/bundle/nsis/DEVONE Local_0.1.0_x64-setup.exe`; release-artifacts.json ระบุ hash/size/target หลัง build สำเร็จ ผลจริงอยู่ด้านล่าง
5. **Home** default `%LOCALAPPDATA%/Devone` application executable อยู่ installer location แยกจาก SQLite/settings/config, projects www, runtimes, database, certs/credentials, logs/cache/backups ไม่มี mutable data ใน Program Files; custom DEVONE_HOME เป็น advanced developer override หรือ --home สำหรับ startup command
6. **Tray** CloseRequested ซ่อน window ไม่หยุด core/watcher; tray มี Open, Environment Start/Stop/Restart, discovered Sites, Open www, Quit เมนู/status refresh แม้ window hidden ผ่าน Rust worker ไม่พึ่ง React polling Quit graceful shutdown + Job fallback แล้ว Exit cleanup
7. **Single instance** per-Home OS file lock เพิ่ม desktop activation endpoint loopback random port+UUID token ส่งเฉพาะ activate ไม่ execute commands Second launch ไม่สร้าง Application/DNS/supervisor และ AllowSetForegroundWindow ช่วย restore/focus Core devone.lock ยังเป็น authoritative ownership safeguard ไม่เชื่อ stale PID; stale activation file ถูกแทนเมื่อ acquire lock สำเร็จ
8. **Windows startup** HKCU Run value DEVONE Local ระบุ exact executable + --startup --home แบบ quote ไม่ต้อง Admin entry conflict ไม่ overwrite Start login ไม่ register PHP/MySQL/Caddy Startup UI ตรวจ actual registry แยกจาก environment_autostart SQLite setting Manual Start/Stop ไม่เปลี่ยน preferences Launch --startup ซ่อนเมื่อ setup completed
9. **DNS lifecycle** owned UDP/TCP resolver อยู่ตลอด desktop lifetime/tray Quit หยุด resolverแต่ policyคงอยู่ Settingsแสดง Policy/Resolver/System lookupแยกกัน ไม่ Ready เมื่อ stopped Retry DNS แก้ bind conflictได้ Cancel1223/Denied5 มีข้อความactionable Foreign test subnamespace/GPO conflicts ปฏิเสธ Owned install idempotent Remove verifyownedkeyหายจริง ไม่มี adapter DNS hijack
10. **CA lifecycle** actual current-user Windows Root lookup Prepare/Install/Remove/Recreate แยกกัน Remove ต้อง identity SHA-256 ตรงกับ recorded ownership ลบ exact certificate context ด้วย CryptoAPI ไม่ใช้ชื่อ subject กว้าง Recreate confirmation, stop Caddyเท่านั้น,ย้าย storage ไป backups,สร้างใหม่,reloadroutes ส่วน PHP/MySQL PIDคงเดิม Rollbackเก็บ partial CA และคืนของเดิมเมื่อ preparefail
11. **Bundled catalog** metadata compile-in ของเดิม fresh Homeพร้อม install choices Local/HTTPS remote cache/integrity behaviorคงเดิม ไม่มี invented production feed Normal importมี drag/drop folder และ Detect version Advanced binary rolesซ่อนในdetails
12. **Caddy decision** เลือก A: ติดตั้งผ่าน catalogในSetup เป็นinfrastructure ไม่ให้ผู้ใช้หา/downloadเอง Installerเล็กและคง exact version/checksum/import/update isolation ไม่มี automatic upgrades; first-run downloads ต้องinternet
13. **Upgrade/uninstall** standard NSIS file installationไม่แตะ Home Standard Tauri NSISลบproduct HKCU startup valueเมื่อuninstallและคงไว้ใน/UPDATE hookของDEVONEแสดงข้อมูลเท่านั้น ไม่มี DEVONE Home removal/data reset/hosts/trust deletion (standard NSIS checkbox ของ app-data กระทบเฉพาะ bundle-id WebView data ไม่ใช่ Home) คงSQLite migrations001/002 guard newerstatefail-safe เปลี่ยน0.1.0→0.1.1เป็นexplicitpackage/Cargochange ไม่releaseincrementเอง
14. **Developer workflow** pnpm install --frozen-lockfile / desktop / test / build / release:windows ใช้Nodehelpers cross-platformversioncheck ไม่ต้องPowerShell scripts Shellcacheoptionalเท่านั้น Core orchestrationยังRust
15. **Tests executed** ผลล่าสุดด้านล่าง Automated DNS conflict matcher/activation stale/token/startup quoting/preferences/CA identity/schema guard/UI missingruntime+Settings/nativeCA recreation เพิ่มจากsuiteเดิม
16. **Release results** ผลด้านล่าง ไม่อ้างinstallerสำเร็จก่อนartifactมีจริง
17. **Manual GUI checks performed** ยังไม่มี manual GUI installer/UAC/tray clicks ในรอบนี้ Compile/automatedไม่เท่ากับmanualpass
18. **Checks requiring manual verification** checklistถัดไปครอบคลุมinstalledfresh/upgrade/uninstall/login/reboot/UACcancel/trust/browser/trayและruntimeUI
19. **Remaining gaps** signing/SmartScreen reputation, manual installed validation และ actual future-version upgrade ไม่มี destructive reset, production remote feed, Phase2 หรือmacOS/Linux execution
20. **Phase 1 completion** implementationพร้อมส่งตรวจ แต่ยังไม่ประกาศproductPhase1completeจนinstalledWindows/manualchecklistผ่านจริง

## Verification ล่าสุด

- frozen pnpm install, lint/typecheck/build ผ่าน; frontend 10 tests ผ่าน
- Rust fmt/clippy และ cargo suite: 27 unit + 1 preferences + 5 process tests ผ่าน
- Native workflow ที่เพิ่ม CA recreation: ผ่าน 1 test (60.87s)
- Release packaging: `pnpm release:windows` ผ่าน exit 0 สร้าง NSIS x64 installer จริง ขนาด 8,427,176 bytes (8.04 MiB) ชื่อ `DEVONE Local_0.1.0_x64-setup.exe`; executable และ installer มี ProductName DEVONE Local และ ProductVersion/FileVersion 0.1.0 ตรวจ SHA-256 ซ้ำด้วย Get-FileHash ตรงกับ release-artifacts.json: `3edb090a89c037666aab2a78951b3b3ecb78899c82f8220c84dbd1bfef58bc22`
- Release memory workaround: compiler memory allocation fail ใน windows bindings ที่ opt-level=3 และ opt-level=0/codegen256 แม้ jobs=1; รอบสำเร็จใช้ profile.release.package.windows opt-level=0/codegen-units=1 เฉพาะ generated FFI bindings และ CARGO_BUILD_JOBS=1 โค้ดแอปยัง optimized ตาม release profile ไม่เปลี่ยน dependency versions/lockfiles ไม่แก้ global pagefile หรือหยุดแอปอื่น Log อยู่ `.devone-test/release-windows.log` (local ignored file)
- ไม่มี commit/push อัตโนมัติในรอบนี้

## Manual-ready checklist (ยังไม่ผ่านด้วยมือ)

ใช้ Windows 10/11 x64 VM/current-user account ที่ไม่มี development tools บันทึกpass/failและlogสำหรับแต่ละข้อ

| ขั้น | สิ่งที่ต้องตรวจ |
|---|---|
| Fresh install | เปิด setup.exe, icon/name/version0.1.0, StartMenu, ARP, WebView2 bootstrap, เปิดnormaluser |
| Setup | เปิด/ปิด/reopenwizard,Home/wwwพร้อม,เลือกinitialruntime,installprogress/checksumerror/retry,Importfolderdragdrop/Detectversion |
| DNS | UDP/TCP53available/occupied,foreign.test/subpolicyreject,ownedretry,UACcancel/denied/install/remove,resultnormalapp,OSlookup.testและunrelateddomains |
| CA | Prepare/installtrust/removeexactownedtrust/recreateconfirmation,newbrowserHTTPS,unrelatedrootcertคงอยู่ |
| Runtimes | PHP7directives/extensions invalidconfig,MySQLinit/start/stop/restart,persistentSQL,siteselector/defaultoverride,missingruntimeไม่fallback |
| Project database | provision/retry,restrictedgrants,pwdreveal,credentialsคงหลังเปิดใหม่,.envไม่เปลี่ยน |
| Sites | OpenSite/Folder/Terminal/Logs,scopedPATH,Openwww,copyfolderauto-discoveryไม่มีAddSite |
| Tray | closehide,watchercopyprojectขณะhidden,restore,doublelaunchfocus,start/stop/restart,status,Quitownedgracefulunrelatedservicesคง |
| Login/reboot | Windowsstartup ON/OFF และenvironment ON/OFF ทั้งสี่คู่; setupincompleteไม่ซ่อน;stalePIDไม่adopt;DNS/CA/data/portreconcile |
| Unexpected termination | terminateเฉพาะDEVONEtestinstance,ownedJobchildrenหยุด,nextlaunchไม่reinit/rotatecredential/overwriteproject |
| Upgrade | VM snapshot,สร้างproject/DB+credentials/siteoverride/CA/DNS,explicit0.1.1upgrade;ค่าทั้งหมดคง;newerschemaถูกปฏิเสธไม่downgrade |
| Uninstall | Quitก่อนuninstall,ARP/shortcut/binaryหาย,ownedstartupentryหาย,Home/www/database/runtimes/backups/settings/credentials/CAคง;ถอนDNS/trustก่อนถ้าต้องการ |

## คำสั่ง

```text
pnpm install --frozen-lockfile
pnpm lint
pnpm typecheck
pnpm test
pnpm build
pnpm version:check
pnpm release:windows
cargo fmt --manifest-path src-tauri/Cargo.toml --check
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
cargo test --manifest-path src-tauri/Cargo.toml
cargo test --manifest-path src-tauri/Cargo.toml --test windows_workflow -- --ignored --nocapture
```

Native fixture variablesคงตาม architecture.md Windowsfixturetestsไม่เขียนmachineNRPTหรือtruststore HTTPclienttrustเฉพาะCAของtest
