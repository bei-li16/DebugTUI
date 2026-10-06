"""Development-only target model executing actual Tcl control flow; no board I/O.

Uses the standard Python tkinter Tcl interpreter without creating a Tk window.
Input/output are JSON; the target model permits reads, adapted selector MCRs,
CP15ISB and raw VFP writes. Tcl runs the production host control flow; this model
supplies register behavior, while separate C tests execute the physical backend.
"""
import json
import sys
import tkinter
import re
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

    def memory(name, op, *args):
        cpu = state['targets'][name]
        if op == 'curstate':
            return (0, cpu['status'])
        state['trace'].append([name, op, *args])
        if cpu['status'] != 'halted':
            return (1, 'physical M target is not halted')
        cpu.setdefault('m_rnr', 3)
        cpu.setdefault('m_original', cpu['m_rnr'])
        count = cpu.get('m_count', 8)
        if op == 'write_memory':
            assert len(args) == 3 and int(args[0], 0) == 0xe000ed98 and args[1] == '32'
            value = int(args[2], 0)
            assert 0 <= value < count
            restoring_m = cpu.get('m_failed', False) or cpu.get('m_pairs', 0) >= count
            if fault == 'm_restore_write' and restoring_m:
                return (1, 'fixture refuses RNR restoration')
            cpu['m_rnr'] = value
            cpu['m_last_restore'] = restoring_m
            cpu['m_writes'] = cpu.get('m_writes', 0) + 1
            if fault == 'm_select_after_write' and not restoring_m:
                cpu['m_failed'] = True
                return (1, 'fixture write applied before error')
            return (0, '')
        assert op == 'read_memory' and len(args) == 3 and args[1:] == ('32', '1')
        address = int(args[0], 0)
        model = cpu.get('m_cpuid', 0x410fc231)
        values = {0xe000ed00: model, 0xe000e004: 2, 0xe000ed90: count << 8,
                  0xe000ed94: 5, 0xe000ed98: cpu['m_rnr'], 0xe000edfc: 0,
                  0xe0002000: 0x10000060, 0xe0001000: 0x40000000,
                  0xe000ef40: 0x10110021, 0xe000ef44: 0x11000011, 0xe000ef48: 0}
        if address == 0xe000ed9c:
            if fault == 'm_base_read' and cpu['m_rnr'] == 2:
                cpu['m_failed'] = True
                return (1, 'fixture RBAR read refused')
            values[address] = (0x30000000 if name == 'cpu1' else 0x20000000) + cpu['m_rnr'] * 0x10000 + cpu['m_rnr']
        if address == 0xe000eda0:
            if fault == 'm_rasr_read' and cpu['m_rnr'] == 2:
                cpu['m_failed'] = True
                return (1, 'fixture RASR read refused')
            values[address] = 0x0307001f + cpu['m_rnr'] * 0x100
            cpu['m_pairs'] = cpu.get('m_pairs', 0) + 1
            if fault == 'm_context_change' and cpu['m_rnr'] == 2:
                Path(state['context_file']).write_text('{"thread":"2","frame":1}', encoding='utf-8')
        if address == 0xe000ed98 and fault == 'm_select_readback' and cpu.get('m_writes') and not cpu.get('m_failed'):
            cpu['m_failed'] = True
            values[address] ^= 1
        if address == 0xe000ed98 and fault == 'm_restore_readback' and cpu.get('m_last_restore'):
            values[address] ^= 1
        if address == 0xe000ed90 and fault == 'm_capacity':
            values[address] = (16 if count == 8 else 8) << 8
        if address == 0xe000ed00 and fault == 'm_final_identity' and cpu.get('m_pairs', 0) >= count:
            values[address] ^= 0x10
        if address == 0xe000ed98 and fault == 'm_invalid_original' and not cpu.get('m_writes'):
            values[address] = count
        assert address in values, f'unexpected MPU fixture address {address:#x}'
        return (0, (f'0x{values[address]:08x}',))


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
        if op == 'debugtui_timer_protocol':
            return (0, 'old-timer-adapter' if fault == 'timer_protocol' else
                    'debugtui-armv8-timer-1 external-identity current-el dspsr dlr scratch-readback no-mode-change stop-on-fault')
        if op == 'debugtui_pmu_protocol':
            return (0, 'old-pmu-adapter' if fault == 'pmu_protocol' else
                    'debugtui-armv8-pmu-1 external-identity current-el fresh-count direct-index mrrc64 no-enable no-selector-write stop-on-fault')
        if op == 'debugtui_gic_protocol':
            return (0, 'old-gic-adapter' if fault == 'gic_protocol' else
                    'debugtui-armv8-gic-1 external-identity current-el fresh-capacity physical-icc hyp-ich no-ack no-enable stop-on-fault')
        if op == 'gic':
            assert len(args) == 1
            reg = args[0]
            if fault == 'gic_fault':
                cpu['status'] = 'unknown'
                return (1, 'Core state restoration failed: GIC fixture outcome unknown', -1)
            reason = cpu.get('gic_errors', {}).get(reg)
            if fault == 'gic_identity': reason = 'reader-unsupported'
            if fault == 'gic_unknown': reason = 'access-unknown'
            if reg in ('icc_iar0', 'icc_iar1'): reason = 'access-restricted'
            if re.fullmatch('(icc|ich)_ap[01]r[123]', reg): reason = 'not-implemented'
            dscr = cpu.get('gic_dscr', '0x01000200')
            if ((int(dscr,16) >> 8) & 3) != 2: reason = 'access-unknown'
            elif int(dscr,16) & (1 << 16): reason = 'access-restricted'
            if reason:
                return (1, 'debugtui-gic:' + reason, -300 if reason == 'reader-unsupported' else -308)
            view = 'hypervisor_ich' if reg.startswith('ich_') else 'physical_icc'
            controls = {'icc_hsre':'0x0000000f','icc_sre':'0x00000007','icc_ctlr':'0x00000403','ich_vtr':'0x90180003','ich_hcr':'0x00007c01'}
            raw = cpu.get('gic_values', {}).get(reg,controls.get(reg,'0xf1234567'))
            if fault == 'gic_short': raw = '0x1234567'
            if fault == 'gic_forged_el': dscr = '0x01000100'
            if fault == 'gic_forged_capacity': controls['ich_vtr'] = '0xd4180003'
            if fault == 'gic_forged_view': view = 'hypervisor_ich' if view == 'physical_icc' else 'physical_icc'
            if fault == 'gic_context_change':
                Path(state['context_file']).write_text('{"thread":"2","frame":1}',encoding='utf-8')
            dspsr = cpu.get('gic_dspsr','0xa2000410')
            return (0,f"view {view} midr 0x411fd134 dscr {dscr} dspsr {dspsr} dlr 0x81234568 id_pfr1 0x10111011 icc_hsre {controls['icc_hsre']} icc_sre {controls['icc_sre']} icc_ctlr {controls['icc_ctlr']} ich_vtr {controls['ich_vtr']} hcr 0x00000038 ich_hcr {controls['ich_hcr']} hstr 0x00001000 value {raw}")
        if op == 'pmu':
            assert len(args) == 1
            reg = args[0]
            if fault == 'pmu_fault':
                cpu['status'] = 'unknown'
                return (1, 'Core state restoration failed: PMU fixture outcome unknown', -1)
            reason = cpu.get('pmu_errors', {}).get(reg)
            if fault == 'pmu_identity': reason = 'reader-unsupported'
            if fault == 'pmu_unknown': reason = 'access-unknown'
            if reason:
                return (1, 'debugtui-pmu:' + reason, -300 if reason == 'reader-unsupported' else -308)
            dscr = cpu.get('pmu_dscr', '0x01000200')
            pmcr = cpu.get('pmu_pmcr', '0x41132048')
            sel = int(cpu['pmselr'])
            if ((int(dscr, 16) >> 8) & 3) != 2:
                return (1, 'debugtui-pmu:access-unknown', -308)
            if int(pmcr, 16) & 0xffffff80 != 0x41132000:
                return (1, 'debugtui-pmu:reader-unsupported', -300)
            if (reg == 'pmxevcntr' and sel >= 4) or (reg == 'pmxevtyper' and sel >= 4 and sel != 31):
                return (1, 'debugtui-pmu:access-restricted', -308)
            default = '0xfedcba9876543210' if reg == 'pmccntr' else '0xf1234567'
            if reg == 'pmxevtyper': default = '0x00000000' if sel == 31 else '0x00140011'
            if reg == 'pmxevcntr': default = f'0x{0xf123ab00+sel:08x}'
            if re.fullmatch('pmevtyper[0-3]', reg): default = '0x00140011'
            if re.fullmatch('pmevcntr[0-3]', reg): default = f'0x{0xf123ab00+int(reg[-1]):08x}'
            raw = cpu.get('pmu_values', {}).get(reg, default)
            if reg == 'pmcr': raw = pmcr
            if reg == 'pmselr': raw = f'0x{sel:08x}'
            if fault == 'pmu_short': raw = '0x76543210'
            if fault == 'pmu_forged_el': dscr = '0x01000100'
            if fault == 'pmu_forged_count': pmcr = '0x41131048'
            if fault == 'pmu_context_change':
                Path(state['context_file']).write_text('{"thread":"2","frame":1}', encoding='utf-8')
            dspsr = cpu.get('pmu_dspsr', '0xa2000410')
            return (0, f'midr 0x411fd134 dscr {dscr} dspsr {dspsr} dlr 0x81234568 id_dfr0 0x03010066 pmcr {pmcr} hdcr 0x00400e02 pmselr 0x{sel:08x} value {raw}')
        if op == 'timer':
            assert len(args) == 1
            reg = args[0]
            if fault == 'timer_fault':
                cpu['status'] = 'unknown'
                return (1, 'Core state restoration failed: Timer fixture outcome unknown', -1)
            reason = cpu.get('timer_errors', {}).get(reg)
            if fault == 'timer_unknown': reason = 'access-unknown'
            if fault == 'timer_restricted': reason = 'access-restricted'
            if fault == 'timer_identity': reason = 'reader-unsupported'
            if reason:
                return (1, 'debugtui-timer:' + reason, -300 if reason == 'reader-unsupported' else -308)
            raw = cpu.get('timer_values', {}).get(reg, '0xfedcba9876543210')
            sequence = cpu.get('timer_sequences', {}).get(reg)
            if sequence:
                raw = sequence.pop(0)
                cpu.setdefault('timer_values', {})[reg] = raw
            if fault == 'timer_short': raw = '0x76543210'
            dscr = cpu.get('timer_dscr', '0x01000200')
            if fault == 'timer_forged_el': dscr = '0x01000100'
            if fault == 'timer_context_change':
                Path(state['context_file']).write_text('{"thread":"2","frame":1}', encoding='utf-8')
            dspsr = cpu.get('timer_dspsr', '0xa2000410')
            return (0, f'midr 0x411fd134 dscr {dscr} dspsr {dspsr} dlr 0x81234568 value {raw}')
        if op == 'debugtui_banked_protocol':
            return (0, 'old-banked-adapter' if fault == 'bank_protocol' else
                    'debugtui-armv8-banked-2 external-identity current-el dspsr dlr mrs physical-readback no-mode-change stop-on-fault')
        if op == 'debugtui_vfp_protocol':
            return (0, 'old-vfp-adapter' if fault == 'vfp_protocol' else
                    'debugtui-armv8-vfp-2 external-identity current-el dspsr dlr vmrs pair-readback no-enable stop-on-fault')
        if op == 'debugtui_vfp_write_protocol':
            return (0, 'old-vfp-writer' if fault == 'vfp_write_protocol' else
                    'debugtui-armv8-vfp-write-2 external-identity current-el dspsr dlr vmov raw-pair fresh-merge scratch-readback no-enable stop-on-fault')
        vfp_dscr = cpu.get('vfp_dscr', '0x01000200')
        vfp_dspsr = cpu.get('vfp_dspsr', '0xa2000410')
        vfp_midr = '0x511fd134' if fault == 'vfp_forged_identity' else '0x411fd134'
        if fault == 'vfp_forged_el': vfp_dscr = '0x01000100'
        vfp_hcptr = '0x00000400' if fault == 'vfp_forged_trap' else '0x00000000'
        vfp_proof = f'midr {vfp_midr} dscr {vfp_dscr} dspsr {vfp_dspsr} dlr 0x81234568 hcptr {vfp_hcptr}'
        if op == 'vfp_write':
            assert len(args) == 2 and re.fullmatch(r'[sdq](0|[1-9][0-9]?)', args[0])
            reg, raw = args
            index = int(reg[1:])
            bits = 32 if reg[0] == 's' else 64 if reg[0] == 'd' else 128
            assert index < (16 if bits == 128 else 32)
            assert re.fullmatch(r'0x[0-9a-f]{%d}' % (bits // 4), raw)
            if fault == 'vfp_write_refused':
                return (0, 'outcome not_sent reason pending-register-write')
            pair = index // 4 if bits == 32 else index // 2 if bits == 64 else index
            offset = pair + (0x100000000 if name == 'cpu1' else 0)
            initial = ((0x7ff8000012345678 + offset) << 64) | (0x800000003f800000 + offset)
            pairs = cpu.setdefault('vfp_pairs', {})
            before = int(pairs.get(str(pair), str(initial)))
            shift = (index % 4) * 32 if bits == 32 else (index % 2) * 64 if bits == 64 else 0
            mask = ((1 << bits) - 1) << shift
            expected = (before & ~mask) | (int(raw, 16) << shift)
            observed = expected
            outcome = 'verified'
            if fault in ('vfp_write_mismatch', 'vfp_write_mismatch_context_change'):
                observed ^= 1 << shift
                outcome = 'mismatch'
            if fault == 'vfp_write_neighbour_mismatch':
                observed ^= 1 << (0 if shift else 127)
                outcome = 'mismatch'
            pairs[str(pair)] = str(observed)
            if fault == 'vfp_write_unknown':
                cpu['status'] = 'unknown'
                return (1, 'Core state restoration failed: VFP write result unknown', -1)
            if fault in ('vfp_write_context_change', 'vfp_write_mismatch_context_change'):
                Path(state['context_file']).write_text('{"thread":"2","frame":1}', encoding='utf-8')
            if fault == 'vfp_write_forged_expected':
                expected ^= 1 << (0 if shift else 127)
            fpexc = 0x700 if fault == 'vfp_write_forged_enable' else 0x40000700
            receipt_proof = vfp_proof
            if fault == 'vfp_write_forged_el': receipt_proof = receipt_proof.replace('0x01000200', '0x01000100')
            if fault == 'vfp_write_forged_identity': receipt_proof = receipt_proof.replace('0x411fd134', '0x511fd134')
            if fault == 'vfp_write_forged_trap': receipt_proof = receipt_proof.replace('0x00000000', '0x00000400')
            if fault == 'vfp_write_legacy_receipt': receipt_proof = ''
            return (0, f'outcome {outcome} before 0x{before:032x} expected 0x{expected:032x} value 0x{observed:032x} mvfr0 0x10110222 mvfr1 0x12111111 fpexc 0x{fpexc:08x} {receipt_proof}')
        if op == 'vfp':
            reg = args[0]
            if fault in ('vfp_el0', 'vfp_el1'):
                return (1, 'debugtui-vfp:access-restricted: current EL permissions unproven', -308)
            if fault == 'vfp_refusal_context_change':
                Path(state['context_file']).write_text('{"thread":"2","frame":1}', encoding='utf-8')
                return (1, 'debugtui-vfp:access-restricted', -308)
            if fault == 'vfp_identity':
                return (1, 'debugtui-vfp:reader-unsupported', -300)
            if fault == 'vfp_trapped':
                return (1, 'debugtui-vfp:access-restricted', -308)
            if fault == 'vfp_fault':
                cpu['status'] = 'unknown'
                return (1, 'Core state restoration failed: VFP fixture outcome unknown', -1)
            single = fault == 'vfp_d16'
            mvfr0, mvfr1 = (0x10110021, 0x11000011) if single else (0x10110222, 0x12111111)
            if fault == 'vfp_unknown':
                mvfr1 = 0x12113111
            enabled = fault != 'vfp_disabled'
            fpexc = 0x40000700 if enabled else 0x700
            controls = {'fpsid':0x41034025, 'fpscr':0xa000009f, 'mvfr0':mvfr0,
                        'mvfr1':mvfr1, 'mvfr2':0x40 if single else 0x43, 'fpexc':fpexc}
            data = reg.startswith(('d','q'))
            if fault == 'vfp_unknown' and data:
                return (1, 'debugtui-vfp:reader-unsupported', -300)
            if single and data and (reg.startswith('q') or int(reg[1:]) >= 16):
                return (1, 'debugtui-vfp:not-implemented', -308)
            if not enabled and (data or reg == 'fpscr'):
                return (1, 'debugtui-vfp:feature-disabled', -308)
            if data:
                pair = int(reg[1:]) // 2 if reg.startswith('d') else int(reg[1:])
                offset = pair + (0x100000000 if name == 'cpu1' else 0)
                initial = ((0x7ff8000012345678 + offset) << 64) | (0x800000003f800000 + offset)
                value = f'0x{int(cpu.setdefault("vfp_pairs", {}).get(str(pair), str(initial))):032x}'
            else:
                value = f'0x{controls[reg]:08x}'
            if fault == 'vfp_short':
                value = '0x1'
            if fault == 'vfp_context_change':
                Path(state['context_file']).write_text('{"thread":"2","frame":1}', encoding='utf-8')
            body = f'mvfr0 0x{mvfr0:08x} mvfr1 0x{mvfr1:08x} fpexc 0x{fpexc:08x} value {value}'
            return (0, body if fault == 'vfp_legacy_response' else vfp_proof + ' ' + body)
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
                     'lr_svc', 'spsr_svc', 'sp_hyp', 'elr_hyp', 'spsr_hyp',
                     'r8_usr', 'r9_usr', 'r10_usr', 'r11_usr', 'r12_usr', 'sp_usr', 'lr_usr']
            if len(args) != 1 or args[0] not in names:
                return (1, 'unadapted banked fixture name', -603)
            if fault == 'bank_context_change':
                Path(state['context_file']).write_text('{"thread":"2","frame":1}', encoding='utf-8')
            reg = args[0]
            mode = int(cpu.get('bank_mode', 0x1a))
            bank_mode = {'usr':0x10,'fiq':0x11,'irq':0x12,'svc':0x13,
                         'abt':0x17,'und':0x1b,'hyp':0x1a}[reg.split('_')[1]]
            current = (mode == bank_mode and reg != 'elr_hyp') or (bank_mode == 0x10 and
                      ((reg.startswith('r') and mode != 0x11) or mode == 0x1f or (reg == 'lr_usr' and mode == 0x1a)))
            if mode not in (0x10,0x1a):
                return (1, 'current Debug mode cannot access Hyp bank' if bank_mode == 0x1a else
                        'debugtui-banked:access-unknown: current EL1 mode cannot be proven', -308)
            if not current and (mode == 0x10 or (bank_mode == 0x1a and mode != 0x1a)):
                return (1, 'current Debug mode cannot access this bank', -308)
            method = ('mrs32' if reg.startswith('spsr') else 'mov32') if current else 'banked_mrs32'
            el = 0 if mode == 0x10 else 2 if mode == 0x1a else 1
            dscr = cpu.get('bank_dscr', f'0x{0x01000000 | (el << 8):08x}')
            midr = '0x511fd134' if fault == 'bank_forged_identity' else '0x411fd134'
            if fault == 'bank_forged_mode': dscr = '0x01000100'
            if fault == 'bank_forged_method': method = 'mov32'
            value = f'0x{0x51000000 + names.index(reg) + (0x10000000 if name == "cpu1" else 0):08x}'
            if fault == 'bank_short': value = '0x1'
            if fault == 'bank_legacy_value': return (0, value)
            dspsr = cpu.get('bank_dspsr', '0xa2000410')
            return (0, f'midr {midr} dscr {dscr} dspsr {dspsr} dlr 0x81234568 value {value} method {method}')
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
            encoding = ' '.join(args)
            if encoding in cpu.get('timer64', {}):
                return (0, cpu['timer64'][encoding])
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
        if encoding in cpu.get('timer32', {}):
            if encoding == '15 4 14 2 0' and fault == 'timer_read_error':
                return (1, 'fixture Timer access unavailable; original cause unknown')
            return (0, cpu['timer32'][encoding])
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
                  '15 4 1 1 2': 1 << 10 if fault == 'vfp_trapped' else cpu.get('hcptr', 0),
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
    interp.createcommand('_fixture_memory', memory)
    interp.eval('''
    proc cpu0 {op args} {set r [_fixture_memory cpu0 $op {*}$args]; if {[lindex $r 0]} {error [lindex $r 1]}; return [lindex $r 1]}
    proc cpu1 {op args} {set r [_fixture_memory cpu1 $op {*}$args]; if {[lindex $r 0]} {error [lindex $r 1]}; return [lindex $r 1]}
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
