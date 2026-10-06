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
    timer_executable = out/('timer-transfer.exe' if os.name == 'nt' else 'timer-transfer')
    subprocess.run([args.cc, '-std=c11', '-Wall', '-Wextra', '-Werror',
                    '-I', str(source/'src/target'), str(here/'tests/timer-transfer.c'),
                    '-o', str(timer_executable)], check=True)
    timer_tested = subprocess.run([str(timer_executable)], capture_output=True, text=True, check=True)
    (out/'timer-transfer-test.log').write_text(timer_tested.stdout+timer_tested.stderr, encoding='utf-8')
    pmu_executable = out/('pmu-transfer.exe' if os.name == 'nt' else 'pmu-transfer')
    subprocess.run([args.cc, '-std=c11', '-Wall', '-Wextra', '-Werror',
                    '-I', str(source/'src/target'), str(here/'tests/pmu-transfer.c'),
                    '-o', str(pmu_executable)], check=True)
    pmu_tested = subprocess.run([str(pmu_executable)], capture_output=True, text=True, check=True)
    (out/'pmu-transfer-test.log').write_text(pmu_tested.stdout+pmu_tested.stderr, encoding='utf-8')
    gic_executable = out/('gic-transfer.exe' if os.name == 'nt' else 'gic-transfer')
    subprocess.run([args.cc, '-std=c11', '-Wall', '-Wextra', '-Werror',
                    '-I', str(source/'src/target'), str(here/'tests/gic-transfer.c'),
                    '-o', str(gic_executable)], check=True)
    gic_tested = subprocess.run([str(gic_executable)], capture_output=True, text=True, check=True)
    (out/'gic-transfer-test.log').write_text(gic_tested.stdout+gic_tested.stderr, encoding='utf-8')
    r52_executable = out/('r52-transfer.exe' if os.name == 'nt' else 'r52-transfer')
    subprocess.run([args.cc, '-std=c11', '-Wall', '-Wextra', '-Werror',
                    '-I', str(source/'src/target'), str(here/'tests/r52-transfer.c'),
                    '-o', str(r52_executable)], check=True)
    r52_tested = subprocess.run([str(r52_executable)], capture_output=True, text=True, check=True)
    (out/'r52-transfer-test.log').write_text(r52_tested.stdout+r52_tested.stderr, encoding='utf-8')
    r52_selector_executable = out/('r52-selector-transfer.exe' if os.name == 'nt' else 'r52-selector-transfer')
    subprocess.run([args.cc, '-std=c11', '-Wall', '-Wextra', '-Werror',
                    '-I', str(source/'src/target'), str(here/'tests/r52-selector-transfer.c'),
                    '-o', str(r52_selector_executable)], check=True)
    r52_selector_tested = subprocess.run([str(r52_selector_executable)], capture_output=True, text=True, check=True)
    (out/'r52-selector-transfer-test.log').write_text(r52_selector_tested.stdout+r52_selector_tested.stderr, encoding='utf-8')
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
              'pmu_transaction_passed': True, 'pmu_protocol': lock['pmu_protocol'],
              'pmu_transaction_binary_sha256': digest(pmu_executable),
              'gic_transaction_passed': True, 'gic_protocol': lock['gic_protocol'],
              'gic_transaction_binary_sha256': digest(gic_executable),
              'timer_transaction_passed': True, 'timer_protocol': lock['timer_protocol'],
              'timer_transaction_binary_sha256': digest(timer_executable),
              'r52_core_transaction_passed': True, 'r52_core_protocol': lock['r52_core_protocol'],
              'r52_core_transaction_binary_sha256': digest(r52_executable),
              'r52_selector_transaction_passed': True,
              'r52_selector_protocol': lock['r52_selector_protocol'],
              'r52_selector_transaction_binary_sha256': digest(r52_selector_executable),
              'backend_commands_passed': False, 'limitations':
              ['Transport/exception execution requires the deferred physical-core cases.']}
    if args.openocd:
        backend = args.openocd.resolve()
        protocol = lock['protocol']
        bank_protocol = lock['banked_protocol']
        vfp_protocol = lock['vfp_protocol']
        vfp_write_protocol = lock['vfp_write_protocol']
        timer_protocol = lock['timer_protocol']
        pmu_protocol = lock['pmu_protocol']
        gic_protocol = lock['gic_protocol']
        r52_core_protocol = lock['r52_core_protocol']
        r52_selector_protocol = lock['r52_selector_protocol']
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
if {[aarch64 debugtui_timer_protocol] ne "%s"} {error "Timer protocol mismatch"}
help aarch64 mrrc
help aarch64 isb
help aarch64 banked
help aarch64 vfp
help aarch64 vfp_write
help aarch64 timer
if {[aarch64 debugtui_pmu_protocol] ne "%s"} {error "PMU protocol mismatch"}
help aarch64 pmu
if {[aarch64 debugtui_gic_protocol] ne "%s"} {error "GIC protocol mismatch"}
help aarch64 gic
if {[aarch64 debugtui_r52_protocol] ne "%s"} {error "R52 protocol mismatch"}
help aarch64 r52_read
if {[aarch64 debugtui_r52_selector_protocol] ne "%s"} {error "R52 selector protocol mismatch"}
help aarch64 r52_select
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
expect_error {aarch64 debugtui_timer_protocol 0} -601
expect_error {aarch64 debugtui_r52_protocol 0} -601
expect_error {aarch64 debugtui_r52_selector_protocol 0} -601
foreach operands {{} {el1 0} {el1 0 16 0}} {expect_error [list aarch64 r52_select {*}$operands] -601}
foreach operands {{pmu 0 16} {EL2 0 20} {el1 16 16} {el2 20 20} {el2 24 24} {el1 0 0} {el2 0 255}} {
    expect_error [list aarch64 r52_select {*}$operands] -603
}
foreach bank {el1 el2} {
    foreach count {16 20 24} {
        for {set index 0} {$index < $count} {incr index} {
            expect_error [list aarch64 r52_select $bank $index $count] -311
        }
    }
}
expect_error {aarch64 r52_read} -601
expect_error {aarch64 r52_read sctlr 0} -601
foreach invalid {MIDR prbar24 hprlar24 prbar00 prlar-1 pmselr bpiall sctlr;} {expect_error [list aarch64 r52_read $invalid] -603}
foreach reg {midr mpidr sctlr hsctlr cpacr hcr mpuir hmpuir prselr hprselr hprenr mair0 mair1 hmair0 hmair1} {expect_error [list aarch64 r52_read $reg] -311}
foreach bank {pr hpr} {
    for {set index 0} {$index < 24} {incr index} {
        foreach part {bar lar} {expect_error [list aarch64 r52_read $bank$part$index] -311}
    }
}
expect_error {aarch64 timer} -601
expect_error {aarch64 timer cntpct 0} -601
foreach invalid {CNTPCT cntp cntpct; cntpct0} {expect_error [list aarch64 timer $invalid] -603}
foreach reg {cntfrq cntkctl cntp_tval cntp_ctl cntv_tval cntv_ctl cnthctl cnthp_tval cnthp_ctl cntpct cntvct cntp_cval cntv_cval cntvoff cnthp_cval} {
    expect_error [list aarch64 timer $reg] -311
}
expect_error {aarch64 debugtui_pmu_protocol 0} -601
expect_error {aarch64 pmu} -601
expect_error {aarch64 pmu pmcr 0} -601
foreach invalid {PMCR pmswinc pmevcntr4 pmevcntr01 pmcr;} {expect_error [list aarch64 pmu $invalid] -603}
foreach reg {pmcr pmcntenset pmcntenclr pmovsr pmselr pmceid0 pmceid1 pmxevtyper pmxevcntr pmuserenr pmintenset pmintenclr pmovsset pmccfiltr pmevcntr0 pmevcntr1 pmevcntr2 pmevcntr3 pmevtyper0 pmevtyper1 pmevtyper2 pmevtyper3 pmccntr} {
    expect_error [list aarch64 pmu $reg] -311
}
expect_error {aarch64 debugtui_gic_protocol 0} -601
expect_error {aarch64 gic} -601
expect_error {aarch64 gic icc_ctlr 0} -601
foreach invalid {ICC_CTLR icv_ap0r0 ich_lr4 ich_lr01 icc_eoir0 icc_dir icc_sgi1r icc_ctlr;} {expect_error [list aarch64 gic $invalid] -603}
foreach reg {icc_ctlr icc_sre icc_hsre icc_pmr icc_rpr icc_bpr0 icc_bpr1 icc_igrpen0 icc_igrpen1 icc_hppir0 icc_hppir1 icc_ap0r0 icc_ap0r1 icc_ap0r2 icc_ap0r3 icc_ap1r0 icc_ap1r1 icc_ap1r2 icc_ap1r3 ich_vtr ich_hcr ich_misr ich_eisr ich_elrsr ich_vmcr ich_ap0r0 ich_ap0r1 ich_ap0r2 ich_ap0r3 ich_ap1r0 ich_ap1r1 ich_ap1r2 ich_ap1r3 ich_lr0 ich_lr1 ich_lr2 ich_lr3 ich_lrc0 ich_lrc1 ich_lrc2 ich_lrc3 icc_iar0 icc_iar1} {expect_error [list aarch64 gic $reg] -311}
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
''' % (protocol, bank_protocol, vfp_protocol, vfp_write_protocol, timer_protocol, pmu_protocol, gic_protocol, r52_core_protocol, r52_selector_protocol)
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
