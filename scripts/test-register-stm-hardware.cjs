// Deferred REG-406. Default invocation never creates a debug session.
const fs=require('node:fs'),path=require('node:path'),assert=require('node:assert/strict');
const {root,hash,outputDirectory,parseOptions,Cases,Session}=require('./test-support/session.cjs');
const options=parseOptions(process.argv.slice(2),['--binary','--project','--core','--case'],['--run','--software-fixture']);
const phases=['STM-H-STOP','STM-H-PROOF','STM-H-RAW','STM-H-UNCHANGED','STM-H-CLEANUP'];
const out=outputDirectory('register-stm-hardware');
const suite=new Cases(out,{todo:'REG-406',board_tests_executed:!!options.run&&!options['software-fixture'],layer:options['software-fixture']?'software MI/TCP fixture':'deferred STM-500 environment'});
if(!options.run){for(const id of phases)suite.skip(id,'STM control observation','Requires --run, verified STM-500 control mapping and independent stopped firmware RAM');suite.finish();process.exit(0);}
assert(options.binary&&options.project&&options.core&&options.case,'--run requires --binary --project --core --case');
const binary=path.resolve(options.binary),project=path.resolve(options.project),caseFile=path.resolve(options.case);
for(const file of [binary,project,caseFile])assert(fs.existsSync(file),'Missing '+file);
const spec=JSON.parse(fs.readFileSync(caseFile));
assert(!spec.software_example||options['software-fixture'],'Replace software examples before physical execution');
assert(spec.mapping_verified===true&&spec.frame_function&&spec.evidence_source,'Verified component mapping and independent capture required');
assert(/^chip:.+/.test(spec.component?.owner));assert(/^0x[0-9a-f]{8}$/i.test(spec.component.base));
assert.equal(BigInt(spec.component.base)&0xfffn,0n,'Control aperture must be 4 KiB aligned');
assert(BigInt(spec.component.base)+0xfffn<=0xffffffffn);assert.equal(typeof spec.component.little_endian,'boolean');
const route=spec.component.route;assert(['gdb_memory','tcl_memory'].includes(route?.kind)&&route.endpoint);
if(route.kind==='tcl_memory')assert(route.channel&&route.target&&route.configuration_source);
const registers=[['stm_tcsr',0xe80],['stm_sper',0xe00],['stm_spscr',0xe60],['stm_spmscr',0xe64],['stm_feat1r',0xea0],['stm_devid',0xfc8]];
const variable=s=>assert(/^[a-zA-Z_]\w*(?:\[\d+\]|\.[a-zA-Z_]\w*)*$/.test(s),'Use a RAM variable/field, without evaluating functions');
const raw=s=>{assert(/^0x[0-9a-f]{8}$/i.test(s),'Exact raw 32-bit word required');return BigInt(s);};
variable(spec.ready);variable(spec.core_tag);assert(Number.isSafeInteger(spec.expected_core_tag)&&spec.expected_core_tag>=0&&spec.expected_core_tag<4);
assert.equal(spec.registers.length,registers.length);assert.deepEqual(spec.registers.map(e=>e.id).sort(),registers.map(e=>e[0]).sort());
for(const e of spec.registers){variable(e.reference);raw(e.expected);}
const original=hash(project);Object.assign(suite.metadata,{binary,binary_sha256:hash(binary),project,project_sha256:original,case_file:caseFile,case_sha256:hash(caseFile),evidence_source:spec.evidence_source});
let session,context,before;
const firmware=async symbol=>{const r=await session.command('evaluate',{expression:'(unsigned long long)'+symbol});assert(/^(0x[0-9a-f]+|\d+)$/i.test(r.value));return BigInt(r.value);};
const read=async()=>{const r=await session.command('registers_read',{context,ids:registers.map(e=>e[0])});const values={};for(const [id,offset] of registers){const s=r.samples.find(s=>s.id===id),a=s?.provenance?.access;assert.equal(s?.state,'valid',id+': '+s?.detail);assert.equal(s.owner,spec.component.owner);assert.equal(s.source,'mmio:stm');assert.equal(s.value.bits,32);raw(s.value.hex);assert.deepEqual(s.context,context);assert.equal(a.phase,'responded');assert.deepEqual(a.context,context);assert(a.completed_ms>=a.timestamp_ms);assert.equal(a.route.kind,route.kind);assert.equal(a.route.endpoint,route.endpoint);assert.equal(a.route.address,'0x'+(BigInt(spec.component.base)+BigInt(offset)).toString(16));assert.equal(a.route.byte_order,spec.component.little_endian?'little':'big');if(route.kind==='tcl_memory'){for(const key of ['target','channel','configuration_source'])assert.equal(a.route[key],route[key]);assert.equal(a.route.bus_width,32);assert.equal(a.route.count,1);assert.equal(a.route.atomic,false);}assert(s.eligibility.conditions.every(c=>c.source==='observation'));values[id]=s.value.hex;}return values;};
(async()=>{try{
  session=new Session(binary,project,out);await session.command('connect');
  if(!await suite.test(phases[0],'Require stopped physical core and independent capture',async()=>{if(options.core!=='default')await session.command('select_core',{name:options.core});const s=await session.command('status');assert.equal(s.state,'STOPPED');assert.equal(s.frame.level,0);assert.equal(s.frame.function,spec.frame_function);assert.equal(await firmware(spec.ready),1n);assert.equal(await firmware(spec.core_tag),BigInt(spec.expected_core_tag));const list=await session.command('registers_list');context=list.context;assert.equal(context.core,options.core);for(const [id,offset]of registers){const r=list.catalogue.registers.find(r=>r.id===id);assert.equal(r.scope,'chip');assert.equal(r.bits,32);assert.equal(r.reader.kind,'mmio');assert.equal(r.reader.component,'stm');assert.equal(r.reader.offset,offset);assert.equal(r.reader.require_owner_mapping,true);assert(!r.writer);}return{context};}))return;
  if(!await suite.test(phases[1],'Obtain fresh STM identity/features independently of CPU MIDR',async()=>{const r=await session.command('registers_probe',{context});assert.equal(r.facts['stm.present'],1);assert.equal(r.facts['stm.part'],0x963);assert.equal(r.facts['stm.sper'],1);const words=r.probe.samples.filter(s=>s.source==='mmio:stm');assert.equal(words.length,30);assert(words.every(s=>s.state==='valid'&&s.owner===spec.component.owner));return{probe:r.probe};}))return;
  if(!await suite.test(phases[2],'Compare six exact words with independent per-core firmware RAM',async()=>{before=await read();for(const e of spec.registers){const n=await firmware(e.reference);assert.equal(n,raw(e.expected),'Independent baseline changed: '+e.id);assert.equal(raw(before[e.id]),n,'STM value mismatch: '+e.id);}return{values:before};}))return;
  await suite.test(phases[3],'Preserve static controls, bank selectors, project and stopped context',async()=>{assert.deepEqual(await read(),before);const s=await session.command('status');assert.equal(s.state,'STOPPED');assert.equal(hash(project),original);const list=await session.command('registers_list');assert.deepEqual(list.context,context);return{unchanged:true};});
}catch(e){await suite.test('STM-H-DRIVER','Driver failure',async()=>{throw e;});}
finally{if(session)await suite.test(phases[4],'Disconnect without executing firmware',()=>session.close());for(const id of phases)if(!suite.results.some(e=>e.id===id))suite.skip(id,'STM environment','Earlier precondition failed');suite.finish();}})();
