"""Compile the actual pinned adapter transaction and optionally check OpenOCD.

Initializes only the in-process dummy adapter; no target examination, physical
adapter connection, halt, resume or board access.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--source', required=True, type=Path)
    parser.add_argument('--out', required=True, type=Path)
    parser.add_argument('--cc', default='gcc')
    parser.add_argument('--openocd', type=Path)
    args = parser.parse_args()
    here = Path(__file__).resolve().parent
    source = args.source.resolve()
    out = args.out.resolve()
    out.mkdir(parents=True, exist_ok=True)
    lock = json.loads((here/'source.lock.json').read_text(encoding='utf-8'))
    revision = subprocess.check_output(['git', '-C', str(source), 'rev-parse', 'HEAD'], text=True).strip()
    assert revision == lock['revision'], 'Unverified source revision'
    jim_revision = subprocess.check_output(['git', '-C', str(source/'jimtcl'), 'rev-parse', 'HEAD'], text=True).strip()
    assert jim_revision == lock['jimtcl_revision'], 'Unverified Jim Tcl revision'
    assert digest(here/lock['patch']) == lock['patch_sha256'], 'Patch checksum mismatch'
    for name, expected in lock['patched_sources_sha256'].items():
        assert digest(source/name) == expected, f'Patched source checksum mismatch: {name}'
    executable = out/('mrrc-transfer.exe' if os.name == 'nt' else 'mrrc-transfer')
    subprocess.run([args.cc, '-std=c11', '-Wall', '-Wextra', '-Werror',
                    '-I', str(source/'src/target'), str(here/'tests/mrrc-transfer.c'),
                    '-o', str(executable)], check=True)
    tested = subprocess.run([str(executable)], capture_output=True, text=True, check=True)
    (out/'transfer-test.log').write_text(tested.stdout+tested.stderr, encoding='utf-8')
    bank_executable = out/('banked-transfer.exe' if os.name == 'nt' else 'banked-transfer')
    subprocess.run([args.cc, '-std=c11', '-Wall', '-Wextra', '-Werror',
                    '-I', str(source/'src/target'), str(here/'tests/banked-transfer.c'),
                    '-o', str(bank_executable)], check=True)
    banks_tested = subprocess.run([str(bank_executable)], capture_output=True, text=True, check=True)
    (out/'banked-transfer-test.log').write_text(banks_tested.stdout+banks_tested.stderr, encoding='utf-8')
    vfp_executable = out/('vfp-transfer.exe' if os.name == 'nt' else 'vfp-transfer')
    subprocess.run([args.cc, '-std=c11', '-Wall', '-Wextra', '-Werror',
                    '-I', str(source/'src/target'), str(here/'tests/vfp-transfer.c'),
                    '-o', str(vfp_executable)], check=True)
    vfp_tested = subprocess.run([str(vfp_executable)], capture_output=True, text=True, check=True)
    (out/'vfp-transfer-test.log').write_text(vfp_tested.stdout+vfp_tested.stderr, encoding='utf-8')
    vfp_write_executable = out/('vfp-write-transfer.exe' if os.name == 'nt' else 'vfp-write-transfer')
    subprocess.run([args.cc, '-std=c11', '-Wall', '-Wextra', '-Werror',
                    '-I', str(source/'src/target'), str(here/'tests/vfp-write-transfer.c'),
                    '-o', str(vfp_write_executable)], check=True)
    vfp_write_tested = subprocess.run([str(vfp_write_executable)], capture_output=True, text=True, check=True)
    (out/'vfp-write-transfer-test.log').write_text(vfp_write_tested.stdout+vfp_write_tested.stderr, encoding='utf-8')
    subprocess.run([sys.executable, str(here/'tests/vfp-write-driver.py'),
                    '--out', str(out/'vfp-write-driver')], check=True)
    report = {'board_tests_executed': False, 'revision': revision,
              'patch_sha256': lock['patch_sha256'], 'transaction_passed': True,
              'transaction_binary_sha256': digest(executable),
              'banked_transaction_passed': True, 'banked_protocol': lock['banked_protocol'],
              'banked_transaction_binary_sha256': digest(bank_executable),
              'vfp_transaction_passed': True, 'vfp_protocol': lock['vfp_protocol'],
              'vfp_transaction_binary_sha256': digest(vfp_executable),
              'vfp_write_transaction_passed': True, 'vfp_write_protocol': lock['vfp_write_protocol'],
              'vfp_write_transaction_binary_sha256': digest(vfp_write_executable),
              'vfp_write_deferred_driver_passed': True,
              'backend_commands_passed': False, 'limitations':
              ['Transport/exception execution requires the deferred physical-core cases.']}
    if args.openocd:
        backend = args.openocd.resolve()
        protocol = lock['protocol']
        bank_protocol = lock['banked_protocol']
        vfp_protocol = lock['vfp_protocol']
        vfp_write_protocol = lock['vfp_write_protocol']
        # Initialize only the virtual adapter, keeping the target unexamined.
        # Exact native error codes prevent an init-mode rejection from falsely
        # passing an argument/state-guard test.
        script = '''adapter driver dummy
adapter speed 100
gdb port disabled
tcl port disabled
telnet port disabled
jtag newtap dt cpu -irlen 4
dap create dt.dap -tap dt.cpu
target create dt.cpu armv8r -dap dt.dap -dbgbase 0 -defer-examine
if {[aarch64 debugtui_adapter] ne "%s"} {error "adapter protocol mismatch"}
if {[aarch64 debugtui_banked_protocol] ne "%s"} {error "banked protocol mismatch"}
if {[aarch64 debugtui_vfp_protocol] ne "%s"} {error "VFP protocol mismatch"}
if {[aarch64 debugtui_vfp_write_protocol] ne "%s"} {error "VFP writer protocol mismatch"}
help aarch64 mrrc
help aarch64 isb
help aarch64 banked
help aarch64 vfp
help aarch64 vfp_write
catch {init} dummy_init_result
proc expect_error {body expected} {
    if {![catch {uplevel 1 $body} result]} {error "command unexpectedly succeeded"}
    if {$::errorCode ne [list OpenOCD $expected]} {error "wrong rejection: $result / $::errorCode"}
}
foreach operands {{16 0 14} {15 16 14} {15 0 16}} {
    expect_error [list aarch64 mrrc {*}$operands] -603
}
foreach operands {{15 0} {15 0 14 0}} {expect_error [list aarch64 mrrc {*}$operands] -601}
expect_error {aarch64 mrrc 15 0 14} -311
expect_error {aarch64 isb} -311
expect_error {aarch64 isb 0} -601
expect_error {aarch64 banked} -601
expect_error {aarch64 banked sp_irq 0} -601
foreach invalid {sp_mon spsr_usr SP_IRQ} {expect_error [list aarch64 banked $invalid] -603}
foreach bank {sp_irq lr_irq spsr_irq r8_fiq r9_fiq r10_fiq r11_fiq r12_fiq sp_fiq lr_fiq spsr_fiq sp_und lr_und spsr_und sp_abt lr_abt spsr_abt sp_svc lr_svc spsr_svc sp_hyp elr_hyp spsr_hyp} {
    expect_error [list aarch64 banked $bank] -311
}
expect_error {aarch64 vfp} -601
expect_error {aarch64 vfp d0 0} -601
foreach invalid {d32 q16 d01 D0 s0 fpinst fpinst2 d-1} {expect_error [list aarch64 vfp $invalid] -603}
foreach control {fpsid fpscr mvfr0 mvfr1 mvfr2 fpexc} {expect_error [list aarch64 vfp $control] -311}
for {set index 0} {$index < 32} {incr index} {expect_error [list aarch64 vfp d$index] -311}
for {set index 0} {$index < 16} {incr index} {expect_error [list aarch64 vfp q$index] -311}
expect_error {aarch64 debugtui_vfp_write_protocol 0} -601
foreach operands {{} {s0} {s0 0x00000000 0}} {expect_error [list aarch64 vfp_write {*}$operands] -601}
foreach invalid {s32 s01 S0 s-1 d32 q16 d01 fpscr fpexc mvfr0} {
    expect_error [list aarch64 vfp_write $invalid 0x00000000] -603
}
foreach invalid {0 0x0 0x000000000 0x0000000g 0x1234567; 0X12345678 -0x12345678} {
    expect_error [list aarch64 vfp_write s0 $invalid] -603
}
for {set index 0} {$index < 32} {incr index} {
    expect_error [list aarch64 vfp_write s$index 0x7fa12345] -311
    expect_error [list aarch64 vfp_write d$index 0x7ff0123456789abc] -311
}
for {set index 0} {$index < 16} {incr index} {
    expect_error [list aarch64 vfp_write q$index 0x8123456789abcdef7ff0123456789abc] -311
}
puts "PASS: adapter protocol, command help, encoding bounds, unexamined target guards"
shutdown
''' % (protocol, bank_protocol, vfp_protocol, vfp_write_protocol)
        script_file = out/'backend-commands.tcl'
        script_file.write_text(script, encoding='utf-8')
        result = subprocess.run([str(backend), '-f', str(script_file)],
                                capture_output=True, text=True, timeout=30)
        (out/'backend-commands.log').write_text(result.stdout+result.stderr, encoding='utf-8')
        assert result.returncode == 0, result.stdout+result.stderr
        assert 'PASS: adapter protocol' in result.stdout+result.stderr
        version = subprocess.run([str(backend), '--version'], capture_output=True, text=True, check=True)
        report.update(backend_commands_passed=True, backend_path=str(backend),
                      backend_sha256=digest(backend), backend_version=(version.stdout+version.stderr).strip())
    (out/'report.json').write_text(json.dumps(report, indent=2)+'\n', encoding='utf-8')
    print(json.dumps(report, ensure_ascii=True))


if __name__ == '__main__':
    main()
