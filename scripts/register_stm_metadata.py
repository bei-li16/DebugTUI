"""Arm STM v1.1 IHI0054B and STM-500 DDI0528B, control-register area.

The chip mapping is explicit. No stimulus address, control writer, unlock or
bank-selector transaction is provided. Other STM parts expose common IDs only.
"""
from register_mmio_metadata import field

STM_METADATA = {}


def add(name, component, offset, description, access='ro', fields=None, conditions=None):
    requirements = [('stm.present', 1, 1)]
    if component != 'stm':
        requirements.append((component + '.present', 1, 1))
    if offset < 0xf00 and offset not in (0xea0, 0xea4, 0xea8):
        requirements.append(('stm.part', 0x963, 0x963))
    STM_METADATA[name] = dict(component=component, offset=offset,
        group='stm_core' if component == 'stm' else component, scope='chip',
        bits=32, access=access, fields=fields or [], effect=False,
        conditions=[*requirements, *(conditions or [])], description=description,
        access_condition='Explicit chip owner and 4 KiB control aperture; '
            'fresh STM identity/features and optional control-class proof required. '
            'Power, authentication and bus permission remain separate. '
            'This viewer does not write controls, selectors, locks or stimulus ports. '
            'Event and stimulus masks describe the currently selected bank only.')


def mask(name, description):
    return [field(name, 0, 32, description)]


for n in range(4):
    add(f'stm_cidr{n}', 'stm', 0xff0 + 4*n, f'CoreSight component identification byte {n}.',
        fields=[field('ID', 0, 8, 'Raw identification byte.')])
for n, offset in [(0, 0xfe0), (1, 0xfe4), (2, 0xfe8), (3, 0xfec), (4, 0xfd0)]:
    add(f'stm_pidr{n}', 'stm', offset, f'Peripheral identification byte {n}; CPU MIDR does not identify STM.',
        fields=[field('ID', 0, 8, 'Raw identification byte.')])
add('stm_devarch', 'stm', 0xfbc, 'Architectural identity; this adapter requires Arm STM v1.1.', fields=[
    field('ARCHID', 0, 16, 'STM architecture identifier 0x0A63.'),
    field('REVISION', 16, 4, 'Architecture revision.'), field('PRESENT', 20, 1, 'Architectural identity present.'),
    field('ARCHITECT', 21, 11, 'Architecture designer.')])
add('stm_devtype', 'stm', 0xfcc, 'Trace-source device classification; adapted STM value 0x63.',
    fields=[field('MAJOR', 0, 4, 'Major device class.'), field('SUB', 4, 4, 'Device subclass.')])
add('stm_devid', 'stm', 0xfc8, 'Implemented stimulus-port count; reading this ID sends no stimulus.',
    fields=[field('NUMSP', 0, 17, 'Stimulus ports, up to 65536.')])
for n, fields in [
    (1, [('PROT',0,4),('TS',4,2),('TSFREQ',6,1),('FORCETS',7,1),('SYNC',8,2),
         ('TRACEBUS',10,4),('TRIGCTL',14,2),('TSPRESCALE',16,2),('HWTEN',18,2),('SYNCEN',20,2),('SWOEN',22,2)]),
    (2, [('SPTER',0,2),('SPER',2,1),('SPCOMP',4,2),('SPOVERRIDE',6,1),('PRIVMASK',7,2),
         ('SPTRTYPE',9,2),('DSIZE',12,4),('SPTYPE',16,2)]),
    (3, [('NUMMAST',0,7)])]:
    add(f'stm_feat{n}r', 'stm', 0xea0 + 4*(n-1), f'Raw architectural feature word {n}; undefined/reserved capability encodings remain unknown.',
        fields=[field(name, off, width, 'Raw architectural feature encoding.') for name,off,width in fields])
