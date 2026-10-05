"""Verify corresponding sources after extraction without workspace fixtures.

Compiles the seven production transaction models and runs the independent TCP
driver. An optional backend is checked with dummy commands, never a probe.
"""
import argparse
import hashlib
import json
from pathlib import Path
import subprocess
import sys
import zipfile


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--archive', type=Path, required=True)
    parser.add_argument('--out', type=Path, required=True)
    parser.add_argument('--cc', default='gcc')
    parser.add_argument('--openocd', type=Path)
    args = parser.parse_args()
    archive, out = args.archive.resolve(), args.out.resolve()
    if out.exists():
        raise ValueError('Choose a new extraction directory')
    out.mkdir(parents=True)
    extracted = out / 'extracted'
    extracted.mkdir()
    fixture = 'tests/fixtures/register-vfp-write-board.example.json'
    here = Path(__file__).resolve().parents[1]
    with zipfile.ZipFile(archive) as bundle:
        if bundle.testzip() is not None:
            raise ValueError('Damaged source ZIP')
        if fixture not in bundle.namelist():
            raise ValueError('Source ZIP lacks the standalone driver fixture')
        # Execute only the current verified recipe, with its bundled fixture.
        for path in here.rglob('*'):
            if path.is_file() and '__pycache__' not in path.parts:
                name = 'recipe/openocd-adapter/' + path.relative_to(here).as_posix()
                if bundle.read(name) != path.read_bytes():
                    raise ValueError(f'Different build/test recipe: {name}')
        for name in bundle.namelist():
            if not (extracted / name).resolve().is_relative_to(extracted):
                raise ValueError(f'Unsafe ZIP member: {name}')
        bundle.extractall(extracted)
    recipe = extracted / 'recipe/openocd-adapter/test.py'
    command = [sys.executable, str(recipe), '--source', str(extracted / 'openocd-source'),
               '--out', str(out / 'tests'), '--cc', args.cc]
    if args.openocd:
        command += ['--openocd', str(args.openocd.resolve())]
    with (out / 'standalone.log').open('w', encoding='utf-8') as log:
        subprocess.run(command, cwd=extracted, stdout=log, stderr=subprocess.STDOUT, check=True)
    report = json.loads((out / 'tests/report.json').read_text(encoding='utf-8'))
    fields = ['transaction_passed', 'banked_transaction_passed', 'vfp_transaction_passed',
              'vfp_write_transaction_passed', 'timer_transaction_passed',
              'pmu_transaction_passed', 'gic_transaction_passed', 'vfp_write_deferred_driver_passed']
    assert all(report[field] for field in fields)
    assert not report['board_tests_executed']
    result = {'archive': str(archive), 'archive_sha256': hashlib.sha256(archive.read_bytes()).hexdigest(),
              'extracted_root': str(extracted), 'workspace_fixture_used': False,
              'bundled_fixture_present': True, 'production_models_passed': 7,
              'independent_driver_passed': True, 'backend_commands_passed': report['backend_commands_passed'],
              'board_tests_executed': False, 'passed': True}
    (out / 'report.json').write_text(json.dumps(result, indent=2) + '\n', encoding='utf-8')
    print(json.dumps(result))


if __name__ == '__main__':
    main()
