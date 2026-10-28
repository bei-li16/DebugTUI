// Real running-state image reads. Only the fixture's copied VM0 ELF is mutated.
const fs=require('node:fs'),path=require('node:path'),assert=require('node:assert/strict');
const {Session,Cases,outputDirectory,parseOptions,hash,quote,delay}=require('./test-support/session.cjs');
const o=parseOptions(process.argv.slice(2),['--binary','--project'],['--run']);
assert(o.run&&o.binary&&o.project,'Requires --run --binary EXE --project Bao/debug-multi-images.toml');
const binary=path.resolve(o.binary),original=path.resolve(o.project),bao=path.dirname(original).replaceAll('\\','/'),out=outputDirectory('bao-images-live-hardware');
const project=path.join(out,'debug.toml'),vm0=path.join(out,'guest0.elf');
fs.copyFileSync(path.join(bao,'build/tha6xxx/tha6206-dual/guest/guest0.elf'),vm0);
let text=fs.readFileSync(original,'utf8').replace(/^elf = "(?![A-Za-z]:|\/)/gm,'elf = "'+bao+'/').replaceAll('source_root = "."','source_root = '+quote(bao));
text=text.replace('elf = "'+bao+'/build/tha6xxx/tha6206-dual/guest/guest0.elf"','elf = '+quote(vm0));
assert(!text.includes('[live_watch]'));text+='\n[live_watch]\ntcl_endpoint="127.0.0.1:6666"\nbus_target="AHB_3"\ninterval_ms=100\n';fs.writeFileSync(project,text);
const suite=new Cases(out,{board_tests_executed:true,binary,binary_sha256:hash(binary),project,original_project:original,scope:'Raw image-owned globals while both cores run; explicit user continues only; ELF stale binding refusal',elfs:{bao:hash(path.join(bao,'bin/tha6xxx/tha6206-dual/bao.elf')),vm0:hash(vm0),vm1:hash(path.join(bao,'build/tha6xxx/tha6206-dual/guest/guest1.elf'))}});
let s,baseline,bindings={};
const core=n=>s.command('select_core',{name:'core.'+n});
const stop=()=>s.command('wait_stopped',{timeout_ms:20000},true,25000);
const transport=()=>s.logs('mi>').map(e=>e.text).filter(t=>/-target-select|-gdb-exit|monitor.*(?:reset|halt)|-target-download|-exec-interrupt/.test(t));
async function samples(n,image,from){
 const deadline=Date.now()+15000,expression='@'+image+':debug_domain_counter';
 while(Date.now()<deadline){const found=s.events.slice(from).filter(e=>e.event==='live_watch'&&e.sample.core===n&&e.sample.expression===expression).map(e=>e.sample);const values=found.filter(s=>s.value!==null&&s.value!==undefined);if(values.length>=2&&values.some(s=>s.value!==values[0].value)){assert(values.every(s=>s.image===image&&!s.error));return values;}await delay(50);}
 throw Error('No changing live samples for '+image);
}
(async()=>{try{
 s=new Session(binary,project,out);
 if(!await suite.test('IMAGE-LIVE-PREPARE','Both cores reach their own VM, resolving Watch types once while stopped',async()=>{
  await s.command('connect',{},true,65000);await s.command('control_scope',{scope:'core'});
  for(const n of [0,1]){await core(n);await s.command('break',{image:'vm'+n,location:'debug_domain_marker',hardware:true,temporary:true});}
  await core(0);await s.command('run');await stop();await s.command('continue');
  for(const n of [0,1]){await core(n);await stop();for(const image of ['bao','vm'+n]){const expression='@'+image+':debug_domain_counter';await s.command('watch',{expression});bindings[image]=await s.command('watch_resolve',{expression});assert.equal(bindings[image].image_origin.image,image);}}
  await s.command('control_scope',{scope:'all'});baseline=transport();return bindings;
 }))return;
 if(!await suite.test('IMAGE-LIVE-VM0','VM0 and Bao counters change through image-specific AHB addresses while both cores run',async()=>{
  await core(0);const from=s.events.length;await s.command('continue');const vm=await samples(0,'vm0',from),hyp=await samples(0,'bao',from);assert(vm.every(v=>v.address===bindings.vm0.address));assert(hyp.every(v=>v.address===bindings.bao.address));const st=await s.command('status');assert(st.cores.every(c=>c.state==='RUNNING'));assert.deepEqual(transport(),baseline);return {vm,hyp,status:st};
 }))return;
 if(!await suite.test('IMAGE-LIVE-VM1','Changing selected core replaces the poller and keeps VM1 image/revision/core provenance',async()=>{
  const from=s.events.length;await core(1);const vm=await samples(1,'vm1',from);assert(vm.every(v=>v.address===bindings.vm1.address));assert.notEqual(bindings.vm1.address,bindings.vm0.address);assert.deepEqual(transport(),baseline);return vm;
 }))return;
 if(!await suite.test('IMAGE-LIVE-STALE','Changing the copied ELF expires live bindings without halt, reconnect or symbol reload',async()=>{
  await core(0);const from=s.events.length;await samples(0,'vm0',from);const bytes=fs.readFileSync(vm0);bytes[9]^=1;fs.writeFileSync(vm0,bytes);
  let expired;const deadline=Date.now()+10000;while(Date.now()<deadline){expired=s.events.slice(from).find(e=>e.event==='live_watch'&&e.sample.image==='vm0'&&e.sample.error?.includes('ELF content changed'));if(expired)break;await delay(50);}assert(expired,'Missing stale-image error');assert.equal(expired.sample.value,null);
  const st=await s.command('status');assert(st.cores.every(c=>c.state==='RUNNING'));assert.deepEqual(transport(),baseline);return {expired,status:st,changed_fixture_sha256:hash(vm0)};
 }))return;
}catch(error){await suite.test('IMAGE-LIVE-SETUP','Execute physical live Watch regression',async()=>{throw error;});}
finally{if(s)await suite.test('IMAGE-LIVE-CLEANUP','Disconnect only owned debugger processes',()=>s.close());suite.finish();}})();
