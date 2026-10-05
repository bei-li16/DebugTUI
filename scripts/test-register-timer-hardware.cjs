// Deferred REG-H05: explicit paused R52 fixture, genuine 64-bit reads; no timer writes.
const fs = require('node:fs'), path = require('node:path'), assert = require('node:assert/strict');
const {root, hash, outputDirectory, parseOptions, Cases, Session} = require('./test-support/session.cjs');
const options = parseOptions(process.argv.slice(2), ['--binary', '--project', '--core', '--case'], ['--run', '--software-fixture']);
const phases = ['REG-H05-STOP', 'REG-H05-PROBE', 'REG-H05-WIDTH', 'REG-H05-COUNTERS', 'REG-H05-UNCHANGED'];
const out = outputDirectory('register-timer-hardware');
const suite = new Cases(out, {todo:'REG-H05 MRRC, high word, monotonic counters and selected-core ownership',
  layer:options['software-fixture'] ? 'software fixture; no board' : options.run ? 'explicit paused timer fixture' : 'deferred; no target access',
  board_tests_executed:!!options.run && !options['software-fixture']});
if (!options.run) {
  for (const id of phases) suite.skip(id, '64-bit Timer backend validation', 'Requires --run and independent firmware baseline at a dedicated paused fixture');
  suite.finish(); process.exit(0);
}
assert(options.project && options.core && options.case, '--run requires --project FILE --core NAME --case JSON');
const binary = path.resolve(options.binary || path.join(root, 'target/release/debugtui.exe'));
const project = path.resolve(options.project), caseFile = path.resolve(options.case);
for (const file of [binary, project, caseFile]) assert(fs.existsSync(file), `Missing ${file}`);
const spec = JSON.parse(fs.readFileSync(caseFile));
assert(!spec.software_example || options['software-fixture'], 'Replace software expectations before physical execution');
assert(spec.frame_function && spec.expected_midr && spec.evidence_source, 'Declare stop hook, MIDR and independent evidence');
assert(Array.isArray(spec.stable_registers) && spec.stable_registers.includes('cpsr'), 'Declare stable controls and CPSR');
assert(Array.isArray(spec.stable64) && spec.stable64.length, 'Declare a known stable 64-bit Timer register');
assert(Array.isArray(spec.counters) && spec.counters.length, 'Declare independent counter baselines');
const raw = (text, bits) => { assert(new RegExp(`^0x[0-9a-f]{${bits/4}}$`, 'i').test(text), `Exact ${bits}-bit raw required: ${text}`); return BigInt(text); };
const reference = value => assert(/^[a-zA-Z_]\w*(\[\d+\])?$/.test(value), 'Reference must be one firmware variable/array element');
if (spec.ready) reference(spec.ready);
for (const entry of spec.stable64) {
  assert(['cntp_cval','cntv_cval','cnthp_cval','cntvoff'].includes(entry.id)); raw(entry.expected,64);
  if (entry.reference) reference(entry.reference);
}
assert(spec.stable64.some(e => raw(e.expected,64) >> 32n), 'At least one stable expected high word must be nonzero');
for (const entry of spec.counters) {
  assert(['cntpct','cntvct'].includes(entry.id));
  reference(entry.reference);
  assert(BigInt(entry.max_delta_ticks) > 0n && BigInt(entry.max_delta_ticks) < (1n<<63n));
}
const projectHash = hash(project);
Object.assign(suite.metadata, {binary, binary_sha256:hash(binary), project, project_sha256:projectHash,
  case_file:caseFile, case_sha256:hash(caseFile), evidence_source:spec.evidence_source,
  physical_scratch_check:'Pinned adapter reads back R0/R1 internally; GDB guard samples are logical register views'});
const timerIds = new Set(['cntfrq','cntkctl','cntp_tval','cntp_ctl','cntv_tval','cntv_ctl',
  'cnthctl','cnthp_tval','cnthp_ctl','cntpct','cntvct','cntp_cval','cntv_cval','cntvoff','cnthp_cval']);
