// Deferred R52 read-only cases. Default mode performs no target I/O.
const fs=require('node:fs'),path=require('node:path'),assert=require('node:assert/strict');
const {hash,outputDirectory,parseOptions,Cases,Session}=require('./test-support/session.cjs');
const options=parseOptions(process.argv.slice(2),['--binary','--project','--core','--case'],['--run','--software-fixture']);
const phases=['R52-CORE-H01-ROUTE','R52-CORE-H02-VALUES','R52-CORE-H03-PEER','R52-CORE-H04-PRESERVATION'];
const out=outputDirectory('register-r52-core-hardware');
const suite=new Cases(out,{board_tests_executed:!!options.run&&!options['software-fixture'],
  layer:options['software-fixture']?'actual EXE/MI/Tcl software fixture; no board':options.run?'explicit stopped R52 core check':'SKIPPED; zero target I/O'});
if(!options.run){for(const id of phases)suite.skip(id,'R52 bounded read check','Requires --run, binary/project/core and independent board expectations');suite.finish();process.exit(0);}
assert(options.binary&&options.project&&options.core&&options.case,'--run requires --binary FILE --project FILE --core NAME --case JSON');
const binary=path.resolve(options.binary),project=path.resolve(options.project),caseFile=path.resolve(options.case);
for(const file of [binary,project,caseFile])assert(fs.existsSync(file),`Missing input: ${file}`);
const spec=JSON.parse(fs.readFileSync(caseFile));
assert(!spec.software_example||options['software-fixture'],'Replace software example with independent board evidence');
for(const key of ['revision','probe','openocd_version','gdb_version','evidence_source','target','endpoint','expected_midr'])assert(typeof spec[key]==='string'&&spec[key],`Declare ${key}`);
assert(Number.isInteger(spec.ap)&&spec.ap>=0,'Declare the selected core AP');
const exact=raw=>{assert(/^0x[0-9a-f]{8}$/i.test(raw),`Expected exact 32-bit raw value: ${raw}`);return BigInt(raw);};
exact(spec.expected_midr);
const scalars=['midr','mpidr','sctlr','hsctlr','cpacr','hcr','mpuir','hmpuir','prselr','hprselr','hprenr','mair0','mair1','hmair0','hmair1'];
const expected=values=>{assert(values&&Object.keys(values).length);for(const [id,raw] of Object.entries(values)){const region=/^(h?)pr[bl]ar(0|[1-9][0-9]?)$/.exec(id);assert(scalars.includes(id)||(region&&Number(region[2])<24),`Unreviewed read: ${id}`);exact(raw);}};
expected(spec.values);
assert(['midr','mpidr','sctlr','mpuir','hmpuir'].every(id=>Object.hasOwn(spec.values,id)),'Declare identity/control and both capacity baselines');
if(spec.peer_core){assert.notEqual(spec.peer_core,options.core);expected(spec.peer_values);assert(spec.peer_target,'Declare peer target');}
const original=hash(project);
Object.assign(suite.metadata,{binary,binary_sha256:hash(binary),project,project_sha256:original,core:options.core,case_file:caseFile,case_sha256:hash(caseFile),fixture:spec});
let session,context,before;
const select=async core=>{if(core!=='default')await session.command('select_core',{name:core});};
const read=async(core,target,values)=>{
  const list=await session.command('registers_list');assert.equal(list.context.core,core);assert.equal(list.context.frame,0);
  const result=await session.command('registers_read',{context:list.context,ids:Object.keys(values),manual:true});
  for(const [id,raw] of Object.entries(values)){
    const sample=result.samples.find(s=>s.id===id);assert.equal(sample?.state,'valid',`${id}: ${sample?.detail}`);
    assert.equal(sample.owner,`core:${core}`);assert.equal(sample.view,'physical_core');assert.equal(sample.source,'openocd:aarch64 r52_read');
    assert.equal(exact(sample.value.hex),exact(raw));assert.equal(sample.value.bits,32);
    const access=sample.provenance.access,proof=access.r52_core;
    assert.equal(access.phase,'responded');assert.deepEqual(access.context,list.context);assert(access.completed_ms>=access.timestamp_ms);
    assert.equal(access.route.target,target);assert.equal(access.route.endpoint,spec.endpoint);
    assert.equal(exact(proof.midr.hex),exact(spec.expected_midr));
    const dscr=exact(proof.dscr.hex);assert.equal((dscr>>8n)&3n,2n);assert.equal(dscr&0x8000n,0n);assert(dscr&0x1000000n);
    assert.equal(proof.dspsr.bits,32);assert.equal(proof.dlr.bits,32);
    if(spec.expected_dspsr)assert.equal(exact(proof.dspsr.hex),exact(spec.expected_dspsr));
  }
  return result;
};
(async()=>{try{
  session=new Session(binary,project,out);await session.command('connect');await session.command('control_scope',{scope:'all'});await select(options.core);
  if(!await suite.test(phases[0],'Inspect selected R52 identity and native capacity provenance',async()=>{
    assert.equal((await session.command('status')).state,'STOPPED');
    const list=await session.command('registers_list');context=list.context;assert.equal(context.core,options.core);assert.equal(context.frame,0);assert.equal(list.catalogue.cpu,'cortex-r52');
    const inspected=await session.command('registers_probe',{context});assert.equal(inspected.probe.identity.model,'Cortex-R52');
    for(const id of ['midr','mpuir','hmpuir']){const sample=inspected.probe.samples.find(s=>s.id===id);assert.equal(sample?.state,'valid');assert.equal(sample.source,'openocd:aarch64 r52_read');assert(sample.provenance.access.r52_core);}
    return inspected;
  }))return;
  if(!await suite.test(phases[1],'Read independent baseline with current Debug evidence',async()=>{before=await read(options.core,spec.target,spec.values);return before;}))return;
  if(spec.peer_core){
    if(!await suite.test(phases[2],'Select peer explicitly and keep same-address values isolated',async()=>{
      await select(spec.peer_core);const peer=(await session.command('registers_list')).context;await session.command('registers_probe',{context:peer});
      const result=await read(spec.peer_core,spec.peer_target,spec.peer_values);await select(options.core);return result;
    }))return;
  }else suite.skip(phases[2],'Peer isolation','No separate physical peer provided; run the multicore case later');
  await suite.test(phases[3],'Re-read stable controls/selectors and retain project bytes',async()=>{
    const after=await read(options.core,spec.target,spec.values);
    for(const sample of after.samples){const previous=before.samples.find(s=>s.id===sample.id);assert.deepEqual(sample.value,previous.value);for(const key of ['dspsr','dlr'])assert.deepEqual(sample.provenance.access.r52_core[key],previous.provenance.access.r52_core[key]);}
    assert.equal(hash(project),original);assert(!session.logs('mi>').some(log=>/\b-data-write-|\b-var-assign|\bexec-(continue|step)/.test(log.text)));
    assert(!session.logs('tcl>').some(log=>/\b(mcr|vfp_write|resume|reset)\b/.test(log.text)));return after;
  });
}catch(error){await suite.test('R52-CORE-SETUP','Prepare the explicit R52 environment',async()=>{throw error;});}
finally{if(session)await suite.test('R52-CORE-CLEANUP','Close this dedicated test session',()=>session.close());for(const id of phases)if(!suite.results.some(r=>r.id===id))suite.skip(id,'R52 bounded read phase','Earlier prerequisite failed');suite.finish();}})();
