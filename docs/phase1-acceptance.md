# Final Phase 1 acceptance — 4 ตุลาคม 2026

ฐาน `main`: `8dc8c5f85091a9e9a2bfe294e160d45d6a7f3e7d` Windows x64 เครื่องปัจจุบัน ไม่ใช่ clean Windows VM ผล silent installer/process/data checks ด้านล่างไม่ใช่ GUI acceptance ไม่เพิ่ม Phase 2 ไม่เปลี่ยนสถาปัตยกรรม ไม่ upgrade dependencies และไม่ commit/push อัตโนมัติ

1. **Installer tested: yes (silent installation only).** รัน `pnpm release:windows` จริงหลังแก้ packaging แล้ว ติดตั้ง NSIS current-user ในโฟลเดอร์แยก สร้าง executable, ARP และ Start Menu shortcut สำเร็จ ไม่ได้คลิก installer หรือเปิดผ่าน Start Menu GUI

2. **Installer path and SHA-256.** `src-tauri/target/x86_64-pc-windows-msvc/release/bundle/nsis/DEVONE Local_0.1.0_x64-setup.exe` ขนาด **8,404,438 bytes (8.02 MiB)** SHA-256 **`5f34bc3453bf320215bd75f7309e21d96ec530d9ef00881a9cd421c86f90f27a`**; report `release-artifacts.json` ข้าง installer Installed executable ที่ใช้จริง: `D:\Project\devone-local\.devone-test\installed-acceptance-20261004b\Application\devone-local.exe`; ProductName DEVONE Local, ProductVersion/FileVersion และ ARP DisplayVersion **0.1.0** หลังทดสอบถอน test installation แล้ว เก็บ Home ไว้

3. **Actual GUI tests performed: none.** Computer Use `node_repl` เปิดไม่ได้ทั้งครั้งแรกและหลัง reset: `windows sandbox failed: helper_unknown_error: setup refresh had errors` / kernel exited code 1 จึงไม่มี screenshots หรือผลการคลิก wizard/Settings/tray ใช้ executable จาก installation จริงผ่าน subprocess โดย cwd อยู่ test root และ PATH มีเฉพาะ Windows/System32 กับ Windows ตัด CARGO_HOME/RUSTUP_HOME/NODE_PATH/PNPM_HOME ออกจาก process เปิดได้ สร้าง SQLite และ Home directories ทั้ง 9 รายการ การตรวจ source ไม่พบ product invocation ของ Node/pnpm/Cargo/PowerShell หรือการ lookup developer/test paths แต่ไม่ได้ถอน developer tools ออกจากเครื่อง และไม่ได้พิสูจน์ first-run wizard ที่มองเห็น

4. **UAC/DNS result: not accepted yet.** Installed startup log ยืนยัน resolver started port 53 แต่ไม่ได้กด Setup DNS, ทดสอบ UAC cancel/retry, สร้าง NRPT policy หรือ Windows system lookup `.test`/public hostname รอบนี้ Unit tests ของ UDP/TCP lifecycle/conflicts และ foreign namespace matcher ผ่าน การถอน test installation ไม่เปลี่ยน local/GPO NRPT registry trees ที่ตรวจ ไม่ใช้ผล resolver bind เป็นหลักฐานว่า system DNS พร้อม

5. **Real browser HTTPS result: not tested.** ยังไม่ได้ prepare/install/remove/recreate trust ผ่าน GUI หรือเปิด HTTPS ใน browser ไม่มีการเพิ่ม/ลบ certificate ในรอบนี้ current-user Root store คงเดิมระหว่าง update/uninstall Exact CA identity/confirmation unit test ผ่าน แต่ไม่แทน real browser acceptance

6. **Project discovery result: passed for installed watcher and root selection.** สร้าง `Home/www/demo/index.php` และ `Home/www/framework/artisan` + `public/index.php` ขณะ installed app ทำงาน watcher สร้าง sites เอง `demo` ใช้ project root และ `framework` ใช้ `framework/public` โดยไม่ restart ย้าย demo ไป `Home/backups/demo-moved` แล้ว present=0 อย่างปลอดภัย ข้อมูล framework คงอยู่ ยังไม่ได้ทดสอบ actual PHP HTTP routing เพราะไม่มี runtime ที่ติดตั้งใน Home ทดสอบนี้

