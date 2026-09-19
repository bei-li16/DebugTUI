// Development-only regression with real GDB. --hardware uses STM32F429 without flashing.
const {spawn,execFileSync}=require('node:child_process');
const fs=require('node:fs'),path=require('node:path'),readline=require('node:readline'),assert=require('node:assert/strict');
const root=path.dirname(__dirname),hardware=process.argv.includes('--hardware'),multi=process.argv.includes('--multi');
const binary=path.resolve(process.argv.find((a,i)=>i>1&&!a.startsWith('--'))||path.join(root,'target/release/debugtui.exe'));
const out=path.join(root,'artifacts',`breakpoints-${hardware?'stm32':multi?'multi':'native'}-${Date.now()}`);
fs.mkdirSync(out,{recursive:true});const quote=s=>JSON.stringify(s.replaceAll('\\','/'));
let config,variable,location;
if(hardware){
  assert(!multi,'Hardware test is for the available single-core STM32F429');
  assert(process.env.DEBUGTUI_TEST_ELF,'Set DEBUGTUI_TEST_ELF to the ELF currently on the target');
  config=`version=2\n[tools]\nprofile=${quote(path.join(root,'tools/debug-env-openocd.toml'))}\n[program]\nelf=${quote(process.env.DEBUGTUI_TEST_ELF)}\n`;
  variable='xTickCount';location='vTaskSwitchContext';
}else{
  fs.writeFileSync(path.join(out,'sample.c'),`volatile unsigned int counter=0; volatile unsigned int observed;\n__attribute__((noinline)) void probe_tick(void) { counter++; observed=counter; }\nint main(void) { for(;;) { probe_tick(); } }\n`);
  execFileSync(process.env.DEBUGTUI_TEST_CC||'C:/MinGW/bin/gcc.exe',['-g','-O0',path.join(out,'sample.c'),'-o',path.join(out,'sample.exe')]);
  config=`version=2\n[gdb]\nexecutable=${quote(process.env.DEBUGTUI_TEST_GDB||'C:/MinGW/bin/gdb.exe')}\n[target]\nmode="local"\n[program]\nelf=${quote(path.join(out,'sample.exe'))}\n`;
  variable='counter';location='probe_tick';
}
config+=`[session]\nlog_dir=${quote(out)}\non_exit="${hardware?'detach':'disconnect'}"\n`;
if(multi)for(let i=0;i<2;i++)config+=`[[cores]]\nname="core${i}"\nendpoint="local${i}"\n`;
const project=path.join(out,'project.toml');fs.writeFileSync(project,config);
let next=1,child,exit,events;const pending=new Map(),report=[],logs=[];
function launch(){
 child=spawn(binary,['--project',project,'--headless','--stdio'],{windowsHide:true});exit=new Promise(r=>child.once('exit',r));
 events=fs.createWriteStream(path.join(out,`events-${next}.jsonl`));
 readline.createInterface({input:child.stdout}).on('line',line=>{events.write(line+'\n');const e=JSON.parse(line);if(e.event==='log')logs.push(e);if(e.event==='response')pending.get(e.id)?.(e);});
 child.stderr.on('data',d=>fs.appendFileSync(path.join(out,'stderr.log'),d));
}
async function cmd(method,params={},ok=true){const id=next++;const e=await new Promise((resolve,reject)=>{const timer=setTimeout(()=>{pending.delete(id);reject(Error(`Timed out: ${method}`));},30000);pending.set(id,e=>{clearTimeout(timer);pending.delete(id);resolve(e);});child.stdin.write(JSON.stringify({id,method,params})+'\n');});assert.equal(e.ok,ok,`${method}: ${e.error}`);return e.result;}
const breaks=async()=> (await cmd('status')).breakpoints;
const one=async()=>{const list=await breaks();assert.equal(list.length,1);return list[0];};
const clear=()=>cmd('delete_break');
async function stop(){await cmd('quit');assert.equal(await exit,0);events.end();child=null;}
(async()=>{try{
 launch();await cmd('connect');
 if(hardware){await cmd('console',{command:'compare-sections .text'});assert(logs.some(e=>e.text.includes('matched.')),'ELF must match the firmware');}
 else if(!multi){await cmd('break',{location:'main'});await cmd('run');await cmd('wait_stopped');await clear();}
 if(multi){
   await cmd('select_core',{index:0});
   await cmd('break',{location,enabled:false,condition:'counter >= 1',ignore_count:2});
   const core0=await one();await cmd('select_core',{index:1});assert.equal((await breaks()).length,0);
   await cmd('break',{location:'main',enabled:false});await cmd('enable_break',{all:true,enabled:true});assert.equal((await one()).enabled,true);
   await cmd('select_core',{index:0});assert.equal((await one()).enabled,false);assert.equal((await one()).condition,core0.condition);
   await cmd('reconnect');assert.equal((await one()).enabled,false);await cmd('select_core',{index:1});assert.equal((await one()).enabled,true);
   await stop();report.push('Independent per-core lists, disable/enable-all isolation and reconnect persistence');
   launch();await cmd('connect');await cmd('select_core',{index:0});assert.equal((await one()).enabled,false);await cmd('select_core',{index:1});assert.equal((await one()).enabled,true);await stop();
   report.push('Both core preference records survive process restart');
 }else{
   await cmd('break',{location,enabled:false,condition:`${variable} >= 0`,ignore_count:2});let b=await one();assert.equal(b.enabled,false);assert.equal(b.ignore_count,2);
   assert(fs.readFileSync(project,'utf8').includes('enabled = false'),'Disable state must save immediately');
   await cmd('enable_break',{number:b.id,enabled:true});assert.equal((await one()).id,b.id);await cmd('continue');await cmd('wait_stopped');b=await one();assert(b.hit_count>=1);assert.equal(b.ignore_count,0);
   await cmd('update_break',{number:b.id,enabled:false,condition:`${variable} >= 1`,ignore_count:3});b=await one();assert.equal(b.enabled,false);assert.equal(b.ignore_count,3);
   await cmd('update_break',{number:b.id,condition:'NOT_A_SYMBOL + ('},false);assert.equal((await one()).condition,b.condition);assert.equal((await one()).enabled,false);
   report.push('Code breakpoint checkbox semantics, condition, ignore count, hit count and failed edit rollback');
   await clear();await cmd('break',{location,temporary:true});await cmd('continue');await cmd('wait_stopped');assert.equal((await breaks()).length,0);
   if(hardware){await cmd('break',{location,hardware:true});assert((await one()).kind.includes('hw'));await clear();}
   report.push(hardware?'Temporary breakpoint removed after hit; hardware code breakpoint on STM32':'Temporary breakpoint removed after hit');
   for(const access of hardware?['write','read','access']:['write']){
     const expression=`*(unsigned int *)&${variable}`;
     await cmd('data_break',{expression,access,enabled:false});let w=await one();assert.equal(w.enabled,false);
     await cmd('enable_break',{number:w.id,enabled:true});assert.equal((await one()).id,w.id);
     await cmd('continue');await cmd('wait_stopped');const status=await cmd('status');assert(status.stop_reason.includes('watchpoint'),`${access} reason=${status.stop_reason}`);
     w=await one();assert(w.hit_count>=1);if(access==='read')assert(w.kind.includes('read'));if(access==='access')assert(w.kind.includes('acc'));
     await cmd('enable_break',{number:w.id,enabled:false});assert.equal((await one()).enabled,false);await clear();
     report.push(`${access} data breakpoint: pointer expression, disable/enable, actual target hit, delete`);
   }
   await cmd('data_break',{expression:variable,access:'execute'},false);assert.equal((await breaks()).length,0);
   await cmd('data_break',{expression:variable,condition:'INVALID_CONDITION + ('},false);assert.equal((await breaks()).length,0,'Failed watch condition must remove partial watchpoint');
   await cmd('break',{location,enabled:false,condition:`${variable} >= 1`,ignore_count:5});
   for(const access of hardware?['write','read','access']:['write'])await cmd('data_break',{expression:variable,access,enabled:false});
   await cmd('enable_break',{all:true,enabled:true});assert((await breaks()).every(b=>b.enabled));await cmd('enable_break',{all:true,enabled:false});
   const expected=(await breaks()).map(({location,kind,enabled,condition,ignore_count})=>({location,kind,enabled,condition,ignore_count}));
   await cmd('reconnect');assert.deepEqual((await breaks()).map(({location,kind,enabled,condition,ignore_count})=>({location,kind,enabled,condition,ignore_count})),expected);
   await stop();launch();await cmd('connect');assert.deepEqual((await breaks()).map(({location,kind,enabled,condition,ignore_count})=>({location,kind,enabled,condition,ignore_count})),expected);
   assert(!(await breaks()).some(b=>b.restore_error));
   await clear();await stop();report.push('Enable/disable all, rich code/data records across reconnect and full restart; invalid request cleanup');
   if(!hardware){
     fs.appendFileSync(project,'\n[[breakpoints]]\nlocation="MISSING_VARIABLE"\nkind="read"\nenabled=false\n');
     // Remove the existing empty array before appending its table form.
     fs.writeFileSync(project,fs.readFileSync(project,'utf8').replace(/^breakpoints = \[\]\r?\n/m,''));
     launch();await cmd('connect');const failed=await one();assert(failed.restore_error);assert.equal(failed.enabled,false);
     await cmd('enable_break',{number:failed.id,enabled:true},false);assert.equal((await one()).location,'MISSING_VARIABLE');
     await cmd('disconnect');assert(fs.readFileSync(project,'utf8').includes('MISSING_VARIABLE'));
     await cmd('connect');assert((await one()).restore_error);await clear();await stop();
     report.push('Unrestorable data records remain visible and saved; retry reports error; explicit delete removes them');
   }
 }
 fs.writeFileSync(path.join(out,'verification.json'),JSON.stringify({binary,hardware,multi,requests:next-1,passed:report},null,2));
 console.log(JSON.stringify({out,requests:next-1,passed:report},null,2));
}catch(e){console.error(e);process.exitCode=1;}finally{
 if(child && child.exitCode===null){try{await cmd('quit');await exit;}catch{child.kill();}}events?.end();
}})();
