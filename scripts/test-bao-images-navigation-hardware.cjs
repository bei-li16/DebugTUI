// Physical core context remains unchanged by image-owned source navigation.
const fs=require('node:fs'),path=require('node:path'),assert=require('node:assert/strict');
const {Session,Cases,outputDirectory,parseOptions,hash,quote}=require('./test-support/session.cjs');
const o=parseOptions(process.argv.slice(2),['--binary','--project'],['--run']);assert(o.run&&o.binary&&o.project,'Requires --run --binary EXE --project Bao/debug-multi-images.toml');
const binary=path.resolve(o.binary),original=path.resolve(o.project),bao=path.dirname(original).replaceAll('\\','/'),out=outputDirectory('bao-images-navigation-hardware');
const project=path.join(out,'debug.toml');fs.writeFileSync(project,fs.readFileSync(original,'utf8').replace(/^elf = "(?![A-Za-z]:|\/)/gm,'elf = "'+bao+'/').replaceAll('source_root = "."','source_root = '+quote(bao)));
const suite=new Cases(out,{board_tests_executed:true,binary,binary_sha256:hash(binary),project,scope:'Per-image file/symbol/assembly queries, stopped and running, with stable physical core/frame/transport'});
let s,baseline;
const status=()=>s.command('status');const transport=()=>s.logs('mi>').map(l=>l.text).filter(t=>/-target-select|-gdb-exit|monitor.*(?:reset|halt)|-target-download|-exec-interrupt/.test(t));
(async()=>{try{
 s=new Session(binary,project,out);
 if(!await suite.test('NAV-BOARD-PREPARE','Real VM0 HVC stops core0 in Bao with VM0 symbols resident',async()=>{
  await s.command('connect',{},true,65000);await s.command('control_scope',{scope:'core'});await s.command('select_core',{name:'core.0'});await s.command('run');await s.command('wait_stopped',{timeout_ms:20000});await s.command('break',{image:'bao',location:'hvc_handler',hardware:true,temporary:true});await s.command('continue');await s.command('wait_stopped',{timeout_ms:20000});const st=await status();assert.equal(st.image_state.execution.image,'bao');assert(st.frame.file.startsWith('@bao:'));assert.equal(st.frame.function,'hvc_handler');baseline=transport();return st;
 }))return;
 if(!await suite.test('NAV-BOARD-STOPPED','Files and same-name symbol searches never select a physical frame or overwrite its source identity',async()=>{
  const before=await status(),start=s.logs('mi>').length;const files=await s.command('files');assert(files.files.some(f=>f.startsWith('@bao:')));assert(files.files.some(f=>f.startsWith('@vm0:')));assert(!files.files.some(f=>f.startsWith('@vm1:')));
  const symbols=await s.command('symbols',{query:'debug_domain_marker'});assert.equal(symbols.symbols.length,2);assert(symbols.symbols.some(f=>f.file.startsWith('@bao:')));assert(symbols.symbols.some(f=>f.file.startsWith('@vm0:')));
  const after=await status();assert.deepEqual(after.frame,before.frame);assert.deepEqual(after.image_state,before.image_state);assert.equal(s.logs('mi>').length,start);assert.deepEqual(transport(),baseline);return {files,symbols,before,after};
 }))return;
 if(!await suite.test('NAV-BOARD-ASM','Current Bao assembly stays on physical GDB; browsing VM0 ELF assembly does not switch the processor',async()=>{
  const before=await status();const physical=await s.command('disassemble');assert.equal(physical.image,'bao');assert.equal(physical.source,'gdb');assert.equal(physical.target_io,true);assert(physical.assembly.length>2);
  const start=s.logs('mi>').length;const guest=await s.command('disassemble',{address:'@vm0:guest_main',source:'elf'});assert.equal(guest.image,'vm0');assert.equal(guest.target_io,false);assert.equal(guest.source,'elf');assert(guest.assembly[0].startsWith('ELF [vm0]'));assert.equal(s.logs('mi>').length,start);assert.deepEqual((await status()).frame,before.frame);assert.deepEqual(transport(),baseline);return {physical,guest};
 }))return;
 if(!await suite.test('NAV-BOARD-RUNNING','Both image metadata tables can be searched while core0 runs without an extra halt or symbol reload',async()=>{
  await s.command('continue');const before=await status();assert.equal(before.state,'RUNNING');const start=s.logs('mi>').length;const files=await s.command('files');const symbols=await s.command('symbols',{query:'@vm0:debug_domain_id'});assert.equal(files.target_io,false);assert.equal(symbols.target_io,false);assert.equal(symbols.symbols.length,1);assert(symbols.symbols[0].file.startsWith('@vm0:'));const after=await status();assert.equal(after.state,'RUNNING');assert.equal(s.logs('mi>').length,start);assert.deepEqual(transport(),baseline);return {files,symbols,before,after};
 }))return;
}catch(error){await suite.test('NAV-BOARD-SETUP','Execute image source navigation board regression',async()=>{throw error;});}
finally{if(s)await suite.test('NAV-BOARD-CLEANUP','Close only owned debugger processes',()=>s.close());suite.finish();}})();