7. **PHP multi-version result: not tested on installed GUI.** ยังไม่ได้ download/import สองรุ่น, per-site switch หรือยืนยัน PHP_VERSION ผ่าน HTTP ในรอบนี้ ผล native workflow จากรอบก่อนเป็น historical evidence ใน verification.md เท่านั้น

8. **PHP config/extensions result: not tested on installed GUI.** ยังไม่ได้ปรับ memory_limit/upload_max_filesize/date.timezone หรือ toggle extension/ตรวจ `--ini` และ `-m` ผ่าน installed GUI/scoped terminal Unit validation regression ผ่าน

9. **MySQL multi-version result: not tested on installed GUI.** ไม่ initialize MySQL หรือสร้าง persistent SQL ใน acceptance Home ยังไม่มีผลสอง instances/ports/data persistence จาก installed product รอบนี้

10. **Provisioning result: not tested on installed GUI.** ยังไม่ได้สร้าง project database/user, reveal credentials, ตรวจ grants/log secrecy หรือ SQL restart persistence ผ่าน installed product รอบนี้

11. **Tray result: not tested through actual tray.** Installed app/watcher ทำงานได้ แต่ไม่ได้ปิดหน้าต่างไป tray, คลิก tray menu หรือ Quit จึงไม่มี graceful Quit acceptance การหยุด process ทดสอบเป็น abnormal termination ที่ระบุแยกชัดเจน ไม่มี PHP/MySQL/Caddy runtimes ใน Home นี้

12. **Single-instance result: backend passed on installed executable; focus not observed.** เปิด installed executable ครั้งที่สองด้วย Home เดิม จบ exit 0 endpoint/PID ของ owner คงเดิม ไม่มี controller ใหม่ หลัง terminate เฉพาะ process ที่ test เป็นผู้สร้าง เปิดใหม่และ endpoint เปลี่ยนเป็น PID ใหม่ได้โดย sites คงอยู่ ยังไม่ได้ดู restore/focus หน้าต่างจริง

13. **Windows startup result: not tested through GUI/login.** Quote/command ownership unit test และ preferences test ผ่าน ไม่ได้เปลี่ยน HKCU startup setting หรือ logout/login รอบนี้ ยังต้องทดสอบ startup/environment preference ทั้งสี่คู่และ normal-user launch

14. **Restart/reboot result: partial reopen passed; reboot not performed.** เปิดใหม่หลัง abnormal termination ได้ sites และ missing state คงอยู่ ไม่ได้ reboot/logout เครื่องผู้ใช้ ไม่ได้ทดสอบ credentials/MySQL/CA/NRPT restoration หลัง reboot

15. **Uninstall result: silent uninstall passed for test installation.** Same-build `/S /UPDATE` จบ exit 0 ไฟล์ Home 15 ไฟล์ hash คงเดิมครบ รวม SQLite, project PHP files และ preservation sentinels ใน database/runtimes/backups/certs/config จากนั้น `/S` uninstall จบ exit 0 app/core executable, ARP entry และ Start Menu shortcut หาย Home และไฟล์ทั้ง 15 คงเดิม Local/GPO NRPT และ current-user Root certificate store ที่ตรวจไม่เปลี่ยน **sentinels ไม่ใช่หลักฐาน real MySQL database/credential/CA preservation** การไม่ลบ Home/ไม่ถอน policy/trust เงียบ ๆ เป็นพฤติกรรมที่ตั้งใจไว้ ผู้ใช้ถอน DEVONE integration ผ่าน Settings ก่อน uninstall ได้

