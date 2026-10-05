"""R52 TRM 100026_0104_01_en tables 10-4, 10-35/36 and 12-5.

No address is discovered or assumed. GICR base means its control page;
the SGI/PPI page is another 64 KiB. Distributor ownership is the R52 cluster.
All write semantics are descriptive; this catalogue does not enable writers.
"""

MMIO_METADATA = {}


def field(name, offset, width, description, access=None):
    return dict(name=name, segments=[(offset, width)], enums=[],
                description=description, access=access)


def add(name, component, offset, group, scope, description, bits=32,
        access='ro', fields=None, conditions=None, effect=False):
    assert name not in MMIO_METADATA
    MMIO_METADATA[name] = dict(component=component, offset=offset, group=group,
        scope=scope, bits=bits, access=access, description=description,
        fields=fields or [], conditions=[(component + '.present', 1, 1), *(conditions or [])],
        effect=effect,
        access_condition='Explicit board mapping for this core/cluster/chip owner, '
            'and an available configured memory channel or GDB memory route. '
            'Power, authentication, locks and bus permissions may restrict access. '
            'This reader submits no control writes, Debug unlock or interrupt '
            'acknowledge commands, and never halts or resumes another core. '
            'Manual reads can have the documented register side effects. '
            '64-bit MMIO uses two 32-bit bus words, '
            'not an atomic snapshot. Capacity declarations remain configuration evidence.')


def gic_ids(prefix, component, scope):
    for n in range(8):
        offset = 0xffe0 + n * 4 if n < 4 else 0xffd0 + (n - 4) * 4
        add(f'{prefix}_pidr{n}', component, offset, prefix, scope,
            f'Peripheral identity byte {n}; Arm/R52 GIC identity must be checked independently.',
            fields=[field('IDENTITY_BYTE', 0, 8, 'Raw peripheral identification byte.')])
    for n in range(4):
        add(f'{prefix}_cidr{n}', component, 0xfff0 + n * 4, prefix, scope,
            f'Component identification byte {n}. An address declaration alone does not identify a GIC.',
            fields=[field('IDENTITY_BYTE', 0, 8, 'Raw component identification byte.')])


iidr = [field('Implementer', 0, 12, 'Arm JEP106 identifier 0x43B.'),
        field('Revision', 12, 4, 'GIC revision, retained without assuming the CPU revision.'),
        field('Variant', 16, 4, 'GIC variant.'),
        field('ProductID', 24, 8, 'R52 GIC product identifier 0x01.')]
add('gicd_ctlr', 'gicd', 0, 'gicd', 'cluster', 'Shared R52 Distributor control. This viewer does not enable or disable interrupt groups.',
    access='rw', fields=[field('EnableGrp0',0,1,'Group 0 forwarding enable.'), field('EnableGrp1',1,1,'Group 1 forwarding enable.'),
        field('ARE',4,1,'Affinity routing, RAO/WI on R52.','ro'), field('DS',6,1,'No Security extension, RAO/WI on R52.','ro'),
        field('RWP',31,1,'Register write pending.','ro')])
add('gicd_typer', 'gicd', 4, 'gicd', 'cluster', 'Distributor capacity. ITLinesNumber describes blocks of 32 INTIDs; displaying this raw value does not publish a verified capacity fact.',
    fields=[field('ITLinesNumber',0,5,'Implemented INTID capacity is 32*(value+1); R52 maximum INTID 991.'),
        field('CPUNumber',5,3,'Legacy CPU-number field; do not infer actual core mapping.'),
        field('SecurityExtn',10,1,'Security extension; not implemented by R52.'),
        field('LPIS',17,1,'LPI support; not implemented by R52.'), field('IDBits',19,5,'Supported interrupt-identifier width minus one.'),
        field('A3V',24,1,'Affinity level 3 support.')])
