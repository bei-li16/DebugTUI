// Deferred read-only R52 capability proof. Default mode never starts a target.
const fs=require('node:fs'),path=require('node:path'),assert=require('node:assert/strict');
const {root,hash,outputDirectory,parseOptions,Cases,Session}=require('./test-support/session.cjs');
const options=parseOptions(process.argv.slice(2),['--binary','--project','--core','--case'],['--run','--software-fixture']);
const phases=['MMIO-P-STOP','MMIO-P-PROOF','MMIO-P-CAPACITY','MMIO-P-LIFETIME','MMIO-P-CLEANUP'];
// Handwritten independent TRM Tables 10-7/35/36 and 12-5/40/41.
const words=[
 ['ed_midr','debug_external',0xd00,32],['edcidr0','debug_external',0xff0,32],['edcidr1','debug_external',0xff4,32],
 ['edcidr2','debug_external',0xff8,32],['edcidr3','debug_external',0xffc,32],['eddevaff0','debug_external',0xfa8,32],
 ['eddevaff1','debug_external',0xfac,32],['eddfr_word0','debug_external',0xd28,32],['eddfr_word1','debug_external',0xd2c,32],
 ['gicd_iidr','gicd',8,32],['gicd_cidr0','gicd',0xfff0,32],['gicd_cidr1','gicd',0xfff4,32],
 ['gicd_cidr2','gicd',0xfff8,32],['gicd_cidr3','gicd',0xfffc,32],['gicd_typer','gicd',4,32],
 ['gicr_iidr','gicr',4,32],['gicr_cidr0','gicr',0xfff0,32],['gicr_cidr1','gicr',0xfff4,32],
 ['gicr_cidr2','gicr',0xfff8,32],['gicr_cidr3','gicr',0xfffc,32],['gicr_typer','gicr',8,64]
];
const out=outputDirectory('register-mmio-probe-hardware');
const suite=new Cases(out,{todo:'REG-405/BUS-006 fresh MMIO identity/capacity',board_tests_executed:!!options.run&&!options['software-fixture'],layer:options['software-fixture']?'software fixture; no board':'deferred board proof'});
if(!options.run){for(const id of phases)suite.skip(id,'R52 MMIO capability proof','Requires --run, verified owner mappings, static state and independent firmware RAM');suite.finish();process.exit(0);}
assert(options.project&&options.core&&options.case,'Require --project TOML --core NAME --case JSON');
const binary=path.resolve(options.binary||path.join(root,'target/release/debugtui.exe')),project=path.resolve(options.project),caseFile=path.resolve(options.case);
for(const file of [binary,project,caseFile])assert(fs.existsSync(file),'Missing '+file);
const spec=JSON.parse(fs.readFileSync(caseFile));
assert(!spec.software_example||options['software-fixture'],'Replace software expectations before board execution');
assert(spec.mapping_verified===true&&spec.evidence_source&&spec.frame_function&&spec.ready,'Require independent verified mapping and capture');
const variable=s=>assert(/^[a-zA-Z_]\w*(?:\[\d+\]|\.[a-zA-Z_]\w*)*$/.test(s),'Only firmware variable/field/array expressions allowed');
const raw=(s,bits)=>{assert(new RegExp('^0x[0-9a-f]{'+bits/4+'}$','i').test(s),'Exact-width hex required');return BigInt(s);};
variable(spec.ready);assert.equal(spec.words.length,21);assert.deepEqual(spec.words.map(w=>w.id).sort(),words.map(w=>w[0]).sort());
const factNames=['debug_external.present','debug_external.breakpoints','debug_external.watchpoints','debug_external.context_breakpoints','debug_external.aff0','gicd.present','gicd.interrupts','gicr.present','gicr.target_id'];
assert.deepEqual(Object.keys(spec.expected_facts||{}).sort(),factNames.sort(),'Declare every independent capacity expectation');
assert(Object.values(spec.expected_facts).every(Number.isSafeInteger),'Capacity expectations must be exact integers');
for(const [id,c,,bits] of words){const w=spec.words.find(w=>w.id===id),binding=spec.components[c];assert.equal(w.bits,bits);variable(w.reference);raw(w.expected,bits);assert(binding?.owner&&binding.route?.endpoint&&/^0x[0-9a-f]{8}$/i.test(binding.base)&&typeof binding.little_endian==='boolean','Explicit owner/aperture/route required');assert(['gdb_memory','tcl_memory'].includes(binding.route.kind));}
const originalHash=hash(project);Object.assign(suite.metadata,{binary,binary_sha256:hash(binary),project,project_sha256:originalHash,case_file:caseFile,case_sha256:hash(caseFile),evidence_source:spec.evidence_source});
let session,context,proof;
const firmware=async symbol=>{const r=await session.command('evaluate',{expression:'(unsigned long long)'+symbol});assert(/^(0x[0-9a-f]+|\d+)$/i.test(r.value));return BigInt(r.value);};
(async()=>{
 try{
  session=new Session(binary,project,out);await session.command('connect');
  if(!await suite.test(phases[0],'Require physical stopped frame and independent ready capture',async()=>{
   if(options.core!=='default')await session.command('select_core',{name:options.core});const status=await session.command('status');assert.equal(status.state,'STOPPED');assert.equal(status.frame.level,0);assert.equal(status.frame.function,spec.frame_function);assert.equal(await firmware(spec.ready),1n);context=(await session.command('registers_list')).context;assert.equal(context.core,options.core);return {context};
  }))return;
  if(!await suite.test(phases[1],'Compare all 42 before/after memory requests with independent RAM',async()=>{
   proof=(await session.command('registers_probe',{context})).probe;assert.deepEqual(proof.context,context);
   const samples=proof.samples.filter(s=>s.source.startsWith('mmio:'));assert.equal(samples.length,42);
   for(const [id,component,offset,bits] of words){const binding=spec.components[component],w=spec.words.find(w=>w.id===id),reference=await firmware(w.reference);assert.equal(reference,raw(w.expected,bits));
    for(const suffix of ['', '.after']){const s=samples.find(s=>s.id==='mmio_probe.'+id+suffix),a=s?.provenance?.access,r=a?.route;assert.equal(s?.state,'valid',s?.detail);assert.equal(s.owner,binding.owner);assert.equal(s.view,'physical_core');assert.equal(s.source,'mmio:'+component);assert.equal(s.value.bits,bits);assert.equal(raw(s.value.hex,bits),reference);assert.equal(s.provenance.acquisition,'capability_probe');assert.deepEqual(s.provenance.catalogue_reader,{kind:'mmio',component,offset,require_owner_mapping:true});assert.equal(a?.phase,'responded');assert.deepEqual(a.context,context);assert(a.completed_ms>=a.timestamp_ms);assert.equal(r?.kind,binding.route.kind);assert.equal(r.endpoint,binding.route.endpoint);assert.equal(r.address,'0x'+(BigInt(binding.base)+BigInt(offset)).toString(16));assert.equal(r.bits,bits);assert.equal(r.byte_order,binding.little_endian?'little':'big');
     if(r.kind==='tcl_memory'){for(const key of ['target','channel','configuration_source'])assert.equal(r[key],binding.route[key]);assert.equal(r.bus_width,32);assert.equal(r.count,bits/32);assert.equal(r.atomic,false);}
    }
   }return {samples};
  }))return;
  if(!await suite.test(phases[2],'Require observed capacities while retaining configured values separately',async()=>{
   for(const [key,value] of Object.entries(spec.expected_facts)){assert.equal(proof.facts[key]?.value,value,key);assert(proof.facts[key].source.startsWith('mmio:'));}
   const list=await session.command('registers_list');assert.deepEqual(list.context,context);for(const [key,value]of Object.entries(spec.expected_facts))assert.equal(list.facts[key],value);return {observed:proof.facts};
  }))return;
  await suite.test(phases[3],'Keep context, stopped state and project file unchanged',async()=>{const status=await session.command('status');assert.equal(status.state,'STOPPED');assert.equal(status.frame.level,0);assert.deepEqual((await session.command('registers_list')).context,context);assert.equal(hash(project),originalHash);return {project_sha256:originalHash};});
 }catch(error){await suite.test('MMIO-P-DRIVER','Driver failure',async()=>{throw error;});}
 finally{if(session)await suite.test(phases[4],'Disconnect without executing firmware',()=>session.close());for(const id of phases)if(!suite.results.some(r=>r.id===id))suite.skip(id,'R52 proof','Earlier precondition failed');suite.finish();}
})();