16. **Bugs discovered.** Generated NSIS ของ base build รวม `devone-process-fixture.exe` ซึ่งเป็น test-only executable พบจาก installer script จริง อีก observation: `/S` reinstall ทันทีหลัง forced termination คืน exit 2 แต่ Home ไม่เปลี่ยน; retry `/S /UPDATE` หลัง process หยุดผ่าน ยังไม่มีหลักฐานชี้สาเหตุ exit 2 และต้องตรวจ clean GUI Quit → reinstall ต่อ Test harness พบ Windows environment key case และ SQLite read connections ไม่ปิด แก้ harness แล้ว; ไม่จัดเป็น product bugs

17. **Bugs fixed.** เพิ่ม non-default Cargo feature `process-fixture` และ required-features ให้ fixture binary กับ process_lifecycle test target Release/default build ไม่รวม fixture อีกต่อไป ยังรัน process tests ครบด้วย feature Explicit installed file list หลังแก้มีเพียง devone-local.exe, devone-core.exe, uninstall.exe เพิ่ม packaging guard ใน toolchain test และบันทึกคำสั่ง tests ไม่มี dependency/lockfile change ใช้ [Tauri CLI binary enumeration](https://github.com/tauri-apps/tauri/blob/tauri-cli-v2.12.0/crates/tauri-cli/src/interface/rust.rs) ตาม behavior required-features ที่รองรับ

18. **Automated test results.** frozen pnpm install, lint, typecheck, frontend 10 tests และ Vite build ผ่าน; cargo fmt, clippy all-targets ทั้ง default และ all-features ผ่าน; default cargo test 27 unit + 1 preferences ผ่าน; all-features suite เพิ่ม process lifecycle 5 tests ผ่าน รวม **33 tests** native Windows fixture workflow ไม่รันซ้ำ: ไม่พบ PHP/MySQL fixtures เดิมในตำแหน่งที่ตรวจ ไม่เอาผลเก่ามานับเป็นรอบนี้ Final release exit 0 และ silent installed tests ตามข้อ 1–15 Logs/results อยู่ local ignored `.devone-test/acceptance-release-final.log` และ `.devone-test/installed-acceptance-20261004b/results.json` ไม่มีการเปลี่ยน global PATH/pagefile, ไม่หยุด foreign runtimes, ไม่ลบ user Home

19. **Remaining critical blockers.** ต้องกู้ Windows GUI automation หรือให้ผู้ทดสอบเดิน actual installed GUI: first-run wizard + bundled runtime download/progress/checksum/retry, one-time UAC/NRPT cancel/retry + OS wildcard lookup, CA lifecycle + real browser HTTPS, PHP/MySQL multi-version + config/provisioning/scoped terminal, Stop/Restart/Quit ownership/tray, single-instance focus, Windows startup preferences และ installed error states ต้องยืนยัน reinstall หลัง graceful GUI Quit ด้วย ประเด็นเหล่านี้ยังขาด acceptance evidence; ไม่ใช่การสรุปว่าทุก feature เสีย

20. **Remaining non-critical limitations.** Installer **NotSigned** ตาม Get-AuthenticodeSignature เพิ่ม signing ภายหลังใน Tauri packaging ได้โดยไม่ redesign ไม่มี clean VM/Windows 10 แยกจากเครื่องนี้ ไม่มี actual newer-version upgrade หรือ reboot test; same-build update และ abnormal reopen ให้หลักฐานเพียงบางส่วน Native folder picker/Composer Manager และ Phase 2/แพลตฟอร์มอื่นอยู่นอกขอบเขต workspace-env.ps1 ยังคงเป็น optional local-cache adapter สำหรับเครื่อง developer ไม่ใช่ product requirement

21. **Explicit verdict: `PHASE 1 NOT COMPLETE`.** Critical blockers คือ installed GUI/UAC/DNS/browser HTTPS/runtime/tray/startup acceptance ที่ยังไม่ได้ทำเพราะ GUI runtime เปิดไม่ได้ Packaging/installed startup/watcher/activation/update/uninstall ที่ตรวจได้ผ่านตามขอบเขตข้างต้น ยังไม่อนุญาตให้ตีความ automated หรือ silent results ว่าผ่าน manual acceptance ทั้งหมด
