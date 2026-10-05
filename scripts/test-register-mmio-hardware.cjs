// Deferred REG-405/BUS-006. No target access without explicit --run.
const fs=require('node:fs'),path=require('node:path'),assert=require('node:assert/strict');
const {root,hash,outputDirectory,parseOptions,Cases,Session}=require('./test-support/session.cjs');
const options=parseOptions(process.argv.slice(2),['--binary','--project','--core','--case'],['--run','--software-fixture']);
const phases=['MMIO-H-STOP','MMIO-H-IDENTITY','MMIO-H-RAW','MMIO-H-UNCHANGED','MMIO-H-CLEANUP'];
// Independent, fixed observational subset of R52 TRM Tables 10-4/35/36/12-5.
// GICR base is the control page. Its private interrupt page is +0x10000.
const registers=[
  ['gicd_ctlr','gicd',0,32],['gicd_typer','gicd',4,32],['gicd_iidr','gicd',8,32],
  ['gicr_ctlr','gicr',0,32],['gicr_iidr','gicr',4,32],['gicr_typer','gicr',8,64],
  ['gicr_waker','gicr',0x14,32],['gicr_igroupr0','gicr',0x10080,32],
  ['gicr_isenabler0','gicr',0x10100,32],['gicr_icenabler0','gicr',0x10180,32],
  ['gicr_icfgr0','gicr',0x10c00,32],['gicr_icfgr1','gicr',0x10c04,32],
  ['gicr_ipriorityr0','gicr',0x10400,32],['gicd_irouter32','gicd',0x6100,64],
  ['ed_midr','debug_external',0xd00,32],
  ['edpidr0','debug_external',0xfe0,32],['edpidr1','debug_external',0xfe4,32],
  ['edpidr2','debug_external',0xfe8,32],['edpidr3','debug_external',0xfec,32],
  ['edpidr4','debug_external',0xfd0,32],
  ['edcidr0','debug_external',0xff0,32],['edcidr1','debug_external',0xff4,32],
  ['edcidr2','debug_external',0xff8,32],['edcidr3','debug_external',0xffc,32]
];
const controls=['gicd_ctlr','gicr_waker','gicr_igroupr0','gicr_isenabler0','gicr_icenabler0','gicr_icfgr0','gicr_icfgr1','gicr_ipriorityr0','gicd_irouter32'];
const out=outputDirectory('register-mmio-hardware');
const suite=new Cases(out,{todo:'REG-405/BUS-006 MMIO owner/address and independent raw baselines',layer:options['software-fixture']?'software fixture; no board':options.run?'explicit static halted MMIO fixture':'deferred; no target access',board_tests_executed:!!options.run&&!options['software-fixture']});
if(!options.run){for(const id of phases)suite.skip(id,'R52 owner-scoped MMIO','Requires --run, verified board mapping, static state and an independent per-core firmware capture');suite.finish();process.exit(0);}
assert(options.project&&options.core&&options.case,'--run requires --project FILE --core NAME --case JSON');
const binary=path.resolve(options.binary||path.join(root,'target/release/debugtui.exe')),project=path.resolve(options.project),caseFile=path.resolve(options.case);
for(const file of [binary,project,caseFile])assert(fs.existsSync(file),'Missing '+file);
const spec=JSON.parse(fs.readFileSync(caseFile));
assert(!spec.software_example||options['software-fixture'],'Replace software expectations before physical execution');
assert(spec.frame_function&&spec.evidence_source&&spec.ready&&spec.mapping_verified===true,'Independent stopped capture and verified mapping required');
const raw=(text,bits)=>{assert(new RegExp('^0x[0-9a-f]{'+bits/4+'}$','i').test(text),'Exact raw '+bits+' bits required');return BigInt(text);};
const variable=s=>assert(/^[a-zA-Z_]\w*(?:\[\d+\]|\.[a-zA-Z_]\w*)*$/.test(s),'Use a firmware variable/field/element, without function evaluation');
variable(spec.ready);raw(spec.expected_midr,32);
assert.equal(raw(spec.expected_midr,32)&0xff0ffff0n,0x410fd130n,'This fixture requires an independently identified Arm/D13 R52');
assert(Array.isArray(spec.registers)&&spec.registers.length===registers.length);
assert.deepEqual(spec.registers.map(e=>e.id).sort(),registers.map(e=>e[0]).sort());
for(const [id,component,offset,bits] of registers){
  const e=spec.registers.find(e=>e.id===id),c=spec.components?.[component];
  assert.equal(e.bits,bits);variable(e.reference);raw(e.expected,bits);
  assert(c&&c.owner&&c.route&&typeof c.little_endian==='boolean','Declare owner, base and actual route for '+component);
  assert(/^0x[0-9a-f]{8}$/i.test(c.base),'Exact 32-bit component base required');
  assert(BigInt(c.base)+BigInt(offset)+BigInt(bits/8-1)<=0xffffffffn,'R52 aperture address overflow');
  assert(['gdb_memory','tcl_memory'].includes(c.route.kind)&&c.route.endpoint,'Actual transport kind/endpoint required');
  if(c.route.kind==='tcl_memory')assert(c.route.target&&c.route.channel&&c.route.configuration_source,'Actual AP target/channel/source required');
}
const projectHash=hash(project),samples=[];
Object.assign(suite.metadata,{binary,binary_sha256:hash(binary),project,project_sha256:projectHash,case_file:caseFile,case_sha256:hash(caseFile),evidence_source:spec.evidence_source,samples});
let session,before,context;
const firmware=async symbol=>{const r=await session.command('evaluate',{expression:'(unsigned long long)'+symbol});assert(/^(0x[0-9a-f]+|\d+)$/i.test(r.value));return BigInt(r.value);};
const read=async ids=>{
  const list=await session.command('registers_list'),ctx=list.context;
  const r=await session.command('registers_read',{context:ctx,ids});
  return Object.fromEntries(ids.map(id=>{
    const [,component,offset,bits]=registers.find(e=>e[0]===id),c=spec.components[component];
    const s=r.samples.find(s=>s.id===id);
    assert.equal(s?.state,'valid','Fresh '+id+': '+s?.detail);assert.equal(s.owner,c.owner);
    assert.equal(s.source,'mmio:'+component);assert.equal(s.value.bits,bits);raw(s.value.hex,bits);
    assert(s.eligibility.conditions.every(c=>c.source==='configuration'),'MMIO capability conditions must retain their actual declared source');
    const a=s.provenance?.access,route=a?.route;
    assert.equal(a?.phase,'responded');assert.deepEqual(a.context,ctx);
    assert(Number.isSafeInteger(a.timestamp_ms)&&Number.isSafeInteger(a.completed_ms)&&a.completed_ms>=a.timestamp_ms);
    assert.equal(route?.kind,c.route.kind);assert.equal(route.endpoint,c.route.endpoint);
    assert.equal(route.address,'0x'+(BigInt(c.base)+BigInt(offset)).toString(16));
    assert.equal(route.bits,bits);assert.equal(route.byte_order,c.little_endian?'little':'big');
    if(route.kind==='tcl_memory'){assert.equal(route.target,c.route.target);assert.equal(route.channel,c.route.channel);assert.equal(route.configuration_source,c.route.configuration_source);assert.equal(route.bus_width,32);assert.equal(route.count,bits/32);assert.equal(route.atomic,false);}
    assert.equal(s.provenance.catalogue_reader.require_owner_mapping,true);
    // Coordinator workers supply a shared-owner generation. A single session
    // supplies only its own generation, already matched by the exact context.
    if(component==='gicd'&&list.owner_generations){assert(Number.isSafeInteger(s.owner_generation));assert.equal(s.owner_generation,list.owner_generations[c.owner]);}
    else if(component==='gicd')assert(!('owner_generation' in s));
    samples.push(s);return [id,s.value.hex];
  }));
};
(async()=>{
  try{
    session=new Session(binary,project,out);await session.command('connect');
    if(!await suite.test(phases[0],'Require stopped physical frame, capture readiness and explicit owner mappings',async()=>{
      if(options.core!=='default')await session.command('select_core',{name:options.core});
      const status=await session.command('status');assert.equal(status.state,'STOPPED');assert.equal(status.frame.level,0);assert.equal(status.frame.function,spec.frame_function);
      assert.equal(await firmware(spec.ready),1n,'Independent MMIO baseline not ready');
      const list=await session.command('registers_list');context=list.context;assert.equal(context.core,options.core);
      for(const [id,component,offset,bits] of registers){const r=list.catalogue.registers.find(r=>r.id===id);assert(r&&r.access!=='wo'&&!r.read_side_effect,'Only observational catalogue entries accepted');assert.equal(r.bits,bits);assert.equal(r.reader.kind,'mmio');assert.equal(r.reader.component,component);assert.equal(r.reader.offset,offset);assert.equal(r.reader.require_owner_mapping,true);assert.equal(r.scope,component==='gicd'?'cluster':'core');if(component!=='gicd')assert.equal(spec.components[component].owner,'core:'+options.core);else assert(spec.components[component].owner.startsWith('cluster:'));}
      return {context,mapping:spec.components};
    }))return;
    if(!await suite.test(phases[1],'Validate fresh external MIDR and retain declared capacity provenance',async()=>{
      const identity=await read(['ed_midr']);assert.equal(identity.ed_midr,spec.expected_midr);
      before=await read(controls);return {identity,before,capacity_source:'configuration; not a fresh physical capability Probe'};
    }))return;
    if(!await suite.test(phases[2],'Compare 24 exact-width MMIO values with independent firmware RAM',async()=>{
      const values=await read(registers.map(e=>e[0])),references={};
      for(const e of spec.registers){const baseline=await firmware(e.reference);assert.equal(baseline,raw(e.expected,e.bits),'Independent MMIO baseline differs from declared expectation');assert.equal(raw(values[e.id],e.bits),baseline,'MMIO '+e.id+' differs from firmware');references[e.id]=baseline.toString();}
      return {values,references,wide_mmio_atomic:false};
    }))return;
    await suite.test(phases[3],'Preserve static interrupt controls, stopped state and project',async()=>{
      assert.deepEqual(await read(controls),before);const status=await session.command('status');assert.equal(status.state,'STOPPED');assert.equal(hash(project),projectHash);return {project_sha256:hash(project)};
    });
  }catch(e){await suite.test('MMIO-H-DRIVER','Driver failure',async()=>{throw e;});}
  finally{if(session)await suite.test(phases[4],'Disconnect without executing firmware',()=>session.close());for(const id of phases)if(!suite.results.some(e=>e.id===id))suite.skip(id,'R52 MMIO fixture','Earlier precondition failed');suite.finish();}
})();
