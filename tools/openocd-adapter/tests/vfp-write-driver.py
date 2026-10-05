"""Run the deferred driver over real TCP with a strict target/bit fixture."""
import argparse
import importlib.util
import json
from pathlib import Path
import re
import socketserver
import threading
import types
import unittest
from unittest.mock import patch

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[2]
module = importlib.util.spec_from_file_location('vfp_hardware', HERE / 'vfp-write-hardware.py')
hardware = importlib.util.module_from_spec(module)
module.loader.exec_module(hardware)
OWNER_BEFORE = int('fedcba987654320ffedcba987654320e', 16)
PEER = int('123456789abcdef00123456789abcdef', 16)


class Fixture(socketserver.ThreadingTCPServer):
    allow_reuse_address = True
    daemon_threads = True

    def __init__(self, mode='verified'):
        super().__init__(('127.0.0.1', 0), Handler)
        self.mode = mode
        self.owner = OWNER_BEFORE
        self.selected = 'board.cpu1'
        self.commands = []
        self.writes = []
        self.error = None

    def execute(self, script):
        assert script.startswith('set __dt_saved [target current]; set __dt_code [catch {targets ')
        assert 'set __dt_restore [catch {targets $__dt_saved}' in script
        assert 'if {$__dt_restore || [target current] ne $__dt_saved}' in script
        found = re.search(r'targets (board\.cpu[01]); if \{\[\[target current\] curstate\] ne "halted"\} \{error "target not halted"\}; (.*?)\} __dt_result\]', script)
        assert found, 'No exact halted target guard'
        target, body = found.groups()
        saved = self.selected
        self.selected = target
        self.commands.append((target, body))
        try:
            if body == 'aarch64 debugtui_adapter':
                return hardware.ADAPTER_PROTOCOL
            if body == 'aarch64 debugtui_vfp_protocol':
                return hardware.READ_PROTOCOL
            if body == 'aarch64 debugtui_vfp_write_protocol':
                return hardware.PROTOCOL
            if body == 'aarch64 mrc 15 3 0 4 5':
                return '0xa200041a'
            if body == 'aarch64 mrc 15 4 2 1 1':
                return '0x00000000'
            if re.fullmatch(r'aarch64 vfp [dq]\d+', body):
                raw = self.owner if target == 'board.cpu0' else PEER
                return f'mvfr0 0x10110222 mvfr1 0x12111111 fpexc 0x40000700 value 0x{raw:032x}'
            if body == 'aarch64 vfp fpscr':
                return 'mvfr0 0x10110222 mvfr1 0x12111111 fpexc 0x40000700 value 0xa000009f'
            found = re.fullmatch(r'aarch64 vfp_write ([sdq]\d+) (0x[0-9a-fA-F]+)', body)
            assert found, 'Unexpected command: ' + body
            assert target == 'board.cpu0', 'No write to peer permitted'
            name, raw = found.groups()
            if self.mode == 'refused':
                return 'outcome not_sent reason pending-register-write'
            before = self.owner
            if name == 's31':
                assert len(raw) == 10
                self.owner = (before & ((1 << 96) - 1)) | (int(raw, 16) << 96)
            elif name == 'd14':
                assert len(raw) == 18
                self.owner = (before & (((1 << 64) - 1) << 64)) | int(raw, 16)
            elif name == 'q7':
                assert len(raw) == 34
                self.owner = int(raw, 16)
            else:
                raise AssertionError('Fixture register unsupported')
            self.writes.append((target, name, raw))
            if self.mode == 'unknown':
                return 'Core state restoration failed: VFP write outcome unknown; reconnect required'
            expected = self.owner
            outcome = 'verified'
            if self.mode == 'mismatch':
                self.owner ^= 1
                outcome = 'mismatch'
            return f'outcome {outcome} before 0x{before:032x} expected 0x{expected:032x} value 0x{self.owner:032x} mvfr0 0x10110222 mvfr1 0x12111111 fpexc 0x40000700'
        finally:
            self.selected = saved


class Handler(socketserver.BaseRequestHandler):
    def handle(self):
        buffer = bytearray()
        while data := self.request.recv(65536):
            buffer.extend(data)
            while b'\x1a' in buffer:
                script, remaining = buffer.split(b'\x1a', 1)
                buffer = bytearray(remaining)
                try:
                    result = self.server.execute(script.decode('ascii'))
                except Exception as error:
                    self.server.error = str(error)
                    result = 'fixture unexpected command'
                self.request.sendall(result.encode('ascii') + b'\x1a')


