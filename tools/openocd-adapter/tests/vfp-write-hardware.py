"""Deferred physical S/D/Q writer cases. Default execution never connects.

Uses the independent OpenOCD writer; DebugTUI integration is checked separately.
No reset, halt, resume, mode/FPU enable, retry or uncertain-write rollback.
"""
import argparse
import hashlib
import json
from pathlib import Path
import re
import socket
import time

PROTOCOL = 'debugtui-armv8-vfp-write-2 external-identity current-el dspsr dlr vmov raw-pair fresh-merge scratch-readback no-enable stop-on-fault'
READ_PROTOCOL = 'debugtui-armv8-vfp-2 external-identity current-el dspsr dlr vmrs pair-readback no-enable stop-on-fault'
ADAPTER_PROTOCOL = 'debugtui-armv8-1 mrrc isb scratch-readback stop-on-fault'
PHASES = ['VFP-WH01-STOP', 'VFP-WH02-WRITE', 'VFP-WH03-UNCHANGED', 'VFP-WH04-RESTORE']


def exact(raw, bits):
    assert isinstance(raw, str) and re.fullmatch(r'0x[0-9a-fA-F]{%d}' % (bits // 4), raw), 'Exact raw hexadecimal width required'
    return int(raw, 16)


def parse_pairs(text):
    words = text.split()
    assert len(words) % 2 == 0, 'Malformed adapter response'
    assert len(set(words[::2])) == len(words[::2]), 'Duplicate response keys'
    return dict(zip(words[::2], words[1::2]))


class Tcl:
    def __init__(self, host, port, target):
        self.socket = socket.create_connection((host, port), timeout=5)
        self.socket.settimeout(5)
        self.target = target
        self.log = []

    def execute(self, body, target=None):
        target = target or self.target
        # Target identifiers have already been validated as simple Tcl words.
        script = ('set __dt_saved [target current]; '
                  f'set __dt_code [catch {{targets {target}; '
                  'if {[[target current] curstate] ne "halted"} {error "target not halted"}; '
                  f'{body}}} __dt_result]; '
                  'set __dt_restore [catch {targets $__dt_saved} __dt_restore_result]; '
                  'if {$__dt_restore || [target current] ne $__dt_saved} {error "target selection restoration failed"}; '
                  'if {$__dt_code} {error $__dt_result}; set __dt_result')
        self.log.append({'target': target, 'body': body})
        self.socket.sendall(script.encode('ascii') + b'\x1a')
        buffer = bytearray()
        while b'\x1a' not in buffer:
            data = self.socket.recv(65536)
            if not data:
                raise RuntimeError('Connection closed; transaction result unknown')
            buffer.extend(data)
            if len(buffer) > 1048576:
                raise RuntimeError('Oversized response; transaction result unknown')
        response, trailing = buffer.split(b'\x1a', 1)
        assert not trailing, 'Unexpected second response'
        return response.decode('ascii').strip()

    def close(self):
        self.socket.close()


PROOF_KEYS = {'midr', 'dscr', 'dspsr', 'dlr', 'hcptr'}
STATE_MASK = 0x00053f00
def checked_proof(result):
    for key in PROOF_KEYS:
        exact(result[key], 32)
    dscr = exact(result['dscr'], 32)
    assert exact(result['midr'], 32) & 0xff0ffff0 == 0x410fd130, 'R52 physical identity required'
    assert (dscr >> 8) & 3 == 2 and dscr & (1 << 24), 'Current Debug EL2 and ITE required'
    assert not dscr & (0x1c0000c0 | (1 << 12) | (1 << 16)), 'Invalid Debug execution state or fault'
    assert not exact(result['hcptr'], 32) & (1 << 10), 'HCPTR.TCP10 restricts FP access'


def snapshot_signature(sample):
    # DTR full/empty and other transport bits may vary between transactions.
    # Each raw EDSCR was already checked for ITE, faults, AArch32 and current EL.
    stable = dict(sample)
    stable['dscr'] = exact(sample['dscr'], 32) & STATE_MASK
    return stable


def same_evidence(left, right, key):
    if key == 'dscr':
        return ((exact(left[key], 32) ^ exact(right[key], 32)) & STATE_MASK) == 0
    return left[key] == right[key]

def snapshot(client, name, target=None):
    reader = name if name[0] != 's' else 'd' + str(int(name[1:]) // 2)
    result = parse_pairs(client.execute('aarch64 vfp ' + reader, target))
    assert set(result) == {'mvfr0', 'mvfr1', 'fpexc', 'value'} | PROOF_KEYS
    checked_proof(result)
    for key in ['mvfr0', 'mvfr1', 'fpexc']:
        exact(result[key], 32)
    exact(result['value'], 128)
    fpscr = parse_pairs(client.execute('aarch64 vfp fpscr', target))
    assert set(fpscr) == {'mvfr0', 'mvfr1', 'fpexc', 'value'} | PROOF_KEYS
    checked_proof(fpscr)
    for key in PROOF_KEYS | {'mvfr0', 'mvfr1', 'fpexc'}:
        assert same_evidence(fpscr, result, key), 'Physical state changed between samples'
    result['fpscr'] = fpscr['value']
    exact(result['fpscr'], 32)
    return result


def merged(before, name, raw):
    bits = 32 if name[0] == 's' else 64 if name[0] == 'd' else 128
    shift = (int(name[1:]) % 4) * 32 if bits == 32 else (int(name[1:]) % 2) * 64 if bits == 64 else 0
    mask = ((1 << bits) - 1) << shift
    return (before & ~mask) | (exact(raw, bits) << shift)


def checked_write(client, name, raw, before):
    # From this send onwards any malformed/error/timeout result stops the case.
    # A fixed not_sent response may be reported, but is never replayed.
    result = parse_pairs(client.execute(f'aarch64 vfp_write {name} {raw}'))
    if result.get('outcome') == 'not_sent':
        raise RuntimeError('Writer refused before sending: ' + result.get('reason', 'unknown'))
    assert set(result) == {'outcome', 'before', 'expected', 'value', 'mvfr0', 'mvfr1', 'fpexc'} | PROOF_KEYS, 'Write response unknown; reconnect before any further instruction'
    assert result['outcome'] == 'verified', 'Write was not verified; stop without retry or restore'
    expected = merged(exact(before['value'], 128), name, raw)
    assert exact(result['before'], 128) == exact(before['value'], 128), 'Physical baseline changed during write'
    assert exact(result['expected'], 128) == expected and exact(result['value'], 128) == expected, 'Independent pair calculation mismatch'
    checked_proof(result)
    for key in PROOF_KEYS | {'mvfr0', 'mvfr1', 'fpexc'}:
        assert same_evidence(result, before, key), 'Physical/control evidence changed'
    return result


def run_case(args):
    out = args.out.resolve()
    out.mkdir(parents=True, exist_ok=True)
    report = {'board_tests_executed': False, 'results': [],
              'layer': 'software fixture' if args.software_fixture else 'physical backend' if args.run else 'deferred',
              'limitations': ['This backend driver does not exercise DebugTUI preview/apply/UI; use the separate host driver.',
                              'Physical R0/R1 restoration is checked internally; external PC/GPR and independent firmware samples are additional manual cases.']}
    client = None
    failed = None
    current = PHASES[0]
    try:
        if not args.run:
            report['results'] = [{'id': phase, 'state': 'skipped', 'detail': 'Requires explicit --run --case and a dedicated paused core-private fixture'} for phase in PHASES]
            return report
        assert args.case, '--run requires --case JSON'
        spec = json.loads(args.case.read_text(encoding='utf-8'))
        assert not spec.get('software_example') or args.software_fixture, 'Replace all software example expectations before board execution'
        report.update(case_sha256=hashlib.sha256(args.case.read_bytes()).hexdigest(), evidence_source=spec['evidence_source'])
        assert spec['evidence_source'] and spec['fixture_core_private'] is True and spec['restore_on_verified'] is True
        for key in ['target', 'peer_target']:
            assert re.fullmatch(r'[A-Za-z_][A-Za-z0-9_.:-]{0,127}', spec[key]), 'Target must be a simple Tcl identifier'
        assert spec['target'] != spec['peer_target'], 'Use distinct physical owner and peer'
        name = spec['register']
        assert re.fullmatch(r'[sdq](0|[1-9][0-9]?)', name)
        assert int(name[1:]) < (16 if name[0] == 'q' else 32)
        bits = 32 if name[0] == 's' else 64 if name[0] == 'd' else 128
        exact(spec['raw'], bits)
        exact(spec['expected_before'], 128)
        exact(spec['expected_peer'], 128)
        endpoint = spec['endpoint']
        assert isinstance(endpoint['host'], str) and re.fullmatch(r'[A-Za-z0-9_.:-]{1,253}', endpoint['host'])
        assert isinstance(endpoint['port'], int) and 1 <= endpoint['port'] <= 65535
        client = Tcl(endpoint['host'], endpoint['port'], spec['target'])
        report['board_tests_executed'] = not args.software_fixture
        for command, expected in [('aarch64 debugtui_adapter', ADAPTER_PROTOCOL),
                                  ('aarch64 debugtui_vfp_protocol', READ_PROTOCOL),
                                  ('aarch64 debugtui_vfp_write_protocol', PROTOCOL)]:
            assert client.execute(command) == expected, 'Backend protocol unsupported'
        before = snapshot(client, name)
        peer = snapshot(client, name, spec['peer_target'])
        assert exact(before['value'], 128) == exact(spec['expected_before'], 128)
        assert exact(peer['value'], 128) == exact(spec['expected_peer'], 128)
        report['results'].append({'id': current, 'state': 'passed', 'before': before, 'peer': peer})
        current = PHASES[1]
        written = checked_write(client, name, spec['raw'], before)
        report['results'].append({'id': current, 'state': 'passed', 'write': written})
        current = PHASES[2]
        after = snapshot(client, name)
        assert exact(after['value'], 128) == merged(exact(before['value'], 128), name, spec['raw'])
        assert {k: v for k, v in snapshot_signature(after).items() if k != 'value'} == {k: v for k, v in snapshot_signature(before).items() if k != 'value'}
        assert snapshot_signature(snapshot(client, name, spec['peer_target'])) == snapshot_signature(peer), 'Peer was changed'
        report['results'].append({'id': current, 'state': 'passed', 'after': after, 'peer_unchanged': True})
        current = PHASES[3]
        shift = (int(name[1:]) % 4) * 32 if bits == 32 else (int(name[1:]) % 2) * 64 if bits == 64 else 0
        original = (exact(before['value'], 128) >> shift) & ((1 << bits) - 1)
        restored = checked_write(client, name, f'0x{original:0{bits // 4}x}', after)
        assert snapshot_signature(snapshot(client, name)) == snapshot_signature(before), 'Explicit verified restoration mismatch'
        assert snapshot_signature(snapshot(client, name, spec['peer_target'])) == snapshot_signature(peer)
        report['results'].append({'id': current, 'state': 'passed', 'write': restored})
    except Exception as error:
        failed = str(error) or type(error).__name__
        report['results'].append({'id': current, 'state': 'failed', 'detail': failed})
    finally:
        if client:
            report['transactions'] = client.log
            client.close() # Closing the owned socket injects no target instruction.
        for phase in PHASES:
            if not any(item['id'] == phase for item in report['results']):
                report['results'].append({'id': phase, 'state': 'skipped', 'detail': 'Earlier phase failed; no retry or rollback'})
        report['failed'] = bool(failed)
        (out / 'report.json').write_text(json.dumps(report, indent=2) + '\n', encoding='utf-8')
    return report


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--run', action='store_true')
    parser.add_argument('--software-fixture', action='store_true')
    parser.add_argument('--case', type=Path)
    parser.add_argument('--out', type=Path, default=Path('artifacts') / ('vfp-write-hardware-' + str(time.time_ns())))
    args = parser.parse_args()
    report = run_case(args)
    print(json.dumps({'out': str(args.out.resolve()), 'failed': report['failed'],
                      'board_tests_executed': report['board_tests_executed'], 'results': report['results']}))
    raise SystemExit(1 if report['failed'] else 0)


if __name__ == '__main__':
    main()
