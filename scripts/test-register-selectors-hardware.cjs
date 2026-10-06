// Deferred REG-H04/REG-H06 selector subset; no target is opened without --run.
const fs = require('node:fs'), path = require('node:path'), assert = require('node:assert/strict');
const {root, hash, outputDirectory, parseOptions, Cases, Session} = require('./test-support/session.cjs');
const options = parseOptions(process.argv.slice(2), ['--binary', '--project', '--core', '--case'], ['--run', '--software-fixture']);
const ids = ['REG-H04-SEL-PRECONDITION', 'REG-H04-SEL-PROBE', 'REG-H04-SEL-READ', 'REG-H04-SEL-UNCHANGED'];
const out = outputDirectory('register-selectors-hardware');
const suite = new Cases(out, {
  layer: options['software-fixture'] ? 'actual binary with MI and real Tcl target model; no board' : options.run ? 'explicit paused physical fixture' : 'deferred cases; no target access',
  todo: 'REG-H04/REG-H06 selector subset; no MPU/control/event configuration writes',
  board_tests_executed: !!options.run && !options['software-fixture'], core: options.core || null,
});
if (!options.run) {
  for (const id of ids) suite.skip(id, 'Read one saved/restored physical-core selector bank', 'Requires --run with a declared paused fixture and matching verified selector backend');
  suite.finish(); process.exit(0);
}
assert(options.project && options.core && options.case, '--run requires --project FILE --core NAME --case JSON');
const binary = path.resolve(options.binary || path.join(root, 'target/release/debugtui.exe'));
const project = path.resolve(options.project), caseFile = path.resolve(options.case);
for (const file of [binary, project, caseFile]) assert(fs.existsSync(file), `Missing input: ${file}`);
const spec = JSON.parse(fs.readFileSync(caseFile));
assert(!spec.software_example || options['software-fixture'], 'Replace the software example with an independently verified board baseline');
assert(typeof spec.frame_function === 'string' && spec.frame_function, 'Declare a dedicated paused fixture function');
assert(spec.expected_midr && Array.isArray(spec.requests) && spec.requests.length, 'Declare known MIDR and selector requests');
const exact = raw => { assert(/^0x[0-9a-f]{8}$/i.test(raw), `Not an exact 32-bit raw value: ${raw}`); return BigInt(raw); };
const selection = {mpu_el1:['prselr', 'mpu.el1.regions', 'prbar', 'prlar'], mpu_el2:['hprselr', 'mpu.el2.regions', 'hprbar', 'hprlar'], pmu:['pmselr', 'pmu.counters', 'pmevtyper', 'pmevcntr']};
for (const request of spec.requests) {
  assert(selection[request.kind] && Number.isInteger(request.index) && request.index >= 0 && request.index < 24, 'Unknown selector/index');
  assert(Array.isArray(request.expected) && request.expected.length === 2, 'Declare an independently known pair for every request');
  request.expected.forEach(exact);
}
assert(Array.isArray(spec.stable_registers) && (spec.current_debug ? ['cpsr'] : ['cpsr', 'pmcr']).every(id=>spec.stable_registers.includes(id)), 'Declare stable controls, including CPSR and PMCR when using the legacy selector');
assert(spec.stable_registers.includes('sctlr') || spec.stable_registers.includes('hsctlr'), 'Include the controlling SCTLR or HSCTLR');
if (spec.current_debug) {
  assert(spec.current_debug.endpoint, 'Declare the native Tcl endpoint');
  for (const core of [options.core, spec.peer_core].filter(Boolean)) {
    assert(spec.current_debug.targets?.[core], `Declare target for ${core}`);
    assert(Number.isInteger(spec.current_debug.ap?.[core]) && spec.current_debug.ap[core]>=0, `Declare AP for ${core}`);
    exact(spec.current_debug.saved_dspsr?.[core]);
  }
  for (const request of spec.requests) { assert(request.kind!=='pmu', 'Native selector scope is MPU only'); exact(request.expected_capacity); }
}
const projectHash = hash(project);
Object.assign(suite.metadata, {binary, binary_sha256:hash(binary), project, project_sha256:projectHash, case_file:caseFile, case_sha256:hash(caseFile), peer_core:spec.peer_core || null});
let session, context, before, probe, peerBefore;
const selectedIds = [...new Set(spec.requests.map(r=>selection[r.kind][0]))];
const controlIds = [...new Set([...spec.stable_registers, ...selectedIds])];
const halted = async () => {
  const status = await session.command('status');
  assert.equal(status.state, 'STOPPED'); assert.equal(status.frame.level, 0);
  assert.equal(status.frame.function, spec.frame_function, 'Stop at the fixture first; the driver never pauses/runs/resets/downloads');
};
const read = async ids => {
  const listed = await session.command('registers_list');
  const response = await session.command('registers_read', {ids, context:listed.context, manual:true});
  const result = {};
  for (const id of ids) {
    const sample = response.samples.find(s=>s.id===id);
    assert.equal(sample?.state, 'valid', `Fresh ${id} required for independent evidence`);
    assert.equal(sample.owner, `core:${listed.context.core}`); result[id] = sample.value.hex; exact(result[id]);
  }
  return result;
};
const select = async core => { if (core !== 'default' || spec.peer_core) await session.command('select_core', {name:core}); await halted(); };
(async () => {
  try {
    session = new Session(binary, project, out); await session.command('connect');
    if (spec.control_scope) {
      assert(['core', 'all'].includes(spec.control_scope), 'Declare core/all scope');
      await session.command('control_scope', {scope:spec.control_scope});
    }
    if (!await suite.test(ids[0], 'Capture paused core, selectors and unchanged control state', async () => {
      if (spec.peer_core) { assert.notEqual(spec.peer_core, options.core); await select(spec.peer_core); peerBefore = await read(controlIds); }
      await select(options.core); context = (await session.command('registers_list')).context;
      assert.equal(context.core, options.core); before = await read(controlIds);
      if (spec.requests.some(r=>r.kind==='pmu')) assert.equal(exact(before.pmcr) & 1n, 0n, 'Use a fixture with counting disabled beforehand; this test does not disable the PMU');
      return {context, before, peerBefore};
    })) return;
    if (!await suite.test(ids[1], 'Observe identity/counts on this physical core at this stop', async () => {
      probe = await session.command('registers_probe', {context});
      const midr = probe.probe.samples.find(s=>s.id==='midr');
      assert.equal(midr?.state, 'valid'); assert.equal(exact(midr.value.hex), exact(spec.expected_midr));
      assert.equal(probe.probe.identity?.model, 'Cortex-R52');
      for (const request of spec.requests) {
        const count = probe.probe.facts[selection[request.kind][1]];
        assert(count?.source && request.index < count.value, 'Only an observed implemented index is usable');
      }
      return probe;
    })) return;
    if (!await suite.test(ids[2], 'Read known MPU/PMU pairs and independently verify values and restoration', async () => {
      const results = [];
      for (const request of spec.requests) {
        const [selector, , a, b] = selection[request.kind];
        const result = await session.command('registers_select', {context, kind:request.kind, index:request.index});
        assert.deepEqual(result.context, context); assert.equal(result.selector, selector);
        assert.equal(exact(result.evidence.original.hex), exact(before[selector]));
        assert.deepEqual(result.evidence.restored, result.evidence.original);
        assert.equal(result.evidence.selected, request.index);
        assert(result.evidence.synchronization.includes(spec.current_debug ? 'Genuine ISB in native R52' : 'CP15ISB'));
        const pair = [a+request.index, b+request.index];
        const direct = await read(pair);
        for (let i=0;i<2;i++) {
          const sample = result.samples.find(s=>s.id===pair[i]);
          assert.equal(sample?.state, 'valid'); assert.equal(sample.owner, `core:${options.core}`);
          assert.deepEqual(sample.context, context); assert(sample.source.includes(`:selector:${result.target}:`));
          if (spec.current_debug) {
            const access = sample.provenance?.access, proof = access?.r52_core;
            assert.equal(access?.phase, 'responded'); assert.deepEqual(access.context, context);
            assert.equal(access.route.target, spec.current_debug.targets[options.core]);
            assert.equal(access.route.endpoint, spec.current_debug.endpoint);
            assert.equal(proof?.bank, request.kind==='mpu_el2' ? 'el2' : 'el1');
            assert.equal(exact(proof.capacity.hex),exact(request.expected_capacity));
            assert.equal(exact(proof.midr.hex),exact(spec.expected_midr));
            assert.equal(exact(proof.dspsr.hex),exact(spec.current_debug.saved_dspsr[options.core]));
            const dscr=exact(proof.dscr.hex);
            assert.equal((dscr>>8n)&3n,2n); assert.equal(dscr&(1n<<15n),0n);
            assert.equal(dscr&((1n<<16n)|(1n<<18n)),(1n<<16n)|(1n<<18n));
            assert.equal(dscr&(1n<<24n),1n<<24n); assert.equal(dscr&0x1c0000c0n,0n);
            exact(proof.dlr.hex);
          }
          assert.equal(exact(sample.value.hex), exact(request.expected[i]));
          assert.equal(exact(direct[pair[i]]), exact(request.expected[i]), 'Independent direct-index read must agree');
        }
        assert.deepEqual(await read(controlIds), before, 'Every selector/control must remain unchanged after each transaction');
        results.push({request, result, direct});
      }
      fs.writeFileSync(path.join(out, 'selector-evidence.json'), JSON.stringify(results,null,2)); return results;
    })) return;
    await suite.test(ids[3], 'Preserve stopped context, peer selectors and original project', async () => {
      await halted(); assert.deepEqual((await session.command('registers_list')).context, context);
      assert.deepEqual(await read(controlIds), before);
      if (spec.peer_core) { await select(spec.peer_core); assert.deepEqual(await read(controlIds), peerBefore); await select(options.core); }
      assert(!session.logs('mi>').some(log=>/-exec-|\b-data-write-|\b-var-assign/.test(log.text)));
      assert.equal(hash(project), projectHash); return {context, before, peerBefore};
    });
  } catch(error) {
    await suite.test('REG-H04-SEL-SETUP', 'Initialize the explicit selector fixture', async()=>{throw error;});
  } finally {
    if(session) await suite.test('REG-H04-SEL-CLEANUP', 'Close only this test session', ()=>session.close());
    if(hash(project)!==projectHash) await suite.test('REG-H04-SEL-CONFIG', 'Preserve original project', async()=>{throw Error('Project changed');});
    for(const id of ids) if(!suite.results.some(item=>item.id===id)) suite.skip(id,'Deferred fixture phase','Earlier precondition failed');
    suite.finish();
  }
})();
