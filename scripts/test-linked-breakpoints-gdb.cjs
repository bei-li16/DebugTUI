// Three real GDB sessions: membership, persistence and rollback without hardware.
const {spawn,execFileSync}=require('node:child_process');
const fs=require('node:fs'),path=require('node:path'),readline=require('node:readline'),assert=require('node:assert/strict');
const root=path.dirname(__dirname),binary=path.resolve(process.argv[2]||'target/debug/debugtui.exe');
const out=path.join(root,'artifacts',`linked-breakpoints-${Date.now()}`);fs.mkdirSync(out,{recursive:true});
const q=s=>JSON.stringify(s.replaceAll('\\','/'));
fs.writeFileSync(path.join(out,'sample.c'),'volatile unsigned counter;\n__attribute__((noinline)) void tick(void){counter++;}\nint main(void){for(;;)tick();}\n');
execFileSync(process.env.DEBUGTUI_TEST_CC||'gcc',['-g','-O0',path.join(out,'sample.c'),'-o',path.join(out,'sample.exe')]);
const file=path.join(out,'debug.toml'),elf=path.join(out,'sample.exe');
fs.writeFileSync(file,`version=2\n[gdb]\nexecutable=${q(process.env.DEBUGTUI_TEST_GDB||'gdb')}\n[target]\nmode="local"\n[program]\nelf=${q(elf)}\n[session]\non_exit="disconnect"\nlog_dir=${q(out)}\n[multicore]\nscope="all"\n`+[0,1,2].map(i=>`[[cores]]\nname="cpu${i}"\nendpoint="local${i}"\n`).join(''));
let child,exit,seq=0,events=[],pending=new Map();const report=[];
function launch(){child=spawn(binary,['--project',file,'--headless','--stdio'],{windowsHide:true});exit=new Promise(r=>child.once('exit',r));readline.createInterface({input:child.stdout}).on('line',line=>{const e=JSON.parse(line);events.push(e);if(e.event==='response')pending.get(e.id)?.(e);});child.stderr.on('data',b=>fs.appendFileSync(path.join(out,'stderr.txt'),b));}
async function cmd(method,params={},ok=true){const id=++seq;const e=await new Promise((resolve,reject)=>{const timer=setTimeout(()=>reject(Error(`timeout ${method}`)),30000);pending.set(id,e=>{clearTimeout(timer);pending.delete(id);resolve(e);});child.stdin.write(JSON.stringify({id,method,params})+'\n');});assert.equal(e.ok,ok,`${method}: ${e.error}`);return e;}
const select=i=>cmd('select_core',{index:i});
const list=async()=> (await cmd('status')).result.breakpoints;
const linked=async()=>{const b=(await list()).filter(b=>b.group);assert.equal(b.length,1);return b[0];};
async function stop(){await cmd('quit');assert.equal(await exit,0);child=null;}
(async()=>{try{
 launch();await cmd('connect');await select(0);
 await cmd('break',{location:'tick',enabled:false,condition:'counter >= 0',ignore_count:2});let source=(await list())[0];assert(!source.group);
 await select(1);assert.equal((await list()).length,0);await cmd('break',{location:'tick',enabled:false});
 await select(2);await cmd('break',{location:'main',enabled:false});await select(0);
 await cmd('break_cores',{number:source.id,cores:[0,2]});let first=await linked();assert.deepEqual(first.cores,[0,2]);
 await select(2);let peer=await linked();assert.notEqual(first.id,peer.id);assert.equal(peer.condition,first.condition);assert.equal(peer.ignore_count,2);
 await cmd('update_break',{number:peer.id,condition:'counter >= 3',ignore_count:4,enabled:true});
 await select(0);first=await linked();assert.equal(first.condition,'counter >= 3');assert.equal(first.ignore_count,4);assert(first.enabled);
 await select(1);assert.equal((await list()).length,1);assert(!(await list())[0].enabled);assert(!(await list())[0].group);
 report.push('click/API creation stays local; arbitrary core subset, independent equal locations and per-core GDB IDs');
 await select(0);await cmd('enable_break',{number:first.id,enabled:false});await select(2);assert(!(await linked()).enabled);
 await cmd('reconnect');await select(0);const group=(await linked()).group;await stop();
 launch();await cmd('connect');await select(2);peer=await linked();assert.equal(peer.group,group);assert.deepEqual(peer.cores,[0,2]);
 await cmd('enable_break',{all:true,enabled:true});await select(0);assert((await linked()).enabled);
 await cmd('delete_break',{number:(await linked()).id});assert.equal((await list()).length,0);
 await select(2);assert.equal((await list()).length,1);assert.equal((await list())[0].location,'main');
 await select(1);assert.equal((await list()).length,1);assert.equal((await list())[0].location,'tick');
 report.push('linked edits/toggle/enable-all/delete and reconnect/process-restart persistence preserve unrelated breakpoints');
 for(const i of [1,2]){await select(i);await cmd('delete_break');}
 await select(0);await cmd('break',{location:'tick',enabled:false});source=(await list())[0];
 for(const cores of [[],[1],[0,9],[0,0]])await cmd('break_cores',{number:source.id,cores},false);
 await select(1);await cmd('console',{command:'symbol-file'});await select(0);
 const failed=await cmd('break_cores',{number:source.id,cores:[0,1]},false);assert.match(failed.error,/cpu1/);
 const restored=(await list())[0];assert.equal(restored.id,source.id);assert(!restored.group);assert.equal(restored.enabled,false);
 await select(1);assert.equal((await list()).length,0);await cmd('console',{command:`symbol-file ${q(elf)}`});
 await select(0);await cmd('break_cores',{number:source.id,cores:[0,1,2]});assert.deepEqual((await linked()).cores,[0,1,2]);
 await select(1);peer=await linked();await cmd('break_cores',{number:peer.id,cores:[1]});assert(!(await list())[0].group);
 await select(0);assert.equal((await list()).length,0);await select(2);assert.equal((await list()).length,0);
 report.push('invalid membership rejected; late insertion failure rolls back earlier core; reduce linked breakpoint to one selected core');
 await select(1);source=(await list())[0];await cmd('break_cores',{number:source.id,cores:[0,1,2]});
 await cmd('enable_break',{number:(await linked()).id,enabled:false});await cmd('continue');await cmd('pause',{scope:'core'});
 await cmd('break_cores',{number:(await linked()).id,cores:[1]},false);
 assert((await cmd('status')).result.cores.some(c=>c.state==='RUNNING'));
 await cmd('pause');await cmd('enable_break',{number:(await linked()).id,enabled:true});
 await cmd('continue');await cmd('wait_stopped',{timeout_ms:8000});assert((await cmd('status')).result.cores.every(c=>c.state==='STOPPED'));
 await cmd('delete_break',{number:(await linked()).id});for(const i of [0,1,2]){await select(i);assert.equal((await list()).length,0);}
 report.push('mixed running state refuses breakpoint edits without implicit pause; linked hit cooperates with group halt');
 await stop();fs.writeFileSync(path.join(out,'verification.json'),JSON.stringify({passed:true,requests:seq,report},null,2));console.log(JSON.stringify({out,requests:seq,report}));
 }finally{if(child){try{await stop();}catch{child.kill();}}fs.writeFileSync(path.join(out,'events.json'),JSON.stringify(events));}})().catch(e=>{console.error(e);process.exitCode=1;});
