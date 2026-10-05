// Deferred REG-404: read-only, explicitly halted R52 PMU and firmware baselines.
const fs=require('node:fs'),path=require('node:path'),assert=require('node:assert/strict');
const {root,hash,outputDirectory,parseOptions,Cases,Session}=require('./test-support/session.cjs');
const options=parseOptions(process.argv.slice(2),['--binary','--project','--core','--case'],['--run','--software-fixture']);
const phases=['PMU-H-STOP','PMU-H-PROBE','PMU-H-RAW','PMU-H-UNCHANGED','PMU-H-CLEANUP'];
const out=outputDirectory('register-pmu-hardware');
const suite=new Cases(out,{todo:'REG-404 PMU native widths/count/direct indices; no enable/reset/selector writes',
  layer:options['software-fixture']?'software fixture; no board':options.run?'explicit paused PMU fixture':'deferred; no target access',
  board_tests_executed:!!options.run&&!options['software-fixture']});
if(!options.run){for(const id of phases)suite.skip(id,'Read-only R52 PMU','Requires --run and a halted, independently captured firmware fixture');suite.finish();process.exit(0);}
assert(options.project&&options.core&&options.case,'--run requires --project FILE --core NAME --case JSON');
const binary=path.resolve(options.binary||path.join(root,'target/release/debugtui.exe'));
const project=path.resolve(options.project),caseFile=path.resolve(options.case);
for(const file of [binary,project,caseFile])assert(fs.existsSync(file),`Missing ${file}`);
const spec=JSON.parse(fs.readFileSync(caseFile));
assert(!spec.software_example||options['software-fixture'],'Replace software expectations before physical execution');
assert(spec.frame_function&&spec.expected_midr&&spec.evidence_source&&spec.ready,'Declare dedicated stop hook, actual MIDR and independent ready/baseline evidence');
assert(Array.isArray(spec.registers)&&spec.registers.length===23,'Declare all 23 adapted PMU registers');
assert(spec.registers.filter(e=>e.id==='pmccntr'&&e.bits===64).length===1);
const raw=(text,bits)=>{assert(new RegExp(`^0x[0-9a-f]{${bits/4}}$`,'i').test(text),`Exact ${bits}-bit raw required`);return BigInt(text);};
const reference=symbol=>assert(/^[a-zA-Z_]\w*(?:\[\d+\]|\.[a-zA-Z_]\w*)*$/.test(symbol),'Use one firmware variable/field/array element, no function evaluation');
reference(spec.ready);for(const e of spec.registers){reference(e.reference);raw(e.expected,e.bits);assert(e.bits===32||e.id==='pmccntr');}
assert.equal(new Set(spec.registers.map(e=>e.id)).size,23);
const projectHash=hash(project),samples=[];
Object.assign(suite.metadata,{binary,binary_sha256:hash(binary),project,project_sha256:projectHash,case_file:caseFile,case_sha256:hash(caseFile),evidence_source:spec.evidence_source,pmu_samples:samples});
let session,context,before,peerBefore;
const halted=async()=>{const s=await session.command('status');assert.equal(s.state,'STOPPED');assert.equal(s.frame.level,0);assert.equal(s.frame.function,spec.frame_function);};
const select=async core=>{if(core!=='default'||spec.peer_core)await session.command('select_core',{name:core});await halted();};
const read=async ids=>{
  const ctx=(await session.command('registers_list')).context;
  const response=await session.command('registers_read',{context:ctx,ids,manual:true});
  return Object.fromEntries(ids.map(id=>{
    const s=response.samples.find(s=>s.id===id);assert.equal(s?.state,'valid',`Fresh ${id}: ${s?.detail}`);
    assert.equal(s.owner,`core:${ctx.core}`);assert.equal(s.source,'openocd:aarch64 pmu');assert.equal(s.view,'physical_core');
    const bits=id==='pmccntr'?64:32;assert.equal(s.value.bits,bits);raw(s.value.hex,bits);
    const a=s.provenance?.access,e=a?.pmu;assert(e,'Fresh PMU proof missing');
    for(const f of ['midr','dscr','dspsr','dlr','id_dfr0','pmcr','hdcr','pmselr']){assert.equal(e[f]?.bits,32);raw(e[f].hex,32);}
    assert.equal(e.midr.hex,spec.expected_midr);assert.equal((raw(e.dscr.hex,32)>>8n)&3n,2n,'Current Debug EL2 required');
    assert.equal(raw(e.dspsr.hex,32)&31n,26n,'Dedicated firmware baseline stopped in Hyp');
    assert.equal((raw(e.pmcr.hex,32)>>11n)&31n,4n);assert.equal(e.read_method,bits===64?'mrrc64':'mrc32');
    assert.equal(a.phase,'responded');assert(Number.isSafeInteger(a.timestamp_ms)&&Number.isSafeInteger(a.completed_ms)&&a.completed_ms>=a.timestamp_ms);
    samples.push({id,core:ctx.core,value:s.value,evidence:e,host_request_interval_ms:[a.timestamp_ms,a.completed_ms]});
    return [id,s.value.hex];
  }));
};
const firmware=async symbol=>{const r=await session.command('evaluate',{expression:`(unsigned long long)${symbol}`});assert(/^(0x[0-9a-f]+|\d+)$/i.test(r.value));return BigInt(r.value);};
const controls=['pmcr','pmselr','pmcntenset','pmcntenclr','pmintenset','pmintenclr','pmovsr','pmovsset','pmccfiltr'];
(async()=>{
  try{
    session=new Session(binary,project,out);await session.command('connect');
    if(spec.control_scope){assert(['core','all'].includes(spec.control_scope));await session.command('control_scope',{scope:spec.control_scope});}
    if(!await suite.test(phases[0],'Capture halted owner, disabled counters and preserved controls',async()=>{
      await select(options.core);assert.equal(await firmware(spec.ready),1n,'Independent PMU baseline not ready');
      if(spec.peer_core){assert.notEqual(spec.peer_core,options.core);await select(spec.peer_core);peerBefore=await read(controls);}
      await select(options.core);context=(await session.command('registers_list')).context;assert.equal(context.core,options.core);
      before=await read(controls);assert.equal(raw(before.pmcr,32)&1n,0n,'Dedicated frozen baseline requires counters already disabled; reader does not disable them');
      const e=samples.at(-1).evidence;assert.equal(raw(e.hdcr.hex,32)&128n,0n,'Dedicated frozen baseline requires HDCR.HPME already clear');
      assert(raw(before.pmselr,32)<4n,'Dedicated full-bank baseline needs valid selected event');return {context,before,peerBefore};
    }))return;
    if(!await suite.test(phases[1],'Probe physical count with fresh current Debug EL2 evidence',async()=>{
      const r=await session.command('registers_probe',{context});assert.equal(r.probe.identity.model,'Cortex-R52');assert.equal(r.facts['pmu.counters'],4);
      assert.equal(r.probe.facts['pmu.counters'].source,'openocd:aarch64 pmu');return r;
    }))return;
    if(!await suite.test(phases[2],'Compare all raw values to independent firmware capture including high cycle word',async()=>{
      const values=await read(spec.registers.map(e=>e.id)),references={};
      for(const e of spec.registers){const baseline=await firmware(e.reference);assert.equal(baseline,raw(e.expected,e.bits),'Independent PMU baseline differs from declared expectation');assert.equal(raw(values[e.id],e.bits),baseline,`Native ${e.id} differs from firmware`);references[e.id]=baseline.toString();}
      assert(raw(values.pmccntr,64)>>32n,'Frozen cycle fixture must expose a nonzero high word');return {values,references};
    }))return;
    await suite.test(phases[3],'Preserve own/peer selectors, enable bits, overflow flags and project',async()=>{
      await select(options.core);assert.deepEqual(await read(controls),before);assert.equal(hash(project),projectHash);
      if(spec.peer_core){await select(spec.peer_core);assert.deepEqual(await read(controls),peerBefore);await select(options.core);}
      return {project_sha256:hash(project),peer_checked:!!spec.peer_core};
    });
  }catch(e){await suite.test('PMU-H-DRIVER','Driver failure',async()=>{throw e;});}
  finally{
    if(session)await suite.test(phases[4],'Disconnect without executing target firmware',()=>session.close());
    for(const id of phases)if(!suite.results.some(e=>e.id===id))suite.skip(id,'PMU fixture','Earlier precondition failed');
    suite.finish();
  }
})();
