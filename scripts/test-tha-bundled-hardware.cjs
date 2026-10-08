#!/usr/bin/env node
'use strict';
// Explicit THA6206 / MCAL regression using the installed bundled toolchain.
// Test preferences are isolated; --build-download additionally rebuilds/flashes.
const fs=require('node:fs'), path=require('node:path'), assert=require('node:assert/strict');
const {spawnSync}=require('node:child_process');
const {root,quote,hash,delay,outputDirectory,parseOptions,Cases,Session}=require('./test-support/session.cjs');
const options=parseOptions(process.argv.slice(2),['--binary','--project-root'],['--run','--build-download']);
assert(options.run&&options.binary&&options['project-root'],'Requires --run --binary EXE --project-root MCAL_ROOT [--build-download]');
const binary=path.resolve(options.binary), projectRoot=path.resolve(options['project-root']);
const example=path.join(projectRoot,'debug-builtin.toml');
const firmware=path.join(projectRoot,'DemoWorkspace/CodeProject/Debug/THA6206_Demo_Prj.elf');
const hex=firmware.replace(/\.elf$/,'.hex');
const source=fs.readFileSync(example,'utf8');
assert(source.includes('profile = "builtin:arm-openocd"'),'Use the installed built-in profile');
const protectedFiles=['debug.toml','debug-chip.toml','debug-core0.toml','debug-core1.toml','debug-multi.toml','debug-builtin.toml']
 .map(file=>path.join(projectRoot,file)).filter(file=>fs.existsSync(file));
const protectedHashes=Object.fromEntries(protectedFiles.map(file=>[file,hash(file)]));
const out=outputDirectory('tha-bundled-hardware');
const suite=new Cases(out,{binary,binary_sha256:hash(binary),projectRoot,board_tests_executed:true,
 build_download:!!options['build-download'],layer:'THA6206 physical board; installed GDB/OpenOCD/SVD; MCAL reset authorized',
 limitations:['R52+ injected CP15/banked/VFP/timer/PMU/GIC readers are not implemented by the bundled R52-only adapter; refusal is tested, availability is not claimed.']});