add('gicd_iidr', 'gicd', 8, 'gicd', 'cluster', 'Distributor implementer, product, variant and revision.', fields=iidr)
gic_ids('gicd', 'gicd', 'cluster')
for stem, base, meaning in [
    ('igroupr',0x80,'Interrupt group assignment'), ('isenabler',0x100,'Enabled interrupt state; writes set enable'),
    ('icenabler',0x180,'Enabled interrupt state; writes clear enable'), ('ispendr',0x200,'Pending state; writes set pending'),
    ('icpendr',0x280,'Pending state; writes clear pending'), ('isactiver',0x300,'Active state; writes set active'),
    ('icactiver',0x380,'Active state; writes clear active')]:
    for n in range(1,31):
        add(f'gicd_{stem}{n}', 'gicd', base + n * 4, 'gicd', 'cluster',
            f'{meaning} for INTID{n*32}–{n*32+31}. Read is observational; no state-changing write is supplied.',
            access='rw', conditions=[('gicd.interrupts',(n+1)*32)],
            fields=[field(f'INTID{n*32+k}',k,1,f'{meaning} for INTID{n*32+k}.') for k in range(32)])
for n in range(8,248):
    add(f'gicd_ipriorityr{n}', 'gicd', 0x400 + n * 4, 'gicd', 'cluster',
        f'Priorities for INTID{n*4}–{n*4+3}; only five high bits per byte are implemented on R52.', access='rw',
        conditions=[('gicd.interrupts',(n+1)*4)],
        fields=[field(f'INTID{n*4+k}',k*8+3,5,f'R52 five-bit priority for INTID{n*4+k}.') for k in range(4)])
for n in range(2,62):
    add(f'gicd_icfgr{n}', 'gicd', 0xc00 + n * 4, 'gicd', 'cluster',
        f'Trigger configuration for INTID{n*16}–{n*16+15}; viewer never changes level/edge selection.', access='rw',
        conditions=[('gicd.interrupts',(n+1)*16)],
        fields=[field(f'INTID{n*16+k}',k*2+1,1,f'0 level, 1 edge for INTID{n*16+k}.') for k in range(16)])
for n in range(32,992):
    add(f'gicd_irouter{n}', 'gicd', 0x6000 + n * 8, 'gicd', 'cluster',
        f'Routing for INTID{n}; fixed affinity target, not a CPU ownership declaration. Two-word MMIO is non-atomic.',
        bits=64, access='rw', conditions=[('gicd.interrupts',n+1)],
        fields=[field('Aff0',0,8,'Configured target ID; actual core/export-port mapping must be supplied.'),
            field('Aff1',8,8,'RAZ/WI on R52.','ro'), field('Aff2',16,8,'RAZ/WI on R52.','ro'),
            field('IRM',31,1,'1-of-N routing is not implemented by R52.','ro'), field('Aff3',32,8,'RAZ/WI on R52.','ro')])

add('gicr_ctlr', 'gicr', 0, 'gicr', 'core', 'Redistributor control on R52 is read-only; LPI/1-of-N features are not supported.',
    fields=[field('RWP',3,1,'Private interrupt write pending.'), field('UWP',31,1,'Upstream write pending.')])
add('gicr_iidr', 'gicr', 4, 'gicr', 'core', 'Redistributor product/variant/revision. Same base must not be reused for another core.', fields=iidr)
add('gicr_typer', 'gicr', 8, 'gicr', 'core', 'Redistributor target and affinity, including optional export port. Explicit owner/base mapping is still required.', bits=64,
    fields=[field('PLPIS',0,1,'Physical LPI support, absent on R52.'), field('VLPIS',1,1,'Virtual LPI support, absent on R52.'),
        field('Last',4,1,'Last Redistributor, possibly the export port.'), field('ProcessorNumber',8,16,'R52 target ID, independent of a host core name.'),
        field('Aff0',32,8,'Target affinity level zero; validate against board mapping.')])
add('gicr_waker', 'gicr', 0x14, 'gicr', 'core', 'Sleep/quiescence state. Reading does not request powerdown or wake-up.', access='rw',
    fields=[field('ProcessorSleep',1,1,'Sleep request; viewer does not write it.'), field('ChildrenAsleep',2,1,'Interface quiescence.','ro')])
