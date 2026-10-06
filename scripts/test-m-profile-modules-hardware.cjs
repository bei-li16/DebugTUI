// Deferred Debug/DWT/FPB/FPU cases. Default execution creates SKIPPED evidence and never connects.
const fs = require('node:fs'), path = require('node:path'), assert = require('node:assert/strict');
const {hash, outputDirectory, parseOptions, Cases, Session} = require('./test-support/session.cjs');
const options = parseOptions(process.argv.slice(2), ['--binary','--project','--core','--case'], ['--run','--software-fixture']);
const ids = ['M-MOD-H01-IDENTITY','M-MOD-H02-DEBUG','M-MOD-H03-DHCSR','M-MOD-H04-FPU','M-MOD-H05-UNCHANGED'];
const out = outputDirectory('m-profile-modules-hardware');
const suite = new Cases(out, {board_tests_executed:!!options.run && !options['software-fixture'],
  layer:options['software-fixture'] ? 'actual EXE/Coordinator/MI software fixture, no board' : options.run ? 'explicit halted board' : 'SKIPPED, zero target I/O'});
if (!options.run) { for (const id of ids) suite.skip(id,'M Debug/DWT/FPB/FPU module','Requires explicit --run and independent project/core/case inputs'); suite.finish(); process.exit(0); }
assert(options.binary && options.project && options.core && options.case, '--run requires --binary FILE --project FILE --core NAME --case JSON');
const binary=path.resolve(options.binary), project=path.resolve(options.project), caseFile=path.resolve(options.case);
for (const file of [binary,project,caseFile]) assert(fs.existsSync(file),`Missing input: ${file}`);
const spec=JSON.parse(fs.readFileSync(caseFile));
assert(!spec.software_example || options['software-fixture'],'Replace software example with actual independently recorded board expectations');
assert(['cortex-m3','cortex-m4','cortex-m7'].includes(spec.cpu));
for (const key of ['revision','probe','openocd_version','gdb_version','frame_function','evidence_source','target','endpoint']) assert(typeof spec[key]==='string' && spec[key],`Declare ${key}`);
assert(Number.isInteger(spec.ap) && spec.ap>=0,'Declare selected core AP');
const raw=(value,bits=32)=>{assert(new RegExp(`^0x[0-9a-f]{${bits/4}}$`,'i').test(value),`Expected exact ${bits}-bit value: ${value}`);return BigInt(value);};
raw(spec.cpuid); raw(spec.demcr);raw(spec.fpb.ctrl);
assert(typeof spec.trace_enabled==='boolean');assert.equal(Boolean(raw(spec.demcr)&(1n<<24n)),spec.trace_enabled);
assert(spec.manual_dhcsr===true,'Explicit case consent is required for the one read that clears sticky DHCSR flags');raw(spec.dhcsr.mask);raw(spec.dhcsr.value);
for (const n of ['revision','code','literal']) assert(Number.isInteger(spec.fpb[n]) && spec.fpb[n]>=0,`Independent FPB ${n} required`);
if(spec.trace_enabled){raw(spec.dwt.ctrl);assert(Number.isInteger(spec.dwt.comparators));}
if(spec.fpu){raw(spec.fpu.mvfr0);assert.equal(spec.fpu.d_registers,16);for(const id of ['scb.cpacr','fpu.fpccr','fpu.fpcar','fpu.fpdscr'])raw(spec.fpu.configuration[id]);for(const id of ['d0','s0','s1','fpscr'])raw(spec.fpu.storage[id],id==='d0'?64:32);}
const original=hash(project);Object.assign(suite.metadata,{binary,binary_sha256:hash(binary),project,project_sha256:original,core:options.core,case_file:caseFile,case_sha256:hash(caseFile),fixture:spec});
let session,context,probe,baseline={};
const inspect=(sample)=>{assert.equal(sample.state,'valid',`${sample.id}: ${sample.reason} ${sample.detail}`);assert.equal(sample.owner,`core:${options.core}`);assert.deepEqual(sample.context,context);const access=sample.provenance.access;assert.equal(access.phase,'responded');assert.deepEqual(access.context,context);assert(access.completed_ms>=access.timestamp_ms);assert.equal(access.route.endpoint,spec.endpoint);if(access.route.target)assert.equal(access.route.target,spec.target);return sample.value.hex;};
const read=async(wanted)=>{const result=await session.command('registers_read',{context,ids:Object.keys(wanted),manual:true});for(const sample of result.samples){assert.equal(inspect(sample),wanted[sample.id]);baseline[sample.id]=wanted[sample.id];}assert.equal(result.samples.length,Object.keys(wanted).length);return result;};
(async()=>{try{
  session=new Session(binary,project,out);await session.command('connect');if(options.core!=='default')await session.command('select_core',{name:options.core});
  if(!await suite.test(ids[0],'Verify selected CPU, halted physical context and independently expected CPUID',async()=>{
    const status=await session.command('status');assert.equal(status.state,'STOPPED');assert.equal(status.frame.level,0);assert.equal(status.frame.function,spec.frame_function);
    const list=await session.command('registers_list');context=list.context;assert.equal(context.core,options.core);assert.equal(list.catalogue.cpu,spec.cpu);
    probe=await session.command('registers_probe',{context});assert.equal(inspect(probe.probe.samples.find(s=>s.id==='scb.cpuid')),spec.cpuid);return {context,probe};
  }))return;
  if(!await suite.test(ids[1],'Verify FPB split capacity and existing TRCENA state without enabling or comparator writes',async()=>{
    const facts=probe.probe.facts;for(const [key,n] of [['fpb.revision','revision'],['fpb.code_comparators','code'],['fpb.literal_comparators','literal']])assert.equal(facts[key].value,spec.fpb[n]);
    const values=await read({'dcb.demcr':spec.demcr,'fpb.ctrl':spec.fpb.ctrl});
    const automatic=await session.command('registers_read',{context,ids:['dcb.dhcsr','dwt.ctrl']});assert.equal(automatic.samples[0].state,'not_read');assert.equal(automatic.samples[0].provenance.access,null);
    if(spec.trace_enabled){assert.equal(facts['dwt.comparators'].value,spec.dwt.comparators);assert.equal(inspect(automatic.samples[1]),spec.dwt.ctrl);baseline['dwt.ctrl']=spec.dwt.ctrl;}
    else {assert.equal(automatic.samples[1].reason,'feature_disabled');assert.equal(automatic.samples[1].provenance.access,null);assert.equal(facts['dwt.comparators'],undefined);}
    return {values,automatic};
  }))return;
  if(!await suite.test(ids[2],'Perform exactly one explicitly requested DHCSR read; no status poll',async()=>{
    const result=await session.command('registers_read',{context,ids:['dcb.dhcsr'],manual:true});assert.equal(result.samples.length,1);assert.equal(raw(inspect(result.samples[0]))&raw(spec.dhcsr.mask),raw(spec.dhcsr.value));return result;
  }))return;
  if(spec.fpu){if(!await suite.test(ids[3],'Compare FPU configuration and GDB D/S/FPSCR values with independent expectations',async()=>{
    assert.equal(inspect(probe.probe.samples.find(s=>s.id==='fpu.mvfr0')),spec.fpu.mvfr0);assert.equal(probe.probe.facts['vfp.d_registers'].value,spec.fpu.d_registers);
    const result=await read({...spec.fpu.configuration,...spec.fpu.storage});for(const sample of result.samples){if(Object.hasOwn(spec.fpu.storage,sample.id))assert.equal(sample.view,'selected_frame');else assert.equal(sample.view,'physical_core');}
    const d=result.samples.find(s=>s.id==='d0');for(const id of ['s0','s1'])assert.deepEqual(result.samples.find(s=>s.id===id).provenance.access,d.provenance.access);return result;
  }))return;}else suite.skip(ids[3],'FPU storage','This explicit case has no positive FPU identity; no FPU configuration/storage read attempted');
  await suite.test(ids[4],'Verify unchanged safe configuration, project and absence of DebugTUI write/control commands',async()=>{
    const expected={...baseline};const result=await read(expected);assert.equal(hash(project),original);
    assert(!session.logs('mi>').some(log=>/-exec-|\b-data-write-|\b-var-assign|\bmonitor\s|\bmcr\s/.test(log.text)));return result;
  });
}catch(error){await suite.test('M-MOD-SETUP','Prepare explicit environment',async()=>{throw error;});}
finally{if(session)await suite.test('M-MOD-CLEANUP','Close this test session',()=>session.close());for(const id of ids)if(!suite.results.some(r=>r.id===id))suite.skip(id,'M module phase','Earlier prerequisite failed');suite.finish();}})();
