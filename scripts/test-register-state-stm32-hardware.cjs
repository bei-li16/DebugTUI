// Opt-in STM32F429 board regression. Uses an isolated copy of the user's project.
// Never flashes firmware; temporary R0/MPU selector writes are verified/restored while halted.
const fs = require('node:fs'), path = require('node:path'), net = require('node:net');
const assert = require('node:assert/strict');
const {root, quote, hash, delay, outputDirectory, parseOptions, Cases, Session} = require('./test-support/session.cjs');
const options = parseOptions(process.argv.slice(2), ['--binary','--project'], ['--run']);
const out = outputDirectory('register-state-stm32');
const binary = path.resolve(options.binary || path.join(root,'target/release/debugtui.exe'));
const source = options.project && path.resolve(options.project);
const suite = new Cases(out,{layer:'STM32F429 physical board; UI replay is a separate Rust test',board_tests_executed:!!options.run});
if (!options.run) {
  suite.skip('STM32-REGISTER-STATE','Read/probe/persistence/temporary writes/run boundaries','Requires --run --project FILE and a connected STM32F429 CMSIS-DAP');
  suite.finish(); process.exit(0);
}
assert(source && fs.existsSync(source) && fs.existsSync(binary),'Missing --project or binary');
const sourceHash=hash(source), project=path.join(out,'debug.toml');
// Preserve the source project's Chip/Core, PPB routes and commands, resolving its relative paths.
let text=fs.readFileSync(source,'utf8');
for(const key of ['elf','source_root']) text=text.replace(new RegExp(`(^\\s*${key}\\s*=\\s*)"([^"]*)"`,'m'),(_,prefix,value)=>prefix+quote(path.resolve(path.dirname(source),value)));
// The isolated test stays halted on disconnect, including a failed write phase.
text=text.replace(/(^\s*on_exit\s*=\s*)"[^"]*"/m,'$1"disconnect"');
assert(!/^\s*\[actions\]/m.test(text),'Fixture needs explicit review of existing [actions]');
text+='\n[actions]\nbefore_disconnect = []\n';
fs.writeFileSync(project,text);
Object.assign(suite.metadata,{binary,binary_sha256:hash(binary),project:source,project_sha256:sourceHash,firmware_sha256:hash(path.join(path.dirname(source),'Debug/FreeRTOS_Project.elf'))});
const addresses={'mpu.type':0xe000ed90,'mpu.ctrl':0xe000ed94,'mpu.rnr':0xe000ed98,'mpu.rbar':0xe000ed9c,'mpu.rasr':0xe000eda0,'scb.cpacr':0xe000ed88,'fpu.mvfr0':0xe000ef40,'fpu.mvfr1':0xe000ef44,'fpu.mvfr2':0xe000ef48,'fpu.fpccr':0xe000ef34,'fpu.fpcar':0xe000ef38,'fpu.fpdscr':0xe000ef3c};
const valid=result=>{assert(result.samples.length);for(const s of result.samples){assert.equal(s.state,'valid',`${s.id}: ${s.detail}`);assert.equal(s.owner,'core:core.0');assert(s.value?.hex);}};
const save=(name,value)=>fs.writeFileSync(path.join(out,name),JSON.stringify(value,null,2)+'\n');
const replay={project,reads:[]};
let session,context,originalR0,needsRestore=false;
const read=async(ids,capture=false)=>{
  const result=await session.command('registers_read',{context,ids,manual:true});valid(result);
  if(capture) replay.reads.push({result,status:await session.command('status')});
  return result;
};
const preview=async value=>session.command('write_preview',{target:{kind:'register',id:'r0'},selection:{kind:'register'},context,input:{kind:'unsigned',text:value}});
const apply=async draft=>{const result=await session.command('write_apply',{draft:draft.draft});assert.equal(result.outcome,'verified',JSON.stringify(result));return result;};
function rpc(command){return new Promise((resolve,reject)=>{const s=net.createConnection({host:'127.0.0.1',port:6666});let text='';s.setTimeout(8000,()=>s.destroy(Error('Tcl timeout')));s.on('error',reject);s.on('connect',()=>s.write(command+'\x1a'));s.on('data',chunk=>{text+=chunk;const end=text.indexOf('\x1a');if(end>=0){s.end();resolve(text.slice(0,end).trim());}});});}
(async()=>{
  try {
    session=new Session(binary,project,out);
    if(!await suite.test('CONNECT-IDENTITY','Connect FreeRTOS_Project at physical frame zero and probe STM32F429 identity',async()=>{
      await session.command('connect');replay.initial=await session.command('status');
      assert.equal(replay.initial.state,'STOPPED');assert.equal(replay.initial.frame.level,0);
      context=(await session.command('registers_list')).context;
      const probe=await session.command('registers_probe',{context});
      assert.equal(probe.probe.samples.find(s=>s.id==='scb.cpuid').value.hex,'0x410fc241');
      assert.equal(probe.probe.facts['mpu.regions'].value,8);assert.equal(probe.probe.facts['vfp.d_registers'].value,16);
      // Replay begins before probe: the first staged read supplies actual probe facts.
      save('probe.json',probe);return {context,frame:replay.initial.frame,facts:probe.probe.facts};
    })) return;
    await suite.test('STAGED-PPB-VALUES','Stage MPU/FPU reads and compare all 12 values against independent GDB memory addresses',async()=>{
      const comparisons=[];
      for(const ids of [['mpu.type','mpu.ctrl','mpu.rnr','mpu.rbar','mpu.rasr'],['scb.cpacr','fpu.mvfr0','fpu.mvfr1','fpu.mvfr2'],['fpu.fpccr','fpu.fpcar','fpu.fpdscr']]){
        const result=await read(ids,true);
        for(const s of result.samples){const raw=await session.command('memory_read',{address:addresses[s.id],bits:32,little_endian:true,channel:''});assert.equal(BigInt(s.value.hex),BigInt(raw.raw.hex));assert.equal(s.provenance.access.route.target,'stm32f4x.cpu');assert.equal(s.provenance.access.route.address,`0x${addresses[s.id].toString(16)}`);comparisons.push({id:s.id,register:s.value.hex,gdb:raw.raw.hex});}
      }
      save('comparisons.json',comparisons);return comparisons;
    });
    await suite.test('CORE-FLOAT-VALUES','Read 23 core and 49 floating-point definitions and verify all D/S aliases',async()=>{
      const list=await session.command('registers_list');
      const result=await read(list.catalogue.registers.filter(r=>['core','vfp'].includes(r.group)).map(r=>r.id),true);assert.equal(result.samples.length,72);
      for(let i=0;i<16;i++){const value=id=>BigInt(result.samples.find(s=>s.id===id).value.hex);assert.equal(value(`s${2*i}`),value(`d${i}`)&0xffffffffn);assert.equal(value(`s${2*i+1}`),value(`d${i}`)>>32n);}
      save('core-fpu.json',result);return {core:23,floating:49,aliases_checked:32};
    });
    await suite.test('MPU-READ-RESTORE','Read eight MPU regions and independently verify values and selector restoration',async()=>{
      const before=await read(['mpu.ctrl','mpu.rnr']);
      const result=await session.command('registers_mpu',{context,bank:'m',read:true});
      assert.equal(result.view.state,'valid');assert.equal(result.view.count,8);assert.equal(result.view.original_selector.hex,result.view.restored_selector.hex);
      const script='set original [lindex [stm32f4x.cpu read_memory 0xe000ed98 32 1] 0]; set values {}; set rc [catch {for {set i 0} {$i < 8} {incr i} {stm32f4x.cpu write_memory 0xe000ed98 32 [list $i]; lappend values {*}[stm32f4x.cpu read_memory 0xe000ed9c 32 2]}} err]; stm32f4x.cpu write_memory 0xe000ed98 32 [list $original]; set restored [lindex [stm32f4x.cpu read_memory 0xe000ed98 32 1] 0]; if {$restored != $original} {error {RNR restoration mismatch}}; if {$rc} {error $err}; set values';
      const raw=(await rpc(script)).split(/\s+/).map(BigInt);assert.equal(raw.length,16);
      for(let i=0;i<8;i++){assert.equal(BigInt(result.view.regions[i].base.value.hex),raw[2*i]);assert.equal(BigInt(result.view.regions[i].attributes.value.hex),raw[2*i+1]);}
      assert.deepEqual((await read(['mpu.ctrl','mpu.rnr'])).samples.map(s=>s.value),before.samples.map(s=>s.value));
      save('mpu.json',result);return {regions:8,selector_restored:result.view.restored_selector.hex,control_preserved:true};
    });
    await suite.test('PREFERENCES','Persist group/filter/field settings through the Chip/Core coordinator without target I/O',async()=>{
      const start=session.logs('mi>').length;
      const scope=JSON.stringify(['stm32f429','core.0','cortex-m4','armv7e-m','builtin:cortex-m4',1]);
      const preferences={open:['core','system','m_mpu','m_fpu'],fields:['fpu.fpccr'],filter:3};
      assert((await session.command('register_preferences',{scope,preferences})).saved);
      assert.equal(session.logs('mi>').length,start);
      const stored=fs.readFileSync(project,'utf8');assert(stored.includes('m_mpu')&&stored.includes('fpu.fpccr'));
      const reopened=new Session(binary,project,out);
      try {assert((await reopened.command('register_preferences',{scope,preferences})).saved);assert.equal(reopened.logs('mi>').length,0);}finally{await reopened.close();}
      return {scope,preferences,reopen_saved:true,target_io:false};
    });
    if(!await suite.test('CORE-WRITE-CANCEL-APPLY-RESTORE','Cancel and apply an R0 edit; verify independently, prevent duplicate apply, then restore before execution',async()=>{
      originalR0=(await read(['r0'])).samples[0].value.hex;
      const neighbours=await read(['r1','sp','pc','xpsr']);
      const pattern=BigInt(originalR0)===0x89abcdefn?'0x12345678':'0x89abcdef';
      const cancelled=await preview(pattern);assert.equal(cancelled.verification_basis,'gdb_register_view');assert.equal(cancelled.physical_storage_verified,false);assert(cancelled.warning.includes('write-back cache'));assert.equal((await session.command('write_cancel',{draft:cancelled.draft})).outcome,'not_sent');
      assert.equal(BigInt.asUintN(32,BigInt((await session.command('evaluate',{expression:'$r0'})).value)),BigInt(originalR0));
      const draft=await preview(pattern);needsRestore=true;const result=await apply(draft);assert.equal(result.verification_basis,'gdb_register_view');assert.equal(result.physical_storage_verified,false);assert(result.warning.includes('not been independently verified'));
      assert.equal(BigInt.asUintN(32,BigInt((await session.command('evaluate',{expression:'$r0'})).value)),BigInt(pattern));
      assert.equal((await session.command('write_apply',{draft:draft.draft})).outcome,'not_sent');
      context=(await session.command('registers_list')).context;
      const restored=await apply(await preview(originalR0));
      assert.equal(BigInt.asUintN(32,BigInt((await session.command('evaluate',{expression:'$r0'})).value)),BigInt(originalR0));needsRestore=false;
      context=(await session.command('registers_list')).context;
      assert.deepEqual((await read(['r1','sp','pc','xpsr'])).samples.map(s=>s.value),neighbours.samples.map(s=>s.value));
      return {register:'r0',original:originalR0,pattern,result,restored};
    })) return;
    await suite.test('TRUE-STALE-REFRESH','Continue/pause invalidates the old stop; Probe/Read produces fresh valid values',async()=>{
      await session.command('continue');await delay(200);await session.command('pause');await session.command('wait_stopped',{timeout_ms:8000});
      replay.stale=await session.command('status');assert(replay.stale.register_samples.some(s=>s.state==='stale'));
      context=(await session.command('registers_list')).context;await session.command('registers_probe',{context});
      const result=await read([...Object.keys(addresses),'fpscr']);replay.refreshed={result,status:await session.command('status')};
      return {context,valid_refreshed:13};
    });
  }catch(error){await suite.test('SETUP','Complete STM32 register regression',async()=>{throw error;});}
  finally {
    // A failed/unknown write remains halted: never auto-resume an unverified R0.
    if(needsRestore) await suite.test('WRITE-RESTORE-REQUIRED','Verify that the temporary register was restored',async()=>{throw Error(`R0 needs manual verification/restoration to ${originalR0}; do not resume`);});
    save('ui-replay.json',replay);
    if(session) await suite.test('CLEANUP','Detach only the test-owned session',()=>session.close());
    await suite.test('SOURCE-PRESERVED','Keep FreeRTOS_Project configuration byte-for-byte unchanged',async()=>{assert.equal(hash(source),sourceHash);return {sha256:sourceHash};});
    suite.finish();
  }
})();
