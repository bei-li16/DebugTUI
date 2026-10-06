// Deferred running AP check. Default mode has zero target I/O; no board is run by CI.
const fs=require('node:fs'),path=require('node:path'),assert=require('node:assert/strict');
const {hash,outputDirectory,parseOptions,Cases,Session}=require('./test-support/session.cjs');
const options=parseOptions(process.argv.slice(2),['--binary','--project','--core','--case'],['--run','--software-fixture']);
const ids=['REG-RUN-H01-ROUTE','REG-RUN-H02-AP','REG-RUN-H03-HALT','REG-RUN-H04-STALE'];
const out=outputDirectory('register-running-hardware');
const suite=new Cases(out,{board_tests_executed:!!options.run&&!options['software-fixture'],layer:options['software-fixture']?'actual EXE/MI/AP software fixture, no board':options.run?'explicit running AP board check':'SKIPPED, zero target I/O'});
if(!options.run){for(const id of ids)suite.skip(id,'Running AP readonly check','Requires explicit --run, project/core and independent case expectations');suite.finish();process.exit(0);}
assert(options.binary&&options.project&&options.core&&options.case,'--run requires --binary FILE --project FILE --core NAME --case JSON');
const binary=path.resolve(options.binary),project=path.resolve(options.project),caseFile=path.resolve(options.case);
for(const file of [binary,project,caseFile])assert(fs.existsSync(file),`Missing input: ${file}`);
const spec=JSON.parse(fs.readFileSync(caseFile));
assert(!spec.software_example||options['software-fixture'],'Replace software example with independent board expectations');
assert(['cortex-m3','cortex-m4','cortex-m7'].includes(spec.cpu));
for(const key of ['revision','probe','openocd_version','gdb_version','evidence_source','target','channel','endpoint'])assert(typeof spec[key]==='string'&&spec[key],`Declare ${key}`);
assert(Number.isInteger(spec.ap)&&spec.ap>=0,'Declare AP');
const exact=value=>{assert(/^0x[0-9a-f]{8}$/i.test(value),`Expected exact 32-bit value: ${value}`);return BigInt(value);};
const safe=['scb.cpuid','scb.ccr','scb.clidr','scb.ctr','scb.itcmcr','scb.dtcmcr','scb.ahbpcr','scb.cacr','scb.ahbscr'];
assert(spec.values&&Object.keys(spec.values).length>0);
for(const [id,value] of Object.entries(spec.values)){assert(safe.includes(id),`Declare a supported ordinary readonly register: ${id}`);exact(value);}
const original=hash(project);
Object.assign(suite.metadata,{binary,binary_sha256:hash(binary),project,project_sha256:original,core:options.core,case_file:caseFile,case_sha256:hash(caseFile),fixture:spec});
let session,context,result,resumed=false;
(async()=>{try{
  session=new Session(binary,project,out);await session.command('connect');if(options.core!=='default')await session.command('select_core',{name:options.core});
  if(!await suite.test(ids[0],'Verify selected catalogue and explicit running AP route',async()=>{
    const status=await session.command('status');assert.equal(status.state,'STOPPED');
    const list=await session.command('registers_list');context=list.context;assert.equal(context.core,options.core);assert.equal(list.catalogue.cpu,spec.cpu);
    const channels=await session.command('memory_channels'),route=channels.channels.find(c=>c.configuration.id===spec.channel);
    assert(route?.available_for_core);assert.equal(route.while_running_declared,true);assert.equal(route.configuration.target,spec.target);assert.equal(route.configuration.tcl_endpoint,spec.endpoint);return {context,route};
  }))return;
  await session.command('continue',{scope:'core'});resumed=true;
  if(!await suite.test(ids[1],'Read independent safe values through the running AP route',async()=>{
    assert.equal((await session.command('status')).state,'RUNNING');
    context=(await session.command('registers_list')).context;
    result=await session.command('registers_read',{context,ids:Object.keys(spec.values),manual:true});
    for(const [id,value] of Object.entries(spec.values)){
      const sample=result.samples.find(s=>s.id===id);assert.equal(sample?.state,'valid');assert.equal(sample.view,'running_memory');assert.equal(sample.owner,`core:${options.core}`);assert.equal(exact(sample.value.hex),exact(value));
      const access=sample.provenance.access;assert.deepEqual(access.context,context);assert.equal(access.phase,'responded');assert(access.completed_ms>=access.timestamp_ms);assert.equal(access.route.kind,'tcl_memory');assert.equal(access.route.target,spec.target);assert.equal(access.route.channel,spec.channel);assert.equal(access.route.endpoint,spec.endpoint);
    }
    fs.writeFileSync(path.join(out,'running-evidence.json'),JSON.stringify(result,null,2));return result;
  }))return;
  if(!await suite.test(ids[2],'Reject GDB regfile and protected stopped cache reads without data requests',async()=>{
    const blocked=['r0'];if(spec.cpu==='cortex-m7')blocked.push('scb.ccsidr');
    const denied=await session.command('registers_read',{context,ids:blocked,manual:true});
    for(const sample of denied.samples){assert.equal(sample.state,'unavailable');assert(sample.detail.includes('NeedHalt'));assert(!sample.provenance.access);assert(!sample.value);}
    const reads=session.logs('mi>').filter(e=>/-data-list-register-values|-data-read-memory-bytes/.test(e.text));
    // Connection setup can query a register file; only the actual running interval is checked.
    const runningAt=session.events.findIndex(e=>e.event==='snapshot'&&e.snapshot?.state==='RUNNING');
    assert(runningAt>=0);assert(!session.events.slice(runningAt).some(e=>e.event==='log'&&e.channel==='mi>'&&/-thread-info|-stack-|\b-data-read-|\b-data-list-register-values/.test(e.text)));
    return {denied,connection_data_queries:reads.length};
  }))return;
  await session.command('pause',{scope:'core'});resumed=false;
  await suite.test(ids[3],'Pause invalidates running samples while retaining their original raw source',async()=>{
    const status=await session.command('status');assert.equal(status.state,'STOPPED');
    for(const old of result.samples){const sample=status.register_samples.find(s=>s.id===old.id);assert.equal(sample?.state,'stale');for(const key of ['value','timestamp_ms','provenance','context'])assert.deepEqual(sample[key],old[key]);}
    assert.equal(hash(project),original);assert(!session.logs('mi>').some(log=>/\b-data-write-|\b-var-assign/.test(log.text)));return status.register_samples;
  });
}catch(error){await suite.test('REG-RUN-SETUP','Prepare explicit running environment',async()=>{throw error;});}
finally{if(session){if(resumed)await suite.test('REG-RUN-RECOVER','Pause the selected test core after an earlier failure',()=>session.command('pause',{scope:'core'}));await suite.test('REG-RUN-CLEANUP','Close this test session',()=>session.close());}for(const id of ids)if(!suite.results.some(r=>r.id===id))suite.skip(id,'Running AP phase','Earlier prerequisite failed');suite.finish();}})();
