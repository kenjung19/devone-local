"""Package verified Windows runtimes with DEVONE; never copy an existing user Home."""

import argparse
from contextlib import closing
import hashlib
import json
from pathlib import Path, PurePosixPath
import re
import shutil
import sqlite3
import subprocess
import tempfile
import time
import urllib.request
import zipfile


def sha256(path):
    with path.open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()


HOME_DIRECTORIES = ['www', 'runtimes', 'tools', 'database', 'config', 'certs', 'logs', 'backups', 'cache']
# Files this script has ever written to release/. Only these are replaced;
# anything else (for example a Home created by running an EXE there) is
# reported, never deleted.
GENERATED = re.compile(
    r'^(DEVONE-Local-[0-9.]+-Windows-x64-(Setup\.exe|Portable\.zip)'
    r'|release-artifacts\.json|bundled-artifacts\.json|devone-local\.exe)$')


def release_names(version):
    return {
        'installer': f'DEVONE-Local-{version}-Windows-x64-Setup.exe',
        'portable': f'DEVONE-Local-{version}-Windows-x64-Portable.zip',
    }


def prepare_release_directory(release):
    if release.is_symlink():
        raise ValueError('release/ must not be a link')
    release.mkdir(parents=True, exist_ok=True)
    for item in release.iterdir():
        if GENERATED.match(item.name):
            if item.is_symlink() or not item.is_file():
                raise ValueError(f'Generated release path must be a regular file: {item}')
            item.unlink()


def publish(release, staging, artifacts):
    """Replace generated outputs only after every artifact was built."""
    prepare_release_directory(release)
    for artifact in artifacts:
        (staging / artifact['filename']).replace(release / artifact['filename'])
    (release / 'release-artifacts.json').write_text(json.dumps(artifacts, indent=2)+'\n', encoding='utf-8')
    expected = {a['filename'] for a in artifacts} | {'release-artifacts.json'}
    foreign = sorted(i.name for i in release.iterdir() if i.name not in expected and i != staging)
    if foreign:
        print('WARNING: release/ contains entries that are not part of this release '
              f'(left untouched): {", ".join(foreign)}', flush=True)


def runtime_inputs(root):
    document = json.loads((root / 'scripts/windows-bundle.json').read_text(encoding='utf-8'))
    cache = Path(tempfile.gettempdir()) / 'devone-current-runtime-archives'
    cache.mkdir(exist_ok=True)
    if cache.is_symlink():
        raise ValueError('Runtime archive cache must not be a link')
    inputs = []
    for manifest in document['manifests']:
        archive = cache / f'{manifest["runtime"]}-{manifest["version"]}.zip'
        if not archive.exists():
            print(f'Downloading build input: {manifest["runtime"]} {manifest["version"]}', flush=True)
            partial = archive.with_suffix('.partial')
            with urllib.request.urlopen(manifest['download'], timeout=120) as incoming, partial.open('wb') as outgoing:
                shutil.copyfileobj(incoming, outgoing)
            partial.replace(archive)
        inputs.append({'manifest': manifest, 'archive': str(archive)})
    return inputs


def extract_runtime(archive, manifest, destination):
    prefix = manifest.get('metadata', {}).get('archive_root')
    with zipfile.ZipFile(archive) as source:
        for item in source.infolist():
            relative = PurePosixPath(item.filename)
            if relative.is_absolute() or '..' in relative.parts or '\\' in item.filename or ':' in item.filename:
                raise ValueError(f'Unsafe archive path: {item.filename}')
            if prefix:
                relative = relative.relative_to(prefix)
            if not relative.parts or item.is_dir() or relative.suffix.lower() == '.pdb':
                continue
            if (item.external_attr >> 16) & 0o170000 == 0o120000:
                raise ValueError(f'Archive contains a link: {item.filename}')
            target = destination.joinpath(*relative.parts)
            target.parent.mkdir(parents=True, exist_ok=True)
            with source.open(item) as incoming, target.open('wb') as outgoing:
                shutil.copyfileobj(incoming, outgoing)
    for role in manifest['binaries'].values():
        if not (destination / role).is_file():
            raise ValueError(f'Missing runtime binary: {role}')
    role = 'server' if manifest['runtime'] in ('mysql', 'caddy') else 'cli'
    argument = 'version' if manifest['runtime'] == 'caddy' else '--version'
    result = subprocess.run([str(destination / manifest['binaries'][role]), argument],
                            capture_output=True, text=True, timeout=30, check=True)
    if manifest['version'] not in result.stdout + result.stderr:
        raise ValueError(f'Runtime version mismatch: {result.stdout} {result.stderr}')
    print(f'Validated {manifest["runtime"]} {manifest["version"]}', flush=True)


