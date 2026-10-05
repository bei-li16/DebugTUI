// Deferred WRITE-H03 host path. Default invocation never connects a target.
const fs = require('node:fs'), path = require('node:path'), assert = require('node:assert/strict');
const {root, hash, outputDirectory, parseOptions, Cases, Session} = require('./test-support/session.cjs');
const options = parseOptions(process.argv.slice(2), ['--binary','--project','--core','--case'], ['--run','--software-fixture']);
const phases = ['WRITE-H03-HOST-STOP','WRITE-H03-HOST-CANCEL','WRITE-H03-HOST-APPLY','WRITE-H03-HOST-RESTORE'];
const out = outputDirectory('register-vfp-host-write');
const suite = new Cases(out, {todo:'WRITE-H03 raw S/D/Q host drafts, owner, neighbours and explicit verified restoration',
  layer: options['software-fixture'] ? 'real binary with software target; no board' : options.run ? 'explicit paused VFP fixture' : 'deferred; no target access',
  board_tests_executed: !!options.run && !options['software-fixture']});
if (!options.run) {
  for (const id of phases) suite.skip(id, 'Dedicated physical FP storage writer', 'Requires --run and independently sampled per-core firmware');
  suite.finish(); process.exit(0);
}
assert(options.project && options.core && options.case, '--run requires --project FILE --core NAME --case JSON');
const binary = path.resolve(options.binary || path.join(root,'target/release/debugtui.exe'));
const project = path.resolve(options.project), caseFile = path.resolve(options.case);
for (const file of [binary,project,caseFile]) assert(fs.existsSync(file), `Missing ${file}`);
const spec = JSON.parse(fs.readFileSync(caseFile));
assert(!spec.software_example || options['software-fixture'], 'Replace software expectations before physical execution');
assert(spec.fixture_core_private === true && spec.restore_on_verified === true && spec.evidence_source && spec.frame_function, 'Dedicated fixture and explicit verified restore required');
assert(/^[A-Za-z_]\w*$/.test(spec.reference), 'Declare the independent firmware ready symbol prefix');
const match = /^([sdq])(0|[1-9][0-9]?)$/.exec(spec.register);
assert(match, 'Only canonical raw S/D/Q storage is supported');
const [,kind,indexText] = match, index = Number(indexText), bits = kind === 's' ? 32 : kind === 'd' ? 64 : 128;
assert(index < (kind === 'q' ? 16 : 32));
const pair = kind === 's' ? Math.floor(index/4) : kind === 'd' ? Math.floor(index/2) : index;
const shift = BigInt(kind === 's' ? (index%4)*32 : kind === 'd' ? (index%2)*64 : 0);
const full = (1n<<128n)-1n, mask = ((1n<<BigInt(bits))-1n)<<shift;
const exact = (text,width) => {assert(new RegExp(`^0x[0-9a-f]{${width/4}}$`,'i').test(text), `Exact ${width}-bit bits required: ${text}`); return BigInt(text);};
const hex = (raw,width) => `0x${raw.toString(16).padStart(width/4,'0')}`;
const originalPair = exact(spec.expected_before,128), peerPair = exact(spec.expected_peer,128), command = exact(spec.raw,bits);
const expected = (originalPair & (full^mask)) | (command<<shift);
assert(spec.peer_core && spec.peer_core !== options.core && spec.target && spec.peer_target, 'Declare distinct physical core and peer target');
const projectHash = hash(project);
Object.assign(suite.metadata,{binary,binary_sha256:hash(binary),project,project_sha256:projectHash,case_file:caseFile,
  case_sha256:hash(caseFile),evidence_source:spec.evidence_source,register:spec.register,
  physical_check:'Initial pair comes from independent firmware. Subsequent adapter reads are separate from writer receipts; physical GPR/PC and resumed VMOV evidence remain additional board cases.'});
