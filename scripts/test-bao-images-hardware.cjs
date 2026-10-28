// Explicit physical THA6206 milestones. Connect includes the reviewed project reset.
const fs=require('node:fs'),path=require('node:path'),assert=require('node:assert/strict');
const {execFileSync}=require('node:child_process');
const {Session,Cases,outputDirectory,parseOptions,hash}=require('./test-support/session.cjs');
const options=parseOptions(process.argv.slice(2),['--binary','--project'],['--run']);
assert(options.run&&options.binary&&options.project,'Requires --run --binary EXE --project Bao/debug-multi-images.toml');
const binary=path.resolve(options.binary),project=path.resolve(options.project),out=outputDirectory('bao-images-hardware');
const guestSource=fs.readFileSync(path.join(path.dirname(project),'configs/tha6206-dual/guest.c'),'utf8');
const irqAssignmentLine=guestSource.split(/\r?\n/).findIndex(line=>line.includes('STATUS->last_irq = id;'))+1;assert(irqAssignmentLine>0);
const suite=new Cases(out,{board_tests_executed:true,binary,binary_sha256:hash(binary),project,project_sha256:hash(project),
 scope:'Physical Bao/VM source, saved Guest native GDB stack/locals, HVC return/user-stop priority, 20 HVC round trips, timer IRQ and mixed-core milestones; full goal acceptance remains pending',
 preparation:'matching firmware downloaded separately; project connect performs one explicit chip reset'});