def seed_registry(root, home, manifests):
    with closing(sqlite3.connect(home / 'devone.db')) as conn, conn:
        migrations = sorted((root / 'src-tauri/migrations').glob('*.sql'))
        for version, migration in enumerate(migrations, 1):
            conn.executescript(migration.read_text(encoding='utf-8'))
            conn.execute(f'PRAGMA user_version={version}')
        for item in manifests:
            kind, version = item['runtime'], item['version']
            identity = f'{kind}:{version}'
            encoded = json.dumps(item, separators=(',', ':'))
            conn.execute('INSERT INTO runtime_installations VALUES(?,?,?,?,?,?)',
                         (identity, kind, version, encoded, f'runtimes/{kind}/{version}', int(time.time())))
            conn.execute('INSERT INTO settings VALUES(?,?)', (f'default.{kind}', version))
        assert conn.execute('SELECT count(*) FROM sites').fetchone()[0] == 0
        assert conn.execute('SELECT count(*) FROM database_instances').fetchone()[0] == 0
        assert conn.execute('SELECT count(*) FROM certificates').fetchone()[0] == 0
        assert conn.execute('PRAGMA integrity_check').fetchone()[0] == 'ok'


def installer_hooks(home, manifests):
    """NSIS hooks: install bundled Home files, keep the Home writable, and on a
    real uninstall remove only what the installer shipped."""
    hook = [
        # Home lives beside the executable, so the folder must stay writable
        # without elevation. Reject system locations on the directory page.
        'Function .onVerifyInstDir',
        '  StrLen $0 "$PROGRAMFILES64"',
        '  StrCpy $1 "$INSTDIR" $0',
        '  StrCmp $1 "$PROGRAMFILES64" devone_bad_dir',
        '  StrLen $0 "$PROGRAMFILES"',
        '  StrCpy $1 "$INSTDIR" $0',
        '  StrCmp $1 "$PROGRAMFILES" devone_bad_dir',
        '  StrLen $0 "$WINDIR"',
        '  StrCpy $1 "$INSTDIR" $0',
        '  StrCmp $1 "$WINDIR" devone_bad_dir',
        '  Return',
        '  devone_bad_dir:',
        '  Abort',
        'FunctionEnd',
        '!macro NSIS_HOOK_POSTINSTALL', '  SetOverwrite off']
    for name in HOME_DIRECTORIES:
        hook.append(f'  CreateDirectory "$INSTDIR\\{name}"')
    for item in sorted(home.rglob('*')):
        if not item.is_file() or item.name == 'devone-local.exe':
            continue
        relative = item.relative_to(home)
        parent = str(relative.parent) if relative.parent != Path('.') else ''
        hook.extend([f'  SetOutPath "$INSTDIR\\{parent}"', f'  File "{item}"'])
    hook.extend(['  SetOutPath "$INSTDIR"', '  SetOverwrite on',
                 '  DetailPrint "PHP, MySQL LTS, Caddy and Node LTS are included. Existing Home data is preserved."',
                 '!macroend',
                 '!macro NSIS_HOOK_PREUNINSTALL',
                 '  DetailPrint "Projects, databases, credentials and CA data are preserved."',
                 '!macroend',
                 # Runs after Tauri stopped the app and removed its own files.
                 '!macro NSIS_HOOK_POSTUNINSTALL',
                 '  ${If} $UpdateMode <> 1'])
    kinds = sorted({m['runtime'] for m in manifests})
    for manifest in sorted(manifests, key=lambda m: (m['runtime'], m['version'])):
        hook.append(f'    RMDir /r "$INSTDIR\\runtimes\\{manifest["runtime"]}\\{manifest["version"]}"')
    hook.extend(f'    RMDir "$INSTDIR\\runtimes\\{kind}"' for kind in kinds)
    hook.extend(['    Delete "$INSTDIR\\README.txt"',
                 '    Delete "$INSTDIR\\config\\runtime-catalog.json"'])
    # Non-recursive: only folders that are already empty disappear.
    hook.extend(f'    RMDir "$INSTDIR\\{name}"' for name in HOME_DIRECTORIES)
    hook.extend(['    RMDir "$INSTDIR"', '  ${EndIf}', '!macroend'])
    return '\n'.join(hook)+'\n'


