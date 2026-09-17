// Development-only GDB integration checks. The installed TUI needs no Node runtime.
const { spawn, execFileSync } = require('node:child_process');
const fs = require('node:fs');
const path = require('node:path');
const readline = require('node:readline');
const assert = require('node:assert/strict');
const root = path.dirname(__dirname);
const hardware = process.argv[2] === '--hardware';
const binary = path.resolve(process.argv[3] || path.join(root, 'target/release/debugtui.exe'));
const output = path.join(root, 'artifacts', `svd-${hardware ? 'hardware' : 'native'}-${Date.now()}`);
fs.mkdirSync(output, { recursive: true });
const portable = p => JSON.stringify(path.resolve(p).replaceAll('\\', '/'));
let project;
if (hardware) {
  project = `version=2\n[tools]\nprofile=${portable(path.join(root,'tools/debug-env.toml'))}\n`;
} else {
  fs.writeFileSync(path.join(output, 'sample.c'), 'volatile unsigned int counter = 0x12345678;\nint main(void) { for (;;) { counter += 0; } }\n');
  execFileSync(process.env.DEBUGTUI_TEST_CC || 'gcc', ['-g','-O0',path.join(output,'sample.c'),'-o',path.join(output,'sample.exe')]);
  project = `version=2\n[gdb]\nexecutable=${portable(process.env.DEBUGTUI_TEST_GDB || 'C:/MinGW/bin/gdb.exe')}\n[target]\nmode="local"\n[program]\nelf=${portable(path.join(output,'sample.exe'))}\n`;
}
project += `\n[session]\nlog_dir=${portable(output)}\non_exit="${hardware ? 'detach' : 'disconnect'}"\n`;
fs.writeFileSync(path.join(output,'project.toml'),project);
const child = spawn(binary,['--project',path.join(output,'project.toml'),'--headless','--stdio'],{windowsHide:true});
const events = fs.createWriteStream(path.join(output,'events.jsonl'));
const pending = new Map();
const exit = new Promise(resolve => child.once('exit',resolve));
let nextId=1;
readline.createInterface({input:child.stdout}).on('line', line => {
  events.write(line+'\n');
  const event=JSON.parse(line);
  if(event.event==='response') pending.get(event.id)?.(event);
});
child.stderr.on('data',data => fs.appendFileSync(path.join(output,'stderr.log'),data));
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
async function read(address,bits=32,ok=true){return command('peripheral_read',{address,bits,little_endian:true},ok);}
(async()=>{
  try {
    await command('connect');
    if(hardware) {
      const results=[];
      for(const [name,address] of [['RCC.CR',0x40023800],['GPIOB.MODER',0x40020400],['DBGMCU.IDCODE',0xe0042000]]) {
        const value=(await read(address)).value;
        const direct=await command('evaluate',{expression:`(unsigned int)*(volatile unsigned int*)0x${address.toString(16)}`});
        assert.equal(value,Number(direct.value),name);
        results.push({name,address:'0x'+address.toString(16),value:'0x'+value.toString(16)});
      }
      fs.writeFileSync(path.join(output,'registers.json'),JSON.stringify(results,null,2));
      console.log(results);
    } else {
      await command('break',{location:'main'});
      await command('run'); await command('wait_stopped');
      const pointer=(await command('evaluate',{expression:'&counter'})).value.match(/0x[0-9a-f]+/i)[0];
      const address=Number(pointer);
      await command('memory',{address:pointer,count:4});
      const memoryBefore=(await command('status')).memory;
      assert.equal((await read(address)).value,0x12345678);
      assert.equal((await read(address,16)).value,0x5678);
      assert.equal((await read(address,8)).value,0x78);
      assert.deepEqual((await command('status')).memory,memoryBefore);
      await command('console',{command:'set variable counter = 0xaabbccdd'});
      assert.equal((await read(address)).value,0xaabbccdd);
      await read(address+1,32,false);
      await read(address,24,false);
      await read(0,32,false);
      assert.equal((await read(address)).value,0xaabbccdd);
      await command('delete_break',{number:''});
      await command('continue');
      await read(address,32,false);
      await command('pause');
      assert.equal((await read(address)).value,0xaabbccdd);
    }
    await command('quit');
    assert.equal(await exit,0);
    fs.writeFileSync(path.join(output,'verification.json'),JSON.stringify({passed:true,hardware,commands:nextId-1,binary},null,2));
    console.log(`PASS SVD ${hardware?'STM32 hardware':'native GDB'} reads. ${output}`);
  } finally {
    if(child.exitCode===null) {
      child.stdin.end(JSON.stringify({id:nextId++,method:'quit'})+'\n');
      const timer=setTimeout(()=>child.kill(),10000);
      await exit; clearTimeout(timer);
    }
    events.end();
  }
})().catch(error=>{console.error(error);process.exitCode=1;});