const stableIds = ['r0','r1','pc','cpsr','hcptr','fpsid','mvfr0','mvfr1','mvfr2','fpexc','fpscr'];
let session, baseline, peerBaseline, neon;
const select = async core => {
  await session.command('select_core',{name:core});
  const status = await session.command('status');
  assert.equal(status.state,'STOPPED'); assert.equal(status.frame.level,0);
  assert.equal(status.frame.function,spec.frame_function);
  assert((status.cores||[]).every(c=>c.state==='STOPPED'));
  const context = (await session.command('registers_list')).context;
  assert.equal(context.core,core); return context;
};
const read = async ids => {
  const context = (await session.command('registers_list')).context;
  const result = await session.command('registers_read',{context,ids,manual:true});
  return Object.fromEntries(ids.map(id=>{
    const sample = result.samples.find(s=>s.id===id);
    assert.equal(sample?.state,'valid',`${id}: ${sample?.detail}`);
    assert.equal(sample.owner,`core:${context.core}`); assert.deepEqual(sample.context,context);
    return [id,sample.value];
  }));
};
const storage = async expectedPair => {
  const ids = [`d${pair*2}`,`d${pair*2+1}`];
  if (pair<8) ids.push(...Array.from({length:4},(_,n)=>`s${pair*4+n}`));
  if (neon) ids.push(`q${pair}`);
  const values = await read(ids);
  assert.equal(exact(values[ids[0]].hex,64) | (exact(values[ids[1]].hex,64)<<64n),expectedPair);
  for (const id of ids) {
    const n = Number(id.slice(1)), width = id[0]==='s'?32:id[0]==='d'?64:128;
    const offset = id[0]==='s'?BigInt((n%4)*32):id[0]==='d'?BigInt((n%2)*64):0n;
    assert.equal(exact(values[id].hex,width),(expectedPair>>offset)&((1n<<BigInt(width))-1n),id);
  }
  return values;
};
const independentInitial = async (reference,expectedPair) => {
  assert(/^[A-Za-z_]\w*$/.test(reference));
  const ready = await session.command('evaluate',{expression:`*(unsigned int *)&${reference}_ready`});
  assert.equal(BigInt(ready.value),1n);
  let observed = 0n;
  for(let word=0;word<4;word++) {
    const result = await session.command('evaluate',{expression:`((unsigned int *)&${reference})[${8+pair*4+word}]`});
    assert(/^(0x[0-9a-f]+|\d+)$/i.test(result.value));
    const value = BigInt(result.value); assert(value>=0n && value<(1n<<32n));
    observed |= value<<BigInt(word*32);
  }
  assert.equal(observed,expectedPair,'Independent per-core VMOV firmware words must match the declared baseline');
};
const preview = async raw => {
  const context = await select(options.core);
  const draft = await session.command('write_preview',{context,target:{kind:'register',id:spec.register},selection:{kind:'register'},input:{kind:'unsigned',text:raw}});
  assert.equal(draft.owner,options.core); assert.equal(draft.target_name,spec.target);
  assert.equal(draft.channel,'register_tcl'); assert.equal(draft.atomic,false);
  assert.equal(draft.plan.value.bits,bits); return draft;
};
const apply = async draft => {
  const result = await session.command('write_apply',{draft:draft.draft});
  assert.equal(result.outcome,'verified',JSON.stringify(result));
  assert.equal(result.owner,options.core); assert.equal(result.target_name,spec.target);
  assert.equal(result.neighbours_preserved,true); return result;
};
const unchanged = async () => {
  await select(options.core); assert.deepEqual(await read(stableIds),baseline);
  await select(spec.peer_core); assert.deepEqual(await read(stableIds),peerBaseline); await storage(peerPair);
  await select(options.core); assert.equal(hash(project),projectHash);
  assert(!session.logs('mi>').some(log=>/-exec-|\b-data-write-|\b-var-assign/.test(log.text)));
};
(async()=>{
  try {
    session = new Session(binary,project,out); await session.command('connect');
    await session.command('control_scope',{scope:spec.control_scope||'core'});
    if (!await suite.test(phases[0],'Confirm halted owner, independent initial pair, FP controls and peer',async()=>{
      await select(options.core); baseline = await read(stableIds);
      assert.equal(exact(baseline.cpsr.hex,32)&31n,26n); assert.equal(exact(baseline.hcptr.hex,32)&(1n<<10n),0n);
      assert.equal(baseline.mvfr0.hex,spec.expected_mvfr0); assert.equal(baseline.mvfr1.hex,spec.expected_mvfr1);
      assert((exact(baseline.fpexc.hex,32)&(1n<<30n))!==0n);
      neon = (exact(baseline.mvfr0.hex,32)&15n)===2n;
      const probe = await session.command('registers_probe',{context:(await session.command('registers_list')).context});
      assert.equal(probe.probe.identity.model,'Cortex-R52');
      assert.equal(probe.probe.samples.find(s=>s.id==='midr').value.hex,spec.expected_midr);
      await independentInitial(spec.reference,originalPair);
      await storage(originalPair);
      await select(spec.peer_core); peerBaseline = await read(stableIds);
      await independentInitial(spec.peer_reference||spec.reference,peerPair); await storage(peerPair);
      const peerDraft = await session.command('write_preview',{context:(await session.command('registers_list')).context,
        target:{kind:'register',id:spec.register},selection:{kind:'register'},input:{kind:'unsigned',text:hex((peerPair>>shift)&((1n<<BigInt(bits))-1n),bits)}});
      assert.equal(peerDraft.owner,spec.peer_core); assert.equal(peerDraft.target_name,spec.peer_target);
      assert.equal((await session.command('write_cancel',{draft:peerDraft.draft})).cancelled,true);
      await select(options.core); return {original:hex(originalPair,128),peer:hex(peerPair,128),baseline,peerBaseline};
    })) return;
    if (!await suite.test(phases[1],'Cancel and replay cancelled token without changing storage',async()=>{
      const draft = await preview(spec.raw);
      assert.equal((await session.command('write_cancel',{draft:draft.draft})).cancelled,true);
      assert.equal((await session.command('write_apply',{draft:draft.draft})).outcome,'not_sent');
      await storage(originalPair); await unchanged(); return {cancelled:draft.draft};
    })) return;
    if (!await suite.test(phases[2],'Apply once, refresh every alias and verify fresh siblings and peer',async()=>{
      const draft = await preview(spec.raw), result = await apply(draft);
      assert.equal(exact(result.physical_pair_before.hex,128),originalPair);
      assert.equal(exact(result.physical_pair_expected.hex,128),expected);
      assert.equal(exact(result.physical_pair_observed.hex,128),expected);
      assert.equal((await session.command('write_apply',{draft:draft.draft})).outcome,'not_sent');
      await storage(expected); await unchanged(); return result;
    })) return;
    await suite.test(phases[3],'Explicitly restore only after verified writes and independent reads',async()=>{
      const result = await apply(await preview(hex((originalPair>>shift)&((1n<<BigInt(bits))-1n),bits)));
      await storage(originalPair); await unchanged(); return result;
    });
  } catch(error) {await suite.test('WRITE-H03-HOST-DRIVER','Driver failure',async()=>{throw error;});}
  finally {
    if(session) await suite.test('WRITE-H03-HOST-CLEANUP','Close this debugger session without target control',()=>session.close());
    if(hash(project)!==projectHash) await suite.test('WRITE-H03-HOST-CONFIG','Project hash unchanged',async()=>{throw Error('Project changed');});
    for(const id of phases) if(!suite.results.some(item=>item.id===id)) suite.skip(id,'Deferred phase','Earlier precondition or write verification failed; no retry or rollback');
    suite.finish();
  }
})();
