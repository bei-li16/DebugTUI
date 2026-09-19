// Development-only real GDB + configurable TCL transport integration.
// --hardware uses the connected STM32F429; never downloads firmware.
const {spawn,execFileSync}=require('node:child_process');
const fs=require('node:fs'),path=require('node:path'),net=require('node:net'),readline=require('node:readline'),assert=require('node:assert/strict');
const root=path.dirname(__dirname),hardware=process.argv.includes('--hardware');
const binary=path.resolve(process.argv.find((a,i)=>i>1&&!a.startsWith('--'))||path.join(root,'target/release/debugtui.exe'));
const out=path.join(root,'artifacts',`memory-${hardware?'stm32':'native'}-${Date.now()}`);fs.mkdirSync(out,{recursive:true});
const quote=s=>JSON.stringify(s.replaceAll('\\','/'));
const pause=ms=>new Promise(r=>setTimeout(r,ms));
(async()=>{
  let server,child,exit,events,mode='ok',connections=0,nextId=1;
  const packets=[],mi=[],report=[],logs=[],pending=new Map(),sockets=new Set();
  try {
    let config;
    if(hardware){
      const firmware=process.env.DEBUGTUI_TEST_ELF;
      assert(firmware,'Set DEBUGTUI_TEST_ELF to the ELF matching the board firmware');
      config=`version=2\n[tools]\nprofile=${quote(path.join(root,'tools/debug-env-openocd.toml'))}\n[program]\nelf=${quote(firmware)}\n`;
      if(!process.argv.includes('--own-service'))config+='[service]\nenabled=false\n';
    }else{
      fs.writeFileSync(path.join(out,'sample.c'),`#include <stdint.h>
struct Pair { uint32_t value; int32_t negative; } pair={17,-42};
volatile uint32_t counter=17; volatile float temperature=1.5f; volatile double gain=-2.75;
int main(void) { for (;;) { counter++; } }
`);
      execFileSync(process.env.DEBUGTUI_TEST_CC||'C:/MinGW/bin/gcc.exe',['-g','-O0',path.join(out,'sample.c'),'-o',path.join(out,'sample.exe')]);
      server=net.createServer(socket=>{
        connections++;sockets.add(socket);socket.on('close',()=>sockets.delete(socket));let buffer='';
        socket.on('data',data=>{
          buffer+=data.toString();let i;
          while((i=buffer.indexOf('\x1a'))>=0){
            const command=buffer.slice(0,i);buffer=buffer.slice(i+1);packets.push(command);
            if(mode==='close'){socket.destroy();continue;}
            if(mode==='error'){socket.write('1:AP access failed\x1a');continue;}
            if(mode==='count'){socket.write('0:17 18 19\x1a');continue;}
            if(mode==='timeout'){continue;}
            const words=command.includes('32 2')?'287454020 1432778632':command.includes('soc.core')?'19':'17';
            socket.write(`0:${words}\x1a`);
          }
        });
      });
      await new Promise(r=>server.listen(0,'127.0.0.1',r));
      const endpoint=`127.0.0.1:${server.address().port}`;
      config=`version=2\n[gdb]\nexecutable=${quote(process.env.DEBUGTUI_TEST_GDB||'C:/MinGW/bin/gdb.exe')}\n[target]\nmode="local"\n[program]\nelf=${quote(path.join(out,'sample.exe'))}\n`;
      for(const [id,target,running] of [['ahb','soc.bus',true],['core-tcl','soc.core',false]]){
        config+=`\n[[memory_access]]\nid="${id}"\ntarget="${target}"\ntcl_endpoint="${endpoint}"\nwhile_running=${running}\n`;
      }
    }
    config+=`\n[session]\nlog_dir=${quote(out)}\non_exit="${hardware?'detach':'disconnect'}"\n`;
    fs.writeFileSync(path.join(out,'project.toml'),config);
    child=spawn(binary,['--project',path.join(out,'project.toml'),'--headless','--stdio'],{windowsHide:true});
    exit=new Promise(r=>child.once('exit',r));events=fs.createWriteStream(path.join(out,'events.jsonl'));
    readline.createInterface({input:child.stdout}).on('line',line=>{events.write(line+'\n');const e=JSON.parse(line);if(e.event==='log'){logs.push(e);if(e.channel==='mi>')mi.push(e.text);}if(e.event==='response')pending.get(e.id)?.(e);});
    child.stderr.on('data',d=>fs.appendFileSync(path.join(out,'stderr.log'),d));
    async function cmd(method,params={},ok=true){const id=nextId++;const e=await new Promise((resolve,reject)=>{const timer=setTimeout(()=>{pending.delete(id);reject(Error(`Timeout ${method}; see ${out}`));},25000);pending.set(id,e=>{clearTimeout(timer);pending.delete(id);resolve(e);});child.stdin.write(JSON.stringify({id,method,params})+'\n');});assert.equal(e.ok,ok,`${method}: ${e.error}`);return ok?e.result:e.error;}
    const read=(address,channel='ahb',bits=32,ok=true,little_endian=true)=>cmd('memory_read',{address,bits,little_endian,channel},ok);
    await cmd('connect');
    if(hardware){await cmd('console',{command:'compare-sections .text'});assert(logs.some(e=>e.text.includes('Section .text')&&e.text.includes('matched.')),'Board firmware and ELF .text must match');}
    if(!hardware){await cmd('break',{location:'main'});await cmd('run');await cmd('wait_stopped');}
    const variable=hardware?'xTickCount':'counter';
    await cmd('watch',{expression:variable});
    const binding=await cmd('watch_resolve',{expression:variable});assert.equal(binding.bits,32);assert.equal(binding.little_endian,true);
    assert.equal(binding.signed,false);assert.equal(binding.float,false);
    const stopped=await cmd('status');assert.equal(stopped.state,'STOPPED');assert(!stopped.core,'Single-core stays on direct Session path');
    const gdbValue=(await cmd('evaluate',{expression:variable})).value;
    assert.equal((await read(binding.address)).value,Number(gdbValue));
    if(hardware){
      assert.equal((await read(binding.address,'core-tcl')).value,Number(gdbValue));
      for(const address of [0x40023808,0x40020400,0xe0042000]){
        assert.equal((await read(address)).value,(await read(address,'')).value);
      }
      report.push('STM32F429 stopped AHB/GDB values agree for xTickCount, RCC.CFGR, GPIOB.MODER, DBGMCU.IDCODE');
    }else{
      assert.equal((await read(binding.address,'core-tcl')).value,19);
      for(const [expression,signed,float,bits] of [['pair.negative',true,false,32],['temperature',true,true,32],['gain',true,true,64]]){
        await cmd('watch',{expression});const b=await cmd('watch_resolve',{expression});assert.equal(b.signed,signed);assert.equal(b.float,float);assert.equal(b.bits,bits);
      }
      await cmd('watch',{expression:'pair'});await cmd('watch_resolve',{expression:'pair'},false);
      assert.equal((await cmd('watch_resolve',{expression:'pair',path:[1]})).signed,true);
      await cmd('watch',{expression:'counter+1'});await cmd('watch_resolve',{expression:'counter+1'},false);
      await cmd('watch',{expression:`*(uint32_t *)(${binding.address})`});assert.equal((await cmd('watch_resolve',{expression:`*(uint32_t *)(${binding.address})`})).address,binding.address);
      // Exact 64-bit values: JSON numbers from Rust are compared as text via BigInt only below 2^53.
      await read(binding.address+1,'ahb',32,false);await read(binding.address,'ahb',24,false);await read(binding.address,'missing',32,false);
      const address64=Math.ceil(binding.address/8)*8;
      const wide=await read(address64,'ahb',64);assert.equal(wide.value,Number(0x5566778811223344n));
      const big=await read(address64,'ahb',64,true,false);assert.equal(big.value,Number(0x1122334455667788n));
      for(const bad of ['error','count','close','timeout']){mode=bad;const start=Date.now();await read(binding.address,'ahb',32,false);assert(Date.now()-start<4000);mode='ok';assert.equal((await read(binding.address)).value,17);}
      assert(connections>=4);assert(packets.every(p=>!p.includes('targets ')&&!p.includes('halt')&&!p.includes('resume')));
      report.push('Native typed lvalue resolution, cast/child address, core/bus routing, 64-bit byte order, bounds, TCL failures/reconnect');
    }
    await cmd('delete_break',{number:''});await cmd('continue');await pause(100);
    const before=await cmd('status');assert.equal(before.state,'RUNNING');const miStart=mi.length;
    const samples=[];
    for(let i=0;i<20;i++){
      samples.push({at:Date.now(),value:(await read(binding.address)).value});
      if(hardware) await read(0x40020400);
      await pause(100);
    }
    await read(binding.address,'core-tcl',32,false);await read(binding.address,'',32,false);await cmd('watch_resolve',{expression:variable},false);
    assert.equal(mi.length,miStart,'Bus polling must not send GDB commands or implicitly halt/continue');
    const after=await cmd('status');assert.equal(after.state,'RUNNING');assert.equal(after.generation,before.generation);
    if(hardware)assert(new Set(samples.map(s=>s.value)).size>1,'Target tick must keep advancing during bus reads');
    fs.writeFileSync(path.join(out,'live-samples.json'),JSON.stringify(samples,null,2));
    report.push('20 running bus samples at 100 ms; unchanged stopped generation; zero GDB commands; stopped-only channels rejected');
    await cmd('pause');assert.equal((await cmd('status')).state,'STOPPED');
    await cmd('stepi');await cmd('wait_stopped');await cmd('disassemble',{address:'$pc'});
    const final=await cmd('status');assert.equal(final.state,'STOPPED');assert(final.registers.length>0);assert(final.assembly.length>0);
    if(hardware)assert.equal((await read(binding.address)).value,Number((await cmd('evaluate',{expression:variable})).value));
    report.push('Pause, instruction step, disassembly, system registers and Watch remain usable after live polling');
    await cmd('quit');assert.equal(await exit,0);
    if(hardware){assert(logs.some(e=>e.channel==='diagnostic'&&e.text.includes('__DEBUGTUI_RPC__')));assert(!logs.some(e=>['target','server-error'].includes(e.channel)&&e.text.includes('__DEBUGTUI_RPC__')),'Internal TCL replies must not flood the Console');report.push('OpenOCD TCL echoes from GDB and owned server stderr are tagged and routed to diagnostics, preserving target application output');}
    fs.writeFileSync(path.join(out,'verification.json'),JSON.stringify({passed:true,hardware,binary,commands:nextId-1,report},null,2));console.log(`PASS memory access: ${out}`);
  }finally{
    if(child&&child.exitCode===null){child.stdin.end(JSON.stringify({id:nextId++,method:'quit'})+'\n');const timer=setTimeout(()=>child.kill(),10000);await exit;clearTimeout(timer);}
    events?.end();for(const socket of sockets)socket.destroy();if(server)await new Promise(r=>server.close(r));
  }
})().catch(e=>{console.error(e);process.exitCode=1;});
