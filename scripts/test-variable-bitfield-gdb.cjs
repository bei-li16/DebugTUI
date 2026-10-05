// Actual native GDB bitfield writes, including packed cross-byte fields. No board.
const fs=require('node:fs'),path=require('node:path'),assert=require('node:assert/strict');
const {execFileSync}=require('node:child_process');
const {root,hash,outputDirectory,Cases,Session}=require('./test-support/session.cjs');
const out=outputDirectory('variable-bitfield-native'),binary=path.resolve(process.argv[2]||path.join(root,'target/debug/debugtui.exe'));
const cc=process.env.DEBUGTUI_TEST_CXX||'C:/MinGW/bin/g++.exe',gdb=process.env.DEBUGTUI_TEST_GDB||'C:/MinGW/bin/gdb.exe';
const suite=new Cases(out,{layer:'actual native C++ GCC/GDB; no board',board_tests_executed:false,binary,binary_sha256:hash(binary),cc,gdb});
const ids=['WRITE-T-VAR-BITFIELD','WRITE-T-VAR-BITFIELD-RANGE','WRITE-T-VAR-BITFIELD-PACKED','WRITE-T-VAR-BITFIELD-QUALIFIERS','WRITE-T-VAR-BITFIELD-CLEANUP'];
ids.push('WRITE-T-VAR-BITFIELD-64');
const exe=path.join(out,'sample.exe'),project=path.join(out,'project.toml'),fixture=path.join(root,'tests/fixtures/variable-bitfield-board.cpp');
execFileSync(cc,['-g','-O0','-DDEBUGTUI_BITFIELD_HOST_MAIN',fixture,'-o',exe],{windowsHide:true});
const quote=s=>JSON.stringify(s.replaceAll('\\','/'));
fs.writeFileSync(project,`version=2\nwatch=['bitfield_cells','bitfield_packed','bitfield_wide','bitfield_const','bitfield_volatile']\n[[breakpoints]]\nlocation='debug_bitfield_fixture'\nkind='code'\n[gdb]\nexecutable=${quote(gdb)}\n[target]\nmode='local'\n[program]\nelf=${quote(exe)}\n[session]\non_exit='disconnect'\n[[writes.regions]]\nid='native-bitfield-ram'\nkind='ram'\nstart='0x1'\nend='0xffffffffffffffff'\nwidths=[8,16,32,64,128]\nlittle_endian=true\nscope='core'\n`);
const original=hash(project);let session;
const context=async()=>(await session.command('registers_list')).context;
const count=()=>session.logs('mi>').filter(l=>l.text.includes('-var-assign ')).length;
const evaluate=async expression=>(await session.command('evaluate',{expression})).value;
const number=async expression=>BigInt((await evaluate(expression)).split(' ')[0]);
const expand=async expression=>{await session.command('watch_expand',{expression,path:[],expanded:true});await session.command('watch_expand',{expression,path:[0],expanded:true});};
const request=async(root,index,text,kind='unsigned',checked=true)=>session.command('write_preview',{target:{kind:'variable',pane:'watch',expression:root,path:[0,index]},selection:{kind:'register'},input:{kind,text},context:await context()},checked);
const read=async metadata=>(await session.command('memory_dump',{address:metadata.address,count:metadata.bitfield.layout.parent_bytes,channel:'',context:await context()})).bytes;
const apply=async draft=>{const before=await read(draft.metadata),n=count();const result=await session.command('write_apply',{draft:draft.draft});assert.equal(result.outcome,'verified',JSON.stringify(result));assert.equal(result.neighbours_preserved,true);assert.deepEqual(result.parent_before_bytes,before);assert.deepEqual(await read(draft.metadata),result.parent_expected_bytes);assert.equal(count(),n+1);assert.equal((await session.command('write_apply',{draft:draft.draft})).outcome,'not_sent');assert.equal(count(),n+1);return result;};
(async()=>{
 try{
  session=new Session(binary,project,out);await session.command('connect');await session.command('run');await session.command('wait_stopped');
  if(!await suite.test(ids[0],'Actual target DWARF widths and unsigned field writes preserve the complete parent',async()=>{
   await expand('bitfield_cells');const evidence=[];
   for(const [index,width,values] of [[1,5,['0','31']],[3,7,['0','127']],[4,14,['0','16383']]])for(const value of values){const draft=await request('bitfield_cells',index,value);assert.equal(draft.metadata.scalar.bits,width);assert.equal(draft.metadata.declared_bits,32);assert.equal(draft.plan.needs_fresh_read,true);evidence.push(await apply(draft));}
   assert.equal(await number('bitfield_cells.before'),0x12345678n);assert.equal(await number('bitfield_cells.after'),0x87654321n);return evidence;
  }))return;
  if(!await suite.test(ids[1],'Signed 6-bit boundaries, overflow rejection and cancellation without any assignment',async()=>{
   const evidence=[];for(const value of ['-32','31','-1']){const draft=await request('bitfield_cells',2,value,'signed');assert.equal(draft.metadata.scalar.signed,true);evidence.push(await apply(draft));assert.equal(await number('bitfield_cells.middle'),BigInt(value));}
   const n=count();for(const [index,value,kind] of [[1,'32','unsigned'],[1,'-1','signed'],[2,'32','signed'],[2,'-33','signed'],[2,'32','unsigned']])assert(await request('bitfield_cells',index,value,kind,false));
   const draft=await request('bitfield_cells',2,'0','signed'),before=await read(draft.metadata);await session.command('write_cancel',{draft:draft.draft});assert.equal((await session.command('write_apply',{draft:draft.draft})).outcome,'not_sent');assert.deepEqual(await read(draft.metadata),before);assert.equal(count(),n);return evidence;
  }))return;
  if(!await suite.test(ids[2],'Packed unaligned fields across bytes use target offsets and preserve holes and neighbours',async()=>{
   await expand('bitfield_packed');const evidence=[];for(const [index,value,kind] of [[1,'31','unsigned'],[2,'-32','signed'],[3,'127','unsigned']]){const draft=await request('bitfield_packed',index,value,kind);assert.equal(draft.metadata.bitfield.layout.parent_bytes,6);evidence.push(await apply(draft));}
   assert.equal(await number('bitfield_packed.before'),0x12n);assert.equal(await number('bitfield_packed.after'),0x87n);return evidence;
  }))return;
  if(!await suite.test(ids[3],'Const/volatile parents reject inspection/assignment; no target function or raw-memory write',async()=>{
   const n=count(),errors=[];for(const root of ['bitfield_const','bitfield_volatile'])errors.push(await request(root,1,'1','unsigned',false));assert.equal(count(),n);assert.equal(await number('bitfield_calls'),0n);assert(!session.logs('mi>').some(l=>l.text.includes('-data-write-memory')));return errors;
  }))return;
  if(!await suite.test('WRITE-T-VAR-BITFIELD-64','Actual unsigned 64-bit field and signed 63-bit min/max use GDB LONGEST without truncating high bits',async()=>{
   await expand('bitfield_wide');const evidence=[];
   for(const value of ['0','18446744073709551615']){const draft=await request('bitfield_wide',1,value);assert.equal(draft.metadata.scalar.bits,64);evidence.push(await apply(draft));assert.equal(await number('bitfield_wide.full'),BigInt(value));}
   for(const value of ['-4611686018427387904','4611686018427387903','-1']){const draft=await request('bitfield_wide',2,value,'signed');assert.equal(draft.metadata.scalar.bits,63);evidence.push(await apply(draft));assert.equal(await number('bitfield_wide.signed_value'),BigInt(value));}
   const n=count();assert(await request('bitfield_wide',2,'4611686018427387904','signed',false));assert.equal(count(),n);assert.equal(await number('bitfield_wide.spare'),1n);assert.equal(await number('bitfield_wide.before'),0x1122334455667788n);assert.equal(await number('bitfield_wide.after'),0x8877665544332211n);return evidence;
  }))return;
 }catch(error){await suite.test('WRITE-T-VAR-BITFIELD-SETUP','Set up native bitfield fixture',async()=>{throw error;});}
 finally{
  if(session)await suite.test(ids[4],'Close own process and preserve original project/function-call policy',async()=>{const policies=session.logs('mi>').filter(l=>l.text.includes('may-call-functions')).map(l=>l.text);await session.close();assert(policies.at(-1)?.includes(' on'));assert.equal(hash(project),original);});
  for(const id of ids)if(!suite.results.some(r=>r.id===id))suite.skip(id,'Native bitfield phase','Earlier test failed');suite.finish();
 }
})();