for name, off, description, condition in [
    ('stm_sper',0xe00,'Stimulus enable mask for the current port bank.', [('stm.sper',1,1)]),
    ('stm_spter',0xe20,'Stimulus trigger mask for the current port bank.', [('stm.sptrigger',1,1)]),
    ('stm_spscr',0xe60,'Current stimulus-port selector; this reader preserves it.', []),
    ('stm_spmscr',0xe64,'Current stimulus master selector; this reader preserves it.', []),
    ('stm_spoverrider',0xe68,'Port override configuration; no override is applied.', [('stm.override',1,1)]),
    ('stm_spmoverrider',0xe6c,'Master override configuration; no override is applied.', [('stm.override',1,1)]),
    ('stm_sptrigcsr',0xe70,'Stimulus trigger configuration and status.', [('stm.trigger_control',1,2)]),
    ('stm_tsfreqr',0xe8c,'Declared trace timestamp frequency.', [('stm.timestamp_frequency',1,1)]),
    ('stm_syncr',0xe90,'Synchronization configuration; no packets are generated.', [('stm.sync',1,1)]),
    ('stm_auxcr',0xe94,'Raw STM-500 implementation-specific auxiliary control.', [])]:
    add(name,'stm',off,description,'rw',mask('RAW','Raw configuration; masks apply to the current selected bank.'),condition)
add('stm_tcsr','stm',0xe80,'STM-500 trace configuration and busy status; this viewer does not enable tracing.','rw',[
    field('EN',0,1,'Trace enable.','rw'), field('TSEN',1,1,'Timestamp enable.','rw'),
    field('SYNCEN',2,1,'Synchronization enable.','ro'), field('COMPEN',5,1,'Compression enable.','rw'),
    field('TRACEID',16,7,'Trace source ID.','rw'), field('BUSY',23,1,'Trace activity busy.','ro')])
for name,off,access,meaning in [('stm_itctrl',0xf00,'rw','Integration mode configuration'),
    ('stm_claimset',0xfa0,'rw','Implemented claim tag mask'),('stm_claimclr',0xfa4,'rw','Current claim tags'),
    ('stm_lsr',0xfb4,'ro','Software lock status'),('stm_authstatus',0xfb8,'ro','Debug authentication status')]:
    add(name,'stm',off,meaning+'; displayed raw, with no writes.',access,mask('RAW',meaning+'.'))
for name,off,meaning in [('stm_tsstimr',0xe84,'Timestamp stimulus command'),('stm_lar',0xfb0,'Software unlock command')]:
    add(name,'stm',off,meaning+'; write-only and never read or written by this viewer.','wo')
for name,off,meaning,access,condition in [
    ('stm_heer',0xd00,'Hardware event enable mask for the current bank','rw',[]),
    ('stm_heter',0xd20,'Hardware event trigger mask for the current bank','rw',[('stm_hwe.trigger',1,1)]),
    ('stm_hebsr',0xd60,'Current hardware event bank selector; preserved','rw',[('stm_hwe.events',33,256)]),
    ('stm_hemcr',0xd64,'Hardware event main control','rw',[]),
    ('stm_heextmuxr',0xd68,'External hardware event multiplexer','rw',[('stm_hwe.mux',1,5)]),
    ('stm_hemasterr',0xdf4,'Hardware event master assignment','ro',[])]:
    add(name,'stm_hwe',off,meaning+'. Explicit optional interface mapping required.',access,mask('RAW',meaning+'.'),condition)
add('stm_heidr','stm_hwe',0xdfc,'Optional hardware-event control class and revision.',fields=[
    field('CLASS',0,4,'Adapted control class 1.'),field('CLASSREV',4,4,'Adapted class revision 0 or 1.')])
add('stm_hefeat1r','stm_hwe',0xdf8,'Optional hardware-event capacity; core feature HWTEN does not prove this interface absent.',fields=[
    field('HETER',0,1,'Trigger mask implemented.'),field('NUMHE',15,9,'Hardware events, up to 256.'),
    field('HEEXTMUX',28,3,'Multiplexer width encoding.')])
for name,off,access,meaning in [('stm_dmastatr',0xc0c,'ro','DMA status'),('stm_dmactlr',0xc10,'rw','DMA control'),
    ('stm_dmaidr',0xcfc,'ro','Optional DMA control class'),('stm_dmastart',0xc04,'wo','DMA start command'),
    ('stm_dmastop',0xc08,'wo','DMA stop command')]:
    add(name,'stm_dma',off,meaning+'; explicit adapted class 2 interface required, no command is issued.',access,mask('RAW',meaning+'.') if access != 'wo' else [])