gic_ids('gicr', 'gicr', 'core')
for stem, offset, meaning in [('igroupr',0x80,'Group assignment'),('isenabler',0x100,'Enabled state'),('icenabler',0x180,'Enabled state'),
        ('ispendr',0x200,'Pending state'),('icpendr',0x280,'Pending state'),('isactiver',0x300,'Active state'),('icactiver',0x380,'Active state')]:
    add(f'gicr_{stem}0', 'gicr', 0x10000 + offset, 'gicr', 'core', f'{meaning} for SGIs and PPIs in the separate SGI/PPI page; no writes.', access='rw',
        fields=[field('SGIs',0,16,meaning+' for SGIs 0–15.'), field('PPIs',16,16,meaning+' for PPIs 16–31.')])
for n in range(8):
    add(f'gicr_ipriorityr{n}', 'gicr', 0x10400 + n*4, 'gicr', 'core', f'Private priorities for INTID{n*4}–{n*4+3}.', access='rw',
        fields=[field(f'INTID{n*4+k}',k*8+3,5,'Implemented R52 five-bit priority.') for k in range(4)])
for n in range(2):
    add(f'gicr_icfgr{n}', 'gicr', 0x10c00 + n*4, 'gicr', 'core', 'Private trigger configuration; SGIs are fixed edge-triggered.', access='ro' if n==0 else 'rw',
        fields=[field(f'INTID{n*16+k}',k*2+1,1,'Level/edge selection; SGI values are fixed.') for k in range(16)])

