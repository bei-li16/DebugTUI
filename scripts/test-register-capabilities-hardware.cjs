// Deferred REG-H01/H14 capability subset. Default mode never opens a target.
const fs = require('node:fs'), path = require('node:path'), assert = require('node:assert/strict');
const {spawnSync} = require('node:child_process');
const {root, hash, outputDirectory, parseOptions, Cases, Session} = require('./test-support/session.cjs');
const options = parseOptions(process.argv.slice(2), ['--binary', '--project', '--core', '--case', '--gdb', '--openocd'], ['--run', '--software-fixture']);
const ids = ['REG-H01-CAP-PRECONDITION', 'REG-H01-CAP-PROBE', 'REG-H14-CAP-BOUNDARIES', 'REG-H01-CAP-UNCHANGED'];
const out = outputDirectory('register-capabilities-hardware');
const suite = new Cases(out, {
  layer: options['software-fixture'] ? 'local MI software fixture; no board' : options.run ? 'explicit physical-board fixture' : 'deferred case preparation; no target access',
  todo: 'REG-H01/H14 capability subset; optional classes, tdesc, other cores and recovery are separate cases',
  board_tests_executed: !!options.run && !options['software-fixture'], core: options.core || null,
});
if (!options.run) {
  for (const id of ids) suite.skip(id, 'Paused R52 identity/MPU/GIC capability evidence and unchanged control registers', 'Prepared only; --run with an explicit paused fixture is required');
  suite.finish(); process.exit(0);
}
assert(options.project && options.core && options.case, '--run requires --project FILE --core NAME --case JSON');
const binary = path.resolve(options.binary || path.join(root, 'target/release/debugtui.exe'));
const project = path.resolve(options.project), caseFile = path.resolve(options.case);
for (const file of [binary, project, caseFile]) assert(fs.existsSync(file), `Missing input: ${file}`);
const spec = JSON.parse(fs.readFileSync(caseFile));
assert(typeof spec.frame_function === 'string' && spec.frame_function.length, 'Declare the paused fixture function');
assert(Array.isArray(spec.stable_registers) && spec.stable_registers.includes('cpsr'), 'Declare stable readable control registers including CPSR');
assert(spec.expected_raw?.midr, 'Declare the independently known MIDR; errors cannot pass identity verification');
const exact = raw => { assert(/^0x[0-9a-f]+$/i.test(raw), `Not an exact raw value: ${raw}`); return BigInt(raw); };
const projectHash = hash(project);
Object.assign(suite.metadata, {binary, binary_sha256: hash(binary), project, project_sha256: projectHash, case_file: caseFile, case_sha256: hash(caseFile)});
function toolIdentity(name) {
  const supplied = options[name];
  if (!supplied) return {status: 'unknown', source: 'not supplied; no tool provenance assumed'};
  const tool = path.resolve(supplied);
  assert(fs.existsSync(tool), `Missing ${name}: ${tool}`);
  const version = spawnSync(tool, ['--version'], {encoding:'utf8', windowsHide:true, timeout:10000, maxBuffer:65536});
  return {path:fs.realpathSync(tool), sha256:hash(tool), version_status:version.status===0?'observed':'unknown',
    version:(version.stdout || '')+(version.stderr || ''), exit_code:version.status, source:'operator-supplied host executable; not proof of the server process identity'};
}
suite.metadata.tools = {gdb: toolIdentity('gdb'), openocd: toolIdentity('openocd')};
let session, before, context, evidence;
const halted = async () => {
  const status = await session.command('status');
  assert.equal(status.state, 'STOPPED'); assert.equal(status.frame.level, 0);
  assert.equal(status.frame.function, spec.frame_function, 'Stop at the declared fixture first; the driver does not run/pause/reset/download');
  return status;
};
const stable = async () => {
  const listed = await session.command('registers_list');
  const read = await session.command('registers_read', {ids:spec.stable_registers, manual:true, context:listed.context});
  const result = {};
  for (const id of spec.stable_registers) {
    const sample = read.samples.find(s => s.id===id);
    assert.equal(sample?.state, 'valid', `No unchanged-state proof without a fresh readable ${id}`);
    result[id] = sample.value.hex;
  }
  return result;
};
(async () => {
  try {
    session = new Session(binary, project, out);
    await session.command('connect');
    if (options.core!=='default') await session.command('select_core', {name:options.core});
    if (!await suite.test(ids[0], 'Capture paused physical owner, context and declared stable controls', async () => {
      await halted(); const listed=await session.command('registers_list'); context=listed.context;
      assert.equal(context.core, options.core); before=await stable();
      return {context, before, catalogue:listed.source};
    })) return;
    if (!await suite.test(ids[1], 'Probe once; retain exact raw values, errors and capability provenance', async () => {
      evidence=await session.command('registers_probe', {context});
      assert.deepEqual(evidence.context, context);
      assert.equal(evidence.probe.samples.length, 10);
      assert(evidence.probe.samples.every(s=>s.owner===`core:${options.core}` && JSON.stringify(s.context)===JSON.stringify(context)));
      for (const [id, raw] of Object.entries(spec.expected_raw)) {
        const sample=evidence.probe.samples.find(s=>s.id===id); assert.equal(sample?.state,'valid');
        assert.equal(exact(sample.value.hex),exact(raw),id);
      }
      for (const [key, value] of Object.entries(spec.expected_facts || {})) {
        assert.equal(evidence.probe.facts[key]?.value,value,key);
        assert(evidence.probe.facts[key]?.source,'Observed facts need a source');
      }
      fs.writeFileSync(path.join(out, 'capabilities.json'), JSON.stringify(evidence,null,2));
      return evidence;
    })) return;
    if (!await suite.test(ids[2], 'Physical/virtual GIC separation and Unknown/access restrictions stay explicit', async () => {
      for (const id of spec.expected_unavailable || []) {
        const sample=evidence.probe.samples.find(s=>s.id===id);
        assert.equal(sample?.state,'unavailable'); assert.equal(sample.implementation,'unknown'); assert.equal(sample.value,null);
      }
      for (const key of ['vfp.present','vfp.enabled']) assert.equal(evidence.probe.facts[key],undefined,'CPACR must not infer FPU state');
      for (const key of spec.expected_unknown_facts || []) assert.equal(evidence.probe.facts[key],undefined,key);
      const physical=evidence.probe.facts['icc.physical.prebits'];
      if (physical) assert.equal(physical.register,'icc_ctlr');
      const virtual=evidence.probe.facts['icv.virtual.prebits'];
      if (virtual) assert.equal(virtual.register,'ich_vtr');
      return {physical, virtual, unknown:spec.expected_unknown_facts || [], unavailable:spec.expected_unavailable || []};
    })) return;
    await suite.test(ids[3], 'Control values, stop context and project remain unchanged; no write/control commands', async () => {
      await halted(); assert.deepEqual(await stable(),before);
      assert.deepEqual((await session.command('registers_list')).context,context);
      assert(!session.logs('mi>').some(log=>/-exec-|\b-data-write-|\b-var-assign|\bmcr\b/.test(log.text)));
      assert.equal(hash(project),projectHash);
      return {before, after:before, context};
    });
  } catch(error) {
    await suite.test('REG-H01-CAP-SETUP', 'Initialize the explicit capability fixture', async()=>{throw error;});
  } finally {
    if(session) await suite.test('REG-H01-CAP-CLEANUP','Close only this test session',()=>session.close());
    if(hash(project)!==projectHash) await suite.test('REG-H01-CAP-CONFIG','Preserve original project',async()=>{throw Error('Project changed');});
    for(const id of ids) if(!suite.results.some(item=>item.id===id)) suite.skip(id,'Deferred fixture phase','Earlier precondition failed');
    suite.finish();
  }
})();
