// Explicit deferred M7/M4 case. Default invocation performs no target I/O.
const fs = require('node:fs'), path = require('node:path'), assert = require('node:assert/strict');
const {hash, outputDirectory, parseOptions, Cases, Session} = require('./test-support/session.cjs');
const options = parseOptions(process.argv.slice(2), ['--binary','--project','--case'], ['--run','--software-fixture']);
const ids = ['M-MULTI-H01-IDENTITY','M-MULTI-H02-VALUES','M-MULTI-H03-CACHED-SWITCH','M-MULTI-H04-RESTORE'];
const out = outputDirectory('m-profile-multicore-hardware');
const suite = new Cases(out,{board_tests_executed:!!options.run&&!options['software-fixture'],
  layer:options['software-fixture']?'actual EXE/MI/TCP/Tcl software fixture; no board':options.run?'explicit stopped M7/M4 environment':'SKIPPED, zero target I/O'});
if(!options.run){for(const id of ids)suite.skip(id,'M7/M4 isolation','Requires explicit --run and independent project/case');suite.finish();process.exit(0);}
assert(options.binary&&options.project&&options.case,'--run requires --binary FILE --project FILE --case JSON');
const binary=path.resolve(options.binary),project=path.resolve(options.project),caseFile=path.resolve(options.case);
for(const file of [binary,project,caseFile])assert(fs.existsSync(file),`Missing input: ${file}`);
const spec=JSON.parse(fs.readFileSync(caseFile,'utf8'));
assert(!spec.software_example||options['software-fixture'],'Replace software example with independent board expectations');
for(const field of ['revision','probe','openocd_version','gdb_version','evidence_source'])assert(typeof spec[field]==='string'&&spec[field],`Declare ${field}`);
assert(Array.isArray(spec.cores)&&spec.cores.length===2,'Declare exactly two cores');
assert.deepEqual([...new Set(spec.cores.map(core=>core.cpu))].sort(),['cortex-m4','cortex-m7']);
assert(new Set(spec.cores.map(core=>core.name)).size===2);
const exact=value=>{assert(/^0x[0-9a-f]{8}$/i.test(value),`Expected 32-bit hex: ${value}`);return BigInt(value);};
for(const core of spec.cores){
  for(const field of ['name','target','endpoint'])assert(typeof core[field]==='string'&&core[field],`Declare ${field}`);
  assert(Number.isInteger(core.ap)&&core.ap>=0,'Declare the AP used by this target');
  assert([0,8].includes(core.count)||(core.cpu==='cortex-m7'&&core.count===16));
  exact(core.cpuid);if(core.count){exact(core.control);assert(exact(core.rnr)<BigInt(core.count));}
  assert(Array.isArray(core.regions)&&core.regions.length===core.count);
  for(const pair of core.regions){assert(Array.isArray(pair)&&pair.length===2);pair.forEach(exact);}
}
const original=hash(project);
Object.assign(suite.metadata,{binary,binary_sha256:hash(binary),project,project_sha256:original,case_file:caseFile,case_sha256:hash(caseFile),fixture:spec});
let session;
const contexts=new Map(),views=new Map(),controls=new Map();
const route=(sample,core)=>{
  assert.equal(sample.state,'valid');assert.equal(sample.owner,`core:${core.name}`);assert.equal(sample.context.core,core.name);
  assert.equal(sample.provenance.access.phase,'responded');assert.equal(sample.provenance.access.route.target,core.target);assert.equal(sample.provenance.access.route.endpoint,core.endpoint);
};
const readControls=async core=>{
  if(!core.count)return {};
  const result=await session.command('registers_read',{context:contexts.get(core.name),ids:['mpu.ctrl','mpu.rnr'],manual:true});
  return Object.fromEntries(result.samples.map(sample=>{route(sample,core);return [sample.id,sample.value.hex];}));
};
(async()=>{try{
  session=new Session(binary,project,out);await session.command('connect');await session.command('control_scope',{scope:'all'});
  if(!await suite.test(ids[0],'Verify each configured model against its own physical identity and capacity',async()=>{
    for(const core of spec.cores){
      await session.command('select_core',{name:core.name});const status=await session.command('status');assert.equal(status.state,'STOPPED');assert.equal(status.frame.level,0);
      const list=await session.command('registers_list');assert.equal(list.catalogue.cpu,core.cpu);assert.equal(list.context.core,core.name);contexts.set(core.name,list.context);
      const proof=await session.command('registers_probe',{context:list.context});assert.equal(proof.probe.facts['mpu.regions'].value,core.count);
      const identity=proof.probe.samples.find(sample=>sample.id==='scb.cpuid');route(identity,core);assert.equal(exact(identity.value.hex),exact(core.cpuid));
      const before=await readControls(core);if(core.count){assert.equal(exact(before['mpu.ctrl']),exact(core.control));assert.equal(exact(before['mpu.rnr']),exact(core.rnr));}controls.set(core.name,before);
    }
    return {contexts:Object.fromEntries(contexts),controls:Object.fromEntries(controls)};
  }))return;
  if(!await suite.test(ids[1],'Read all MPU regions through each core target and compare independent reference bytes',async()=>{
    for(const core of spec.cores){
      await session.command('select_core',{name:core.name});const result=await session.command('registers_mpu',{context:contexts.get(core.name),bank:'m',read:true});
      const view=result.view;assert.equal(view.owner,`core:${core.name}`);assert.equal(view.state,'valid');assert.equal(view.regions.length,core.count);
      for(let i=0;i<core.count;i++)for(const [j,name] of ['base','attributes'].entries()){
        const sample=view.regions[i][name];route(sample,core);assert.equal(exact(sample.value.hex),exact(core.regions[i][j]));assert.deepEqual(sample.context,contexts.get(core.name));
      }
      if(core.count){assert.equal(exact(view.original_selector.hex),exact(core.rnr));assert.equal(exact(view.restored_selector.hex),exact(core.rnr));}views.set(core.name,view);
    }
    return Object.fromEntries(views);
  }))return;
  if(!await suite.test(ids[2],'Switch cores repeatedly and return only their original cached region values',async()=>{
    for(const core of [...spec.cores,...spec.cores].reverse()){
      await session.command('select_core',{name:core.name});const result=await session.command('registers_mpu',{context:contexts.get(core.name),bank:'m'});assert.deepEqual(result.view,views.get(core.name));
    }
    return {owners:[...views.values()].map(view=>view.owner)};
  }))return;
  await suite.test(ids[3],'Read back selectors and controls; preserve project and prohibit persistent or runtime control writes',async()=>{
    for(const core of spec.cores){await session.command('select_core',{name:core.name});assert.deepEqual(await readControls(core),controls.get(core.name));}
    assert.equal(hash(project),original);assert(!session.logs('mi>').some(log=>/-exec-|\b-data-write-|\b-var-assign/.test(log.text)));
    fs.writeFileSync(path.join(out,'multicore-evidence.json'),JSON.stringify(Object.fromEntries(views),null,2));return {controls:Object.fromEntries(controls)};
  });
}catch(error){await suite.test('M-MULTI-SETUP','Prepare explicit environment',async()=>{throw error;});}
finally{if(session)await suite.test('M-MULTI-CLEANUP','Close the isolated test session',()=>session.close());for(const id of ids)if(!suite.results.some(result=>result.id===id))suite.skip(id,'M7/M4 phase','Earlier prerequisite failed');suite.finish();}})();
