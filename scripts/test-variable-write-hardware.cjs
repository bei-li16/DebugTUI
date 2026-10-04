// Deferred WRITE-H01. Use a dedicated halted ordinary-RAM fixture.
// Default mode only prepares a report. This driver never runs/halts/resets a CPU.
const fs=require('node:fs'),path=require('node:path'),assert=require('node:assert/strict');
const {root,hash,outputDirectory,parseOptions,Cases,Session}=require('./test-support/session.cjs');
const options=parseOptions(process.argv.slice(2),['--binary','--project','--core','--case','--fixture-function'],['--run','--software-fixture','--special-floats']);
const out=outputDirectory('variable-write-hardware');
const ids=['WRITE-H01-VAR-PRECONDITION','WRITE-H01-VAR-CANCEL','WRITE-H01-VAR-APPLY','WRITE-H01-VAR-RESTORE'];
const special='WRITE-H01-VAR-SPECIAL-FLOAT';if(options['special-floats'])ids.push(special);
const suite=new Cases(out,{layer:options['software-fixture']?'software fixture; no board':options.run?'physical board, dedicated variable fixture':'deferred preparation; no target access',board_tests_executed:!!options.run&&!options['software-fixture'],todo:'WRITE-H01 typed ordinary-RAM variable subset; repeat per core and each declared member/frame'});
if(!options.run){for(const id of ids)suite.skip(id,'Typed variable cancel/write/independent read/explicit restore','Requires --run and a dedicated halted fixture');suite.finish();process.exit(0);}
assert(options.project&&options.core&&options.case&&options['fixture-function'],'--run needs --project --core --case --fixture-function');
const binary=path.resolve(options.binary||path.join(root,'target/release/debugtui.exe')),project=path.resolve(options.project),caseFile=path.resolve(options.case);
const specification=JSON.parse(fs.readFileSync(caseFile,'utf8'));
const pure=/^[A-Za-z_]\w*(?:(?:\.|->|::)[A-Za-z_]\w*|\[\d+\])*$/;
assert(pure.test(specification.probe),'Independent probe must be a variable/member with literal indices');
assert(['watch','locals'].includes(specification.target?.pane));
assert(specification.target.kind==='variable'&&pure.test(specification.target.expression));
assert(Number.isSafeInteger(specification.frame)&&specification.frame>=0&&specification.frame_function);
assert(typeof specification.little_endian==='boolean'&&specification.owner&&specification.scope);
assert(Array.isArray(specification.sentinels)&&specification.sentinels.length>=2&&specification.sentinels.every(e=>pure.test(e)),'At least two independent neighbour symbols required');
if(options['special-floats'])assert(Array.isArray(specification.special_inputs)&&specification.special_inputs.length>=4&&specification.special_inputs.every(s=>s.input&&/^0x[0-9a-f]+$/.test(s.expected_raw)&&s.label),'Special float cases require labelled inputs and exact expected raw bits');
const projectHash=hash(project);
suite.metadata={...suite.metadata,binary,binary_sha256:hash(binary),project,project_sha256:projectHash,case_file:caseFile,case_sha256:hash(caseFile),core:options.core};
let session,metadata,original,sentinels;
const context=async()=>(await session.command('registers_list')).context;
const preview=async input=>session.command('write_preview',{target:specification.target,selection:{kind:'register'},input,context:await context()});
const apply=async draft=>{const r=await session.command('write_apply',{draft:draft.draft});assert.equal(r.outcome,'verified',JSON.stringify(r));assert.equal(r.owner,specification.owner);return r;};
const evaluate=async expression=>(await session.command('evaluate',{expression})).value;
const neighbours=async()=>Promise.all(specification.sentinels.map(e=>evaluate(e)));
const rawHex=bytes=>'0x'+(specification.little_endian?[...bytes].reverse():bytes).map(b=>b.toString(16).padStart(2,'0')).join('');
const read=async()=>{
  const address=await evaluate(`(unsigned long long)&(${specification.probe})`);
  assert.equal(BigInt(address.split(' ')[0]),BigInt(metadata.address));
  assert.equal(Number(await evaluate(`sizeof(${specification.probe})`))*8,metadata.scalar.bits);
  const r=await session.command('memory_dump',{address:metadata.address,count:metadata.scalar.bits/8,channel:'',context:await context()});
  assert.equal(r.address,metadata.address);assert.equal(r.bytes.length,metadata.scalar.bits/8);return r.bytes;
};
const restoreInput=()=>({kind:'bytes',text:original.map(b=>b.toString(16).padStart(2,'0')).join(' '),little_endian:specification.little_endian});
(async()=>{
  try{
    session=new Session(binary,project,out);await session.command('connect');
    if(options.core!=='default')await session.command('select_core',{name:options.core});
    if(!await suite.test(ids[0],'Already halted dedicated fixture, exact typed address/owner and neighbour symbols',async()=>{
      const initial=await session.command('status');assert.equal(initial.state,'STOPPED');assert.equal(initial.frame.function,options['fixture-function']);assert((initial.cores||[]).every(c=>c.state==='STOPPED'));
      await session.command('frame',{level:specification.frame});const selected=await session.command('status');assert.equal(selected.frame.function,specification.frame_function);
      const draft=await preview(specification.input);metadata=draft.metadata;
      assert.equal(metadata.expression,specification.path_expression||specification.probe);assert(metadata.address,'Register-resident variable needs a separate independent-register case');
      assert.equal(draft.owner,specification.owner);assert.equal(draft.scope,specification.scope);
      await session.command('write_cancel',{draft:draft.draft});original=await read();sentinels=await neighbours();
      // Prove restoration is representable before changing the fixture.
      const restore=await preview(restoreInput());assert.equal(restore.plan.value.hex,rawHex(original));await session.command('write_cancel',{draft:restore.draft});
      return {initial,selected,metadata,original,sentinels};
    }))return;
    if(!await suite.test(ids[1],'Preview/cancel sends no assignment and preserves bytes/neighbours',async()=>{
      const draft=await preview(specification.input);await session.command('write_cancel',{draft:draft.draft});assert.equal((await session.command('write_apply',{draft:draft.draft})).outcome,'not_sent');
      assert.deepEqual(await read(),original);assert.deepEqual(await neighbours(),sentinels);assert(!session.logs('mi>').some(l=>l.text.includes('-var-assign ')));
    }))return;
    if(!await suite.test(ids[2],'One typed assignment independently verified in RAM; no replay',async()=>{
      const draft=await preview(specification.input),result=await apply(draft);assert.equal(rawHex(await read()),draft.plan.value.hex);assert.deepEqual(await neighbours(),sentinels);
      assert.equal((await session.command('write_apply',{draft:draft.draft})).outcome,'not_sent');return result;
    }))return;
    if(options['special-floats']&&!await suite.test(special,'Signed Infinity and quiet/signaling NaN payloads, independent bytes and neighbouring symbols',async()=>{
      assert.equal(metadata.scalar.float,true);assert([32,64].includes(metadata.scalar.bits));const evidence=[];
      for(const sample of specification.special_inputs){
        assert.equal(sample.expected_raw.length,2+metadata.scalar.bits/4);
        const before=await read(),cancelled=await preview(sample.input);assert.equal(cancelled.plan.value.hex,sample.expected_raw);
        assert.deepEqual(await read(),before);await session.command('write_cancel',{draft:cancelled.draft});
        assert.equal((await session.command('write_apply',{draft:cancelled.draft})).outcome,'not_sent');assert.deepEqual(await read(),before);
        const count=session.logs('mi>').filter(l=>l.text.includes('-var-assign ')).length;
        const draft=await preview(sample.input),result=await apply(draft);assert.equal(result.observed.hex,sample.expected_raw);assert.equal(rawHex(await read()),sample.expected_raw);assert.deepEqual(await neighbours(),sentinels);
        assert.equal(session.logs('mi>').filter(l=>l.text.includes('-var-assign ')).length,count+1);
        assert.equal((await session.command('write_apply',{draft:draft.draft})).outcome,'not_sent');evidence.push({label:sample.label,result});
      }
      assert(!session.logs('mi>').some(l=>l.text.includes('-data-write-memory')));
      return evidence;
    }))return;
    await suite.test(ids[3],'Explicit restoration after verified successful test only',async()=>{const result=await apply(await preview(restoreInput()));assert.deepEqual(await read(),original);assert.deepEqual(await neighbours(),sentinels);return result;});
  }catch(error){await suite.test('WRITE-H01-VAR-SETUP','Initialize dedicated variable test',async()=>{throw error;});}
  finally{
    if(session)await suite.test('WRITE-H01-VAR-CLEANUP','Close own session and preserve original project',async()=>{await session.close();assert.equal(hash(project),projectHash);});
    for(const id of ids)if(!suite.results.some(r=>r.id===id))suite.skip(id,'Deferred variable phase','Earlier precondition/write failed; no automatic rollback or retry');suite.finish();
  }
})();
