// Explicit board test. Verify the existing Flash image before any Bao reset or RAM write.
const fs=require('node:fs'),path=require('node:path'),net=require('node:net'),assert=require('node:assert/strict');
const {spawnSync}=require('node:child_process');
const {quote:q,hash,delay,outputDirectory,Cases,Session}=require('./test-support/session.cjs');
const [binaryArg,rootArg,mode]=process.argv.slice(2);
assert(['--verify-only','--allow-reset'].includes(mode),'Usage: BINARY BAO_ROOT --verify-only|--allow-reset');
const binary=path.resolve(binaryArg),bao=path.resolve(rootArg),profile=path.join(bao,'scripts/tha6206/debug-env.toml');
const out=outputDirectory('bao-devices'),suite=new Cases(out,{binary,sha256:hash(binary),bao,mode});
const ports=spawnSync('netstat.exe',['-ano','-p','TCP'],{encoding:'utf8',windowsHide:true,timeout:10000});
assert.ifError(ports.error);assert.equal(ports.status,0);
assert(!ports.stdout.split(/\r?\n/).some(line=>/LISTENING/.test(line)&&/^\s*TCP\s+\S+:(3333|3334|6666)\s/.test(line)),'Existing debugger owns the probe ports');
const protectedFiles=['debug.toml','debug-dual.toml','scripts/tha6206/debug-env.toml','bin/tha6xxx/tha6206-smoke/bao.elf','bin/tha6xxx/tha6206-smoke/bao.bin'].map(p=>path.join(bao,p));
const before=protectedFiles.map(hash);
async function rpc(command){return await new Promise((resolve,reject)=>{
 const socket=net.createConnection({host:'127.0.0.1',port:6666});let text='';
 socket.setTimeout(20000);socket.on('timeout',()=>socket.destroy(Error('OpenOCD read timeout')));socket.on('error',reject);
 socket.on('connect',()=>socket.write(command+'\x1a'));
 socket.on('data',data=>{text+=data.toString();if(text.includes('\x1a')){socket.end();resolve(text.split('\x1a')[0]);}});
});}
function base(extra='') {return `version=3\nwatch=[]\nbreakpoints=[]\n[debug]\nchip="tha6206"\ncores=[0]\n[tools]\nprofile=${q(profile)}\n[gdb]\nregisters=["r0","sp","lr","pc","cpsr"]\n[program]\nelf=${q(path.join(bao,'bin/tha6xxx/tha6206-smoke/bao.elf'))}\nsource_root=${q(bao)}\n[session]\non_exit="resume"\n${extra}`;}
(async()=>{
 let verified=await suite.test('flash','Read AHB Flash alias and compare every byte with the existing Bao image',async()=>{
  const file=path.join(out,'preflight.toml');fs.writeFileSync(file,base());
  const s=new Session(binary,file,out);
  try{
   await s.command('connect',{},true,65000);
   const expected=fs.readFileSync(path.join(bao,'bin/tha6xxx/tha6206-smoke/bao.bin'));
   const chunks=[];
   for(let offset=0;offset<expected.length;offset+=4096){
    const count=Math.min(4096,expected.length-offset);
    const reply=await rpc(`AHB_3 read_memory ${0x90400000+offset} 8 ${count}`);
    const words=reply.trim().split(/\s+/);assert.equal(words.length,count,reply.slice(0,300));
    chunks.push(Buffer.from(words.map(word=>{const n=Number(word);assert(Number.isInteger(n)&&n>=0&&n<=255);return n;})));
   }
   const actual=Buffer.concat(chunks);
   fs.writeFileSync(path.join(out,'flash.bin'),actual);
   assert(actual.equals(expected),'Flash differs from Bao smoke image; no reset, RAM initialization or download performed');
   return {bytes:expected.length,sha256:hash(path.join(out,'flash.bin'))};
  }finally{await s.close();}
 });
 if(verified&&mode==='--allow-reset')for(const ids of [[0],[1],[0,1]]){
  const passed=await suite.test(ids.join('+'),'Bao chip/core selection: source breakpoint, stepping, Continue/Pause and reset ownership',async()=>{
   const directory=path.join(out,ids.join('-'));fs.mkdirSync(directory);
   const original=fs.readFileSync(path.join(bao,'debug.toml'),'utf8');
   const file=path.join(directory,'debug.toml');
   let config=original.replace('profile = "scripts/tha6206/debug-env.toml"',`profile = ${q(profile)}`)
    .replace('elf = "bin/tha6xxx/tha6206-smoke/bao.elf"',`elf = ${q(path.join(bao,'bin/tha6xxx/tha6206-smoke/bao.elf'))}`)
    .replace('source_root = "."',`source_root = ${q(bao)}`)
    .replace('svd = "../THA6XXX_MC_AS440/.vscode/THA6206/tha6206.svd"',`svd = ${q(path.resolve(bao,'../THA6XXX_MC_AS440/.vscode/THA6206/tha6206.svd'))}`)
    .replaceAll(/\[\[breakpoints\]\][\s\S]*?(?=\n\[|$)/g,'')
    .replace('on_exit = "detach"','on_exit = "resume"');
   fs.writeFileSync(file,config);
   const s=new Session(binary,file,directory,['--chip','tha6206','--cores',ids.join(',')]);
   try{
    const c=await s.command('connect',{},true,65000);assert.deepEqual(c.core_names,ids.map(id=>`core.${id}`));
    const countReset=()=>s.logs('mi>').filter(e=>e.text.includes('monitor chipreset')).length;
    await s.command('run');
    if(ids.includes(0)){
     await s.command('wait_stopped',{timeout_ms:15000});await s.command('select_core',{name:'core.0'});
     const st=await s.command('status');assert.equal(st.frame.function,'init');assert(st.frame.file.endsWith('init.c'));
     await s.command('step');await s.command('wait_stopped');
     assert.equal(countReset(),1,'Bootstrap should reset once');
    }else{await s.command('pause');assert.equal(countReset(),0,'core1 attach must not reset the chip');}
    for(const id of ids){await s.command('select_core',{name:`core.${id}`});await s.command('watch',{expression:'platform.cpu_num'});assert.equal((await s.command('status')).watches.find(w=>w.name==='platform.cpu_num').value,'2');}
    await s.command('continue');await delay(300);assert((await s.command('status')).cores.every(c=>c.state==='RUNNING'));
    await s.command('pause');assert((await s.command('status')).cores.every(c=>c.state==='STOPPED'));
    if(ids.includes(0)){await s.command('restart');assert.equal(countReset(),2);await s.command('run');await s.command('wait_stopped',{timeout_ms:15000});assert.equal(countReset(),2,'Run after group Reset must not reset again');await s.command('continue');}
    else await s.command('restart',{},false);
    return {cores:c.core_names,resets:countReset()};
   }finally{await s.close();}
  });
  if(!passed)break;
 }
 assert.deepEqual(protectedFiles.map(hash),before);suite.finish();
})().catch(error=>{console.error(error);process.exitCode=1;});