const timerEvidence = [];
Object.assign(suite.metadata, {requires_timer_adapter:!!spec.require_timer_adapter, timer_evidence:timerEvidence});
let session, context, before, peerBefore;
const read = async (ids, bits) => {
  const current = (await session.command('registers_list')).context;
  const response = await session.command('registers_read', {context:current, ids, manual:true});
  return Object.fromEntries(ids.map(id => {
    const sample = response.samples.find(s=>s.id===id);
    assert.equal(sample?.state, 'valid', `Fresh ${id}: ${sample?.detail}`);
    assert.equal(sample.owner, `core:${current.core}`); raw(sample.value.hex, bits);
    assert.equal(sample.value.bits, bits);
    if (spec.require_timer_adapter && timerIds.has(id)) {
      assert.equal(sample.source, 'openocd:aarch64 timer', 'Use the independent checked Timer adapter');
      assert.equal(sample.view, 'physical_core');
      const evidence = sample.provenance?.access?.timer;
      for (const field of ['midr','dscr','dspsr','dlr']) {
        assert.equal(evidence?.[field]?.bits,32,`Missing physical Timer ${field}`);
        raw(evidence[field].hex,32);
      }
      assert.equal(evidence.midr.hex,spec.expected_midr);
      assert.equal((raw(evidence.dscr.hex,32)>>8n)&3n,2n,'Dedicated baseline requires current Debug EL2 evidence');
      assert.equal(raw(evidence.dspsr.hex,32)&31n,26n,'Dedicated firmware baseline stopped in Hyp');
      timerEvidence.push({id,core:current.core,value:sample.value,evidence});
    } else if (bits === 64) assert.equal(sample.source, 'openocd:aarch64 mrrc', 'Use the pinned genuine MRRC adapter');
    return [id,sample.value.hex];
  }));
};
const halted = async () => { const s=await session.command('status'); assert.equal(s.state,'STOPPED'); assert.equal(s.frame.level,0); assert.equal(s.frame.function,spec.frame_function); };
const select = async core => { if (core !== 'default' || spec.peer_core) await session.command('select_core',{name:core}); await halted(); };
// This fixture captures normal-execution Hyp-only firmware baselines. It is
// not a generic Debug-state permission decision for EL0/EL1 or EDSCR.HDD.
const hyp = async () => assert.equal(raw((await read(['cpsr'],32)).cpsr,32)&31n,26n, 'Dedicated firmware baseline requires Hyp; no mode change performed');
const firmware = async symbol => {
  const result = await session.command('evaluate',{expression:`(unsigned long long)${symbol}`});
  assert(/^(0x[0-9a-f]+|\d+)$/i.test(result.value), 'Firmware sample must be an exact unsigned integer');
  const value = BigInt(result.value); assert(value>=0n&&value<(1n<<64n)); return value;
};
(async () => {
  try {
    session = new Session(binary,project,out); await session.command('connect');
    if (spec.control_scope) { assert(['core','all'].includes(spec.control_scope)); await session.command('control_scope',{scope:spec.control_scope}); }
    if (!await suite.test(phases[0],'Capture current-core controls and optional peer',async()=>{
      await select(options.core); await hyp();
      if (spec.ready) assert.equal(await firmware(spec.ready),1n,'Independent firmware baseline is not ready');
      if (spec.peer_core) { assert.notEqual(spec.peer_core,options.core); await select(spec.peer_core); await hyp(); peerBefore=await read(spec.stable_registers,32); }
      await select(options.core); context=(await session.command('registers_list')).context;
      assert.equal(context.core,options.core); await hyp(); before=await read(spec.stable_registers,32);
      return {context,before,peerBefore};
    })) return;
    if (!await suite.test(phases[1],'Observe actual R52 identity on this core',async()=>{
      const probe=await session.command('registers_probe',{context});
      assert.equal(probe.probe.identity.model,'Cortex-R52');
      assert.equal(probe.probe.samples.find(s=>s.id==='midr').value.hex,spec.expected_midr);
      return probe;
    })) return;
    if (!await suite.test(phases[2],'Independently validate complete 64-bit stable values and high words',async()=>{
      const values=await read(spec.stable64.map(e=>e.id),64);
      const references={};
      for (const entry of spec.stable64) {
        if(entry.reference) {
          const baseline=await firmware(entry.reference);
          assert.equal(baseline,raw(entry.expected,64),'Independent firmware baseline differs from declared expected value');
          assert.equal(raw(values[entry.id],64),baseline);
          references[entry.id]=`0x${baseline.toString(16).padStart(16,'0')}`;
        }
        assert.equal(raw(values[entry.id],64),raw(entry.expected,64));
        assert.equal(raw(values[entry.id],64)>>32n,raw(entry.expected,64)>>32n);
      }
      return {values,references};
    })) return;
    if (!await suite.test(phases[3],'Bound counter deltas against firmware samples and a second read',async()=>{
      const baseline={};
      for (const entry of spec.counters) {
        baseline[entry.id]=await firmware(entry.reference);
      }
      const ids=spec.counters.map(e=>e.id), first=await read(ids,64), second=await read(ids,64);
      const delta=(newer,older)=>(newer-older+(1n<<64n))% (1n<<64n);
      for (const entry of spec.counters) {
        const a=raw(first[entry.id],64), b=raw(second[entry.id],64), max=BigInt(entry.max_delta_ticks);
        assert(delta(a,baseline[entry.id])<=max); assert(delta(b,baseline[entry.id])<=max);
        assert(delta(b,a)<=max, 'Counter regression or implausible interval');
        if (entry.require_progress) assert(delta(b,a)>0n, 'Counter did not progress; record any debug freeze condition');
      }
      return {baseline:Object.fromEntries(Object.entries(baseline).map(([id,v])=>[id,`0x${v.toString(16).padStart(16,'0')}`])),first,second};
    })) return;
    await suite.test(phases[4],'Verify controls, context, peer and customer configuration unchanged',async()=>{
      await select(options.core); assert.deepEqual((await session.command('registers_list')).context,context);
      assert.deepEqual(await read(spec.stable_registers,32),before);
      if(spec.peer_core){await select(spec.peer_core);assert.deepEqual(await read(spec.stable_registers,32),peerBefore);await select(options.core);}
      assert.equal(hash(project),projectHash);return {controls_unchanged:true,peer_unchanged:!!peerBefore};
    });
  } catch(error) { await suite.test('REG-H05-DRIVER','Driver failure',async()=>{throw error;}); }
  finally { if(session) await suite.test('REG-H05-CLEANUP','Close the owned debugger session',()=>session.close()); assert.equal(hash(project),projectHash); suite.finish(); }
})();
