// Genuine HVC instruction -> VM_EXIT -> dispatch -> VM_ENTRY -> ERET.
// Every run/step/break is explicit in this driver; image/source inspection adds none.
const fs=require('node:fs'),path=require('node:path'),assert=require('node:assert/strict');
const {Session,Cases,outputDirectory,parseOptions,hash,quote}=require('./test-support/session.cjs');
const o=parseOptions(process.argv.slice(2),['--binary','--project'],['--run']);
assert(o.run&&o.binary&&o.project,'Requires --run --binary EXE --project Bao/debug-multi-images.toml');
const binary=path.resolve(o.binary),original=path.resolve(o.project),bao=path.dirname(original).replaceAll('\\','/'),out=outputDirectory('bao-images-edges-hardware');
const project=path.join(out,'debug.toml');fs.writeFileSync(project,fs.readFileSync(original,'utf8').replace(/^elf = "(?![A-Za-z]:|\/)/gm,'elf = "'+bao+'/').replaceAll('source_root = "."','source_root = '+quote(bao)));
const suite=new Cases(out,{board_tests_executed:true,binary,binary_sha256:hash(binary),project,elfs:{bao:hash(path.join(bao,'bin/tha6xxx/tha6206-dual/bao.elf')),vm0:hash(path.join(bao,'build/tha6xxx/tha6206-dual/guest/guest0.elf')),vm1:hash(path.join(bao,'build/tha6xxx/tha6206-dual/guest/guest1.elf'))},scope:'Real instruction-level HVC save/restore validity, dispatch and ERET; physical frame/core identity; no injected CPU registers or altered firmware'});
let s,baseline,returnPC,baselineAborts;
const transport=()=>s.logs('mi>').map(l=>l.text).filter(t=>/-target-select|-gdb-exit|monitor.*(?:reset|halt)|-target-download|-exec-interrupt/.test(t));
const status=()=>s.command('status');
const stopped=()=>s.command('wait_stopped',{timeout_ms:20000},true,25000);
const bp=(location,image='bao')=>s.command('break',{image,location,hardware:true,temporary:true});
const integer=text=>Number(BigInt(/0x[0-9a-f]+|^[0-9]+/i.exec(text)[0]));
async function at(location){await bp(location);await s.command('continue');await stopped();return status();}
async function invalid(){const e=await s.command('guest_context',{image:'vm0'},false,45000);assert.match(e,/not fully saved|VM_ENTRY|magic\/version\/size/);return e;}
async function instruction(){await s.command('stepi');await stopped();return status();}
const aborts=()=>s.events.filter(e=>e.event==='log'&&/abort occurred/i.test(e.text));
function stable(st){assert.deepEqual(transport(),baseline);assert.equal(st.cores.find(c=>c.name==='core.1').state,'RUNNING');assert.equal(aborts().length,baselineAborts,'Debug inspection must not add an abort that can overwrite HSR');}
(async()=>{try{
 s=new Session(binary,project,out);
 if(!await suite.test('EDGE-PREPARE','Both Bao cores start, then VM0 stops immediately before a real HVC while core1 runs',async()=>{
  await s.command('connect',{},true,65000);await s.command('control_scope',{scope:'core'});await s.command('select_core',{name:'core.1'});await bp('init');await s.command('control_scope',{scope:'all'});await s.command('run');
  for(const n of [0,1]){await s.command('select_core',{name:'core.'+n});await stopped();assert.equal((await status()).frame.function,'init');}
  await s.command('control_scope',{scope:'core'});await s.command('select_core',{name:'core.1'});await s.command('continue');await s.command('select_core',{name:'core.0'});
  const asm=await s.command('disassemble',{address:'@vm0:notify',source:'elf'});const hvc=asm.assembly.find(line=>/\bhvc\b/.test(line));assert(hvc,'ELF must contain a real HVC instruction');const pc=integer(hvc);returnPC=pc+4;
  await bp('*0x'+pc.toString(16),'vm0');await s.command('continue');await stopped();const st=await status();assert.equal(st.image_state.execution.image,'vm0');assert.equal(integer(st.frame.address),pc);assert.equal(st.image_state.program_cpsr&31,0x13);baseline=transport();baselineAborts=aborts().length;stable(st);return {asm,hvc,status:st,returnPC,baselineAborts};
 }))return;
 if(!await suite.test('EDGE-HVC-EXIT','HVC reaches the Hyp trap before VM_EXIT is complete; partial saved Guest views are unavailable',async()=>{
  // A real exception-entry breakpoint makes backend stepping policy explicit.
  await bp('hyp_trap_handler');let st=await instruction();assert.equal(st.image_state.execution.image,'bao');assert.equal(st.image_state.program_cpsr&31,0x1a);assert.equal(st.frame.function,'_hyp_vector_table');
  const samples=[{status:st,error:await invalid()}];st=await instruction();assert.equal(st.frame.function,'hyp_trap_handler');samples.push({status:st,error:await invalid()});
  for(let n=0;n<5;n++){st=await instruction();assert.equal(st.image_state.execution.image,'bao');assert.equal(st.image_state.program_cpsr&31,0x1a);samples.push({status:st,error:await invalid()});stable(st);}
  return samples;
 }))return;
 if(!await suite.test('EDGE-SAVE-DISPATCH','Completed VM_EXIT permits the true Guest view before HVC dispatch and syscall handling',async()=>{
  const active=await at('bao_debug_trap_active');assert.equal(active.image_state.execution.image,'bao');const view=await s.command('guest_context',{image:'vm0'},true,45000);assert.equal(view.saved.registers[15],returnPC);assert.equal(view.saved.registers[16]&31,0x13);assert.equal(view.saved.edge,1);assert.equal(view.saved.syndrome>>>26,0x12,'Expected a genuine HVC32 syndrome, got HSR=0x'+view.saved.syndrome.toString(16));assert.equal(view.stack[0].function,'notify');
  const dispatch=await at('hvc_handler');assert.equal(dispatch.frame.function,'hvc_handler');const syscall=await at('syscall_handler');assert.equal(syscall.frame.function,'syscall_handler');stable(syscall);return {active,view,dispatch,syscall};
 }))return;
 if(!await suite.test('EDGE-ENTRY-ERET','VM_ENTRY invalidates the Guest snapshot before restoring registers; ERET restores real VM0 identity',async()=>{
  let st=await at('bao_debug_trap_inactive');assert.equal(st.image_state.execution.image,'bao');const rejected=await invalid();
  const asm=await s.command('disassemble',{address:st.frame.address,image:'bao',source:'elf'});const eret=asm.assembly.find(line=>/\beret\b/.test(line));assert(eret);const eretPC=integer(eret);const samples=[];
  for(let n=0;n<32&&integer(st.frame.address)!==eretPC;n++){st=await instruction();assert.equal(st.image_state.execution.image,'bao');assert.equal(st.image_state.program_cpsr&31,0x1a);samples.push({status:st,error:await invalid()});stable(st);}
  assert.equal(integer(st.frame.address),eretPC,'Stop before actual ERET');st=await instruction();assert.equal(st.image_state.execution.image,'vm0');assert.equal(integer(st.frame.address),returnPC);assert.equal(st.image_state.program_cpsr&31,0x13);assert(st.frame.file.startsWith('@vm0:'));assert.equal(st.breakpoints.length,0);stable(st);return {rejected,asm,eretPC,samples,returned:st};
 }))return;
 if(!await suite.test('EDGE-IRQ-EXIT','A real physical IRQ has no Guest view during partial VM_EXIT, then exposes a valid edge-2 snapshot',async()=>{
  let st=await at('hyp_irq_handler');assert.equal(st.image_state.execution.image,'bao');assert.equal(st.image_state.program_cpsr&31,0x1a);const samples=[{status:st,error:await invalid()}];
  for(let n=0;n<5;n++){st=await instruction();assert.equal(st.image_state.execution.image,'bao');samples.push({status:st,error:await invalid()});stable(st);}
  const active=await at('bao_debug_irq_active');const view=await s.command('guest_context',{image:'vm0'},true,45000);assert.equal(view.saved.edge,2);assert.equal(view.saved.syndrome,0);assert.equal(view.saved.cpu,0);assert.equal(view.saved.vm,0);assert(view.saved.registers[15]>=0x10020000&&view.saved.registers[15]<0x10028000);assert(view.stack.length>0);stable(active);return {samples,active,view};
 }))return;
 if(!await suite.test('EDGE-IRQ-ENTRY','IRQ return invalidates the saved snapshot throughout VM_ENTRY and ERET returns to Guest execution',async()=>{
  let st=await at('bao_debug_irq_inactive');const rejected=await invalid();const asm=await s.command('disassemble',{address:st.frame.address,image:'bao',source:'elf'});const eret=asm.assembly.find(line=>/\beret\b/.test(line));assert(eret);const eretPC=integer(eret),samples=[];
  for(let n=0;n<32&&integer(st.frame.address)!==eretPC;n++){st=await instruction();assert.equal(st.image_state.execution.image,'bao');assert.equal(st.image_state.program_cpsr&31,0x1a);samples.push({status:st,error:await invalid()});stable(st);}
  assert.equal(integer(st.frame.address),eretPC);st=await instruction();assert.equal(st.image_state.execution.image,'vm0');assert([0x13,0x12].includes(st.image_state.program_cpsr&31),'ERET may immediately deliver the pending virtual IRQ');assert(st.frame.file.startsWith('@vm0:'));assert.equal(st.breakpoints.length,0);stable(st);return {rejected,asm,eretPC,samples,returned:st};
 }))return;
}catch(error){await suite.test('EDGE-SETUP','Execute genuine exception boundary regression',async()=>{throw error;});}
finally{if(s)await suite.test('EDGE-CLEANUP','Close only owned debugger processes',()=>s.close());suite.finish();}})();