for name, offset, access, effect, description in [
    ('edesr',0x20,'rw',False,'External debug event status.'), ('edecr',0x24,'rw',False,'External debug execution control; no control writes.'),
    ('edwar_lo',0x30,'ro',False,'Watchpoint address low raw word.'), ('edwar_hi',0x34,'ro',False,'Watchpoint address high raw word; not a simultaneous sample.'),
    ('dbgdtrrx_el0',0x80,'rw',False,'External read returns DTRRX without clearing RXfull; this is not the internal CPU receive operation. No writes to the debugger communication channel.'),
    ('editr',0x84,'wo',False,'Instruction transfer is write-only; no read request or writer.'),
    ('edscr',0x88,'rw',False,'External debug status/control; no clearing errors, changing EL or control writes.'),
    ('dbgdtrtx_el0',0x8c,'ro',True,'Debug data-transfer transmit channel; manual read can consume data/clear TXfull.'),
    ('edrcr',0x90,'wo',False,'Clear-error/reserve controls are write-only; no read request or writer.'),
    ('edeccr',0x98,'rw',False,'Exception catch configuration; viewer does not enable catch.'),
    ('edpcsr_lo',0xa0,'ro',True,'PC sample read updates EDCIDSR/EDVIDSR; manual only, may return UNKNOWN/0xffffffff.'),
    ('edcidsr',0xa4,'ro',False,'Context ID from the last PC sample; not a fresh context sample by itself.'),
    ('edvidsr',0xa8,'ro',False,'Virtual context from the last PC sample; not a fresh context sample by itself.'),
    ('edpcsr_hi',0xac,'ro',True,'PC sample upper aperture word; conservatively manual until board sampling semantics are verified. Do not infer an atomic paired sample or a fresh PC from this raw word.'),
    ('dbgoslar',0x300,'wo',False,'OS Lock Access is write-only; viewer does not unlock.'),
    ('edprcr',0x310,'rw',False,'Power/reset control; viewer does not request reset, halt or restart.'),
    ('edprsr',0x314,'ro',True,'Processor status has clear-after-read sticky bits; manual only.'),
    ('edccr',0xc00,'rw',False,'Debug calibration control; no writes.'),
    ('ed_midr',0xd00,'ro',False,'MIDR through the external Debug aperture; separate from stopped GDB register values.'),
    ('edpfr_word0',0xd20,'ro',False,'Raw feature aperture word at D20; TRM summary/detail disagree on low/high naming, so no paired decode/capacity inference.'),
    ('edpfr_word1',0xd24,'ro',False,'Raw feature aperture word at D24; retain exact address, do not infer low/high pairing.'),
    ('eddfr_word0',0xd28,'ro',False,'Raw Debug feature aperture word at D28; no low/high pairing or breakpoint-count inference.'),
    ('eddfr_word1',0xd2c,'ro',False,'Raw Debug feature aperture word at D2C; no low/high pairing or breakpoint-count inference.'),
    ('edaa32pfr_word0',0xd60,'ro',False,'Raw AArch32 feature aperture word at D60; no low/high pairing or GIC-capacity inference.'),
    ('edaa32pfr_word1',0xd64,'ro',False,'Raw AArch32 feature aperture word at D64; no low/high pairing or GIC-capacity inference.'),
    ('dbgclaimset',0xfa0,'rw',False,'Current CoreSight claim tags; reading does not claim the component.'),
    ('dbgclaimclr',0xfa4,'rw',False,'Current CoreSight claim tags; reading does not release claims.'),
    ('dbgauthstatus',0xfb8,'ro',False,'Raw authentication status; not a grant of all system-register permissions.'),
    ('eddevaff0',0xfa8,'ro',False,'External affinity word zero; check against explicit board ownership.'),
    ('eddevaff1',0xfac,'ro',False,'External affinity word one; RES0 on R52.'),
    ('edlar',0xfb0,'wo',False,'Software lock access; no read request or writer.'),
    ('edlsr',0xfb4,'ro',False,'Software lock status; viewer never unlocks.'),
    ('eddevarch',0xfbc,'ro',False,'Debug architecture identification.'),
    ('eddevid2',0xfc0,'ro',False,'Raw device features.'),('eddevid1',0xfc4,'ro',False,'PC sample offset description.'),
    ('eddevid',0xfc8,'ro',False,'Profiling and auxiliary-register features.'),('eddevtype',0xfcc,'ro',False,'CoreSight component type.')]:
    fields = [field('EL',8,2,'Current debug exception level.'),field('HDD',16,1,'Hyp debug disable.'),field('ERR',6,1,'Cumulative error.'),field('ITE',24,1,'Instruction transfer completed.')] if name=='edscr' else []
    add(name,'debug_external',offset,'debug_external','core',description,access=access,effect=effect,fields=fields)
for n in range(8):
    for stem, base, meaning in [('dbgbvr',0x400,'Breakpoint address'),('dbgbcr',0x408,'Breakpoint control'),
                              ('dbgwvr',0x800,'Watchpoint address'),('dbgwcr',0x808,'Watchpoint control')]:
        fact = 'debug_external.breakpoints' if stem.startswith('dbgb') else 'debug_external.watchpoints'
        add(f'{stem}{n}', 'debug_external', base+n*16, 'debug_external', 'core',
            f'{meaning} {n}; R52 implements eight comparators. Observation never changes a debugger breakpoint/watchpoint.',
            access='rw', conditions=[(fact,n+1)])
for n in [6,7]:
    add(f'dbgbxvr{n}','debug_external',0x404+n*16,'debug_external','core','Context-aware breakpoint extended value; read does not reconfigure matching.',access='rw',conditions=[('debug_external.breakpoints',n+1)])
for n, offset in [(0,0xfe0),(1,0xfe4),(2,0xfe8),(3,0xfec),(4,0xfd0)]:
    add(f'edpidr{n}','debug_external',offset,'debug_external','core',f'Debug peripheral identification byte {n}.',fields=[field('IDENTITY_BYTE',0,8,'Raw peripheral identification byte.')])
for n in range(4):
    add(f'edcidr{n}','debug_external',0xff0+n*4,'debug_external','core',f'Debug component identification byte {n}.',fields=[field('IDENTITY_BYTE',0,8,'Raw component identification byte.')])
