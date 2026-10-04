// Deferred WRITE-H04 RAM case. Default mode makes a report without target access.
const fs = require('node:fs'), path = require('node:path'), assert = require('node:assert/strict');
const {root, hash, outputDirectory, parseOptions, Cases, Session} = require('./test-support/session.cjs');
const options = parseOptions(process.argv.slice(2), ['--binary','--project','--core','--address','--bytes','--channel','--fixture-function'], ['--run','--software-fixture']);
const out = outputDirectory('memory-write-hardware');
const ids = ['WRITE-H04-RAM-PRECONDITION','WRITE-H04-RAM-CANCEL','WRITE-H04-RAM-APPLY','WRITE-H04-RAM-RESTORE'];
const suite = new Cases(out, {layer: options['software-fixture'] ? 'local software fixture; no board' : options.run ? 'physical board, dedicated RAM fixture' : 'deferred case preparation; no target access',
  todo:'WRITE-H04 RAM range subset; Flash, MMIO and cache-coherence cases remain separate', board_tests_executed:!!options.run && !options['software-fixture']});
if (!options.run) {
  for (const id of ids) suite.skip(id,'RAM byte write, cancellation, adjacent sentinels and explicit restoration','Prepared only; --run requires a dedicated halted RAM fixture');
  suite.finish(); process.exit(0);
}
assert(options.project && options.core && options.address && options.bytes && options['fixture-function'], '--run needs --project --core --address --bytes --fixture-function');
assert(/^0x[0-9a-fA-F]{1,16}$/.test(options.address),'Use a literal address');
assert(/^(?:[0-9a-fA-F]{2}){1,4096}$/.test(options.bytes),'Use 1..4096 address-order hexadecimal byte pairs');
const address=BigInt(options.address), count=options.bytes.length/2, limit=address+BigInt(count);
assert(address>0n && limit<0xffffffffffffffffn,'Fixture needs a readable sentinel on both sides');
const bytes=Array.from({length:count},(_,i)=>parseInt(options.bytes.slice(i*2,i*2+2),16));
const binary=path.resolve(options.binary || path.join(root,'target/release/debugtui.exe')), project=path.resolve(options.project);
for (const file of [binary,project]) assert(fs.existsSync(file),`Missing input: ${file}`);
suite.metadata={...suite.metadata,binary,binary_sha256:hash(binary),project,project_sha256:hash(project),core:options.core,address:options.address,count,channel:options.channel||'',fixture_function:options['fixture-function']};
const projectHash=hash(project), hex=n=>`0x${n.toString(16)}`;
let session, original, sentinels;
const read=async(base,size)=>{
  const context=(await session.command('registers_list')).context;
  const result=await session.command('memory_dump',{address:hex(base),count:size,channel:options.channel||'',context});
  assert.equal(result.address,hex(base));assert.equal(result.bytes.length,size);return result.bytes;
};
const neighbours=async()=>[...(await read(address-1n,1)),...(await read(limit,1))];
const preview=async text=>session.command('write_preview',{target:{kind:'memory',address:hex(address),bits:count*8,channel:options.channel||''},selection:{kind:'register'},input:{kind:'bytes',text},context:(await session.command('registers_list')).context});
const apply=async draft=>{
  const result=await session.command('write_apply',{draft:draft.draft});assert.equal(result.outcome,'verified',JSON.stringify(result));assert.equal(result.atomic,false);return result;
};
(async()=>{
  try {
    session=new Session(binary,project,out);await session.command('connect');
    if(options.core!=='default')await session.command('select_core',{name:options.core});
    if(!await suite.test(ids[0],'Dedicated halted RAM fixture and readable adjacent sentinels',async()=>{
      const status=await session.command('status');assert.equal(status.state,'STOPPED');assert.equal(status.frame.function,options['fixture-function']);
      assert((status.cores||[]).every(c=>c.state==='STOPPED'),'All related cores must already be halted; driver never pauses/runs/resets');
      original=await read(address,count);sentinels=await neighbours();return {status,original,sentinels};
    }))return;
    if(!await suite.test(ids[1],'Preview/cancel preserves the RAM range and both sentinels',async()=>{
      const draft=await preview(options.bytes);const cancelled=await session.command('write_cancel',{draft:draft.draft});assert.equal(cancelled.cancelled,true);
      assert.equal((await session.command('write_apply',{draft:draft.draft})).outcome,'not_sent');assert.deepEqual(await read(address,count),original);assert.deepEqual(await neighbours(),sentinels);
      assert(!session.logs('mi>').some(l=>l.text.includes('-data-write-memory-bytes')));assert.equal(session.logs('write').length,0);return {cancelled};
    }))return;
    if(!await suite.test(ids[2],'Apply once and independently read RAM plus adjacent sentinels',async()=>{
      const draft=await preview(options.bytes), result=await apply(draft);assert.deepEqual(await read(address,count),bytes);assert.deepEqual(await neighbours(),sentinels);
      assert.equal((await session.command('write_apply',{draft:draft.draft})).outcome,'not_sent');return {result,sentinels};
    }))return;
    await suite.test(ids[3],'Explicitly restore ordinary RAM only after verified successful test',async()=>{
      const result=await apply(await preview(original.map(b=>b.toString(16).padStart(2,'0')).join('')));assert.deepEqual(await read(address,count),original);assert.deepEqual(await neighbours(),sentinels);return {result};
    });
  } catch(error) {await suite.test('WRITE-H04-RAM-SETUP','Initialize the dedicated test session',async()=>{throw error;});}
  finally {
    // Unknown, partial or mismatched results never trigger an automatic retry or rollback.
    if(session)await suite.test('WRITE-H04-RAM-CLEANUP','Close this test session',()=>session.close());
    if(hash(project)!==projectHash)await suite.test('WRITE-H04-RAM-CONFIG','Original project unchanged',async()=>{throw Error('Project modified');});
    for(const id of ids)if(!suite.results.some(c=>c.id===id))suite.skip(id,'Deferred fixture phase','Earlier precondition or write failed');suite.finish();
  }
})();
