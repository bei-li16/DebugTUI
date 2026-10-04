"""Stage and verify a pinned Windows OpenOCD candidate without a physical probe."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import zipfile


HERE = Path(__file__).resolve().parent


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def read_json(path):
    return json.loads(path.read_text(encoding='utf-8'))


def write_json(path, value):
    path.write_text(json.dumps(value, indent=2, sort_keys=True) + '\n', encoding='utf-8')


def checked(command, **kwargs):
    return subprocess.check_output(command, text=True, stderr=subprocess.STDOUT, **kwargs)


def manifest(directory):
    return {path.relative_to(directory).as_posix(): {'bytes': path.stat().st_size, 'sha256': digest(path)}
            for path in sorted(directory.rglob('*'))
            if path.is_file() and path != directory / 'PROVENANCE.json'}


def check_manifest(directory, expected):
    if manifest(directory) != expected:
        raise ValueError('Staged files changed, disappeared or were added after the build')


def dll_closure(directory, objdump, system_dlls):
    """Inspect every shipped PE before execution; never resolve DLLs from PATH."""
    binaries = [directory / 'openocd.exe'] + sorted(directory.glob('*.dll'))
    available = {path.name.lower() for path in binaries}
    dependencies = {}
    for binary in binaries:
        if 'file format pei-x86-64' not in checked([objdump, '-f', str(binary)]):
            raise ValueError(f'Not a Windows x64 PE: {binary.name}')
        imports = [name.lower() for name in re.findall(r'DLL Name:\s*(\S+)',
                   checked([objdump, '-p', str(binary)]))]
        missing = set(imports) - available - set(system_dlls)
        if missing:
            raise ValueError(f'Missing or unapproved dependency of {binary.name}: {sorted(missing)}')
        dependencies[binary.name] = imports
    return dependencies


def zip_directory(directory, archive):
    with zipfile.ZipFile(archive, 'w', zipfile.ZIP_DEFLATED) as bundle:
        for path in sorted(directory.rglob('*')):
            if path.is_file():
                bundle.write(path, path.relative_to(directory).as_posix())


def check_source_archive(archive, lock):
    with zipfile.ZipFile(archive) as bundle:
        if bundle.testzip() is not None:
            raise ValueError('Corresponding source ZIP is damaged')
        for name, expected in lock['patched_sources_sha256'].items():
            data = bundle.read('openocd-source/' + name)
            if hashlib.sha256(data).hexdigest() != expected:
                raise ValueError(f'Corresponding source ZIP contains a different adapter: {name}')
        patch = bundle.read('recipe/openocd-adapter/' + lock['patch'])
        if hashlib.sha256(patch).hexdigest() != lock['patch_sha256']:
            raise ValueError('Corresponding source ZIP contains a different patch')
        deps = read_json(HERE / 'windows-dependencies.lock.json')
        pins = [('openocd-source', lock['revision']),
                ('openocd-source/jimtcl', lock['jimtcl_revision']),
                ('openocd-source/src/jtag/drivers/libjaylink', deps['libjaylink']['revision']),
                ('hidapi-0.15.0', deps['hidapi']['revision'])]
        members = set(bundle.namelist())
        for prefix, revision in pins:
            # Detached shallow repositories often have empty refs directories.
            # Git refuses to recognize them if an archive silently omits refs.
            if prefix + '/.git/refs/' not in members:
                raise ValueError(f'Corresponding Git cache lost its refs directory: {prefix}')
            if bundle.read(prefix + '/.git/HEAD').decode('ascii').strip() != revision:
                raise ValueError(f'Corresponding Git cache lost its pinned identity: {prefix}')


def stage(args):
    root = args.root.resolve()
    source = root / 'source'
    install = root / 'install'
    lock = read_json(HERE / 'source.lock.json')
    deps = read_json(HERE / 'windows-dependencies.lock.json')
    for checkout, revision in [(source, lock['revision']), (source / 'jimtcl', lock['jimtcl_revision']),
                               (source / 'src/jtag/drivers/libjaylink', deps['libjaylink']['revision']),
                               (root / 'hidapi', deps['hidapi']['revision'])]:
        if checked(['git', '-C', str(checkout), 'rev-parse', 'HEAD']).strip() != revision:
            raise ValueError(f'Unexpected source revision: {checkout}')
    for name, expected in lock['patched_sources_sha256'].items():
        if digest(source / name) != expected:
            raise ValueError(f'Patched source changed: {name}')
    if digest(HERE / lock['patch']) != lock['patch_sha256']:
        raise ValueError('Adapter patch changed')
    archive = root / ('libusb-' + deps['libusb']['version'] + '.tar.bz2')
    if digest(archive) != deps['libusb']['sha256']:
        raise ValueError('libusb source archive changed')
    config = (root / 'build/config.h').read_text(encoding='utf-8')
    for define in deps['required_defines']:
        if not re.search(r'^#define ' + re.escape(define) + r' 1$', config, re.MULTILINE):
            raise ValueError(f'Required probe channel was not compiled: {define}')
    compiler = Path(shutil.which(args.cross + 'gcc') or '').resolve()
    copyright_root = compiler.parent.parent / 'share/doc'
    mingw_license = args.mingw_license or copyright_root / 'mingw-w64-common/copyright'
    gcc_license = args.gcc_license or copyright_root / 'gcc-mingw-w64-base/copyright'
    licenses = install / 'licenses'
    licenses.mkdir(exist_ok=True)
    notices = {
        'COPYING.OpenOCD': source / 'COPYING',
        'LICENSE.JimTcl': source / 'jimtcl/LICENSE',
        'LICENSE.Tcl': source / 'jimtcl/tcl.license.terms',
        'COPYING.LibJaylink': source / 'src/jtag/drivers/libjaylink/COPYING',
        'COPYING.LibUSB': root / ('libusb-' + deps['libusb']['version']) / 'COPYING',
        'LICENSE.HIDAPI': root / 'hidapi/LICENSE.txt',
        'LICENSE.HIDAPI.BSD': root / 'hidapi/LICENSE-bsd.txt',
        'LICENSE.HIDAPI.Original': root / 'hidapi/LICENSE-orig.txt',
        'LICENSE.HIDAPI.GPL3': root / 'hidapi/LICENSE-gpl3.txt',
        'COPYRIGHT.MinGW': mingw_license,
        'COPYRIGHT.GCC': gcc_license,
    }
    for name, original in notices.items():
        shutil.copyfile(original, licenses / name)
    profiles = install / 'config'
    profiles.mkdir(exist_ok=True)
    for name in ['stm32f429-live.cfg', 'stm32f429-dap.cfg']:
        shutil.copyfile(HERE.parent / 'config' / name, profiles / name)
    for binary in [install / 'bin/openocd.exe'] + list((install / 'bin').glob('*.dll')):
        subprocess.run([args.cross + 'strip', '--strip-unneeded', str(binary)], check=True)
    dependencies = dll_closure(install / 'bin', args.cross + 'objdump', deps['system_dlls'])
    sources_zip = root / 'corresponding-source.zip'
    # Include complete checkouts with their local object databases, so cached
    # rebuilding remains possible without network or an external source offer.
    with zipfile.ZipFile(sources_zip, 'w', zipfile.ZIP_DEFLATED) as bundle:
        for checkout, prefix in [(source, 'openocd-source'), (root / 'hidapi', 'hidapi-0.15.0')]:
            for path in sorted(checkout.rglob('*')):
                if (path.is_file() or path.is_dir()) and not set(path.relative_to(checkout).parts) & {'autom4te.cache', '.deps', '.libs'}:
                    bundle.write(path, prefix + '/' + path.relative_to(checkout).as_posix())
        bundle.write(archive, archive.name)
        for path in sorted(HERE.rglob('*')):
            if path.is_file() and '__pycache__' not in path.parts:
                bundle.write(path, 'recipe/openocd-adapter/' + path.relative_to(HERE).as_posix())
        for name in ['stm32f429-live.cfg', 'stm32f429-dap.cfg']:
            bundle.write(HERE.parent / 'config' / name, 'recipe/config/' + name)
    record = {
        'format': 1, 'target': deps['target'], 'board_tests_executed': False,
        'native_windows_verified': False, 'protocol': lock['protocol'],
        'banked_protocol': lock['banked_protocol'],
        'source_revision': lock['revision'], 'jimtcl_revision': lock['jimtcl_revision'],
        'patch_sha256': lock['patch_sha256'], 'dependency_sources': deps,
        'compiler_version': checked([str(compiler), '--version']).strip(),
        'compiler_sha256': digest(compiler), 'pe_dependencies': dependencies,
        'corresponding_source': {'name': sources_zip.name, 'sha256': digest(sources_zip)},
        'recipe_sha256': {name: digest(HERE / name) for name in
                          ['build-windows.sh', 'windows.py', 'test.py', 'source.lock.json',
                           'windows-dependencies.lock.json', lock['patch']]},
        'files': manifest(install),
        'limitations': ['No physical probe, ARM instruction execution or board support was verified.'],
    }
    write_json(install / 'PROVENANCE.json', record)
    zip_directory(install, root / 'openocd-windows-x64-candidate.zip')
    print(json.dumps({'backend_sha256': digest(install / 'bin/openocd.exe'),
                      'dll_closure': dependencies, 'native_windows_verified': False}))


def verify(args):
    if os.name != 'nt':
        raise ValueError('Native verification must run from Windows Python')
    root = args.root.resolve()
    install = root / 'install'
    record = read_json(install / 'PROVENANCE.json')
    deps = read_json(HERE / 'windows-dependencies.lock.json')
    lock = read_json(HERE / 'source.lock.json')
    if record['source_revision'] != lock['revision'] or record['patch_sha256'] != lock['patch_sha256']:
        raise ValueError('Candidate does not correspond to the current source lock')
    if record.get('banked_protocol') != lock['banked_protocol']:
        raise ValueError('Candidate does not correspond to the current banked protocol')
    if record['dependency_sources'] != deps:
        raise ValueError('Candidate does not correspond to the current dependency lock')
    for name, expected in record['recipe_sha256'].items():
        if digest(HERE / name) != expected:
            raise ValueError(f'Build recipe changed; regenerate the staged package: {name}')
    check_manifest(install, record['files'])
    if digest(root / record['corresponding_source']['name']) != record['corresponding_source']['sha256']:
        raise ValueError('Corresponding source package changed or disappeared')
    check_source_archive(root / record['corresponding_source']['name'], lock)
    dependencies = dll_closure(install / 'bin', args.objdump, deps['system_dlls'])
    if dependencies != record['pe_dependencies']:
        raise ValueError('Dependency imports changed')
    system_root = Path(os.environ['SystemRoot']) / 'System32'
    for name in {name for imports in dependencies.values() for name in imports} & set(deps['system_dlls']):
        if not (system_root / name).is_file():
            raise ValueError(f'Required Windows component missing: {name}')
    out = root / 'windows-tests'
    out.mkdir(exist_ok=True)
    backend = install / 'bin/openocd.exe'
    subprocess.run([sys.executable, str(HERE / 'test.py'), '--source', str(root / 'source'),
                    '--out', str(out), '--cc', args.cc, '--openocd', str(backend)], check=True)
    # These commands finish during configuration, before implicit init. Do not
    # select/init a physical driver while inspecting its registration.
    listed = checked([str(backend), '-c', 'puts [adapter list]; shutdown'], timeout=30)
    adapters = re.findall(r'^(\S+)\s+\{', listed, re.MULTILINE)
    if not set(deps['required_adapters']) <= set(adapters):
        raise ValueError(f'Missing native probe drivers: {adapters}')
    (out / 'adapter-list.log').write_text(listed, encoding='utf-8')
    scripts = install / 'share/openocd/scripts'
    cases = {name: ['-f', str(install / 'config' / name)] for name in
             ['stm32f429-live.cfg', 'stm32f429-dap.cfg']}
    cases['cmsis-dap-both-backends'] = ['-c', 'adapter driver cmsis-dap; cmsis-dap backend hid; cmsis-dap backend usb_bulk']
    for name, configuration in cases.items():
        # Explicit init in a future profile is also rejected before access.
        output = checked([str(backend), '-s', str(scripts), '-c',
                          'rename init dt_real_init; proc init {args} {error "physical init prohibited"}',
                          *configuration, '-c', 'gdb port disabled; tcl port disabled; telnet port disabled; puts "PASS: offline profile"; shutdown'], timeout=30)
        if 'PASS: offline profile' not in output:
            raise ValueError(f'Offline profile did not finish: {name}')
        (out / (name + '.log')).write_text(output, encoding='utf-8')
    record.update(native_windows_verified=True, native_adapter_list=adapters,
                  native_profiles_verified=list(cases),
                  native_backend_test=read_json(out / 'report.json'))
    write_json(install / 'PROVENANCE.json', record)
    zip_directory(install, root / 'openocd-windows-x64-candidate.zip')
    print(json.dumps({'native_windows_verified': True, 'board_tests_executed': False,
                      'backend_sha256': digest(backend), 'profiles': list(cases)}))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest='command', required=True)
    staging = sub.add_parser('stage')
    staging.add_argument('--root', type=Path, required=True)
    staging.add_argument('--cross', default='x86_64-w64-mingw32-')
    staging.add_argument('--mingw-license', type=Path)
    staging.add_argument('--gcc-license', type=Path)
    native = sub.add_parser('verify')
    native.add_argument('--root', type=Path, required=True)
    native.add_argument('--cc', default='gcc')
    native.add_argument('--objdump', default='objdump')
    args = parser.parse_args()
    {'stage': stage, 'verify': verify}[args.command](args)


if __name__ == '__main__':
    main()