const configurations=[];
const status=s=>s.command('status');
function netstat() {
 const r=spawnSync('netstat.exe',['-ano','-p','TCP'],{encoding:'utf8',windowsHide:true,timeout:10000});
 assert.ifError(r.error);assert.equal(r.status,0,r.stderr);return r.stdout;
}
function project(name,ids) {
 const directory=path.join(out,name);fs.mkdirSync(directory);
 const file=path.join(directory,'debug.toml');
 const text=source.replace('cores = [0, 1]',`cores = ${JSON.stringify(ids)}`)
  .replace('source_root = "."',`source_root = ${quote(projectRoot)}`)
  .replaceAll('elf = "DemoWorkspace/',`elf = "${projectRoot.replaceAll('\\','/')}/DemoWorkspace/`)
  .replace('on_exit = "detach"','on_exit = "resume"');
 fs.writeFileSync(file,text);configurations.push(file);return {directory,file};
}
async function compare(s) {
 const start=s.logs('gdb').length;
 await s.command('console',{command:'compare-sections -r'},true,60000);
 const logs=s.logs('gdb').slice(start).map(e=>e.text).join('');
 assert(!/MIS-MATCH|does not match/i.test(logs),logs);
 const matched=logs.split(/\r?\n/).filter(line=>/Section .*matched\./.test(line));
 assert(matched.length>0,'No readonly section comparison evidence');return matched;
}
async function select(s,id) { await s.command('select_core',{name:`core.${id}`}); }
async function coreRegisters(s) {
 const listing=await s.command('registers_list');
 const result=await s.command('registers_read',{context:listing.context,ids:[...Array.from({length:13},(_,i)=>`r${i}`),'sp','lr','pc','cpsr'],manual:true});
 assert(result.samples.every(sample=>sample.state==='valid'),JSON.stringify(result.samples));
 return result.samples;
}
async function pause(s) { await s.command('control_scope',{scope:'all'});await s.command('pause');assert((await status(s)).cores.every(c=>c.state==='STOPPED')); }
async function runApplication(s,ids) {
 await s.command('run');
 if(ids.includes(0)) {
  await select(s,0);await s.command('wait_stopped',{timeout_ms:12000});
  const main=await status(s);assert.equal(main.frame.function,'_main',JSON.stringify(main.frame));
  await s.command('continue');
 }
 await delay(1200);assert((await status(s)).cores.every(c=>c.state==='RUNNING'));
 await pause(s);
 const ready=await s.command('evaluate',{expression:'(unsigned int)Init_Process_Is_Ready'});
 assert(['1','true'].includes(ready.value),`MCAL core0 initialization precondition not met: ${JSON.stringify(ready)}`);
 return {initialization_ready:ready};
}
async function firmwareWorkflow() {
 const cfg=project('build-download',[0,1]),s=new Session(binary,cfg.file,cfg.directory);
 const saved=path.join(out,'previous-firmware');fs.mkdirSync(saved);
 for(const file of [firmware,hex])fs.copyFileSync(file,path.join(saved,path.basename(file)));
 try {
  if(!await suite.test('THA-BUILD','GHS rebuild through DebugTUI produces matching ELF/HEX',async()=>{
   const before=fs.statSync(firmware).mtimeMs;
   const result=await s.command('build',{},true,240000);
   assert(result.built===true||result.results?.some(r=>r.ok&&r.result?.built===true));assert(fs.statSync(firmware).mtimeMs>before);
   assert(fs.statSync(hex).size>0);return {result,elf_sha256:hash(firmware),hex_sha256:hash(hex)};
  }))return false;
  return await suite.test('THA-DOWNLOAD','Vendor flash through DebugTUI, reconnect and readonly section verification',async()=>{
   // Connect first so Download exercises releasing both GDBs and OpenOCD,
   // automatic reconnect and loading the new ELF after the vendor flasher exits.
   await s.command('connect',{},true,65000);
   const result=await s.command('download',{},true,150000);
   assert(result.downloaded===true||result.results?.some(r=>r.ok&&r.result?.downloaded===true));
   const st=await status(s);assert(st.cores.every(c=>c.state==='STOPPED'));
   const matched=await compare(s);return {result,matched};
  });
 } finally {await s.close();}
}
async function mode(ids) {
 const label=ids.join('+'),cfg=project(`cores-${ids.join('-')}`,ids),s=new Session(binary,cfg.file,cfg.directory);
 let connected=false;
 try {
  connected=await suite.test(`THA-${label}-CONNECT`,'Connect selected physical cores with fixed ports using installed tools',async()=>{
   const result=await s.command('connect',{},true,65000);
   assert.deepEqual(result.core_names,ids.map(id=>`core.${id}`));
   const st=await status(s);assert(st.cores.every(c=>c.state==='STOPPED'));
   const listeners=netstat();
   for(const id of [0,1])assert.equal(listeners.split(/\r?\n/).some(line=>new RegExp(`^\\s*TCP\\s+127\\.0\\.0\\.1:${3333+id}\\s+.*LISTENING`).test(line)),ids.includes(id));
   return {cores:st.cores,frames:st.frame};
  });
  if(!connected)return false;
  if(!await suite.test(`THA-${label}-ELF`,'Program readonly sections match the local GHS ELF',()=>compare(s)))return false;
  if(!await suite.test(`THA-${label}-RUN`,'MCAL startup reaches _main where applicable and initializes runtime',()=>runApplication(s,ids)))return false;
  await suite.test(`THA-${label}-SOURCE-WATCH`,'Per-core C source, stack, GDB registers, scalar and aggregate Watch',async()=>{
   const evidence=[];
   for(const id of ids) {
    await select(s,id);await s.command('watch',{expression:'uart_cnt'});
    const registers=await coreRegisters(s);
    const st=await status(s);assert.equal(st.state,'STOPPED');
    assert(st.registers.some(r=>r.name==='pc'&&!r.error));assert(st.stack.length>0);
    assert(fs.existsSync(st.frame.file),JSON.stringify(st.frame));
    assert(st.watches.some(w=>w.name==='uart_cnt'&&!w.error));
    await s.command('disassemble');assert((await status(s)).assembly.length>0);
    await s.command('watch',{expression:'Spi_ConfigSet'});
    await s.command('watch_expand',{expression:'Spi_ConfigSet',path:[],expanded:true});
    const aggregate=(await status(s)).watches.find(w=>w.name==='Spi_ConfigSet');assert(aggregate.tree?.children?.length>0);
    await s.command('unwatch',{expression:'Spi_ConfigSet'});
    const channels=await s.command('memory_channels');
    assert.deepEqual(channels.channels.map(c=>c.configuration.target),['AHB_3','APB_1']);
    const binding=await s.command('watch_resolve',{expression:'uart_cnt'});
    const expected=Number((await s.command('evaluate',{expression:'uart_cnt'})).value);
    const value=await s.command('memory_read',{address:binding.address,bits:32,little_endian:true,channel:'ahb'});
    assert.equal(value.value,expected);evidence.push({core:id,frame:st.frame,registers,binding,value});
   }
   return evidence;
  });
  await suite.test(`THA-${label}-BREAK-STEP`,'Default core-local hardware breakpoint hits C code, then instruction step works',async()=>{
   const id=ids.includes(0)?0:ids[0];await select(s,id);
   const location=id===0?'Gpt_Notification_UartPrintf':'Timer_DelayMs';
   await s.command('break',{location,temporary:true});
   const bp=(await status(s)).breakpoints[0];assert.deepEqual(bp.cores,[ids.indexOf(id)]);
   if(ids.length>1){await select(s,1);assert.equal((await status(s)).breakpoints.length,0);await select(s,0);}
   await s.command('continue');await s.command('wait_stopped',{timeout_ms:12000});
   let st=await status(s);assert.equal(st.frame.function,location);assert.equal(st.breakpoints.length,0);
   const before=st.frame;await s.command('stepi');await s.command('wait_stopped');
   st=await status(s);assert.notEqual(st.frame.address,before.address);return {before,after:st.frame};
  });
  await suite.test(`THA-${label}-SVD`,'Read the installed SVD debug-reset register through the peripheral and AHB routes',async()=>{
   const file=path.join(path.dirname(binary),'../tools/svd/THA6206.svd');
   const xml=fs.readFileSync(file,'utf8');
   const peripheral=xml.match(/<peripheral>\s*<name>MAINRESET<\/name>([\s\S]*?)<\/peripheral>/)?.[1];
   assert(peripheral,'Installed MAINRESET peripheral missing');
   const register=peripheral.match(/<register>\s*<name>DBGRSTCON<\/name>([\s\S]*?)<\/register>/)?.[1];
   assert(register,'Installed DBGRSTCON register missing');
   const address=Number(peripheral.match(/<baseAddress>(.*?)<\/baseAddress>/)[1])+Number(register.match(/<addressOffset>(.*?)<\/addressOffset>/)[1]);
   assert.equal(address,0xc00008a0);
   const peripheralRead=await s.command('peripheral_read',{address,bits:32,little_endian:true});
   const ahb=await s.command('memory_read',{address,bits:32,little_endian:true,channel:'ahb'});
   assert.equal(peripheralRead.value,ahb.value);assert.equal(ahb.value&1,0,'Debug reset request must have self-cleared');
   return {svd_sha256:hash(file),address,peripheralRead,ahb};
  });
  await suite.test(`THA-${label}-LIVE`,'Fresh AHB live Watch samples without MI polling; values track whether the writer core runs',async()=>{
   await pause(s);await select(s,ids.includes(0)?0:ids[0]);
   try {
   await s.command('continue');await delay(300);
   const from=s.events.length,mi=s.logs('mi>').length;
   await delay(1400);const st=await status(s);
   assert(st.cores.every(c=>c.state==='RUNNING'));
   const samples=s.events.slice(from).filter(e=>e.event==='live_watch'&&e.sample.expression==='uart_cnt'&&e.sample.value!==null&&!e.sample.error).map(e=>e.sample);
   assert(samples.length>=3,JSON.stringify(samples));
   assert.equal(s.logs('mi>').length,mi);
   const writer=await s.command('memory_read',{address:0x80410314,bits:32,little_endian:true,channel:'apb'});
   if(ids.includes(0)||(writer.value&0x10)===0) {
    assert(new Set(samples.map(v=>v.value)).size>1,JSON.stringify(samples));
   } else {
    // uart_cnt is incremented by core0's interrupt handler, not by core1.
    // Stable values with a physically halted writer are correct live samples.
    assert.equal(new Set(samples.map(v=>v.value)).size,1);
    const actual=await s.command('memory_read',{address:samples[0].address,bits:32,little_endian:true,channel:'ahb'});
    assert.equal(samples.at(-1).value,actual.value);
   }
   return {samples,writer_core0_edprsr:writer};
   } finally { await pause(s); }
  });
  if(ids.length>1) {
   await suite.test('THA-MULTI-SCOPE','Explicit Core scope runs one core; All scope runs and pauses both',async()=>{
    await pause(s);await select(s,0);await s.command('control_scope',{scope:'core'});await s.command('continue');await delay(150);
    const one=await status(s);assert.deepEqual(one.cores.map(c=>c.state),['RUNNING','STOPPED']);
    await s.command('control_scope',{scope:'all'});await s.command('continue');await delay(150);
    const all=await status(s);assert(all.cores.every(c=>c.state==='RUNNING'));await pause(s);return {one:one.cores,all:all.cores};
   });
   await suite.test('THA-MULTI-BREAK','Convert selected breakpoint to both cores and halt peers on a real breakpoint hit',async()=>{
    await pause(s);await select(s,0);await s.command('break',{location:'Gpt_Notification_UartPrintf',enabled:false});
    let bp=(await status(s)).breakpoints[0];await s.command('break_cores',{number:bp.id,cores:[0,1]});
    bp=(await status(s)).breakpoints[0];assert.deepEqual(bp.cores,[0,1]);assert(bp.group);
    await s.command('enable_break',{number:bp.id,enabled:true});
    await s.command('continue');await s.command('wait_stopped',{timeout_ms:12000,scope:'all'});
    const hit=await status(s);assert(hit.cores.every(c=>c.state==='STOPPED'));assert.equal(hit.frame.function,'Gpt_Notification_UartPrintf');
    bp=hit.breakpoints[0];await s.command('delete_break',{number:bp.id});
    for(const id of ids){await select(s,id);assert.equal((await status(s)).breakpoints.length,0);}
    return {frame:hit.frame,cores:hit.cores};
   });
  }
  await suite.test(`THA-${label}-SYSTEM-REGS`,'Observe R52+ identity on APB and verify R52-only injected readers remain disabled',async()=>{
   const evidence=[];
   for(const id of ids) {
    await select(s,id);const listing=await s.command('registers_list');
    assert.equal(listing.catalogue.cpu,'cortex-r52+');
    const physicalMidr=await s.command('memory_read',{address:(id===0?0x80410000:0x80510000)+0xd00,bits:32,little_endian:true,channel:'apb'});
    assert.equal(physicalMidr.value>>>4&0xfff,0xd16,'Observed processor must match the R52+ catalogue');
    const probe=await s.command('registers_probe',{context:listing.context},true,60000);
    const current=await s.command('registers_list');
    const result=await s.command('registers_read',{context:current.context,ids:['midr','mpidr','sctlr','mpuir','hmpuir'],manual:true},true,60000);
    assert(result.samples.every(sample=>sample.owner===`core:core.${id}`));
    assert.equal(result.samples.find(sample=>sample.id==='midr').state,'unsupported');
    assert(result.samples.every(sample=>['unsupported','unavailable'].includes(sample.state)&&sample.value===null),JSON.stringify(result.samples));
    assert(result.samples.every(sample=>!sample.provenance.access),'No unsupported CP15 instruction may be sent');
    evidence.push({core:id,physicalMidr,probe,result});
   }
   return evidence;
  });
  if(ids.includes(0)) await suite.test(`THA-${label}-RESET`,'Three chip resets recover both cold/warm sessions and reach _main',async()=>{
   const frames=[];
   for(let cycle=0;cycle<3;cycle++) {
    await pause(s);await s.command('restart',{},true,60000);await s.command('run');await select(s,0);
    await s.command('wait_stopped',{timeout_ms:12000});const st=await status(s);assert.equal(st.frame.function,'_main');frames.push(st.frame);
    await s.command('continue');await delay(350);await pause(s);
   }
   return frames;
  });
  else await suite.test('THA-CORE1-ATTACH','Core1-only stays on 3334 and rejects unconfigured chip reset',async()=>{
   const error=await s.command('restart',{},false);assert(/not configured|requires/i.test(error));return error;
  });
  await suite.test(`THA-${label}-RECONNECT`,'Reconnect reopens selected sessions and reloads matching symbols',async()=>{
   await s.command('reconnect',{},true,65000);
   await coreRegisters(s);
   const st=await status(s);assert(st.cores.every(c=>c.state==='STOPPED'));assert(st.registers.some(r=>r.name==='pc'));
   return {cores:st.cores,frame:st.frame};
  });
  // Reconnect resets through the configured startup hook. Its first Continue
  // runs the startup action and stops at _main; do not leave the next attach
  // waiting on an uninitialized peer. Prove firmware readiness before exit.
  await runApplication(s,ids);
  await s.command('continue');await delay(300);
  return true;
 } finally {
  await suite.test(`THA-${label}-CLEANUP`,'Exit this dedicated session and release its probe listeners',async()=>{
   await s.close();assert(!netstat().split(/\r?\n/).some(line=>/LISTENING/.test(line)&&/^\s*TCP\s+127\.0\.0\.1:(3333|3334|6666)\s/.test(line)));
  });
 }
}
(async()=>{
 try {
  assert(!netstat().split(/\r?\n/).some(line=>/LISTENING/.test(line)&&/^\s*TCP\s+\S+:(3333|3334|6666)\s/.test(line)),'Existing debug session owns probe ports');
  if(options['build-download']&&!await firmwareWorkflow())return;
  for(const ids of [[0],[1],[0,1]]) if(!await mode(ids))break;
 } catch(error) {await suite.test('THA-SETUP','Prepare the installed THA regression',async()=>{throw error;});}
 finally {
  await suite.test('THA-PROJECT-PRESERVED','Existing project configurations remain byte-for-byte unchanged',async()=>{
   for(const [file,digest] of Object.entries(protectedHashes))assert.equal(hash(file),digest,file);
   return protectedHashes;
  });
  suite.metadata.configurations=configurations;suite.finish();
 }
})().catch(error=>{console.error(error);process.exitCode=1;});
