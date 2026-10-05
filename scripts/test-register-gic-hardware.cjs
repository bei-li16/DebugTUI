// Deferred REG-408/405: observing paused per-core GIC state never acknowledges interrupts.
const fs=require('node:fs'),path=require('node:path'),assert=require('node:assert/strict');
const {root,hash,outputDirectory,parseOptions,Cases,Session}=require('./test-support/session.cjs');
const options=parseOptions(process.argv.slice(2),['--binary','--project','--core','--case'],['--run','--software-fixture']);
const phases=['GIC-H-STOP','GIC-H-PROBE','GIC-H-RAW','GIC-H-UNCHANGED','GIC-H-CLEANUP'];
const ids=['icc_ctlr','icc_sre','icc_hsre','icc_pmr','icc_rpr','icc_bpr0','icc_bpr1','icc_igrpen0','icc_igrpen1','icc_hppir0','icc_hppir1','icc_ap0r0','icc_ap1r0','ich_vtr','ich_hcr','ich_misr','ich_eisr','ich_elrsr','ich_vmcr','ich_ap0r0','ich_ap1r0','ich_lr0','ich_lr1','ich_lr2','ich_lr3','ich_lrc0','ich_lrc1','ich_lrc2','ich_lrc3'];
const out=outputDirectory('register-gic-hardware');
const suite=new Cases(out,{todo:'REG-408; physical ICC and virtual ICV/ICH AP capacity, no acknowledge/enable',layer:options['software-fixture']?'software fixture; no board':options.run?'explicit halted GIC fixture':'deferred; no target access',board_tests_executed:!!options.run&&!options['software-fixture']});
if(!options.run){for(const id of phases)suite.skip(id,'Observational R52 GIC','Requires --run, static interrupt state and independent per-core EL2 firmware capture');suite.finish();process.exit(0);}
assert(options.project&&options.core&&options.case,'--run requires --project FILE --core NAME --case JSON');
const binary=path.resolve(options.binary||path.join(root,'target/release/debugtui.exe')),project=path.resolve(options.project),caseFile=path.resolve(options.case);
for(const file of [binary,project,caseFile])assert(fs.existsSync(file),`Missing ${file}`);
const spec=JSON.parse(fs.readFileSync(caseFile));
assert(!spec.software_example||options['software-fixture'],'Replace software expectations before physical execution');
assert(spec.frame_function&&spec.expected_midr&&spec.evidence_source&&spec.ready,'Declare stopped hook, identity and independent ready/baseline evidence');
assert(Array.isArray(spec.registers)&&spec.registers.length===ids.length);
assert.deepEqual(spec.registers.map(e=>e.id).sort(),[...ids].sort(),'Only the fixed 29 observational registers are accepted');
const raw=text=>{assert(/^0x[0-9a-f]{8}$/i.test(text),'Exact raw 32 bits required');return BigInt(text);};
const reference=symbol=>assert(/^[a-zA-Z_]\w*(?:\[\d+\]|\.[a-zA-Z_]\w*)*$/.test(symbol),'Use firmware variable/field/array element; no function evaluation');
reference(spec.ready);if(spec.peer_core){assert(spec.peer_ready,'Peer capture readiness required');reference(spec.peer_ready);}
for(const e of spec.registers){reference(e.reference);raw(e.expected);assert.equal(e.bits,32);}
const projectHash=hash(project),samples=[];
Object.assign(suite.metadata,{binary,binary_sha256:hash(binary),project,project_sha256:projectHash,case_file:caseFile,case_sha256:hash(caseFile),evidence_source:spec.evidence_source,gic_samples:samples});
let session,context,before,peerBefore;
const select=async core=>{if(core!=='default'||spec.peer_core)await session.command('select_core',{name:core});const s=await session.command('status');assert.equal(s.state,'STOPPED');assert.equal(s.frame.level,0);assert.equal(s.frame.function,spec.frame_function);};
const firmware=async symbol=>{const r=await session.command('evaluate',{expression:`(unsigned long long)${symbol}`});assert(/^(0x[0-9a-f]+|\d+)$/i.test(r.value));return BigInt(r.value);};
const read=async requested=>{
  const ctx=(await session.command('registers_list')).context;
  const r=await session.command('registers_read',{context:ctx,ids:requested,manual:true});
  return Object.fromEntries(requested.map(id=>{
    const s=r.samples.find(s=>s.id===id);assert.equal(s?.state,'valid',`Fresh ${id}: ${s?.detail}`);assert.equal(s.owner,`core:${ctx.core}`);assert.equal(s.source,'openocd:aarch64 gic');assert.equal(s.view,'physical_core');assert.equal(s.value.bits,32);raw(s.value.hex);
    const a=s.provenance?.access,e=a?.gic;assert(e,'Fresh GIC evidence missing');
    for(const field of ['midr','dscr','dspsr','dlr','id_pfr1','icc_hsre','icc_sre','icc_ctlr','ich_vtr','hcr','ich_hcr','hstr']){assert.equal(e[field]?.bits,32);raw(e[field].hex);}
    assert.equal(e.midr.hex,spec.expected_midr);assert.equal((raw(e.dscr.hex)>>8n)&3n,2n);assert.equal(raw(e.dspsr.hex)&31n,26n,'Independent firmware capture must stop in Hyp');
    assert.equal(raw(e.id_pfr1.hex)>>28n,1n);assert.equal(raw(e.icc_hsre.hex),15n);assert.equal(raw(e.icc_sre.hex),7n);assert.equal(raw(e.icc_ctlr.hex)&~3n,0x400n);assert.equal(raw(e.ich_vtr.hex),0x90180003n);
    assert.equal(e.view,id.startsWith('ich_')?'hypervisor_ich':'physical_icc');assert.equal(e.read_method,'mrc32');assert.equal(a.phase,'responded');assert(Number.isSafeInteger(a.timestamp_ms)&&Number.isSafeInteger(a.completed_ms)&&a.completed_ms>=a.timestamp_ms);
    samples.push({id,core:ctx.core,value:s.value,evidence:e,host_request_interval_ms:[a.timestamp_ms,a.completed_ms]});return [id,s.value.hex];
  }));
};
const controls=['icc_ctlr','icc_sre','icc_hsre','icc_pmr','icc_bpr0','icc_bpr1','icc_igrpen0','icc_igrpen1','ich_hcr','ich_vmcr','icc_ap0r0','icc_ap1r0','ich_ap0r0','ich_ap1r0'];
(async()=>{
  try{
    session=new Session(binary,project,out);await session.command('connect');
    if(spec.control_scope){assert(['core','all'].includes(spec.control_scope));await session.command('control_scope',{scope:spec.control_scope});}
    if(!await suite.test(phases[0],'Capture ready halted owner and unchanged control/active-priority state',async()=>{
      await select(options.core);assert.equal(await firmware(spec.ready),1n,'Independent GIC baseline not ready');
      if(spec.peer_core){assert.notEqual(spec.peer_core,options.core);await select(spec.peer_core);assert.equal(await firmware(spec.peer_ready),1n);peerBefore=await read(controls);}
      await select(options.core);context=(await session.command('registers_list')).context;assert.equal(context.core,options.core);before=await read(controls);return {context,before,peerBefore};
    }))return;
    if(!await suite.test(phases[1],'Probe physical ICC and virtual ICV/ICH capacities independently',async()=>{
      const r=await session.command('registers_probe',{context});assert.equal(r.probe.identity.model,'Cortex-R52');
      for(const key of ['icc.physical.pribits','icc.physical.prebits','icv.virtual.pribits','icv.virtual.prebits']){assert.equal(r.facts[key],5);assert.equal(r.probe.facts[key].source,'openocd:aarch64 gic');}
      assert.equal(r.facts['ich.list_registers'],4);
      const absent=[];for(const prefix of ['icc','ich','icv'])for(let bank=0;bank<2;bank++)for(let n=1;n<4;n++)absent.push(`${prefix}_ap${bank}r${n}`);
      const filtered=await session.command('registers_read',{context,ids:absent,manual:true});assert(filtered.samples.every(s=>s.implementation==='no'&&s.reason==='hardware_not_implemented'&&s.value===null));return {probe:r,filtered};
    }))return;
    if(!await suite.test(phases[2],'Compare 29 separate raw MRC values with independent firmware capture',async()=>{
      const values=await read(ids),references={};for(const e of spec.registers){const baseline=await firmware(e.reference);assert.equal(baseline,raw(e.expected),'Independent GIC baseline differs from declared expectation');assert.equal(raw(values[e.id]),baseline,`Native ${e.id} differs from firmware`);references[e.id]=baseline.toString();}return {values,references};
    }))return;
    await suite.test(phases[3],'Preserve own/peer controls and project without acknowledge or enable',async()=>{
      await select(options.core);assert.deepEqual(await read(controls),before);assert.equal(hash(project),projectHash);
      if(spec.peer_core){await select(spec.peer_core);assert.deepEqual(await read(controls),peerBefore);await select(options.core);}return {project_sha256:hash(project),peer_checked:!!spec.peer_core};
    });
  }catch(e){await suite.test('GIC-H-DRIVER','Driver failure',async()=>{throw e;});}
  finally{if(session)await suite.test(phases[4],'Disconnect without executing firmware',()=>session.close());for(const id of phases)if(!suite.results.some(e=>e.id===id))suite.skip(id,'GIC fixture','Earlier precondition failed');suite.finish();}
})();
