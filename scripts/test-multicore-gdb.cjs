// Development regression: independent native GDBs, no hardware or runtime Node dependency.
const {spawn, execFileSync}=require('node:child_process');
const fs=require('node:fs'),path=require('node:path'),readline=require('node:readline'),assert=require('node:assert/strict');
const net=require('node:net');
const root=path.dirname(__dirname),binary=path.resolve(process.argv[2]||path.join(root,'target/debug/debugtui.exe'));
const output=path.join(root,'artifacts',`multicore-${Date.now()}`);fs.mkdirSync(output,{recursive:true});
const q=s=>JSON.stringify(s.replaceAll('\\','/'));
fs.writeFileSync(path.join(output,'sample.c'),'volatile unsigned counter=17; volatile unsigned other=23;\nstruct Pair { unsigned value, enabled; } pair = {41,1};\nint main(void){for(;;){counter+=0;}return 0;}\n');
execFileSync(process.env.DEBUGTUI_TEST_CC||'gcc',['-g','-O0',path.join(output,'sample.c'),'-o',path.join(output,'sample.exe')]);
const base=`version=2\nwatch=[]\nbreakpoints=[]\n[gdb]\nexecutable=${q(process.env.DEBUGTUI_TEST_GDB||'C:/MinGW/bin/gdb.exe')}\n[target]\nmode="local"\n[program]\nelf=${q(path.join(output,'sample.exe'))}\nsource_root=${q(output)}\n[session]\non_exit="disconnect"\nlog_dir=${q(output)}\n`;
const cores='[[cores]]\nname="core.0"\nendpoint="local.0"\nstartup_order=1\n[[cores]]\nname="core.1"\nendpoint="local.1"\nstartup_order=0\n';
function session(name,config){
 const file=path.join(output,`${name}.toml`);if(config!==undefined)fs.writeFileSync(file,config);
 const child=spawn(binary,['--project',file,'--headless','--stdio'],{windowsHide:true});
 const events=[],pending=new Map();let id=0;
 const exit=new Promise(resolve=>child.once('exit',resolve));
 readline.createInterface({input:child.stdout}).on('line',line=>{const e=JSON.parse(line);events.push(e);if(e.event==='response')pending.get(e.id)?.(e);});
 child.stderr.on('data',b=>fs.appendFileSync(path.join(output,`${name}.stderr`),b));
 return {events,file,child,async cmd(method,params={},ok=true){
  const token=++id;const result=await new Promise((resolve,reject)=>{const timer=setTimeout(()=>{pending.delete(token);reject(Error(`Timeout ${name}:${method}`));},30000);pending.set(token,e=>{clearTimeout(timer);pending.delete(token);resolve(e);});child.stdin.write(JSON.stringify({id:token,method,params})+'\n');});
  assert.equal(result.ok,ok,`${name}:${method}: ${result.error}`);return result;
 },async close(consoleQuit=false){await this.cmd(consoleQuit?'console':'quit',consoleQuit?{command:'quit'}:{});assert.equal(await exit,0);fs.writeFileSync(path.join(output,`${name}.events.json`),JSON.stringify(events,null,2));}};
}
const active=[];const report=[];
(async()=>{
 try{
  const s=session('normal',base+'[tasks]\nbuild="echo multicore-build"\n'+cores);active.push(s);
  const connection=(await s.cmd('connect')).result;
  assert.deepEqual(connection.core_names,['core.0','core.1']);assert.equal(connection.active_core,0);
  assert.deepEqual(connection.results.map(r=>r.name),['core.1','core.0']);
  await s.cmd('connect',{},false);
  assert((await s.cmd('status')).result.cores.every(c=>c.state==='READY'),'duplicate Connect must preserve the existing workspace');
  const refusedElf=await s.cmd('set_elf',{path:'different.elf'},false);assert.match(refusedElf.error,/F2 Setup/);
  await s.cmd('watch',{expression:'counter'});await s.cmd('break',{location:'main'});
  let before=s.events.length;
  await s.cmd('select_core',{name:'core.1'});
  assert(s.events.slice(before).some(e=>e.event==='snapshot'&&e.snapshot.core.name==='core.1'));
  assert.deepEqual((await s.cmd('status')).result.watches,[]);
  await s.cmd('watch',{expression:'other'});await s.cmd('break',{location:'main'});
  await s.cmd('run');await s.cmd('wait_stopped');
  assert.equal((await s.cmd('status')).result.core.index,1);
  await s.cmd('select_core',{index:0});await s.cmd('wait_stopped');
  assert.deepEqual((await s.cmd('status')).result.watches.map(w=>w.name),['counter']);
  assert.equal((await s.cmd('status')).result.cores.length,2);
  await s.cmd('watch',{expression:'pair'});
  await s.cmd('watch_expand',{expression:'pair',path:[],expanded:true});
  await s.cmd('console',{command:'set variable pair.value = 91'});
  assert.equal((await s.cmd('status')).result.watches.find(v=>v.name==='pair').tree.children[0].value,'91');
  await s.cmd('select_core',{index:1});
  await s.cmd('watch',{expression:'pair'});
  assert.equal((await s.cmd('status')).result.watches.find(v=>v.name==='pair').tree.expanded,false);
  await s.cmd('watch_expand',{expression:'pair',expanded:true});
  assert.equal((await s.cmd('status')).result.watches.find(v=>v.name==='pair').tree.children[0].value,'41');
  await s.cmd('unwatch',{expression:'pair'});
  await s.cmd('select_core',{index:0});
  assert.equal((await s.cmd('status')).result.watches.find(v=>v.name==='pair').tree.children[0].value,'91');
  await s.cmd('unwatch',{expression:'pair'});
  report.push('same-name Watch trees keep independent values/expansion in two real GDB sessions');
  await s.cmd('select_core',{index:99},false);assert.equal((await s.cmd('status')).result.core.index,0);
  await s.cmd('select_core',{name:'unknown'},false);
  await s.cmd('next');await s.cmd('wait_stopped');
  await s.cmd('reconnect');
  assert((await s.cmd('status')).result.cores.every(c=>c.state==='READY'));
  const build=(await s.cmd('build')).result;
  assert.deepEqual(build.results.map(r=>r.name),['core.0','core.1','core.0','core.1','core.0']);
  assert((await s.cmd('status')).result.cores.every(c=>c.state==='READY'));
  await s.close();active.splice(active.indexOf(s),1);
  const reload=session('normal');active.push(reload);await reload.cmd('connect');await reload.cmd('run');await reload.cmd('wait_stopped');
  assert.deepEqual((await reload.cmd('status')).result.watches.map(w=>w.name),['counter']);
  await reload.cmd('select_core',{index:1});await reload.cmd('wait_stopped');assert.deepEqual((await reload.cmd('status')).result.watches.map(w=>w.name),['other']);
  await reload.cmd('unwatch',{expression:'other'});await reload.close();active.splice(active.indexOf(reload),1);
  report.push('ordered connect, independent snapshots/watches, named/index switch, run/step/reconnect/build, per-core persistence');
  const failing=session('connect-failure',base+cores.replace('startup_order=1','startup_order=1\ninit=["definitely_unknown_command"]'));active.push(failing);
  const failure=await failing.cmd('connect',{},false);assert.match(failure.error,/core\.0/);
  assert((await failing.cmd('status')).result.cores.every(c=>c.state==='DISCONNECTED'));
  await failing.close();active.splice(active.indexOf(failing),1);report.push('second-core connect failure rolls back the already-connected first core');
  const runfail=session('run-failure',base+cores+'run=["definitely_unknown_command"]\n');active.push(runfail);
  await runfail.cmd('connect');const r=await runfail.cmd('run',{},false);assert.match(r.error,/core\.1/);
  await runfail.close();active.splice(active.indexOf(runfail),1);report.push('background-core Run failure is returned to the caller');
  // Explicit one-core workspace must still start its configured shared service.
  const pidFile=path.join(output,'shared-service.pid');
  const serviceCode=`require('node:fs').writeFileSync(process.argv[1],String(process.pid));process.stdout.write('shared ');setTimeout(()=>process.stdout.write('service ready'),100);setInterval(()=>{},1000);`;
  const single=session('explicit-one',base+`[service]\ncommand=${q(process.execPath)}\nargs=${JSON.stringify(['-e',serviceCode,pidFile.replaceAll('\\','/')])}\nready=["shared service ready"]\ntimeout_ms=3000\n[[cores]]\nname="only"\nendpoint="local"\n`);active.push(single);
  await single.cmd('connect');assert(single.events.some(e=>e.event==='log'&&e.channel==='server'&&e.text.includes('service ready')));
  await single.close();active.splice(active.indexOf(single),1);assert.throws(()=>process.kill(Number(fs.readFileSync(pidFile,'utf8')),0));report.push('explicit single-core service, fragmented readiness marker and owned-process cleanup');
  const badservice=session('service-failure',base+'[service]\ncommand="cmd.exe"\nargs=["/d","/c","exit /b 7"]\nready=["never-ready"]\n'+cores);active.push(badservice);
  const failedservice=await badservice.cmd('connect',{},false);assert.match(failedservice.error,/Service exited/);
  assert((await badservice.cmd('status')).result.cores.every(c=>c.state==='DISCONNECTED'));await badservice.close();active.splice(active.indexOf(badservice),1);report.push('service startup failure returns an error and cleans up');
  const cancel=session('cancel-build',base+'[tasks]\nbuild="ping -n 30 127.0.0.1 >nul"\n'+cores);active.push(cancel);
  const buildrequest=cancel.cmd('build',{},false).catch(e=>e);await new Promise(r=>setTimeout(r,500));const start=Date.now();
  await cancel.close(true);const buildresult=await buildrequest;if(buildresult instanceof Error)throw buildresult;assert(Date.now()-start<5000);active.splice(active.indexOf(cancel),1);report.push('console quit propagates cancellation to a running multi-core build');
  for(const multi of [false,true]){
    const name=multi?'ready-multi':'ready-single';const ready=session(name,base+(multi?cores:''));active.push(ready);await ready.cmd('connect');
    await ready.cmd('break',{location:'main'});await ready.close();active.splice(active.indexOf(ready),1);
    const again=session(name);active.push(again);await again.cmd('connect');assert.equal((await again.cmd('status')).result.breakpoints.length,1,'READY breakpoint was lost');
    await again.cmd('console',{command:'delete breakpoints'});await again.close();active.splice(active.indexOf(again),1);
    const deleted=session(name);active.push(deleted);await deleted.cmd('connect');assert.equal((await deleted.cmd('status')).result.breakpoints.length,0,'deleted READY breakpoint returned');await deleted.close();active.splice(active.indexOf(deleted),1);
  }
  report.push('single/multi-core breakpoint add, restore and Console delete persist before first Run');
  let firstPacket;const received=new Promise(r=>firstPacket=r);let packets=0;const texts=[],sockets=new Set();
  const rpc=net.createServer(socket=>{sockets.add(socket);socket.on('close',()=>sockets.delete(socket));let buffer='';socket.on('data',data=>{buffer+=data.toString();while(buffer.includes('\x1a')){const command=buffer.slice(0,buffer.indexOf('\x1a'));texts.push(command);buffer=buffer.slice(buffer.indexOf('\x1a')+1);packets++;firstPacket();setTimeout(()=>{if(!socket.destroyed)socket.write(command.includes('read_memory')?`0:${command.includes('soc.ap3')?31:13}\x1a`:'0:ok\x1a');},300);}});});
  await new Promise(resolve=>rpc.listen(0,'127.0.0.1',resolve));
  try {
    const sync=session('sync-cancel',base+`[sync]\nmethod="tcl"\ntcl_endpoint="127.0.0.1:${rpc.address().port}"\nopen=["first","must_not_execute"]\n`+cores);active.push(sync);
    const connect=sync.cmd('connect',{},false).catch(e=>e);await Promise.race([received,new Promise((_,reject)=>{const timer=setTimeout(()=>reject(Error('No synchronization command received')),5000);timer.unref();})]);
    await sync.close();const outcome=await connect;if(outcome instanceof Error)throw outcome;assert.match(outcome.error,/cancelled/);assert.equal(packets,1);active.splice(active.indexOf(sync),1);
    const endpoint=`127.0.0.1:${rpc.address().port}`;
    const syncCommands=['cti0 enable on','cti1 enable on','cti0 channel 0 ungate','cti1 channel 0 ungate'];
    let configured=base+`[sync]\nmethod="cti"\ntcl_endpoint="${endpoint}"\nopen=${JSON.stringify(syncCommands)}\n`+cores;
    for(const [id,target,allowed,running] of [['borrow','soc.ap1',['core.0'],false],['ahb','soc.ap3',[],true]]){
      configured+=`\n[[memory_access]]\nid="${id}"\ntarget="${target}"\ntcl_endpoint="${endpoint}"\ncores=${JSON.stringify(allowed)}\nwhile_running=${running}\n`;
    }
    const routed=session('cti-ap-routes',configured);active.push(routed);const first=texts.length;
    await routed.cmd('connect');assert.deepEqual(texts.slice(first).map(t=>syncCommands.find(c=>t.includes(c))),syncCommands);
    for(const index of [0,1]){await routed.cmd('select_core',{index});await routed.cmd('break',{location:'main'});}
    await routed.cmd('run');await routed.cmd('wait_stopped');
    const read=(channel,ok=true)=>routed.cmd('memory_read',{channel,address:0x20000000,bits:32,little_endian:true},ok);
    await routed.cmd('select_core',{index:0});await routed.cmd('wait_stopped');assert.equal((await read('borrow')).result.value,13);assert.equal((await read('ahb')).result.value,31);
    await routed.cmd('select_core',{index:1});await routed.cmd('wait_stopped');await read('borrow',false);assert.equal((await read('ahb')).result.value,31);
    await routed.cmd('delete_break',{number:''});await routed.cmd('continue');assert.equal((await read('ahb')).result.value,31);
    assert.equal((await routed.cmd('status')).result.cores[0].state,'STOPPED');await routed.cmd('pause');
    await routed.close();active.splice(active.indexOf(routed),1);report.push('two real GDB cores + simulated multiple CTIs/APs: ordered CTI setup, core-restricted access, running AHB, independent stop states');
  } finally {for(const socket of sockets)socket.destroy();await new Promise(resolve=>rpc.close(resolve));}
  report.push('quit during synchronization skips remaining TCL commands and cleans up');
  const legacy=session('legacy',base);active.push(legacy);await legacy.cmd('connect');
  assert.equal((await legacy.cmd('status')).result.core,undefined);await legacy.cmd('break',{location:'main'});await legacy.cmd('run');await legacy.cmd('wait_stopped');await legacy.cmd('step');await legacy.cmd('wait_stopped');await legacy.close();active.splice(active.indexOf(legacy),1);report.push('legacy single-core connect/break/run/step/quit and unchanged snapshot schema');
  fs.writeFileSync(path.join(output,'verification.json'),JSON.stringify({passed:report},null,2));console.log(JSON.stringify({output,passed:report},null,2));
 }finally{for(const s of active){try{await s.close();}catch{s.child.kill();}}}
})().catch(e=>{console.error(e);process.exitCode=1;});
