// Deliberately exhaust only this test's core0 hardware breakpoints. Real board,
// real backend rejection; no synthetic register writes or failure replies.
const fs=require('node:fs'),path=require('node:path'),assert=require('node:assert/strict');
const {Session,Cases,outputDirectory,parseOptions,hash,quote,delay}=require('./test-support/session.cjs');
const o=parseOptions(process.argv.slice(2),['--binary','--project'],['--run']);
assert(o.run&&o.binary&&o.project,'Requires --run --binary EXE --project Bao/debug-multi-images.toml');
const binary=path.resolve(o.binary),original=path.resolve(o.project),bao=path.dirname(original).replaceAll('\\','/'),out=outputDirectory('bao-images-resources-hardware');
const project=path.join(out,'debug.toml');
fs.writeFileSync(project,fs.readFileSync(original,'utf8').replace(/^elf = "(?![A-Za-z]:|\/)/gm,'elf = "'+bao+'/').replaceAll('source_root = "."','source_root = '+quote(bao)));
const suite=new Cases(out,{board_tests_executed:true,binary,binary_sha256:hash(binary),project,original_project:original,
 scope:'Real core0 hardware-breakpoint exhaustion, failed Guest-return cleanup, independent core1 resources and retry',
 elfs:Object.fromEntries(['bin/tha6xxx/tha6206-dual/bao.elf','build/tha6xxx/tha6206-dual/guest/guest0.elf','build/tha6xxx/tha6206-dual/guest/guest1.elf'].map(p=>[p,hash(path.join(bao,p))]))});
let s,baseline,before,allocated=[],returnPC;
const core=n=>s.command('select_core',{name:'core.'+n});
const status=()=>s.command('status');
const stop=()=>s.command('wait_stopped',{timeout_ms:20000},true,25000);
const bp=(image,location,temporary=true)=>s.command('break',{image,location,temporary,hardware:true});
const transport=()=>s.logs('mi>').map(e=>e.text).filter(t=>/-target-select|-gdb-exit|monitor.*(?:reset|halt)|-target-download|-exec-interrupt/.test(t));
const integer=text=>Number(BigInt(/0x[0-9a-f]+|^[0-9]+/i.exec(text)[0]));
(async()=>{try{
 s=new Session(binary,project,out);
 if(!await suite.test('RESOURCE-PREPARE','Stop core0 at a fully saved HVC and explicitly pause core1 after Guest startup',async()=>{
  await s.command('connect',{},true,65000);await s.command('control_scope',{scope:'core'});await core(1);await bp('bao','init');await s.command('control_scope',{scope:'all'});await s.command('run');
  for(const n of [0,1]){await core(n);await stop();assert.equal((await status()).frame.function,'init');}
  for(const n of [0,1]){await core(n);await bp('vm'+n,'guest_main');}await s.command('control_scope',{scope:'all'});await s.command('continue');
  for(const n of [0,1]){await core(n);await stop();assert.equal((await status()).image_state.execution.image,'vm'+n);}await s.command('control_scope',{scope:'core'});
  // Guest IPC notification needs its peer. Let core1 pass that handshake before
  // deliberately pausing it as explicit resource-test preparation.
  await core(1);await s.command('continue');await core(0);await bp('bao','hvc_handler');await s.command('continue');await stop();before=await status();assert.equal(before.frame.function,'hvc_handler');
  await core(1);await s.command('pause');await core(0);
  const view=await s.command('guest_context',{image:'vm0'},true,45000);assert.equal(view.saved.edge,1);assert.equal(view.saved.syndrome>>>26,0x12);returnPC=view.saved.registers[15];baseline=transport();assert.equal(before.breakpoints.length,0);return {before,view};
 }))return;
 if(!await suite.test('RESOURCE-EXHAUST','Exceed core0 hardware capacity and reject Guest return while preserving every user breakpoint and stopped frame',async()=>{
  const inventory=await s.command('images'),range=inventory.state.images.find(i=>i.id==='bao').executable_ranges.find(r=>r.end-r.start>=256);assert(range,'Need 32 distinct instruction-aligned addresses in Bao code');
  let insertionError;for(let i=0;i<32;i++){const address=range.start+i*4;if(address===integer(before.frame.address))continue;try{await bp('bao','*0x'+address.toString(16),false);}catch(error){insertionError=error.message;break;}}
  allocated=(await status()).breakpoints.map(b=>b.id);assert(allocated.length>0);const from=s.logs('mi>').length,operation=await s.command('return_guest',{image:'vm0'},true,45000);
  // This real GDB acknowledges ^running, then reports insertion failure as an
  // asynchronous diagnostic with no *stopped. The normal UI must recover too,
  // without a test-only wait_stopped call or an injected pause.
  let after;for(let i=0;i<16;i++){after=await status();if(after.state==='STOPPED')break;await delay(250);}
  const errors=s.logs().filter(e=>/Could not insert hardware breakpoints|requested too many hardware|resource not available/i.test(e.text)).map(e=>({channel:e.channel,text:e.text}));
  fs.writeFileSync(path.join(out,'resource-failure-observed.json'),JSON.stringify({allocated,insertionError,operation,after,errors},null,2));assert(errors.length>0,'Need real backend exhaustion evidence');assert.equal(after.state,'STOPPED');assert.deepEqual(after.frame,before.frame);assert.deepEqual(after.breakpoints.map(b=>b.id).sort(),[...allocated].sort(),'Failure may remove only its owned temporary breakpoint');
  assert.equal(after.image_state.return_guest?.state,'failed');assert.match(after.image_state.return_guest.detail,/hardware breakpoint/i);assert.deepEqual(transport(),baseline);return {allocated,insertionError,operation,errors,after,commands:s.logs('mi>').slice(from).map(e=>e.text)};
 }))return;
 if(!await suite.test('RESOURCE-CONTINUE','Ordinary Continue also recovers its stopped state after asynchronous hardware insertion failure',async()=>{
  const from=s.logs('mi>').length,operation=await s.command('continue');let after;for(let i=0;i<16;i++){after=await status();if(after.state==='STOPPED')break;await delay(250);}
  assert.equal(after.state,'STOPPED');assert.equal(after.stop_reason,'hardware-breakpoint-insertion-failed');assert.deepEqual(after.frame,before.frame);assert.deepEqual(after.breakpoints.map(b=>b.id).sort(),[...allocated].sort());assert.deepEqual(transport(),baseline);return {operation,after,commands:s.logs('mi>').slice(from).map(e=>e.text)};
 }))return;
 if(!await suite.test('RESOURCE-PER-CORE','Core1 can still use a hardware code breakpoint while core0 hardware resources are exhausted',async()=>{
  await core(1);await bp('vm1','debug_domain_marker');await s.command('continue');await stop();const st=await status();assert.equal(st.frame.function,'debug_domain_marker');assert.equal(st.image_state.execution.image,'vm1');assert.equal(st.breakpoints.length,0);assert.deepEqual(transport(),baseline);return st;
 }))return;
 if(!await suite.test('RESOURCE-RECOVER','Removing only test-owned core0 breakpoints allows the real HVC return to complete',async()=>{
  await core(0);for(const number of allocated)await s.command('delete_break',{number});assert.equal((await status()).breakpoints.length,0);
  const operation=await s.command('return_guest',{image:'vm0'},true,45000);await stop();const after=await status();assert.equal(after.image_state.return_guest.state,'completed');assert.equal(after.image_state.execution.image,'vm0');assert.equal(integer(after.frame.address),returnPC);assert.equal(after.breakpoints.length,0);assert.deepEqual(transport(),baseline);return {operation,after};
 }))return;
}catch(error){await suite.test('RESOURCE-SETUP','Execute real resource-limit regression',async()=>{throw error;});}
finally{if(s)await suite.test('RESOURCE-CLEANUP','Close only test-owned debugger processes',()=>s.close());suite.finish();}})();