let s,baseline,contextAddress,contextWords,physicalProcesses;
function processes(){
 assert(Number.isInteger(s.child.pid)&&s.child.pid>0);
 const command=`@(Get-CimInstance Win32_Process -Filter "ParentProcessId=${s.child.pid}" | Select-Object ProcessId,ParentProcessId,Name,ExecutablePath,CommandLine) | ConvertTo-Json -Compress`;
 const rows=JSON.parse(execFileSync('pwsh.exe',['-NoProfile','-Command',command],{encoding:'utf8',windowsHide:true,timeout:60000}).trim()||'[]');
 const found=(Array.isArray(rows)?rows:[rows]).filter(p=>/^(?:arm-none-eabi-gdb|openocd)\.exe$/i.test(p.Name));
 assert.equal(found.filter(p=>/gdb\.exe$/i.test(p.Name)).length,2);assert.equal(found.filter(p=>/^openocd/i.test(p.Name)).length,1);
 return found.sort((a,b)=>a.ProcessId-b.ProcessId);
}
const core=async n=>s.command('select_core',{name:'core.'+n});
const scope=async value=>s.command('control_scope',{scope:value});
const status=()=>s.command('status');
const stopped=()=>s.command('wait_stopped',{timeout_ms:20000},true,25000);
const bp=async(location,temporary=true)=>{
 const result=await s.command('break',{location,kind:'hardware',hardware:true,temporary});
 const st=await status();assert(st.breakpoints.some(b=>b.kind.includes('hw')&&b.location===location),'Expected a real hardware breakpoint');return result;
};
const transport=()=>s.logs('mi>').map(l=>l.text).filter(t=>/-target-select|-gdb-exit|monitor.*(?:reset|halt)|-target-download|-exec-interrupt/.test(t));
function integer(text){const match=/0x[0-9a-f]+|^[0-9]+/i.exec(text);assert(match,text);return Number(BigInt(match[0]));}
async function dump(address,count,channel){
 const context=(await s.command('registers_list')).context;
 return s.command('memory_dump',{address:'0x'+address.toString(16),count,channel,context});
}
(async()=>{try{
 s=new Session(binary,project,out);
 if(!await suite.test('BAO-IMAGES-CONNECT','Attach both cores with their resident Bao/Guest symbols',async()=>{
  await s.command('connect',{},true,65000);await scope('core');
  for(const n of [0,1]){await core(n);const inv=await s.command('images');assert.deepEqual(inv.state.images.map(i=>i.id),['bao','vm'+n]);}
  baseline=transport();physicalProcesses=processes();return {status:await status(),transport:baseline,physical_processes:physicalProcesses};
 }))return;
 if(!await suite.test('BAO-IMAGES-A','Both physical cores reach Bao init with valid C source',async()=>{
  await core(1);await bp('init');await scope('all');await s.command('run');
  const evidence=[];for(const n of [0,1]){await core(n);await stopped();const st=await status();assert.equal(st.frame.function,'init');assert.equal(st.image_state.execution.image,'bao');assert(st.frame.file.endsWith('init.c'));evidence.push(st);}
  return evidence;
 }))return;
 if(!await suite.test('BAO-IMAGES-B','Each physical core enters its own Guest source without symbol reload',async()=>{
  for(const n of [0,1]){await core(n);await bp('guest_main');}
  await scope('all');await s.command('continue');const evidence=[];
  for(const n of [0,1]){await core(n);await stopped();const st=await status();assert.equal(st.image_state.execution.image,'vm'+n);assert.equal(st.frame.function,'guest_main');assert(st.frame.file.endsWith('guest.c'));evidence.push(st);}
  assert.deepEqual(transport(),baseline);return evidence;
 }))return;
 if(!await suite.test('BAO-IMAGES-C','Real VM0 HVC enters Bao with a stable physical Guest register snapshot',async()=>{
  await scope('core');await core(0);await bp('hvc_handler');await core(1);await s.command('continue');await core(0);await s.command('continue');await stopped();
  const st=await status();assert.equal(st.image_state.execution.image,'bao');assert(st.frame.function.includes('hvc_handler'));
  contextAddress=integer((await s.command('evaluate',{expression:'(unsigned long)&bao_debug_guest_contexts[0]'})).value);
  const evidence=await dump(contextAddress,112,'ahb');const bytes=Buffer.from(evidence.bytes);contextWords=Array.from({length:28},(_,n)=>bytes.readUInt32LE(n*4));
  assert.equal(contextWords[0],0x42474331);assert.equal(contextWords[1],1);assert.equal(contextWords[2],112);assert.equal(contextWords[3]&1,0);assert.equal(contextWords[4],1);assert.equal(contextWords[5],0);assert.equal(contextWords[25],1);assert.equal(contextWords[26]>>>26,0x12);
  assert(contextWords[21]>=0x10020000&&contextWords[21]<0x10028000,'Real Guest SP must be inside its configured RAM');assert(contextWords[23]>=0x10020000&&contextWords[23]<0x10024000);assert.equal(contextWords[24]&31,0x13);
  const again=await dump(contextAddress,112,'ahb');assert.deepEqual(again.bytes,evidence.bytes);assert.deepEqual(transport(),baseline);
  fs.writeFileSync(path.join(out,'guest-snapshot-words.json'),JSON.stringify(contextWords,null,2));return {status:st,snapshot:evidence,decoded:contextWords};
 }))return;
 if(!await suite.test('BAO-IMAGES-F','Native GDB unwinds suspended Guest registers and AHB stack without changing physical frame',async()=>{
  const before=await status();
  assert.equal(before.image_state.program_cpsr&31,0x1a,'Physical stopped program must be verified Hyp');
  const view=await s.command('guest_context',{image:'vm0',expressions:['r0','r1','r2']},true,45000);
  assert.equal(view.origin,'bao_v1_saved_guest');assert.equal(view.read_only,true);
  assert.equal(view.saved.registers[13],contextWords[21]);assert.equal(view.saved.registers[15],contextWords[23]);
  assert.equal(view.stack[0].function,'notify');assert(view.stack.some(f=>f.function==='guest_main'));assert(view.stack.some(f=>f.function==='guest_reset'));
  assert(view.expressions.every(e=>!e.error),'Guest variables must be evaluated in saved registers');
  assert.equal(integer(view.expressions[0].value),0x86000001);
  const after=await status();assert.deepEqual(after.frame,before.frame);assert.deepEqual(after.image_state.execution,before.image_state.execution);assert.deepEqual(transport(),baseline);
  fs.writeFileSync(path.join(out,'saved-guest-native-gdb.json'),JSON.stringify(view,null,2));return view;
 }))return;
 await suite.test('BAO-IMAGES-GUEST-MEMORY-DIAGNOSTIC','Record whether physical GDB can read suspended Guest stack; retain AP evidence',async()=>{
  const ap=await dump(contextWords[21],64,'ahb');let gdb;
  try{gdb={ok:true,result:await dump(contextWords[21],64,'')};assert.deepEqual(gdb.result.bytes,ap.bytes);}catch(e){gdb={ok:false,error:e.message};}
  fs.writeFileSync(path.join(out,'guest-stack-memory-routes.json'),JSON.stringify({gdb,ap},null,2));return {gdb,ap};
 });
 await suite.test('BAO-IMAGES-C-RETURN','Return from Bao to saved VM0 PC using a core-local hardware breakpoint',async()=>{
  const operation=await s.command('return_guest',{image:'vm0'},true,45000);assert.equal(operation.scope,'selected_core');await stopped();const st=await status();assert.equal(st.image_state.execution.image,'vm0');assert.equal(integer(st.frame.address),contextWords[23]);assert.equal(st.image_state.return_guest.state,'completed');assert.equal(st.breakpoints.length,0);assert(st.frame.file.endsWith('guest.c'));assert.deepEqual(transport(),baseline);return st;
 });
 if(!await suite.test('BAO-IMAGES-H-PRIORITY','An actual user breakpoint interrupts HVC return and removes only its owned temporary breakpoint',async()=>{
  await bp('hvc_handler');await s.command('continue');await stopped();await bp('syscall_handler',false);
  let st=await status();const ownedUser=st.breakpoints.find(b=>b.location==='syscall_handler');assert(ownedUser);
  await s.command('return_guest',{image:'vm0'},true,45000);await stopped();st=await status();assert.equal(st.frame.function,'syscall_handler');assert.equal(st.stop_reason,'breakpoint-hit');assert.equal(st.image_state.return_guest.state,'interrupted');assert.equal(st.breakpoints.length,1);assert.equal(st.breakpoints[0].id,ownedUser.id);
  await s.command('delete_break',{number:ownedUser.id});await s.command('return_guest',{image:'vm0'},true,45000);await stopped();st=await status();assert.equal(st.image_state.execution.image,'vm0');assert.equal(st.image_state.return_guest.state,'completed');assert.equal(st.breakpoints.length,0);assert.deepEqual(transport(),baseline);
  return st;
 }))return;
 if(!await suite.test('BAO-IMAGES-I','Twenty real VM0 HVC round trips keep connections, symbols and hardware breakpoint count stable',async()=>{
  const samples=[];let sequence=contextWords[3];
  for(let n=0;n<20;n++){
   await bp('hvc_handler');await s.command('continue');await stopped();let st=await status();assert.equal(st.image_state.execution.image,'bao');assert.equal(st.breakpoints.length,0);
   const snapshot=await dump(contextAddress,112,'ahb');const raw=Buffer.from(snapshot.bytes);const seq=raw.readUInt32LE(12),pc=raw.readUInt32LE(92);
   assert(seq>sequence);sequence=seq;assert.equal(raw.readUInt32LE(100),1);
   if(n===0||n===19){const view=await s.command('guest_context',{image:'vm0',level:1,expressions:['handled','last_send_tick','tick','seq','payload']},true,45000);assert.equal(view.selected_frame.function,'guest_main');assert(view.stack.some(f=>f.function==='guest_reset'));fs.writeFileSync(path.join(out,'guest-round-'+n+'.json'),JSON.stringify(view,null,2));}
   await bp('*0x'+pc.toString(16));await s.command('continue');await stopped();st=await status();assert.equal(st.image_state.execution.image,'vm0');assert.equal(integer(st.frame.address),pc);assert.equal(st.breakpoints.length,0);assert.deepEqual(transport(),baseline);
   samples.push({round:n+1,sequence:seq,pc,breakpoints:st.breakpoints.length,core1:st.cores[1].state});
  }
  const afterProcesses=processes();assert.deepEqual(afterProcesses,physicalProcesses,'The same two physical GDBs and OpenOCD must survive all twenty domain round trips');fs.writeFileSync(path.join(out,'physical-process-continuity.json'),JSON.stringify({before:physicalProcesses,after:afterProcesses},null,2));
  fs.writeFileSync(path.join(out,'twenty-real-hvc-round-trips.json'),JSON.stringify(samples,null,2));return samples;
 }))return;
 if(!await suite.test('BAO-IMAGES-D-ENTRY','Physical IRQ enters Bao and exposes a saved Guest register context',async()=>{
  await bp('bao_debug_irq_active');await s.command('continue');await stopped();let st=await status();assert.equal(st.image_state.execution.image,'bao');
  const view=await s.command('guest_context',{image:'vm0'},true,45000);assert.equal(view.saved.edge,2);assert.equal(view.saved.cpu,0);assert.equal(view.saved.vm,0);
  await s.command('break',{location:'interrupts_handle',hardware:true,temporary:true,condition:'int_id == 30'});await s.command('continue');await stopped();st=await status();assert(st.frame.function.includes('interrupts_handle'));assert.equal(integer((await s.command('evaluate',{expression:'int_id'})).value),30);assert.deepEqual(transport(),baseline);
  fs.writeFileSync(path.join(out,'physical-irq-saved-guest.json'),JSON.stringify({view,status:st},null,2));return {view,status:st};
 }))return;
 if(!await suite.test('BAO-IMAGES-D-GUEST','Physical timer INTID 30 is delivered as a Guest virtual IRQ and returns',async()=>{
  await s.command('break',{location:'guest.c:'+irqAssignmentLine,hardware:true,temporary:true,condition:'id == 30'});await s.command('continue');await stopped();let st=await status();assert.equal(st.image_state.execution.image,'vm0');assert.equal(st.frame.function,'irq_handler');
  const id=integer((await s.command('evaluate',{expression:'id'})).value);assert.equal(id,30);
  await s.command('finish');await stopped();st=await status();assert.equal(st.image_state.execution.image,'vm0');assert(st.frame.file.endsWith('start.S'));
  const irq_return=[];for(let n=0;n<12&&(st.image_state.program_cpsr&31)===0x12;n++){await s.command('stepi');await stopped();st=await status();irq_return.push({frame:st.frame,cpsr:st.image_state.program_cpsr});}
  assert.equal(st.image_state.program_cpsr&31,0x13);assert.equal(st.image_state.execution.image,'vm0');assert.deepEqual(transport(),baseline);return {status:st,virtual_intid:id,irq_return};
 }))return;
 if(!await suite.test('BAO-IMAGES-G','Core0 stays in VM0 while core1 stops in Bao with an independently restored VM1 stack',async()=>{
  await core(1);let st=await status();if(st.state==='RUNNING')await s.command('pause');
  baseline=transport(); // Explicit, user-visible preparation pause of core1 only.
  await bp('hvc_handler');await core(0);await bp('*0x'+contextWords[23].toString(16));await s.command('continue');await core(1);await s.command('continue');
  await core(0);await stopped();const vm0=await status();assert.equal(vm0.image_state.execution.image,'vm0');
  await core(1);await stopped();const bao1=await status();assert.equal(bao1.image_state.execution.image,'bao');
  const view=await s.command('guest_context',{image:'vm1',expressions:['r0','r1','r2']},true,45000);
  assert.equal(view.saved.cpu,1);assert.equal(view.saved.vm,1);assert.equal(view.image,'vm1');assert.equal(integer(view.expressions[0].value),0x86000001);assert(view.stack.some(f=>f.function==='guest_main'));assert(view.stack.some(f=>f.function==='guest_reset'));
  assert(view.saved.registers[13]>=0x10028000&&view.saved.registers[13]<0x10030000);
  const wrong=await s.command('guest_context',{image:'vm0'},false);assert(wrong.includes('not bound to this core'));
  await core(0);const wrongDomain=await s.command('guest_context',{image:'vm0'},false);assert(wrongDomain.includes('Hyp context'));
  assert.deepEqual(transport(),baseline);fs.writeFileSync(path.join(out,'mixed-cores-saved-vm1.json'),JSON.stringify({vm0,bao1,view},null,2));return {vm0,bao1,view};
 }))return;
}catch(error){await suite.test('BAO-IMAGES-SETUP','Prepare or execute physical milestones',async()=>{throw error;});}
finally{if(s)await suite.test('BAO-IMAGES-CLEANUP','Close only this test workspace',()=>s.close());suite.finish();}})();
