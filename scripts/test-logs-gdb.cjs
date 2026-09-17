// Development-only native GDB check: session rotation, preserved history, per-line timestamps.
const {spawn, execFileSync}=require('node:child_process');
const fs=require('node:fs'), path=require('node:path'), readline=require('node:readline'), assert=require('node:assert/strict');
const root=path.dirname(__dirname);
const binary=path.resolve(process.argv[2] || path.join(root,'target/release/debugtui.exe'));
const output=path.join(root,'artifacts',`logs-native-${Date.now()}`);
fs.mkdirSync(output,{recursive:true});
const quote=p=>JSON.stringify(path.resolve(p).replaceAll('\\','/'));
fs.writeFileSync(path.join(output,'sample.c'),'volatile int counter=17;\nint main(void) { counter++; return counter; }\n');
execFileSync(process.env.DEBUGTUI_TEST_CC || 'gcc',['-g','-O0',path.join(output,'sample.c'),'-o',path.join(output,'sample.exe')]);
fs.writeFileSync(path.join(output,'project.toml'),`version=2\n[gdb]\nexecutable=${quote(process.env.DEBUGTUI_TEST_GDB || 'C:/MinGW/bin/gdb.exe')}\n[target]\nmode="local"\n[program]\nelf=${quote(path.join(output,'sample.exe'))}\n[session]\nlog_dir=${quote(output)}\non_exit="disconnect"\n`);
const child=spawn(binary,['--project',path.join(output,'project.toml'),'--headless','--stdio'],{windowsHide:true});
const pending=new Map(), events=[];let nextId=1;
const exited=new Promise(resolve=>child.once('exit',resolve));
readline.createInterface({input:child.stdout}).on('line',line=>{
  const event=JSON.parse(line); events.push(event);
  if(event.event==='response') pending.get(event.id)?.(event);
});
child.stderr.on('data',data=>fs.appendFileSync(path.join(output,'stderr.log'),data));
async function command(method,params={},ok=true) {
  const id=nextId++;
  const event=await new Promise((resolve,reject)=>{
    const timer=setTimeout(()=>{pending.delete(id);reject(Error(`Timeout: ${method}`));},25000);
    pending.set(id,event=>{clearTimeout(timer);pending.delete(id);resolve(event);});
    child.stdin.write(JSON.stringify({id,method,params})+'\n');
  });
  assert.equal(event.ok,ok,`${method}: ${event.error}`);return event.result;
}
const files=()=>fs.readdirSync(output).filter(n=>/^session-.*\.log$/.test(n)).sort();
const read=f=>fs.readFileSync(path.join(output,f),'utf8');
const stamp=/^\[\d{4}-\d{2}-\d{2} \d{2}:\d{2}:\d{2}\.\d{3}Z?\] \[\+\d+\.\d{3}s\] \[[^\]]+\] /;
(async()=>{
  try {
    await command('connect');
    assert.equal(files().length,1);
    await command('console',{command:'printf "SESSION_ONE_A\\nSESSION_ONE_B\\n"'});
    await command('console',{command:'no_such_debug_command_9287'},false);
    await command('break',{location:'main'});await command('run');await command('wait_stopped');
    await command('next');await command('wait_stopped');
    await command('reconnect');
    assert.equal(files().length,2);
    const first=files()[0], preserved=read(first);
    assert(preserved.includes('[gdb] SESSION_ONE_A') && preserved.includes('[gdb] SESSION_ONE_B'));
    assert(preserved.includes('[error]') && preserved.includes('[mi>]'));
    await command('console',{command:'printf "SESSION_TWO\\n"'});
    assert.equal(read(first),preserved);
    await command('disconnect');await command('connect');
    assert.equal(files().length,3);
    const second=files()[1], preservedSecond=read(second);
    await command('console',{command:'printf "SESSION_THREE\\n"'});
    await command('quit');assert.equal(await exited,0);
    assert.equal(read(first),preserved);assert.equal(read(second),preservedSecond);
    const counts=files().map(file=>{
      assert(/^session-\d{8}-\d{6}-\d{3}(-\d+)?\.log$/.test(file),file);
      const lines=read(file).trimEnd().split('\n');assert(lines.length>5);
      for(const line of lines) assert(stamp.test(line),line);
      return {file,lines:lines.length};
    });
    const logs=events.filter(e=>e.event==='log');
    assert(logs.length>30);
    assert(logs.every(e=>/^\d{4}-\d{2}-\d{2} \d{2}:\d{2}:\d{2}\.\d{3}Z?$/.test(e.timestamp) && Number.isSafeInteger(e.elapsed_ms) && e.elapsed_ms>=0));
    assert(logs.some(e=>e.channel==='gdb' && e.text.includes('SESSION_ONE_A') && !e.text.startsWith('[')));
    fs.writeFileSync(path.join(output,'events.jsonl'),events.map(e=>JSON.stringify(e)).join('\n')+'\n');
    const report={passed:true,binary,commands:nextId-1,files:counts,previousLogsUnchanged:true,allPhysicalLinesTimestamped:true,logEventCount:logs.length,rawTextPreserved:true};
    fs.writeFileSync(path.join(output,'verification.json'),JSON.stringify(report,null,2));
    console.log(JSON.stringify({...report,output},null,2));
  } finally {
    if(child.exitCode===null) {
      child.stdin.end(JSON.stringify({id:nextId++,method:'quit'})+'\n');
      const timer=setTimeout(()=>child.kill(),10000);await exited;clearTimeout(timer);
    }
  }
})().catch(error=>{console.error(error);process.exitCode=1;});
