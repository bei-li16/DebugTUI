"""Development-only target model executing actual Tcl control flow; no board I/O.

Uses the standard Python tkinter Tcl interpreter without creating a Tk window.
Input/output are JSON; the target model permits only reads, adapted selector MCRs
and CP15ISB. It supplies register behavior, not an alternate transaction algorithm.
"""
import json
import sys
import tkinter
from pathlib import Path

def evaluate(data):
    state = data.get('state', {})
    state.setdefault('current', 'outside')
    state.setdefault('trace', [])
    state.setdefault('targets', {})
    fault = data.get('fault', '')
    for name in ('cpu0', 'cpu1'):
        cpu = state['targets'].setdefault(name, {})
        for key, value in {'prselr': 1, 'hprselr': 2, 'pmselr': 31, 'el1_count': 24,
                           'el2_count': 20, 'pmu_count': 4, 'sync': 32, 'status': 'halted'}.items():
            cpu.setdefault(key, value)
    original = {name: {key: cpu[key] for key in ('prselr', 'hprselr', 'pmselr')}
                for name, cpu in state['targets'].items()}
    changed = False
    restoring = False
    barriers = 0
    interp = tkinter.Tcl()


    def targets(*args):
        if len(args) != 1:
            return (1, 'fixture targets requires one explicit target')
        state['trace'].append(['targets', args[0]])
        if args[0] == 'outside' and fault == 'target_restore':
            return (1, 'fixture target restoration refused')
        if args[0] == 'outside' and fault == 'target_mismatch':
            return (0, 'outside')
        state['current'] = args[0]
        return (0, args[0])


    def arm(op, *args):
        nonlocal changed, restoring, barriers
        name = state['current']
        cpu = state['targets'].get(name)
        state['trace'].append([name, op, *args])
        if not cpu or cpu['status'] != 'halted':
            return (1, 'physical target is not halted')
        if op == 'debugtui_adapter':
            return (0, 'old-adapter' if fault == 'adapter_mismatch' else
                    'debugtui-armv8-1 mrrc isb scratch-readback stop-on-fault')
        if op == 'debugtui_banked_protocol':
            return (0, 'old-banked-adapter' if fault == 'bank_protocol' else
                    'debugtui-armv8-banked-1 mrs physical-readback no-mode-change stop-on-fault')
        if op == 'banked':
            if fault == 'bank_refusal_context_change':
                Path(state['context_file']).write_text('{"thread":"2","frame":1}', encoding='utf-8')
                return (1, 'physical mode cannot access this bank', -308)
            if fault == 'bank_unavailable':
                return (1, 'physical mode cannot access this bank', -308)
            if fault == 'bank_identity':
                return (1, 'unadapted CPU identity', -300)
            if fault == 'bank_fault':
                cpu['status'] = 'unknown'
                return (1, 'Core state restoration failed: fixture bank outcome unknown', -1)
            names = ['sp_irq', 'lr_irq', 'spsr_irq', 'r8_fiq', 'r9_fiq', 'r10_fiq',
                     'r11_fiq', 'r12_fiq', 'sp_fiq', 'lr_fiq', 'spsr_fiq', 'sp_und',
                     'lr_und', 'spsr_und', 'sp_abt', 'lr_abt', 'spsr_abt', 'sp_svc',
                     'lr_svc', 'spsr_svc', 'sp_hyp', 'elr_hyp', 'spsr_hyp']
            if len(args) != 1 or args[0] not in names:
                return (1, 'unadapted banked fixture name', -603)
            if fault == 'bank_context_change':
                Path(state['context_file']).write_text('{"thread":"2","frame":1}', encoding='utf-8')
            return (0, '0x1' if fault == 'bank_short' else
                    f'0x{0x51000000 + names.index(args[0]) + (0x10000000 if name == "cpu1" else 0):08x}')
        if op == 'isb':
            if args:
                return (1, 'genuine ISB takes no operands')
            barriers += 1
            if fault == 'genuine_isb_fault' and barriers == 2:
                cpu['status'] = 'unknown'
                return (1, 'Core state restoration failed: fixture ISB outcome unknown')
            return (0, '')
        if op == 'mrrc':
            if len(args) != 3:
                return (1, 'MRRC requires cp/op1/CRm')
            if fault == 'mrrc_fault':
                cpu['status'] = 'unknown'
                return (1, 'Core state restoration failed: fixture MRRC outcome unknown')
            return (0, '0xfedcba9876543210' if name == 'cpu0' else '0x81234567abcdef01')
        encoding = ' '.join(args[:5])
        selector = {'15 0 6 2 1': 'prselr', '15 4 6 2 1': 'hprselr',
                    '15 0 9 12 5': 'pmselr'}.get(encoding)
        if op == 'mcr':
            if len(args) != 6:
                return (1, 'fixture MCR requires one value')
            value = int(args[5], 0)
            if encoding == '15 0 7 5 4':
                barriers += 1
                if (fault == 'select_barrier' and barriers == 1) or (fault == 'restore_barrier' and restoring):
                    return (1, 'fixture ISB failed')
                return (0, '')
            if not selector:
                return (1, 'MCR to MPU/counter/control state is forbidden')
            restoring = changed and value == original[name][selector]
            if restoring and fault == 'restore_error':
                return (1, 'fixture selector restore write failed')
            cpu[selector] = value + 1 if restoring and fault == 'restore_mismatch' else value
            changed = True
            if not restoring and fault == 'select_error':
                return (1, 'fixture reported error after accepting selector write')
            return (0, '')
        if op != 'mrc' or len(args) != 5:
            return (1, 'fixture permits fixed MRC/MCR only')
        if selector:
            value = cpu[selector]
            if not changed and fault == 'invalid_original':
                value = 0xffffffff
            return (0, f'0x{value:08x}')
        scalar = {'15 0 0 0 0': 0x411fd134, '15 0 0 0 4': cpu['el1_count'] << 8,
                  '15 4 0 0 4': cpu['el2_count'], '15 0 9 12 0': 0x41130000 | cpu['pmu_count'] << 11,
                  '15 0 1 0 0': cpu['sync'], '15 4 1 0 0': cpu['sync'],
                  '15 0 10 2 0': cpu.get('mair0', 0xff440400), '15 0 10 2 1': cpu.get('mair1', 0xff440400),
                  '15 4 10 2 0': cpu.get('hmair0', 0xff440400), '15 4 10 2 1': cpu.get('hmair1', 0xff440400),
                  '15 4 1 1 0': cpu.get('hcr', 1), '15 4 6 1 1': (1 << cpu['el2_count']) - 1,
                  '15 0 14 2 1': 0, '15 0 14 3 1': 0, '15 4 14 2 1': 0,
                  '15 0 14 0 0': 100000000, '15 0 14 1 0': 0, '15 4 14 1 0': 0}
        if encoding in scalar:
            value = scalar[encoding]
            if fault == 'no_sync' and encoding in ('15 0 1 0 0', '15 4 1 0 0'):
                value = 0
            if fault == 'count_changed' and encoding == '15 0 0 0 4':
                value = 16 << 8
            if fault == 'identity_changed' and encoding == '15 0 0 0 0':
                value = 0x410fc090
            if fault == 'mair0_error' and encoding == '15 0 10 2 0':
                return (1, 'fixture MAIR0 unavailable')
            return (0, f'0x{value:08x}')
        selected = {'15 0 6 3 0': ('prselr', 0), '15 0 6 3 1': ('prselr', 1),
                    '15 4 6 3 0': ('hprselr', 0), '15 4 6 3 1': ('hprselr', 1),
                    '15 0 9 13 1': ('pmselr', 0), '15 0 9 13 2': ('pmselr', 1)}.get(encoding)
        direct_index = None
        cp, op1, crn, crm, op2 = map(int, args)
        if not selected and cp == 15:
            if crn == 6 and op1 in (0, 1, 4, 5) and 8 <= crm <= 15 and op2 in (0, 1, 4, 5):
                selected = ('hprselr' if op1 >= 4 else 'prselr', op2 % 4)
                direct_index = (op1 % 2) * 16 + (crm - 8) * 2 + op2 // 4
            elif crn == 14 and op1 == 0 and crm in (8, 12) and 0 <= op2 <= 3:
                selected = ('pmselr', 0 if crm == 12 else 1)
                direct_index = op2
        if not selected:
            return (1, f'Unadapted fixture read: {encoding}')
        if fault == 'data_error':
            return (1, 'fixture data read failed')
        selected_id, part = selected
        index = cpu[selected_id] if direct_index is None else direct_index
        if fault == 'region_error' and selected_id == 'prselr' and index == 5 and part == 0:
            return (1, 'fixture region 5 base inaccessible')
        if fault == 'final_mode_change' and selected_id == 'prselr' and index == cpu['el1_count'] - 1 and part == 1:
            Path(state['register_values_file']).write_text('{"cpsr":"0x13"}', encoding='utf-8')
        if selected_id == 'pmselr':
            value = 0x140011 if part == 0 else 0xf123ab00 + index
        else:
            value = 0x2000001f + index * 0x10000 if part == 0 else 0x2000ffcf + index * 0x10000
        return (0, f'0x{value:08x}')


    interp.createcommand('_fixture_targets', targets)
    interp.createcommand('_fixture_arm', arm)
    interp.createcommand('target', lambda action: state['current'] if action == 'current' else '')
    for name in state['targets']:
        interp.createcommand(name, lambda action, name=name: state['targets'][name]['status'] if action == 'curstate' else '')
    interp.eval('''
    proc targets {name} {set r [_fixture_targets $name]; if {[lindex $r 0]} {error [lindex $r 1]}; return [lindex $r 1]}
    proc arm {op args} {set r [_fixture_arm $op {*}$args]; if {[lindex $r 0]} {if {[llength $r] == 3} {return -code error -errorcode [list OpenOCD [lindex $r 2]] [lindex $r 1]}; error [lindex $r 1]}; return [lindex $r 1]}
    proc aarch64 {op args} {return [arm $op {*}$args]}
    ''')
    try:
        result = interp.eval(data['script'])
        output = {'ok': True, 'value': result, 'state': state}
    except tkinter.TclError as error:
        output = {'ok': False, 'value': str(error), 'state': state}
    return output


if __name__ == "__main__":
    if "--jsonl" in sys.argv:
        print(json.dumps({"ready": True}), flush=True)
        for line in sys.stdin:
            print(json.dumps(evaluate(json.loads(line)), separators=(",", ":")), flush=True)
    else:
        print(json.dumps(evaluate(json.load(sys.stdin)), separators=(",", ":")))
