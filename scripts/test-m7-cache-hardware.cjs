// Deferred M7 cache/TCM check. Default mode connects to nothing.
const fs=require('node:fs'),path=require('node:path'),assert=require('node:assert/strict');
const {hash,outputDirectory,parseOptions,Cases,Session}=require('./test-support/session.cjs');
const options=parseOptions(process.argv.slice(2),['--binary','--project','--core','--case'],['--run','--software-fixture']);
const ids=['M7-CACHE-H01-IDENTITY','M7-CACHE-H02-CONFIG','M7-CACHE-H03-BANKS','M7-CACHE-H04-RESTORE'];
const out=outputDirectory('m7-cache-hardware');
const suite=new Cases(out,{board_tests_executed:!!options.run&&!options['software-fixture'],
  layer:options['software-fixture']?'actual EXE/MI/TCP/Tcl software fixture, no board':options.run?'explicit halted M7 board':'SKIPPED, zero target I/O'});
if(!options.run){for(const id of ids)suite.skip(id,'M7 cache/TCM readonly check','Requires explicit --run and independent project/core/case expectations');suite.finish();process.exit(0);}
assert(options.binary&&options.project&&options.core&&options.case,'--run requires --binary FILE --project FILE --core NAME --case JSON');
const binary=path.resolve(options.binary),project=path.resolve(options.project),caseFile=path.resolve(options.case);
for(const file of [binary,project,caseFile])assert(fs.existsSync(file),`Missing input: ${file}`);
const spec=JSON.parse(fs.readFileSync(caseFile));
assert(!spec.software_example||options['software-fixture'],'Replace software example with independent board expectations');
assert.equal(spec.cpu,'cortex-m7');
for(const key of ['revision','probe','openocd_version','gdb_version','frame_function','evidence_source','target'])assert(typeof spec[key]==='string'&&spec[key],`Declare ${key}`);
assert(Number.isInteger(spec.ap)&&spec.ap>=0,'Declare the selected core AP');
const exact=value=>{assert(/^0x[0-9a-f]{8}$/i.test(value),`Expected exact 32-bit value: ${value}`);return BigInt(value);};
for(const key of ['cpuid','clidr','ctr'])exact(spec[key]);
assert(Array.isArray(spec.caches));
for(const cache of spec.caches){assert([0,1].includes(cache.selector));exact(cache.ccsidr);}
assert(new Set(spec.caches.map(c=>c.selector)).size===spec.caches.length,'Duplicate cache expectation');
if(spec.caches.length)assert(exact(spec.csselr)<=1n);
const permitted=['scb.ccr','scb.cacr','scb.itcmcr','scb.dtcmcr','scb.ahbpcr','scb.ahbscr'];
assert(spec.configuration&&Object.keys(spec.configuration).length>0);
for(const [id,value] of Object.entries(spec.configuration)){assert(permitted.includes(id),`Unsupported config check ${id}`);exact(value);}
const original=hash(project);
Object.assign(suite.metadata,{binary,binary_sha256:hash(binary),project,project_sha256:original,core:options.core,case_file:caseFile,case_sha256:hash(caseFile),fixture:spec});
let session,context,before;
const configuration=async()=>{
  const ids=Object.keys(spec.configuration);if(spec.caches.length)ids.push('scb.csselr');
  const result=await session.command('registers_read',{context,ids,manual:true});
  return Object.fromEntries(ids.map(id=>{const sample=result.samples.find(s=>s.id===id);assert.equal(sample?.state,'valid');assert.equal(sample.owner,`core:${options.core}`);exact(sample.value.hex);return [id,sample.value.hex];}));
};
(async()=>{try{
  session=new Session(binary,project,out);await session.command('connect');if(options.core!=='default')await session.command('select_core',{name:options.core});
  if(!await suite.test(ids[0],'Verify halted physical core, M7 identity and cache implementation',async()=>{
    const status=await session.command('status');assert.equal(status.state,'STOPPED');assert.equal(status.frame.level,0);assert.equal(status.frame.function,spec.frame_function);
    const list=await session.command('registers_list');context=list.context;assert.equal(context.core,options.core);assert.equal(list.catalogue.cpu,spec.cpu);
    const {probe}=await session.command('registers_probe',{context});
    for(const [id,key] of [['scb.cpuid','cpuid'],['scb.clidr','clidr'],['scb.ctr','ctr']])assert.equal(exact(probe.samples.find(s=>s.id===id).value.hex),exact(spec[key]));
    assert.equal(BigInt(probe.facts['mcache.clidr'].value),exact(spec.clidr));assert.equal(BigInt(probe.facts['mcache.ctr'].value),exact(spec.ctr));
    return {context,probe};
  }))return;
  if(!await suite.test(ids[1],'Read cache/TCM configuration against independent expectations',async()=>{
    before=await configuration();for(const [id,value] of Object.entries(spec.configuration))assert.equal(exact(before[id]),exact(value));if(spec.caches.length)assert.equal(exact(before['scb.csselr']),exact(spec.csselr));return before;
  }))return;
  if(!await suite.test(ids[2],'Read implemented I/D banks and verify selector restoration evidence',async()=>{
    const result=await session.command('registers_cache',{context,read:true}),view=result.view;assert.equal(view.state,'valid');assert.equal(view.owner,`core:${options.core}`);assert.equal(view.caches.length,spec.caches.length);
    for(const expected of spec.caches){const cache=view.caches.find(c=>c.selector===expected.selector);assert(cache);assert.equal(cache.kind,expected.selector===0?'data':'instruction');assert.equal(cache.size_id.state,'valid');assert.equal(exact(cache.size_id.value.hex),exact(expected.ccsidr));const access=cache.size_id.provenance.access;assert.deepEqual(access.context,context);assert.equal(access.route.target,spec.target);assert.equal(access.phase,'responded');assert(access.completed_ms>=access.timestamp_ms);}
    if(spec.caches.length){assert.equal(exact(view.original_selector.hex),exact(spec.csselr));assert.equal(exact(view.restored_selector.hex),exact(spec.csselr));}else{assert.equal(view.original_selector,null);assert.equal(view.restored_selector,null);}
    fs.writeFileSync(path.join(out,'m7-cache-evidence.json'),JSON.stringify(result,null,2));return result;
  }))return;
  await suite.test(ids[3],'Verify unchanged selector/configuration/project and no runtime-control commands',async()=>{
    const after=await configuration();assert.deepEqual(after,before);assert.equal(hash(project),original);assert(!session.logs('mi>').some(log=>/-exec-|\b-data-write-|\b-var-assign/.test(log.text)));return {before,after};
  });
}catch(error){await suite.test('M7-CACHE-SETUP','Prepare explicit environment',async()=>{throw error;});}
finally{if(session)await suite.test('M7-CACHE-CLEANUP','Close this test session',()=>session.close());for(const id of ids)if(!suite.results.some(r=>r.id===id))suite.skip(id,'M7 cache phase','Earlier prerequisite failed');suite.finish();}})();
