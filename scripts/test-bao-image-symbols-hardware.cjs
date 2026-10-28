// Explicit THA6206 board regression for same-name symbols in three objfiles.
const fs=require('node:fs'),path=require('node:path'),assert=require('node:assert/strict');
const {Session,Cases,outputDirectory,parseOptions,hash,quote}=require('./test-support/session.cjs');
const options=parseOptions(process.argv.slice(2),['--binary','--project'],['--run']);
assert(options.run&&options.binary&&options.project,'Requires --run --binary EXE --project Bao/debug-multi-images.toml');
const binary=path.resolve(options.binary),original=path.resolve(options.project),bao=path.dirname(original).replaceAll('\\','/'),out=outputDirectory('bao-image-symbols-hardware');
const project=path.join(out,'debug.toml');
fs.writeFileSync(project,fs.readFileSync(original,'utf8').replace(/^elf = "(?![A-Za-z]:|\/)/gm,'elf = "'+bao+'/').replaceAll('source_root = "."','source_root = '+quote(bao)));
const suite=new Cases(out,{board_tests_executed:true,binary,binary_sha256:hash(binary),project,original_project:original,
 scope:'Same-name code breakpoint image/core ownership, global/local lookup, configuration restoration; remaining goal items are separate',
 elfs:Object.fromEntries(['bin/tha6xxx/tha6206-dual/bao.elf','build/tha6xxx/tha6206-dual/guest/guest0.elf','build/tha6xxx/tha6206-dual/guest/guest1.elf'].map(p=>[p,hash(path.join(bao,p))]))});
let s,baseline,addresses={};
const core=n=>s.command('select_core',{name:'core.'+n});
const status=()=>s.command('status');
const stop=()=>s.command('wait_stopped',{timeout_ms:20000},true,25000);
const transport=()=>s.logs('mi>').map(e=>e.text).filter(t=>/-target-select|-gdb-exit|monitor.*(?:reset|halt)|-target-download|-exec-interrupt/.test(t));
const number=value=>{const m=/0x[0-9a-f]+|^[0-9]+/i.exec(value);assert(m,value);return Number(BigInt(m[0]));};
async function bp(image,temporary=true){await s.command('break',{image,location:'debug_domain_marker',temporary,hardware:true});const st=await status();const b=st.breakpoints.find(b=>b.image===image&&b.location==='debug_domain_marker');assert(b,'Image annotation/original location missing');assert.equal(number(b.address),addresses[image]);return b;}
async function values(image,id){
 const before=await status();assert.equal(before.image_state.execution.image,image);assert.equal(before.frame.function,'debug_domain_marker');
 const global=number((await s.command('evaluate',{expression:'debug_domain_id'})).value);assert.equal(global,id);
 const constant=await s.command('evaluate',{expression:'(uint32_t)0x1234'});assert.equal(number(constant.value),0x1234);assert.equal(constant.image,image,'Typedefs/constants must not fall back to primary objfile');
 const input=number((await s.command('evaluate',{expression:'input'})).value);
  const expression='input + debug_domain_id';
  const mixed=await s.command('evaluate',{expression});assert.equal(number(mixed.value),(id+input)>>>0);assert.equal(mixed.image,image);assert.equal(mixed.source,'isolated_current_frame');assert(mixed.observed_registers.includes('pc')&&mixed.observed_registers.includes('sp'));assert.deepEqual((await status()).frame,before.frame);
  const r0=number((await s.command('evaluate',{expression:'$r0'})).value),registers=[];
  for(const text of ['$r0 + debug_domain_id','debug_domain_id + $r0']){const value=await s.command('evaluate',{expression:text});assert.equal(number(value.value),(r0+id)>>>0);assert.equal(value.image,image);assert.equal(value.source,'isolated_current_frame');registers.push({expression:text,value});}
  await s.command('watch',{expression});let mixedWatch=(await status()).watches.find(v=>v.name===expression);assert(mixedWatch&&!mixedWatch.error,JSON.stringify(mixedWatch));assert.equal(number(mixedWatch.value),(id+input)>>>0);assert.equal(mixedWatch.image_origin.image,image);await s.command('unwatch',{expression});
 await s.command('next');await stop();const st=await status();assert.equal(st.image_state.execution.image,image);assert.equal(st.frame.function,'debug_domain_marker');
 const local=number((await s.command('evaluate',{expression:'local'})).value);assert.equal(local,(id^input)>>>0);
 await s.command('watch',{expression:'debug_domain_id'});
 await s.command('watch',{expression:'@bao:debug_domain_id'});
 let watched=await status();
 for(const [name,owner,expected]of [['debug_domain_id',image,id],['@bao:debug_domain_id','bao',0xba000006]]){
  const v=watched.watches.find(v=>v.name===name);assert(v&&!v.error,JSON.stringify(v));assert.equal(number(v.value),expected);assert.equal(v.image_origin.image,owner);assert.equal(v.tree.child_count,0);
  const binding=await s.command('watch_resolve',{expression:name});assert.equal(binding.bits,32);assert.equal(binding.image_origin.image,owner);assert(binding.binding_id);assert.equal(binding.image_origin.memory.channel,'ahb');
 }
 await s.command('unwatch',{expression:'debug_domain_id'});await s.command('unwatch',{expression:'@bao:debug_domain_id'});
  assert.deepEqual(transport(),baseline);return {before,status:st,global,input,local,watched,mixed,mixedWatch,registers};
}
(async()=>{try{
 s=new Session(binary,project,out);
 if(!await suite.test('IMAGE-LOCATIONS-OFFLINE','Each image resolves the same function without attaching to the board',async()=>{
  for(const [n,id]of [[0,'bao'],[0,'vm0'],[1,'vm1']]){await core(n);const r=await s.command('image_locations',{image:id,location:'debug_domain_marker'});assert.equal(r.target_io,false);assert.equal(r.locations.length,1);addresses[id]=r.locations[0].address;}
  assert.notEqual(addresses.bao,addresses.vm0);assert.notEqual(addresses.vm0,addresses.vm1);assert.equal(s.logs('mi>').length,0);return addresses;
 }))return;
 if(!await suite.test('IMAGE-BREAK-PRESTART','Bao/VM0 code breakpoints coexist before Guest image copying, each bound to its own image',async()=>{
  await s.command('connect',{},true,65000);await s.command('control_scope',{scope:'core'});baseline=transport();
  await core(0);const a=await bp('bao'),b=await bp('vm0');assert.notEqual(a.id,b.id);await core(1);await bp('vm1');
  await core(0);await s.command('run');await stop();const st=await status();assert.equal(st.frame.function,'init');assert.equal(st.image_state.execution.image,'bao');return st;
 }))return;
 if(!await suite.test('IMAGE-BAO-VALUES','Physical Bao frame chooses Bao global and local types for identical symbol names',async()=>{
  await s.command('continue');await stop();return values('bao',0xba000006);
 }))return;
 if(!await suite.test('IMAGE-VM0-VALUES','Core0 Guest frame chooses VM0 globals/locals and survives Bao RAM image copying',async()=>{
  await s.command('continue');await stop();return values('vm0',0x6d000000);
 }))return;
 if(!await suite.test('IMAGE-CALLER-EXPRESSION','A selected physical caller frame combines its own saved locals and VM0 globals without changing that frame',async()=>{
  const initial=await status(),caller=initial.stack.find(f=>f.level>0&&f.function==='guest_main');assert(caller,'Fixture needs the real Guest caller frame');await s.command('frame',{level:caller.level});
  try{const before=await status(),local=before.locals.find(v=>['tick','seq','handled','last_send_tick'].includes(v.name)&&!v.error&&/^(?:0x[0-9a-f]+|[0-9]+)/i.test(v.value));assert(local,'Fixture needs a recoverable caller local');const own=number((await s.command('evaluate',{expression:local.name})).value),from=s.logs('mi>').length;const expression=local.name+' + debug_domain_id',value=await s.command('evaluate',{expression});assert.equal(number(value.value),(own+0x6d000000)>>>0);assert.equal(value.source,'isolated_current_frame');assert.equal(value.context.frame,caller.level);assert.equal(value.register_frame,0);assert.equal(value.frame_unwind_verified,true);assert.deepEqual((await status()).frame,before.frame);
   // Check the real physical GDB again, not merely an unchanged UI snapshot.
   const physicalAfter=await s.command('evaluate',{expression:local.name});assert.equal(number(physicalAfter.value),own);
   await s.command('watch',{expression});const watched=(await status()).watches.find(v=>v.name===expression);assert(watched&&!watched.error,JSON.stringify(watched));assert.equal(number(watched.value),(own+0x6d000000)>>>0);assert.equal(watched.image_origin.image,'vm0');await s.command('unwatch',{expression});assert.equal(number((await s.command('evaluate',{expression:local.name})).value),own);
   const commands=s.logs('mi>').slice(from).map(e=>e.text);assert(!commands.some(c=>/-stack-select-frame|-data-write-register|-data-write-memory/.test(c)));assert.deepEqual(transport(),baseline);return {before,local:local.name,own,expression,value,physicalAfter,watched,commands};}
  finally{await s.command('frame',{level:initial.frame.level});}
 }))return;
 if(!await suite.test('IMAGE-EXPRESSION-BOUNDARIES','Other-image values use global scope, pure registers retain the physical frame and unsafe mixed expressions cannot reach GDB',async()=>{
  const before=await status(),other=await s.command('evaluate',{expression:'@bao:debug_domain_id + 1'});assert.equal(number(other.value),0xba000007);assert.equal(other.source,'isolated_image_globals');
  const pc=await s.command('evaluate',{expression:'$pc + 4'});assert.equal(number(pc.value),number(before.frame.address)+4);
  const errors=[];for(const expression of ['@bao:$r0 + debug_domain_id','$r0 + debug_domain_id++','$r0 + debug_domain_marker(1)','$r0 = debug_domain_id']){const from=s.logs('mi>').length,error=await s.command('evaluate',{expression},false);errors.push({expression,error});assert(!s.logs('mi>').slice(from).some(e=>/-data-evaluate-expression|-data-write|-exec-/.test(e.text)),error);}
  assert.deepEqual((await status()).frame,before.frame);assert.deepEqual(transport(),baseline);return {before,other,pc,errors};
 }))return;
 if(!await suite.test('IMAGE-WATCH-TREE','Guest aggregate Watch and children use Guest DWARF and AHB, never Bao frame registers',async()=>{
  await core(0);const expression='@vm0:*(struct status*)0x10027000';await s.command('watch',{expression});
  let st=await status(),value=st.watches.find(v=>v.name===expression);assert(!value.error,JSON.stringify(value));assert.equal(value.image_origin.image,'vm0');assert(value.tree.child_count>=28);
  await s.command('watch_expand',{expression,expanded:true});st=await status();value=st.watches.find(v=>v.name===expression);const vm=value.tree.children.find(v=>v.name==='vm_id');assert(vm&&!vm.error);assert.equal(number(vm.value),0);assert.equal(vm.image_origin.image,'vm0');
  const path=vm.tree.path,typed=await s.command('watch_resolve',{expression,path});assert.equal(typed.address,0x10027004);assert.equal(typed.bits,32);assert.equal(typed.image_origin.image,'vm0');
  const bad=await s.command('evaluate',{expression:'@vm0:debug_domain_marker(1)'},false);assert(bad.includes('read-only'));
  const wrong=await s.command('evaluate',{expression:'@vm1:debug_domain_id'},false);assert(wrong.includes('not configured for this core'));
  await s.command('unwatch',{expression});assert.deepEqual(transport(),baseline);return {value,typed,bad,wrong};
 }))return;
 if(!await suite.test('IMAGE-WATCH-WRITE-GUARD','Isolated image Watch cannot fall back to a physical GDB assignment or ambiguous global condition',async()=>{
  await s.command('watch',{expression:'debug_domain_id'});const context=(await s.command('registers_list')).context;
  const from=s.logs('mi>').length;const error=await s.command('write_preview',{context,target:{kind:'variable',pane:'watch',expression:'debug_domain_id'},selection:{kind:'register'},input:{kind:'unsigned',text:'1'}},false);
  assert(error.includes('read-only'),error);
  const condition=await s.command('break',{image:'vm0',location:'debug_domain_marker',hardware:true,condition:'debug_domain_id != 0'},false);assert(condition.includes('not objfile-safe'),condition);
  const tail=s.logs('mi>').slice(from).map(e=>e.text);assert(!tail.some(c=>/-var-assign|-data-write-memory|-break-insert/.test(c)));
  await s.command('unwatch',{expression:'debug_domain_id'});assert.deepEqual(transport(),baseline);return {error,condition};
 }))return;
 if(!await suite.test('IMAGE-DATA-BREAK','A named VM0 scalar data breakpoint resolves through VM0 DWARF and triggers on VM0 writes',async()=>{
  const before=number((await s.command('evaluate',{expression:'debug_domain_counter'})).value);
  await s.command('data_break',{image:'vm0',expression:'debug_domain_counter',access:'write'});let st=await status();const b=st.breakpoints.find(b=>b.image==='vm0'&&b.location==='debug_domain_counter');assert(b&&b.kind.includes('watchpoint'),JSON.stringify(st.breakpoints));
  await s.command('continue');await stop();st=await status();assert.equal(st.image_state.execution.image,'vm0');assert(st.stop_reason.includes('watchpoint'),st.stop_reason);assert(number((await s.command('evaluate',{expression:'debug_domain_counter'})).value)>before);
  await s.command('delete_break',{number:b.id});assert.deepEqual(transport(),baseline);return st;
 }))return;
 if(!await suite.test('IMAGE-VM1-VALUES','Core1 Guest frame independently chooses VM1 globals/locals with the same names',async()=>{
  await core(1);await stop();return values('vm1',0x6d000001);
 }))return;
 if(!await suite.test('IMAGE-BOUNDARY-REJECTION','Cross-core image binding is rejected before any physical breakpoint command',async()=>{
  const count=s.logs('mi>').length;const error=await s.command('break',{image:'vm0',location:'debug_domain_marker'},false);assert(error.includes('not configured for this core'));
  const tail=s.logs('mi>').slice(count).map(e=>e.text);assert(!tail.some(c=>c.includes('-break-insert')));assert.deepEqual(transport(),baseline);return {error,physical_break_insertions:0};
 }))return;
 if(!await suite.test('IMAGE-BREAK-PERSIST','Persist original function names plus image ids for a later connection',async()=>{
  await core(0);await bp('bao',false);await bp('vm0',false);
  const text=fs.readFileSync(project,'utf8');assert(text.includes('image = "bao"'));assert(text.includes('image = "vm0"'));assert(text.includes('location = "debug_domain_marker"'));
  await s.close();s=null;return {saved_project_sha256:hash(project)};
 }))return;
 if(!await suite.test('IMAGE-BREAK-RESTORE','A fresh process restores both image bindings and resolves each function against its own ELF',async()=>{
  const dir=path.join(out,'restore');fs.mkdirSync(dir);s=new Session(binary,project,dir);await s.command('connect',{},true,65000);await core(0);const st=await status();
  for(const image of ['bao','vm0']){const b=st.breakpoints.find(b=>b.image===image);assert(b);assert.equal(b.location,'debug_domain_marker');assert.equal(number(b.address),addresses[image]);assert.equal(b.pending,false);assert.equal(b.restore_error,'');}
  assert.equal(st.breakpoints.length,2);await s.command('delete_break',{number:''});return st;
 }))return;
}catch(error){await suite.test('IMAGE-SYMBOLS-SETUP','Execute image symbol board regression',async()=>{throw error;});}
finally{if(s)await suite.test('IMAGE-SYMBOLS-CLEANUP','Close only owned debugger processes',()=>s.close());suite.finish();}})();
