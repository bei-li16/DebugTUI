// Development-only integration check. No Node dependency in the native TUI.
const {spawn, execFileSync} = require('node:child_process');
const fs = require('node:fs');
const path = require('node:path');
const readline = require('node:readline');
const assert = require('node:assert/strict');
const root = path.dirname(__dirname);
const binary = path.resolve(process.argv[2] || path.join(root,'target/release/debugtui.exe'));
const elf = process.argv[3] && path.resolve(process.argv[3]);
const output = path.join(root,'artifacts',`completion-${elf?'arm-elf':'native'}-${Date.now()}`);
fs.mkdirSync(output,{recursive:true});
const quote = p => JSON.stringify(path.resolve(p).replaceAll('\\','/'));
if (!elf) {
  fs.writeFileSync(path.join(output,'sample.c'), `
volatile unsigned int counter=17, counter_total=23;
static volatile unsigned int counter_static=31;
struct Pair { unsigned int value; unsigned int enabled; } counter_pair = {41,1};
void counter_function(void) { counter++; }
${Array.from({length:90},(_,i)=>`int sym_${String(i).padStart(3,'0')}=${i};`).join('\n')}
int main(void) { for (;;) { counter += 0; } }
`);
  execFileSync(process.env.DEBUGTUI_TEST_CC || 'gcc',['-g','-O0',path.join(output,'sample.c'),'-o',path.join(output,'sample.exe')]);
}
const gdb = process.env.DEBUGTUI_TEST_GDB || (elf ? path.join(root,'tools/bin/gdb/bin/arm-none-eabi-gdb.exe') : 'C:/MinGW/bin/gdb.exe');
const config = `version=2\n[gdb]\nexecutable=${quote(gdb)}\n[target]\nmode="local"\n[program]\nelf=${quote(elf || path.join(output,'sample.exe'))}\n[session]\nlog_dir=${quote(output)}\non_exit="disconnect"\n`;
fs.writeFileSync(path.join(output,'project.toml'),config);
const child = spawn(binary,['--project',path.join(output,'project.toml'),'--headless','--stdio'],{windowsHide:true});
const events = fs.createWriteStream(path.join(output,'events.jsonl'));
const pending = new Map();
const miCommands=[];
const exit = new Promise(resolve=>child.once('exit',resolve));
let nextId=1;
readline.createInterface({input:child.stdout}).on('line',line=>{
  events.write(line+'\n');
  const event=JSON.parse(line);
  if(event.event==='log' && event.channel==='mi>') miCommands.push(event.text);
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
  assert.equal(event.ok,ok,`${method}: ${event.error}`);
  return event.result;
}
const results=[];
async function complete(text,expression=false) {
  const result=await command('complete',{text,expression});
  results.push({text,expression,...result});
  return result.matches;
}
(async()=>{
  try {
    await command('connect');
    assert((await complete('pri')).includes('print'));
    assert((await complete('info reg')).includes('info registers'));
    assert.deepEqual(await complete(''),[]);
    assert.deepEqual(await complete('print counter\nrun'),[]);
    if(elf) {
      assert((await complete('uxCurrent',true)).includes('uxCurrentNumberOfTasks'));
      assert((await complete('xTickC',true)).includes('xTickCount'));
      assert((await complete('p/x uxCurrent')).includes('p/x uxCurrentNumberOfTasks'));
      assert.deepEqual(await complete('prvIdleTask',true),[]); // Function excluded from bare Watch names.
      assert.equal((await command('status')).state,'READY'); // No hardware connection/run.
    } else {
      const names=await complete('counter',true);
      for(const name of ['counter','counter_total','counter_static','counter_pair']) assert(names.includes(name),name);
      assert(!names.includes('counter_function'));
      assert((await complete('p/x coun')).includes('p/x counter'));
      assert((await complete('counter_pair.v',true)).includes('counter_pair.value'));
      assert.equal((await complete('sym_',true)).length,64);
      assert.deepEqual(await complete('no_such_symbol_9287',true),[]);
      await command('break',{location:'main'});
      await command('run'); await command('wait_stopped');
      await command('watch',{expression:'counter'});
      const before=await command('status');
      const uiMiStart=miCommands.length;
      for(const animations of ['off','subtle','full']) {
        const prefs={animations,unicode:animations!=='off',formats:{'watch:counter':'binary','register:eax':'decimal','local:sample.c:main:counter':'hex'}};
        assert.equal((await command('ui_preferences',prefs)).saved,true);
      }
      assert.equal(miCommands.length,uiMiStart,'Display preference changes must not send any GDB commands');
      const saved=fs.readFileSync(path.join(output,'project.toml'),'utf8');
      assert(saved.includes('animations = "full"') && saved.includes('"watch:counter" = "binary"'));
      assert.equal((await command('evaluate',{expression:'counter'})).value,'17');
      const uiAfter=await command('status');
      assert.equal(uiAfter.frame.address,before.frame.address);
      assert.equal(uiAfter.generation,before.generation);
      assert.equal(uiAfter.state,'STOPPED');
      results.push({check:'ui_preferences',passed:true,extraMiCommands:0,unchangedPc:true,unchangedValue:true});
      assert(before.watches.some(w=>w.name==='counter' && w.value==='17'));
      await complete('set variable counter = 77');
      await complete('counter_pair.e',true);
      assert.equal((await command('evaluate',{expression:'counter'})).value,'17');
      const after=await command('status');
      assert.equal(after.generation,before.generation);
      assert.equal(after.frame.address,before.frame.address);
      assert.equal(after.state,'STOPPED');
      await command('watch',{expression:'counter_total'});
      await command('watch',{expression:'counter_pair.value'});
      const removalMiStart=miCommands.length;
      await command('unwatch',{expression:'counter_pair.value'});
      assert.deepEqual((await command('status')).watches.map(w=>w.name),['counter','counter_total']);
      await command('unwatch',{expression:'counter_total'});
      assert.deepEqual((await command('status')).watches.map(w=>w.name),['counter']);
      assert.equal(miCommands.length,removalMiStart,'Removing watches must not access or modify the target');
      assert.equal((await command('evaluate',{expression:'counter_total'})).value,'23');
      assert.equal((await command('evaluate',{expression:'counter_pair.value'})).value,'41');
      await command('delete_break',{number:''});
      await command('continue');
      await command('complete',{text:'cou',expression:true},false);
      assert.equal((await command('status')).state,'RUNNING');
      const runningRemovalStart=miCommands.length;
      await command('unwatch',{expression:'counter'});
      const removedRunning=await command('status');
      assert.equal(removedRunning.state,'RUNNING');assert.deepEqual(removedRunning.watches,[]);
      assert.equal(miCommands.length,runningRemovalStart,'Running removal must not interrupt the target');
      await command('pause');
      assert((await complete('counter',true)).includes('counter'));
      assert.deepEqual((await command('status')).watches,[]);
      assert.equal((await command('evaluate',{expression:'counter'})).value,'17');
      await command('reconnect');
      assert(/^watch = \[\]/m.test(fs.readFileSync(path.join(output,'project.toml'),'utf8')),'Deleted watches must not be persisted');
      await command('break',{location:'main'});await command('run');await command('wait_stopped');
      assert.deepEqual((await command('status')).watches,[],'Reconnect and stop must not restore deleted watches');
      results.push({check:'watch_removal',passed:true,removedWhileStopped:2,removedWhileRunning:1,extraMiCommands:0,unchangedTargetValues:true,noReappearanceAfterReconnect:true});
    }
    await command('quit');
    assert.equal(await exit,0);
    fs.writeFileSync(path.join(output,'verification.json'),JSON.stringify({passed:true,binary,elf:elf||null,commands:nextId-1,results},null,2));
    console.log(`PASS ${elf?'FreeRTOS ELF (offline)':'native GDB'} completion: ${output}`);
  } finally {
    if(child.exitCode===null) {
      child.stdin.end(JSON.stringify({id:nextId++,method:'quit'})+'\n');
      const timer=setTimeout(()=>child.kill(),10000);
      await exit; clearTimeout(timer);
    }
    events.end();
  }
})().catch(error=>{console.error(error);process.exitCode=1;});
