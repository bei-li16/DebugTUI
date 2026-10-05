// Deferred BF-H02/03/06/07. Never runs, halts or resets a CPU.
const fs=require('node:fs'),path=require('node:path'),assert=require('node:assert/strict');
const {root,hash,outputDirectory,parseOptions,Cases,Session}=require('./test-support/session.cjs');
const options=parseOptions(process.argv.slice(2),['--binary','--project','--core','--case','--fixture-function'],['--run','--software-fixture']);
const out=outputDirectory('variable-bitfield-hardware'),ids=['BF-H-PRECONDITION','BF-H-CANCEL','BF-H-APPLY','BF-H-RESTORE'];
const suite=new Cases(out,{layer:options['software-fixture']?'software fixture; no board':options.run?'physical board; dedicated halted RAM fixture':'deferred preparation; no target access',board_tests_executed:!!options.run&&!options['software-fixture']});
if(!options.run){for(const id of ids)suite.skip(id,'Deferred typed bitfield phase','Requires --run, dedicated halted fixture and independently confirmed target DWARF layout');suite.finish();process.exit(0);}
assert(options.project&&options.core&&options.case&&options['fixture-function']);
const binary=path.resolve(options.binary||path.join(root,'target/release/debugtui.exe')),project=path.resolve(options.project),caseFile=path.resolve(options.case),spec=JSON.parse(fs.readFileSync(caseFile,'utf8'));
const pure=/^[A-Za-z_]\w*(?:(?:\.|->|::)[A-Za-z_]\w*|\[\d+\])*$/;
assert(spec.target?.kind==='variable'&&['watch','locals'].includes(spec.target.pane)&&pure.test(spec.target.expression));
assert(pure.test(spec.parent)&&pure.test(spec.field));assert(Number.isSafeInteger(spec.frame)&&spec.frame>=0&&spec.frame_function);
assert(spec.owner&&spec.scope&&typeof spec.little_endian==='boolean');
assert(Array.isArray(spec.sentinels)&&spec.sentinels.length>=2&&spec.sentinels.every(e=>pure.test(e)));
const layout=spec.expected_layout;
assert(layout&&Number.isSafeInteger(layout.bit_offset)&&layout.bit_offset>=0&&Number.isSafeInteger(layout.bits)&&layout.bits>=1&&layout.bits<=64&&Number.isSafeInteger(layout.parent_bytes)&&layout.parent_bytes>=1&&layout.bit_offset+layout.bits<=layout.parent_bytes*8&&[1,2,4,8].includes(layout.declared_bytes));
const originalHash=hash(project);suite.metadata={...suite.metadata,binary,binary_sha256:hash(binary),project,project_sha256:originalHash,case_file:caseFile,case_sha256:hash(caseFile),core:options.core};
let session,metadata,original,originalRaw,sentinels;
const context=async()=>(await session.command('registers_list')).context;
const evaluate=async expression=>(await session.command('evaluate',{expression})).value;
const neighbours=async()=>Promise.all(spec.sentinels.map(e=>evaluate(e)));
const preview=async input=>session.command('write_preview',{target:spec.target,selection:{kind:'register'},input,context:await context()});
const count=()=>session.logs('mi>').filter(l=>l.text.includes('-var-assign ')).length;
// Use the independently reviewed case layout, rather than accepting backend metadata.
const mapping=index=>{const physical=layout.bit_offset+index;return [Math.floor(physical/8),1<<(spec.little_endian?physical%8:7-physical%8),spec.little_endian?index:layout.bits-1-index];};
const extract=bytes=>{let value=0n;for(let i=0;i<layout.bits;i++){const [byte,mask,logical]=mapping(i);if(bytes[byte]&mask)value|=1n<<BigInt(logical);}return value;};
const expected=(before,raw)=>{const bytes=[...before];for(let i=0;i<layout.bits;i++){const [byte,mask,logical]=mapping(i);bytes[byte]=(bytes[byte]&~mask)|((raw&(1n<<BigInt(logical)))?mask:0);}return bytes;};
const read=async()=>{assert.equal(BigInt((await evaluate(`(unsigned long long)&(${spec.parent})`)).split(' ')[0]),BigInt(metadata.address));assert.equal(Number(await evaluate(`sizeof(${spec.parent})`)),layout.parent_bytes);const r=await session.command('memory_dump',{address:metadata.address,count:metadata.bitfield.capture_bytes,channel:'',context:await context()});assert.equal(r.address,metadata.address);assert.equal(r.bytes.length,metadata.bitfield.capture_bytes);return r.bytes;};
const restoreInput=()=>({kind:metadata.scalar.signed?'signed':'unsigned',text:(metadata.scalar.signed&&originalRaw&(1n<<BigInt(layout.bits-1))?originalRaw-(1n<<BigInt(layout.bits)):originalRaw).toString()});
const apply=async draft=>{const n=count(),result=await session.command('write_apply',{draft:draft.draft});assert.equal(result.outcome,'verified',JSON.stringify(result));assert.equal(result.owner,spec.owner);assert.equal(result.neighbours_preserved,true);assert.equal(count(),n+1);return result;};
(async()=>{
 try{
  session=new Session(binary,project,out);await session.command('connect');if(options.core!=='default')await session.command('select_core',{name:options.core});
  if(!await suite.test(ids[0],'Already halted fixture, independently confirmed parent/type/bit layout and representable restore',async()=>{
   const initial=await session.command('status');assert.equal(initial.state,'STOPPED');assert.equal(initial.frame.function,options['fixture-function']);assert((initial.cores||[]).every(c=>c.state==='STOPPED'));await session.command('frame',{level:spec.frame});assert.equal((await session.command('status')).frame.function,spec.frame_function);
   const draft=await preview(spec.input);metadata=draft.metadata;assert.equal(metadata.expression,spec.field);assert.deepEqual(metadata.bitfield.layout,layout);assert.equal(metadata.bitfield.little_endian,spec.little_endian);assert.equal(draft.owner,spec.owner);assert.equal(draft.scope,spec.scope);await session.command('write_cancel',{draft:draft.draft});original=await read();originalRaw=extract(original);sentinels=await neighbours();
   const restore=await preview(restoreInput());assert.equal(BigInt(restore.plan.value.hex),originalRaw);await session.command('write_cancel',{draft:restore.draft});return {initial,metadata,original,original_raw:originalRaw.toString(),sentinels};
  }))return;
  if(!await suite.test(ids[1],'Preview/cancel does not assign and preserves complete parent and neighbours',async()=>{const n=count(),draft=await preview(spec.input);await session.command('write_cancel',{draft:draft.draft});assert.equal((await session.command('write_apply',{draft:draft.draft})).outcome,'not_sent');assert.equal(count(),n);assert.deepEqual(await read(),original);assert.deepEqual(await neighbours(),sentinels);} ))return;
  if(!await suite.test(ids[2],'One typed assignment with independent parent/field/neighbour verification and no replay',async()=>{const before=await read(),draft=await preview(spec.input),result=await apply(draft),after=await read();assert.deepEqual(after,expected(before,BigInt(draft.plan.value.hex)));assert.equal(extract(after),BigInt(draft.plan.value.hex));assert.equal(BigInt.asUintN(layout.bits,BigInt((await evaluate(spec.field)).split(' ')[0])),BigInt(draft.plan.value.hex));assert.deepEqual(await neighbours(),sentinels);const n=count();assert.equal((await session.command('write_apply',{draft:draft.draft})).outcome,'not_sent');assert.equal(count(),n);assert(!session.logs('mi>').some(l=>l.text.includes('-data-write-memory')));return result;}))return;
  await suite.test(ids[3],'Explicit restore after verified success only; independently verify complete original bytes',async()=>{const result=await apply(await preview(restoreInput()));assert.deepEqual(await read(),original);assert.deepEqual(await neighbours(),sentinels);return result;});
 }catch(error){await suite.test('BF-H-SETUP','Prepare dedicated bitfield session',async()=>{throw error;});}
 finally{if(session)await suite.test('BF-H-CLEANUP','Close own session and preserve original project',async()=>{await session.close();assert.equal(hash(project),originalHash);});for(const id of ids)if(!suite.results.some(r=>r.id===id))suite.skip(id,'Deferred bitfield phase','Earlier phase failed; no retry or automatic rollback');suite.finish();}
})();
