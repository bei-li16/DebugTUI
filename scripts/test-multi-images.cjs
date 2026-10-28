// Production EXE/MI-pipe regression. Synthetic target responses; no board access.
const fs=require('node:fs'),path=require('node:path'),assert=require('node:assert/strict');
const {Session,Cases,outputDirectory,quote,hash}=require('./test-support/session.cjs');
const binary=path.resolve(process.argv[2]||'target/debug/debugtui.exe');
const out=outputDirectory('multi-images-software');
const cases=new Cases(out,{board_tests_executed:false,layer:'production DebugTUI/MI coordinator; synthetic ELF and physical target replies',binary,binary_sha256:hash(binary)});
function elf(address){
 const b=Buffer.alloc(512);b.write('\x7fELF\x01\x01\x01',0,'binary');
 for(const [at,v]of [[16,2],[18,40],[40,52],[46,40],[48,4],[50,2]])b.writeUInt16LE(v,at);
 b.writeUInt32LE(256,32);const names=Buffer.from('\0.text\0.shstrtab\0.debug_info\0');names.copy(b,128);
 for(const [n,name,flags,addr,offset,size]of [[1,1,6,address,64,32],[2,7,0,0,128,names.length],[3,17,0,0,96,1]]){
  for(const [field,v]of [[0,name],[8,flags],[12,addr],[16,offset],[20,size]])b.writeUInt32LE(v,256+n*40+field);
 }return b;
}
const frameFile=path.join(out,'frames.json'),transcript=path.join(out,'commands.txt');fs.writeFileSync(transcript,'');
const writeFrames=(first='bao',second='vm1')=>fs.writeFileSync(frameFile,JSON.stringify(Object.fromEntries([first,second].map((id,n)=>['fixture'+n,{pc:id==='bao'?'0x08000008':n?'0x10028008':'0x10020008',cpsr:id==='bao'?'0x1da':'0x1d3',function:id==='bao'?'hyp_trap_handler':'guest_main',file:id+'.c',line:12}]))));writeFrames();
for(const [id,address]of [['bao',0x08000000],['vm0',0x10020000],['vm1',0x10028000]])fs.writeFileSync(path.join(out,id+'.elf'),elf(address));
const project=path.join(out,'debug.toml');
const base=`version=3\n[program]\nelf=${quote(path.join(out,'bao.elf'))}\n[gdb]\nexecutable=${quote(process.execPath)}\nargs=[${quote(path.resolve('tests/mock-gdb.cjs'))}]\n[gdb.env]\nDEBUGTUI_TEST_REGISTERS='["r0","pc","cpsr"]'\nDEBUGTUI_TEST_TRANSCRIPT=${quote(transcript)}\nDEBUGTUI_TEST_IMAGE_FRAME_FILE=${quote(frameFile)}\nDEBUGTUI_TEST_PAUSE='hold'\n[target]\nmode='extended-remote'\n[session]\non_exit='disconnect'\n[multicore]\nscope='core'\nhalt_peers=false\n`;
const images=`[[images]]\nid='bao'\nelf=${quote(path.join(out,'bao.elf'))}\ndomain='hyp'\n[[images]]\nid='vm0'\nelf=${quote(path.join(out,'vm0.elf'))}\ncores=['core.0']\ndomain='guest'\n[[images]]\nid='vm1'\nelf=${quote(path.join(out,'vm1.elf'))}\ncores=['core.1']\ndomain='guest'\n`;
fs.writeFileSync(project,base+images+`[[cores]]\nname='core.0'\nendpoint='fixture0'\n[[cores]]\nname='core.1'\nendpoint='fixture1'\n`);
let session;
(async()=>{try{
 session=new Session(binary,project,out);
 if(!await cases.test('IMAGES-OFFLINE','Per-core file-only inventory does not open GDB',async()=>{
  await session.command('select_core',{name:'core.0'});
  const inv=await session.command('images');assert.deepEqual(inv.state.images.map(i=>i.id),['bao','vm0']);assert.equal(inv.target_io,false);assert.equal(session.logs('mi>').length,0);return inv;
 }))return;
 if(!await cases.test('IMAGES-CONNECT','Resident symbols load before target attachment and stay core-local',async()=>{
  await session.command('connect');const lines=fs.readFileSync(transcript,'utf8').split(/\r?\n/);assert.equal(lines.filter(l=>l.startsWith('-target-select')).length,2);assert.equal(lines.filter(l=>l.includes('add-symbol-file')).length,2);
  await session.command('select_core',{name:'core.0'});assert.deepEqual((await session.command('images')).state.images.map(i=>i.id),['bao','vm0']);
  await session.command('select_core',{name:'core.1'});assert.deepEqual((await session.command('images')).state.images.map(i=>i.id),['bao','vm1']);
 }))return;
 if(!await cases.test('IMAGES-TRANSITIONS','Twenty synthetic domain round trips retain symbol tables and connections',async()=>{
  const seen=[];
  for(let n=0;n<20;n++)for(const [a,b]of [['bao','vm1'],['vm0','bao']]){
   writeFrames(a,b);
   for(const [core,image]of [[0,a],[1,b]]){await session.command('select_core',{name:'core.'+core});await session.command('refresh');const st=await session.command('status');assert.equal(st.image_state.execution.image,image);assert.equal(st.image_state.selected_frame.image,image);seen.push({core,image,context:st.register_generation});}
  }
  const lines=fs.readFileSync(transcript,'utf8').split(/\r?\n/);assert.equal(lines.filter(l=>l.startsWith('-target-select')).length,2);assert.equal(lines.filter(l=>l.includes('add-symbol-file')).length,2);assert(!lines.some(l=>/-exec-interrupt|monitor.*(?:halt|reset)|-target-download/.test(l)));return {samples:seen.length,physical_board:false};
 }))return;
 if(!await cases.test('IMAGES-UNKNOWN','Unknown stopped PC clears typed views and is rejected before physical unwinding or evaluation',async()=>{
  const frames=JSON.parse(fs.readFileSync(frameFile,'utf8'));frames.fixture0={pc:'0x77770000',cpsr:'0x1d3',function:'wrong_resident_symbol',file:'wrong.c',line:12};fs.writeFileSync(frameFile,JSON.stringify(frames));
  await session.command('select_core',{name:'core.0'});const start=session.logs('mi>').length;await session.command('refresh');const st=await session.command('status');assert.equal(st.image_state.execution.state,'unknown');assert.equal(st.locals.length,0);assert.equal(st.watches.length,0);assert.equal(st.stack.length,0);assert.equal(st.frame.file,'');assert.equal(st.frame.function,'');
  const commands=session.logs('mi>').slice(start).map(e=>e.text);assert(!commands.some(c=>/-stack-list-frames|-stack-list-variables/.test(c)));assert(st.registers.some(r=>r.name==='pc'),'Raw physical register viewing remains available at an unknown PC');
  const evalStart=session.logs('mi>').length;const error=await session.command('evaluate',{expression:'counter'},false);assert(error.includes('unknown or ambiguous'));assert.equal(session.logs('mi>').length,evalStart);writeFrames();await session.command('refresh');return {state:st.image_state,error,commands};
 }))return;
 if(!await cases.test('IMAGES-AMBIGUOUS','Overlapping executable images never choose a DWARF scope or an image-specific physical breakpoint by guesswork',async()=>{
  const config=path.join(out,'overlap.toml'),dir=path.join(out,'overlap');fs.mkdirSync(dir);
  fs.writeFileSync(config,base+images+`[[images]]\nid='vm0-alias'\nelf=${quote(path.join(out,'vm0.elf'))}\ncores=['core.0']\ndomain='guest'\n[[cores]]\nname='core.0'\nendpoint='fixture0'\n[[cores]]\nname='core.1'\nendpoint='fixture1'\n`);writeFrames('vm0','vm1');const test=new Session(binary,config,dir);
  try{await test.command('connect');await test.command('select_core',{name:'core.0'});const start=test.logs('mi>').length;await test.command('refresh');const st=await test.command('status');assert.equal(st.image_state.execution.state,'ambiguous');assert.deepEqual(st.image_state.execution.candidates,['vm0','vm0-alias']);assert.equal(st.stack.length,0);assert.equal(st.locals.length,0);assert.equal(st.frame.file,'');assert(!test.logs('mi>').slice(start).some(e=>/-stack-list-frames|-stack-list-variables/.test(e.text)));
   const before=test.logs('mi>').length;const error=await test.command('evaluate',{expression:'counter'},false);assert(error.includes('unknown or ambiguous'));assert.equal(test.logs('mi>').length,before);const breakpoint=await test.command('break',{image:'vm0',location:'*0x10020008',hardware:true},false);assert(breakpoint.includes('overlapping images'));const preflight=test.logs('mi>').slice(before).map(e=>e.text);assert(preflight.length<=1&&preflight.every(command=>/^\[core\.0\] \d+-break-list$/.test(command)),'Only the existing breakpoint table may be read; no breakpoint/control may be sent');return {state:st.image_state,error,breakpoint,preflight};
  }finally{await test.close();writeFrames();await session.command('refresh');}
 }))return;
 if(!await cases.test('IMAGES-RUNNING','Inventory while running never requests a halt or reload',async()=>{
  await session.command('continue');const inv=await session.command('images');assert.equal(inv.running,true);assert.equal(inv.target_io,false);
  const tail=session.logs('mi>').map(e=>e.text);assert(!tail.some(c=>/-exec-interrupt|monitor.*halt/.test(c)));await session.command('pause');
 }))return;
 await cases.test('IMAGES-REBUILD','Same-path ELF changes invalidate symbols without silently reloading',async()=>{
  const file=path.join(out,'vm1.elf');const bytes=fs.readFileSync(file);bytes[70]=19;fs.writeFileSync(file,bytes);
  await session.command('select_core',{name:'core.1'});const start=session.logs('mi>').length;const error=await session.command('evaluate',{expression:'counter'},false);assert(error.includes('content changed'));assert.equal(session.logs('mi>').length,start,'Stale ELF must reject evaluation before issuing MI');
  await session.command('refresh');const st=await session.command('status');assert.equal(st.image_state.execution.state,'stale');assert(st.image_state.error.includes('content changed'));assert.equal(st.locals.length,0);assert.equal(st.frame.file,'');
  assert.equal(session.logs('mi>').filter(l=>l.text.includes('add-symbol-file')).length,2);return st.image_state;
 });
 await cases.test('IMAGES-INVALID-PREFLIGHT','A corrupt image on either core is rejected before the shared service or any GDB starts',async()=>{
  const marker=path.join(out,'unexpected-service.txt'),bad=path.join(out,'bad.elf'),config=path.join(out,'bad.toml'),dir=path.join(out,'bad-preflight');fs.mkdirSync(dir);fs.writeFileSync(bad,'broken ELF');
  const service=`[service]\ncommand=${quote(process.execPath)}\nargs=["-e",${JSON.stringify(`require('node:fs').writeFileSync(${JSON.stringify(marker)},'started');console.log('fixture ready');setInterval(()=>{},1000);`)}]\nready=['fixture ready']\n`;
  fs.writeFileSync(config,base+service+images.replace(quote(path.join(out,'vm1.elf')),quote(bad))+`[[cores]]\nname='core.0'\nendpoint='fixture0'\n[[cores]]\nname='core.1'\nendpoint='fixture1'\n`);
  const test=new Session(binary,config,dir);try{const error=await test.command('connect',{},false);assert.match(error,/ELF|image/i);assert(!fs.existsSync(marker));assert.equal(test.logs('mi>').length,0);return {error,service_started:false,gdb_commands:0};}finally{await test.close();}
 });
 await cases.test('IMAGES-PARTIAL-LOAD','A failed extra-symbol load releases that GDB and reports nonresident symbols instead of success',async()=>{
  const config=path.join(out,'partial.toml'),dir=path.join(out,'partial');fs.mkdirSync(dir);
  const failing=base.replace('[target]',`DEBUGTUI_TEST_IMAGE_FAIL='vm0.elf'\n[target]`);
  fs.writeFileSync(config,failing+images+`[[cores]]\nname='core.0'\nendpoint='fixture0'\nstartup_order=1\n[[cores]]\nname='core.1'\nendpoint='fixture1'\nstartup_order=0\n`);
  const test=new Session(binary,config,dir);try{
   const error=await test.command('connect',{},false);assert.match(error,/Load image vm0/);await test.command('select_core',{name:'core.0'});const inv=await test.command('images');assert.equal(inv.state.symbols_loaded,false);assert(inv.state.error.includes('resources released'));
   const st=await test.command('status');assert(st.cores.every(c=>c.state==='DISCONNECTED'),'Failed group startup must roll back both workers');assert(test.logs('mi>').some(e=>e.text.includes('[core.1]')&&e.text.includes('-target-disconnect')));assert(!test.logs('mi>').some(e=>e.text.includes('[core.0]')&&e.text.includes('-target-select')));return {error,state:st.state,images:inv.state};
  }finally{await test.close();}
 });
}catch(e){await cases.test('IMAGES-SETUP','Run MI-pipe fixture',async()=>{throw e;});}
finally{if(session)await cases.test('IMAGES-CLEANUP','Close only the owned fixture sessions',()=>session.close());cases.finish();}})();
