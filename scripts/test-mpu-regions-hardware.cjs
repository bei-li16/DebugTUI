// Deferred REG-H04 MPU/MAIR overview; no target connection without --run.
const fs=require('node:fs'),path=require('node:path'),assert=require('node:assert/strict');
const {root,hash,outputDirectory,parseOptions,Cases,Session}=require('./test-support/session.cjs');
const options=parseOptions(process.argv.slice(2),['--binary','--project','--core','--case'],['--run','--software-fixture']);
const ids=['REG-H04-MPU-STOP','REG-H04-MPU-PROBE','REG-H04-MPU-VIEWS','REG-H04-MPU-UNCHANGED'];
const out=outputDirectory('mpu-regions-hardware');
const suite=new Cases(out,{layer:options['software-fixture']?'actual binary with MI/Tcl software model; no board':options.run?'explicit paused physical MPU fixture':'deferred cases; no target access',board_tests_executed:!!options.run&&!options['software-fixture'],todo:'REG-H04 full direct MPU regions and current MAIR; no configuration writes'});
if(!options.run){for(const id of ids)suite.skip(id,'Read the current core MPU region/MAIR overview','Requires --run with a dedicated paused fixture and independently recorded expectations');suite.finish();process.exit(0);}
assert(options.project&&options.core&&options.case,'--run requires --project FILE --core NAME --case JSON');
const binary=path.resolve(options.binary||path.join(root,'target/release/debugtui.exe')),project=path.resolve(options.project),caseFile=path.resolve(options.case);
for(const file of [binary,project,caseFile])assert(fs.existsSync(file),`Missing input: ${file}`);
const spec=JSON.parse(fs.readFileSync(caseFile));
assert(typeof spec.frame_function==='string'&&spec.frame_function,'Declare a dedicated paused fixture function');
assert(typeof spec.evidence_source==='string'&&spec.evidence_source,'Record independent firmware/manual expectation provenance');
assert(!spec.software_example||options['software-fixture'],'Replace the software example with independently verified board expectations');
assert(Array.isArray(spec.views)&&spec.views.length&&new Set(spec.views.map(v=>v.bank)).size===spec.views.length,'Declare each bank once');
const exact=raw=>{assert(/^0x[0-9a-f]{8}$/i.test(raw),`Not an exact 32-bit value: ${raw}`);return BigInt(raw);};
exact(spec.expected_midr);
if(spec.current_debug){
 for(const key of ['revision','probe','openocd_version','gdb_version'])assert(typeof spec[key]==='string'&&spec[key],`Declare ${key}`);
 assert(spec.current_debug.endpoint,'Declare actual native Tcl endpoint');
 for(const core of [options.core,...(spec.peer_core?[spec.peer_core]:[])]){
  assert(spec.current_debug.targets?.[core],`Declare actual target for ${core}`);
  assert(Number.isInteger(spec.current_debug.ap?.[core])&&spec.current_debug.ap[core]>=0,`Declare actual AP for ${core}`);
  exact(spec.current_debug.saved_dspsr?.[core]);
 }
}
for(const view of spec.views){assert(['el1','el2'].includes(view.bank));assert([16,20,24].includes(view.count)||(view.bank==='el2'&&view.count===0));assert(Array.isArray(view.regions)&&view.regions.length===view.count,'Provide every implemented raw region pair');for(const pair of view.regions){assert(Array.isArray(pair)&&pair.length===2);pair.forEach(exact);}if(view.count){assert(Array.isArray(view.mair)&&view.mair.length===2);view.mair.forEach(exact);}}
const controls=[...new Set(['cpsr','prselr',...spec.views.flatMap(v=>v.bank==='el2'?(v.count?['hprselr','hsctlr','hcr','hprenr','hmair0','hmair1']:['hsctlr','hcr']):['sctlr','mair0','mair1'])])];
const projectHash=hash(project);
Object.assign(suite.metadata,{binary,binary_sha256:hash(binary),project,project_sha256:projectHash,case_file:caseFile,case_sha256:hash(caseFile),core:options.core,peer_core:spec.peer_core||null,evidence_source:spec.evidence_source});
let session,context,before,peerBefore,lastProbe;
const halted=async()=>{const status=await session.command('status');assert.equal(status.state,'STOPPED');assert.equal(status.frame.level,0);assert.equal(status.frame.function,spec.frame_function,'Stop at the fixture first; this driver never pauses, resumes, resets or downloads');};
const select=async core=>{if(core!=='default'||spec.peer_core)await session.command('select_core',{name:core});await halted();lastProbe=await session.command('registers_probe',{context:(await session.command('registers_list')).context});};
const proof=(sample,ctx)=>{
 if(!spec.current_debug)return;
 const access=sample.provenance.access;assert.equal(access.phase,'responded');assert.deepEqual(access.context,ctx);
 if(sample.id==='cpsr'){assert.equal(access.route.kind,'gdb_register');assert.equal(exact(sample.value.hex),exact(spec.current_debug.saved_dspsr[ctx.core]));return;}
 assert.equal(sample.source,'openocd:aarch64 r52_read');assert.equal(access.route.target,spec.current_debug.targets[ctx.core]);assert.equal(access.route.endpoint,spec.current_debug.endpoint);
 const p=access.r52_core;assert.equal(exact(p.midr.hex),exact(spec.expected_midr));assert.equal(exact(p.dspsr.hex),exact(spec.current_debug.saved_dspsr[ctx.core]));
 const dscr=exact(p.dscr.hex);assert.equal((dscr>>8n)&3n,2n);assert.equal(dscr&0x8000n,0n);assert.equal(dscr&0x1000n,0n);assert.equal(dscr&0x1c0000c0n,0n);assert(dscr&0x1000000n);
 assert.equal(p.dlr.bits,32);assert(access.completed_ms>=access.timestamp_ms);
};
const read=async ids=>{const ctx=(await session.command('registers_list')).context;const samples=(await session.command('registers_read',{context:ctx,ids,manual:true})).samples;return Object.fromEntries(ids.map(id=>{const s=samples.find(s=>s.id===id);assert.equal(s?.state,'valid',`Fresh ${id} required`);assert.equal(s.owner,`core:${ctx.core}`);exact(s.value.hex);proof(s,ctx);return[id,s.value.hex];}));};
(async()=>{try{
 session=new Session(binary,project,out);await session.command('connect');if(spec.control_scope){assert(['core','all'].includes(spec.control_scope));await session.command('control_scope',{scope:spec.control_scope});}
 if(!await suite.test(ids[0],'Capture paused selected/peer cores and stable controls',async()=>{if(spec.peer_core){assert.notEqual(spec.peer_core,options.core);await select(spec.peer_core);peerBefore=await read(controls);}await select(options.core);context=(await session.command('registers_list')).context;assert.equal(context.core,options.core);before=await read(controls);return{context,before,peerBefore};}))return;
 if(!await suite.test(ids[1],'Observe actual R52 identity and implementation counts',async()=>{const probe=lastProbe;assert.deepEqual(probe.context,context);assert.equal(probe.probe.identity.model,'Cortex-R52');assert.equal(exact(probe.probe.samples.find(s=>s.id==='midr').value.hex),exact(spec.expected_midr));for(const v of spec.views)assert.equal(probe.probe.facts[`mpu.${v.bank}.regions`].value,v.count);if(spec.current_debug)for(const id of ['midr','mpuir','hmpuir'])proof(probe.probe.samples.find(s=>s.id===id),context);return probe;}))return;
 if(!await suite.test(ids[2],'Verify every direct region, MAIR selection and independent expected values',async()=>{const results=[];for(const v of spec.views){const result=await session.command('registers_mpu',{context,bank:v.bank,read:true});assert.deepEqual(result.context,context);assert.equal(result.view.owner,`core:${options.core}`);if(spec.current_debug)for(const sample of result.samples)proof(sample,context);assert.equal(result.view.regions.length,v.count);for(let n=0;n<v.count;n++){const r=result.view.regions[n];assert.equal(r.index,n);assert.equal(r.base.state,'valid');assert.equal(r.limit.state,'valid');assert.equal(exact(r.base.value.hex),exact(v.regions[n][0]));assert.equal(exact(r.limit.value.hex),exact(v.regions[n][1]));const base=exact(v.regions[n][0]),limit=exact(v.regions[n][1]);assert.equal(exact(r.decoded.base),base&~63n);assert.equal(exact(r.decoded.limit_inclusive),(limit&~63n)|63n);assert.equal(r.decoded.enabled,!!(limit&1n));assert.equal(r.decoded.execute_never,!!(base&1n));const attr=Number((limit>>1n)&7n);assert.equal(r.decoded.attribute_index,attr);assert.equal(BigInt(r.attribute.raw),(exact(v.mair[attr>>2])>>BigInt((attr%4)*8))&255n);}if(v.count)for(let i=0;i<2;i++)assert.equal(exact(result.view.mair[i].value.hex),exact(v.mair[i]));assert.deepEqual(await read(controls),before);results.push(result);}fs.writeFileSync(path.join(out,'mpu-evidence.json'),JSON.stringify(results,null,2));return{banks:results.length,regions:results.reduce((n,r)=>n+r.view.count,0)};}))return;
 await suite.test(ids[3],'Preserve stopped context, peer controls, selectors and project',async()=>{await halted();assert.deepEqual((await session.command('registers_list')).context,context);assert.deepEqual(await read(controls),before);if(spec.peer_core){await select(spec.peer_core);assert.deepEqual(await read(controls),peerBefore);await select(options.core);}assert(!session.logs('mi>').some(log=>/-exec-|\b-data-write-|\b-var-assign/.test(log.text)));assert.equal(hash(project),projectHash);return{context,before,peerBefore};});
}catch(error){await suite.test('REG-H04-MPU-SETUP','Initialize the explicit fixture',async()=>{throw error;});}finally{if(session)await suite.test('REG-H04-MPU-CLEANUP','Close only this test session',()=>session.close());if(hash(project)!==projectHash)await suite.test('REG-H04-MPU-CONFIG','Preserve original project',async()=>{throw Error('Project changed');});for(const id of ids)if(!suite.results.some(r=>r.id===id))suite.skip(id,'Deferred fixture phase','Earlier precondition failed');suite.finish();}})();