def make_installer(root, home, manifests, output):
    generated = root / 'src-tauri/target/x86_64-pc-windows-msvc/release/nsis/x64'
    script = (generated / 'installer.nsi').read_text(encoding='utf-8-sig')
    # Full vendor runtimes are large; zlib keeps packaging time practical.
    script = re.sub(r'^  SetCompressor /SOLID "lzma"$', '  SetCompressor /SOLID "zlib"', script, flags=re.MULTILINE)
    hooks = home.parent / 'bundled-hooks.nsh'
    hooks.write_text(installer_hooks(home, manifests), encoding='utf-8')
    script, count = re.subn(r'^!include ".*\\installer\\hooks\.nsh"$',
                           lambda _: f'!include "{hooks}"', script, flags=re.MULTILINE)
    assert count == 1
    script = re.sub(r'^!define OUTFILE .*$', lambda _: f'!define OUTFILE "{output}"', script, flags=re.MULTILINE)
    size = sum(p.stat().st_size for p in home.rglob('*') if p.is_file()) // 1024
    script = re.sub(r'^!define ESTIMATEDSIZE .*$', f'!define ESTIMATEDSIZE "{size}"', script, flags=re.MULTILINE)
    # The desktop executable performs its own elevated helper operations.
    script = '\n'.join(line for line in script.splitlines() if not (line.lstrip().startswith('File ') and 'devone-core.exe' in line))+'\n'
    bundled = generated / 'installer-bundled.nsi'
    bundled.write_text(script, encoding='utf-8-sig')
    nsis = Path.home() / 'AppData/Local/tauri/NSIS/makensis.exe'
    subprocess.run([str(nsis), '/V2', str(bundled)], cwd=generated, check=True)
    if not output.is_file():
        raise ValueError('Bundled installer was not created')


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--inputs', type=Path, help='Override pinned manifest/archive records')
    parser.add_argument('--installer', action='store_true')
    args = parser.parse_args()
    root = Path(__file__).resolve().parents[1]
    version = json.loads((root / 'package.json').read_text(encoding='utf-8'))['version']
    inputs = json.loads(args.inputs.read_text(encoding='utf-8')) if args.inputs else runtime_inputs(root)
    manifests = [item['manifest'] for item in inputs]
    if len(manifests) != 4 or {m['runtime'] for m in manifests} != {'php', 'mysql', 'caddy', 'node'}:
        raise ValueError('Bundle requires exactly one PHP, MySQL, Caddy and Node runtime')
    for item in inputs:
        manifest = item['manifest']
        if not re.fullmatch(r'[0-9]+(?:\.[0-9]+)+', manifest['version']):
            raise ValueError('Invalid bundled runtime version')
        for binary in manifest['binaries'].values():
            path = PurePosixPath(binary)
            if path.is_absolute() or '..' in path.parts or '\\' in binary or ':' in binary:
                raise ValueError('Invalid bundled runtime binary path')
        if item['manifest']['platform'] != 'windows-x64' or sha256(Path(item['archive'])) != item['manifest']['sha256']:
            raise ValueError('Runtime archive checksum/platform mismatch')
    home = Path(tempfile.mkdtemp(prefix='devone-bundled-release-')) / 'devone-local'
    home.mkdir()
    for name in ['www', 'runtimes', 'tools', 'database', 'config', 'certs', 'logs', 'backups', 'cache']:
        (home / name).mkdir()
    exe = root / 'src-tauri/target/x86_64-pc-windows-msvc/release/devone-local.exe'
    shutil.copy2(exe, home / exe.name)
    for item in inputs:
        manifest = item['manifest']
        extract_runtime(Path(item['archive']), manifest, home / 'runtimes' / manifest['runtime'] / manifest['version'])
    catalog = json.loads((root / 'src-tauri/assets/runtime-catalog.json').read_text())
    for manifest in manifests:
        catalog['manifests'] = [m for m in catalog['manifests'] if (m['runtime'], m['version'], m['platform']) != (manifest['runtime'], manifest['version'], manifest['platform'])]
        catalog['manifests'].append(manifest)
    catalog['revision'] = max(catalog['revision']+1, 2026100801)
    (home / 'config/runtime-catalog.json').write_text(json.dumps(catalog, indent=2)+'\n', encoding='utf-8')
    seed_registry(root, home, manifests)
    versions = ', '.join(f'{m["runtime"]} {m["version"]}' for m in manifests)
    (home / 'README.txt').write_text(f'''DEVONE Local - ชุดพร้อมใช้ Windows x64
Portable ZIP: แตกโฟลเดอร์ devone-local ไปไว้ที่ไหนก็ได้ที่เขียนไฟล์ได้ (เช่น D:\\ ห้าม Program Files) แล้วเปิด devone-local.exe
เลิกใช้ portable: ปิดแอป (Quit) แล้วลบโฟลเดอร์ได้เลย
Setup.exe: เลือกโฟลเดอร์ติดตั้งเอง ถอนการติดตั้งจาก Settings > Apps หรือ Control Panel > Programs
การถอนการติดตั้งลบตัวโปรแกรมและ runtimes ที่มากับชุด แต่เก็บ www, database, backups, certs และ devone.db ไว้
โปรเจกต์เก็บใน www โปรแกรมที่มาพร้อมชุดเก็บใน runtimes ข้อมูลฐานข้อมูลเก็บใน database
รวม {versions} แล้ว ไม่ต้องดาวน์โหลด runtimes เหล่านี้เพิ่ม
MySQL และ Node.js ใช้ LTS อย่างละหนึ่งเวอร์ชัน รุ่นอื่นเพิ่มภายหลังผ่านแอปได้
เมื่อเปิดครั้งแรกให้ทำขั้นตอนตั้งค่า DNS/HTTPS ในแอปให้ครบ
เครื่องต้องมี WebView2 Runtime และ Visual C++ Redistributable สำหรับรันแอปและ PHP/MySQL
ไม่มีฐานข้อมูลผู้ใช้ รหัสผ่าน หรือ CA private key ที่สร้างไว้จากเครื่องอื่น
ย้ายเฉพาะโปรเจกต์ของคุณมาไว้ใน www ของชุดนี้ได้
''', encoding='utf-8')
    release = root / 'release'
    names = release_names(version)
    # Build inside release/ so publishing is a same-volume rename; nothing in
    # release/ changes unless every artifact was produced.
    release.mkdir(exist_ok=True)
    staging = Path(tempfile.mkdtemp(prefix='.build-', dir=release))
    try:
        archive = staging / names['portable']
        with zipfile.ZipFile(archive, 'w', zipfile.ZIP_DEFLATED, compresslevel=6) as zipped:
            for item in sorted(home.rglob('*')):
                zipped.write(item, str(Path('devone-local') / item.relative_to(home)))
        with zipfile.ZipFile(archive) as zipped:
            if zipped.testzip() is not None:
                raise ValueError('Portable ZIP integrity check failed')
        print(f'Portable bundle built: {archive.name}', flush=True)
        common = {
            'version': version, 'target': 'x86_64-pc-windows-msvc',
            'runtimes': [{'runtime': m['runtime'], 'version': m['version']} for m in manifests],
            'executable_sha256': sha256(exe),
            'packagingAudit': {'fixtureFeature': False, 'projectDependenciesBundled': False,
                               'userDataBundled': False, 'runtimeArchivesVerified': True},
        }
        artifacts = [{'filename': archive.name, 'kind': 'portable-zip',
                      'bytes': archive.stat().st_size, 'sha256': sha256(archive), **common}]
        if args.installer:
            output = staging / names['installer']
            make_installer(root, home, manifests, output)
            print(f'Bundled installer built: {output.name}', flush=True)
            artifacts.insert(0, {'filename': output.name, 'kind': 'nsis-current-user-installer',
                                 'bytes': output.stat().st_size, 'sha256': sha256(output), **common})
        publish(release, staging, artifacts)
    finally:
        shutil.rmtree(staging, ignore_errors=True)
        shutil.rmtree(home.parent, ignore_errors=True)
    for artifact in artifacts:
        print(f'{release / artifact["filename"]} ({artifact["bytes"]} bytes) SHA-256 {artifact["sha256"]}', flush=True)


if __name__ == '__main__':
    main()
