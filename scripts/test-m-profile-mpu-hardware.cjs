// Deferred Cortex-M MPU case. Never connects without explicit --run and inputs.
const fs = require('node:fs'), path = require('node:path'), assert = require('node:assert/strict');
const {root, hash, outputDirectory, parseOptions, Cases, Session} = require('./test-support/session.cjs');
const options = parseOptions(process.argv.slice(2), ['--binary','--project','--core','--case'], ['--run','--software-fixture']);
const ids = ['M-MPU-H01-IDENTITY','M-MPU-H02-REGIONS','M-MPU-H03-RESTORE'];
const out = outputDirectory('m-profile-mpu-hardware');
const suite = new Cases(out, {board_tests_executed: !!options.run && !options['software-fixture'],
  layer: options['software-fixture'] ? 'actual EXE/MI/TCP/Tcl software fixture, no board' : options.run ? 'explicit halted board fixture' : 'SKIPPED, zero target I/O'});
if (!options.run) { for (const id of ids) suite.skip(id, 'M MPU bank sampling and restoration', 'Requires explicit --run, project/core/case and independent expected values'); suite.finish(); process.exit(0); }
assert(options.binary && options.project && options.core && options.case, '--run requires --binary FILE --project FILE --core NAME --case JSON');
const binary = path.resolve(options.binary), project = path.resolve(options.project), caseFile = path.resolve(options.case);
for (const file of [binary,project,caseFile]) assert(fs.existsSync(file), `Missing input: ${file}`);
const spec = JSON.parse(fs.readFileSync(caseFile));
assert(!spec.software_example || options['software-fixture'], 'Replace example with independent board expectations');
assert(['cortex-m3','cortex-m4','cortex-m7'].includes(spec.cpu));
assert([0,8].includes(spec.count) || (spec.cpu === 'cortex-m7' && spec.count === 16));
for (const key of ['revision','probe','openocd_version','gdb_version','frame_function','evidence_source','target']) assert(typeof spec[key] === 'string' && spec[key], `Declare ${key}`);
assert(Number.isInteger(spec.ap) && spec.ap >= 0, 'Declare the selected core AP');
const exact = value => { assert(/^0x[0-9a-f]{8}$/i.test(value), `Expected exact 32-bit value: ${value}`); return BigInt(value); };
exact(spec.cpuid); if(spec.count){exact(spec.control); assert(exact(spec.rnr)<BigInt(spec.count));}
assert(Array.isArray(spec.regions) && spec.regions.length === spec.count, 'Provide all independent region expectations');
for (const pair of spec.regions) { assert(Array.isArray(pair) && pair.length === 2); pair.forEach(exact); }
const original = hash(project);
Object.assign(suite.metadata, {binary,binary_sha256:hash(binary),project,project_sha256:original,core:options.core,case_file:caseFile,case_sha256:hash(caseFile), fixture:spec});
let session, context, before;
const controls = spec.count ? ['mpu.ctrl','mpu.rnr'] : [];
const read = async () => {
  if (!controls.length) return {};
  const result = await session.command('registers_read',{context,ids:controls,manual:true});
  return Object.fromEntries(controls.map(id => {const sample=result.samples.find(s=>s.id===id);assert.equal(sample?.state,'valid');assert.equal(sample.owner,`core:${options.core}`);exact(sample.value.hex);return [id,sample.value.hex];}));
};
(async()=>{try{
  session = new Session(binary,project,out); await session.command('connect');
  if(options.core!=='default') await session.command('select_core',{name:options.core});
  if(!await suite.test(ids[0],'Verify halted physical frame, CPUID, actual MPU capacity and controls',async()=>{
    const status=await session.command('status');assert.equal(status.state,'STOPPED');assert.equal(status.frame.level,0);assert.equal(status.frame.function,spec.frame_function);
    const list=await session.command('registers_list');context=list.context;assert.equal(context.core,options.core);assert.equal(list.catalogue.cpu,spec.cpu);
    const probe=await session.command('registers_probe',{context});assert.equal(probe.probe.facts['mpu.regions'].value,spec.count);
    assert.equal(exact(probe.probe.samples.find(s=>s.id==='scb.cpuid').value.hex),exact(spec.cpuid));
    before=await read();if(spec.count){assert.equal(exact(before['mpu.ctrl']),exact(spec.control));assert.equal(exact(before['mpu.rnr']),exact(spec.rnr));}
    return {context,before,probe};
  })) return;
  if(!await suite.test(ids[1],'Check every region against independent expected RBAR/RASR',async()=>{
    const result=await session.command('registers_mpu',{context,bank:'m',read:true});const view=result.view;
    assert.equal(view.owner,`core:${options.core}`);assert.equal(view.state,'valid');assert.equal(view.regions.length,spec.count);
    for(let i=0;i<spec.count;i++){
      const region=view.regions[i];assert.equal(region.index,i);
      for(const [n,name] of ['base','attributes'].entries()){
        const sample=region[name];assert.equal(sample.state,'valid');assert.equal(exact(sample.value.hex),exact(spec.regions[i][n]));
        const access=sample.provenance.access;assert.deepEqual(access.context,context);assert.equal(access.phase,'responded');assert.equal(access.route.target,spec.target);assert(access.completed_ms>=access.timestamp_ms);
      }
    }
    if(spec.count){assert.equal(exact(view.original_selector.hex),exact(spec.rnr));assert.equal(exact(view.restored_selector.hex),exact(spec.rnr));}
    fs.writeFileSync(path.join(out,'m-mpu-evidence.json'),JSON.stringify(result,null,2));return result;
  })) return;
  await suite.test(ids[2],'Verify unchanged controls, selector and project, with zero runtime-control commands',async()=>{
    assert.deepEqual(await read(),before);assert.equal(hash(project),original);
    assert(!session.logs('mi>').some(log=>/-exec-|\b-data-write-|\b-var-assign/.test(log.text)));
    return {before,after:await read()};
  });
}catch(error){await suite.test('M-MPU-SETUP','Prepare explicit environment',async()=>{throw error;});}
finally{if(session)await suite.test('M-MPU-CLEANUP','Close this test session',()=>session.close());for(const id of ids)if(!suite.results.some(r=>r.id===id))suite.skip(id,'M MPU phase','Earlier prerequisite failed');suite.finish();}})();