class Cases(unittest.TestCase):
    def run_fixture(self, name='s31', raw='0x7fa12345', mode='verified'):
        out = OUTPUT / (self._testMethodName + '-' + name)
        out.mkdir(parents=True, exist_ok=True)
        fixture = Fixture(mode)
        thread = threading.Thread(target=fixture.serve_forever, kwargs={'poll_interval': 0.02}, daemon=True)
        thread.start()
        spec = json.loads((ROOT / 'tests/fixtures/register-vfp-write-board.example.json').read_text())
        spec.update(register=name, raw=raw)
        spec['endpoint']['port'] = fixture.server_address[1]
        case = out / 'case.json'
        case.write_text(json.dumps(spec))
        try:
            report = hardware.run_case(types.SimpleNamespace(out=out, run=True, software_fixture=True, case=case))
        finally:
            fixture.shutdown()
            fixture.server_close()
            thread.join(timeout=5)
        self.assertIsNone(fixture.error)
        self.assertFalse(report['board_tests_executed'])
        self.assertEqual(fixture.selected, 'board.cpu1')
        self.assertEqual(len(report['results']), 4)
        return report, fixture

    def test_all_widths_restore_and_single_owner(self):
        for name, raw in [('s31', '0x7fa12345'), ('d14', '0xfff0123456789abc'),
                          ('q7', '0x8123456789abcdef7ff0123456789abc')]:
            report, fixture = self.run_fixture(name, raw)
            self.assertFalse(report['failed'])
            self.assertTrue(all(item['state'] == 'passed' for item in report['results']))
            self.assertEqual(len(fixture.writes), 2)
            self.assertEqual(fixture.owner, OWNER_BEFORE)
            self.assertTrue(all(target == 'board.cpu0' for target, _, _ in fixture.writes))

    def test_unknown_never_reads_retries_or_restores_after_write(self):
        report, fixture = self.run_fixture(mode='unknown')
        self.assertTrue(report['failed'])
        self.assertEqual(len(fixture.writes), 1)
        self.assertTrue(fixture.commands[-1][1].startswith('aarch64 vfp_write '))
        self.assertEqual([item['state'] for item in report['results']], ['passed', 'failed', 'skipped', 'skipped'])

    def test_mismatch_is_not_restored_or_retried(self):
        report, fixture = self.run_fixture(mode='mismatch')
        self.assertTrue(report['failed'])
        self.assertEqual(len(fixture.writes), 1)
        self.assertTrue(fixture.commands[-1][1].startswith('aarch64 vfp_write '))

    def test_pending_cache_refusal_never_replayed(self):
        report, fixture = self.run_fixture(mode='refused')
        self.assertTrue(report['failed'])
        self.assertEqual(fixture.writes, [])
        self.assertEqual(fixture.owner, OWNER_BEFORE)
        self.assertEqual(sum('vfp_write s' in body for _, body in fixture.commands), 1)

    def test_default_and_invalid_input_never_connect(self):
        with patch.object(hardware.socket, 'create_connection', side_effect=AssertionError('Must not connect')):
            out = OUTPUT / 'default'
            report = hardware.run_case(types.SimpleNamespace(out=out, run=False, software_fixture=False, case=None))
            self.assertFalse(report['failed'])
            self.assertTrue(all(item['state'] == 'skipped' for item in report['results']))
            spec = json.loads((ROOT / 'tests/fixtures/register-vfp-write-board.example.json').read_text())
            spec['raw'] = '0x1234567;'
            case = out / 'bad-case.json'
            case.write_text(json.dumps(spec))
            report = hardware.run_case(types.SimpleNamespace(out=OUTPUT / 'invalid', run=True, software_fixture=True, case=case))
            self.assertTrue(report['failed'])
            self.assertIn('Exact raw', report['results'][0]['detail'])


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--out', type=Path, required=True)
    args = parser.parse_args()
    OUTPUT = args.out.resolve()
    OUTPUT.mkdir(parents=True, exist_ok=True)
    result = unittest.TextTestRunner(verbosity=2).run(unittest.defaultTestLoader.loadTestsFromTestCase(Cases))
    report = {'board_tests_executed': False, 'tests_run': result.testsRun,
              'failures': len(result.failures), 'errors': len(result.errors), 'passed': result.wasSuccessful()}
    (OUTPUT / 'report.json').write_text(json.dumps(report, indent=2) + '\n', encoding='utf-8')
    raise SystemExit(0 if result.wasSuccessful() else 1)
