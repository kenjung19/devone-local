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


def make_installer(root, home, output):
    generated = root / 'src-tauri/target/x86_64-pc-windows-msvc/release/nsis/x64'
    script = (generated / 'installer.nsi').read_text(encoding='utf-8-sig')
    # Full vendor runtimes are large; zlib keeps packaging time practical.
    script = re.sub(r'^  SetCompressor /SOLID "lzma"$', '  SetCompressor /SOLID "zlib"', script, flags=re.MULTILINE)
    hooks = home.parent / 'bundled-hooks.nsh'
    hook = ['!macro NSIS_HOOK_POSTINSTALL', '  SetOverwrite off']
    for name in ['www', 'runtimes', 'tools', 'database', 'config', 'certs', 'logs', 'backups', 'cache']:
        hook.append(f'  CreateDirectory "$INSTDIR\\{name}"')
    for item in sorted(home.rglob('*')):
        if not item.is_file() or item.name == 'devone-local.exe':
            continue
        relative = item.relative_to(home)
        parent = str(relative.parent) if relative.parent != Path('.') else ''
        hook.extend([f'  SetOutPath "$INSTDIR\\{parent}"', f'  File "{item}"'])
    hook.extend(['  SetOutPath "$INSTDIR"', '  SetOverwrite on',
                 '  DetailPrint "PHP, MySQL LTS, Caddy and Node LTS are included. Existing Home data is preserved."',
                 '!macroend', '!macro NSIS_HOOK_PREUNINSTALL',
                 '  DetailPrint "Projects, databases and runtimes are preserved."', '!macroend'])
    hooks.write_text('\n'.join(hook)+'\n', encoding='utf-8')
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
แตกโฟลเดอร์ devone-local ไปที่ D:\\ แล้วเปิด devone-local.exe ได้เลย
โปรเจกต์เก็บใน www โปรแกรมที่มาพร้อมชุดเก็บใน runtimes ข้อมูลฐานข้อมูลเก็บใน database
รวม {versions} แล้ว ไม่ต้องดาวน์โหลด runtimes เหล่านี้เพิ่ม
MySQL และ Node.js ใช้ LTS อย่างละหนึ่งเวอร์ชัน รุ่นอื่นเพิ่มภายหลังผ่านแอปได้
เมื่อเปิดครั้งแรกให้ทำขั้นตอนตั้งค่า DNS/HTTPS ในแอปให้ครบ
เครื่องต้องมี WebView2 Runtime และ Visual C++ Redistributable สำหรับรันแอปและ PHP/MySQL
ไม่มีฐานข้อมูลผู้ใช้ รหัสผ่าน หรือ CA private key ที่สร้างไว้จากเครื่องอื่น
ย้ายเฉพาะโปรเจกต์ของคุณมาไว้ใน www ของชุดนี้ได้
''', encoding='utf-8')
    report = {'home': str(home), 'runtimes': manifests, 'executable_sha256': sha256(exe)}
    release = root / 'release'
    archive = release / f'DEVONE-Local-{version}-Windows-x64-Portable.zip'
    with zipfile.ZipFile(archive, 'w', zipfile.ZIP_DEFLATED, compresslevel=6) as zipped:
        for item in sorted(home.rglob('*')):
            zipped.write(item, str(Path('devone-local') / item.relative_to(home)))
    with zipfile.ZipFile(archive) as zipped:
        if zipped.testzip() is not None:
            raise ValueError('Portable ZIP integrity check failed')
    report['portable'] = {'path': str(archive), 'bytes': archive.stat().st_size, 'sha256': sha256(archive)}
    (release / 'bundled-artifacts.json').write_text(json.dumps(report, indent=2)+'\n', encoding='utf-8')
    print(f'Portable bundle ready: {archive}', flush=True)
    if args.installer:
        output = release / f'DEVONE-Local-{version}-Windows-x64-Setup.exe'
        make_installer(root, home, output)
        report['installer'] = {'path': str(output), 'bytes': output.stat().st_size, 'sha256': sha256(output)}
        (release / 'bundled-artifacts.json').write_text(json.dumps(report, indent=2)+'\n', encoding='utf-8')
        print(f'Bundled installer ready: {output}', flush=True)
        metadata = [{
            'filename': output.name, 'version': version, 'target': 'x86_64-pc-windows-msvc',
            'installer': 'NSIS current-user with bundled runtimes',
            'bytes': output.stat().st_size, 'sha256': sha256(output),
            'runtimes': [{'runtime': m['runtime'], 'version': m['version']} for m in manifests],
            'packagingAudit': {'fixtureFeature': False, 'projectDependenciesBundled': False,
                               'userDataBundled': False, 'runtimeArchivesVerified': True},
        }]
        (release / 'release-artifacts.json').write_text(json.dumps(metadata, indent=2)+'\n', encoding='utf-8')
    print(json.dumps({'home': str(home), 'portable': report['portable'], 'installer': report.get('installer')}), flush=True)


if __name__ == '__main__':
    main()
