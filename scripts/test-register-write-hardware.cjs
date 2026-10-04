// Deferred WRITE-H02 Core case. Default mode prepares a report and never connects.
// Use a dedicated halted fixture whose named working register may be changed.
const fs = require('node:fs'), path = require('node:path'), assert = require('node:assert/strict');
const {root, hash, outputDirectory, parseOptions, Cases, Session} = require('./test-support/session.cjs');
const options = parseOptions(process.argv.slice(2), ['--binary', '--project', '--core', '--register', '--value', '--fixture-function'], ['--run', '--software-fixture']);
const ids = ['WRITE-H02-CORE-PRECONDITION', 'WRITE-H02-CORE-CANCEL', 'WRITE-H02-CORE-APPLY', 'WRITE-H02-CORE-RESTORE'];
const out = outputDirectory('register-write-hardware');
const register = options.register || 'r0';
assert(/^r(?:[0-9]|1[0-2])$/.test(register), 'Only a declared fixture working register r0..r12 is allowed; PC/SP/status are separate cases');
const value = options.value || '0x89abcdef';
assert(/^0x[0-9a-fA-F]{1,8}$/.test(value), 'Use an exact 32-bit hexadecimal command value');
const suite = new Cases(out, {
  layer: options['software-fixture'] ? 'local MI software fixture; no board' : options.run ? 'physical board, explicit fixture' : 'deferred case preparation; no target access',
  todo: 'WRITE-H02 Core subset; other writers and hardware cases remain separate',
  board_tests_executed: !!options.run && !options['software-fixture'], project: options.project || null, core: options.core || null,
  fixture_function: options['fixture-function'] || null, register, command_value: value,
});
if (!options.run) {
  for (const id of ids) suite.skip(id, 'Halted fixture register write, cancel, independent read and explicit restoration', 'Prepared only; --run and a dedicated board fixture are required');
  suite.finish();
  process.exit(0);
}
assert(options.project && options.core && options['fixture-function'], '--run requires --project FILE --core NAME --fixture-function NAME');
const binary = path.resolve(options.binary || path.join(root, 'target/release/debugtui.exe'));
const project = path.resolve(options.project);
for (const file of [binary, project]) assert(fs.existsSync(file), `Missing input: ${file}`);
suite.metadata.binary = binary; suite.metadata.binary_sha256 = hash(binary);
suite.metadata.project = project; suite.metadata.project_sha256 = hash(project);
const projectHash = hash(project);
let session, original, neighbours, catalogue;
const number = text => {
  assert(/^(?:0x[0-9a-fA-F]+|[0-9]+)$/.test(text), `Not an exact integer: ${text}`);
  return BigInt(text);
};
const halted = async () => {
  const status = await session.command('status');
  assert.equal(status.state, 'STOPPED');
  assert.equal(status.frame.level, 0);
  assert.equal(status.frame.function, options['fixture-function'], 'Stop in the declared dedicated fixture; the driver never pauses/runs/resets to reach it');
  assert((status.cores || []).every(core => core.state === 'STOPPED'), 'Related cores must already be stopped');
  return status;
};
const read = async ids => {
  const listed = await session.command('registers_list');
  const result = await session.command('registers_read', {ids, manual: true, context: listed.context});
  const samples = result.samples;
  assert(Array.isArray(samples), 'Register read did not return samples');
  const values = {};
  for (const id of ids) {
    const sample = samples.find(sample => sample.id === id);
    assert.equal(sample?.state, 'valid', `Cannot independently read ${id}`);
    values[id] = sample.value.hex;
  }
  return values;
};
const preview = async text => {
  await halted();
  const listed = await session.command('registers_list');
  return session.command('write_preview', {target: {kind: 'register', id: register}, selection: {kind: 'register'}, context: listed.context, input: {kind: 'unsigned', text}});
};
const apply = async draft => {
  const result = await session.command('write_apply', {draft: draft.draft});
  assert.equal(result.outcome, 'verified', JSON.stringify(result));
  assert.equal(result.owner, options.core);
  assert.equal(result.endpoint, draft.endpoint);
  return result;
};
(async () => {
  try {
    session = new Session(binary, project, out);
    await session.command('connect');
    if (options.core !== 'default') await session.command('select_core', {name: options.core});
    if (!await suite.test(ids[0], 'Verify paused fixture, owner, declared writer and independent neighbouring registers', async () => {
      await halted();
      catalogue = await session.command('registers_list');
      const definition = catalogue.catalogue?.registers.find(item => item.id === register);
      assert.equal(definition?.writer?.kind, 'gdb_integer');
      const adjacent = Array.from({length: 13}, (_, n) => `r${n}`).filter(id => id !== register).slice(0, 3);
      const before = await read([register, ...adjacent]);
      original = before[register]; delete before[register]; neighbours = before;
      return {context: catalogue.context, original, neighbours, catalogue_source: catalogue.source};
    })) return;
    if (!await suite.test(ids[1], 'Cancel before Apply sends no register write and preserves the value', async () => {
      const draft = await preview(value);
      const cancelled = await session.command('write_cancel', {draft: draft.draft});
      assert.equal(cancelled.cancelled, true);
      assert.equal(cancelled.outcome, 'not_sent');
      const current = await session.command('evaluate', {expression: `$${register}`});
      assert.equal(number(current.value), number(original));
      assert(!session.logs('mi>').some(log => /-data-write-register-values /.test(log.text)), 'Preview/cancel sent a write');
      return {draft: draft.draft, cancelled, independently_observed: current.value};
    })) return;
    if (!await suite.test(ids[2], 'Apply once, independently read the target and verify neighbouring working registers', async () => {
      const draft = await preview(value);
      const result = await apply(draft);
      const current = await session.command('evaluate', {expression: `$${register}`});
      assert.equal(number(current.value), number(value));
      assert.deepEqual(await read(Object.keys(neighbours)), neighbours);
      const repeated = await session.command('write_apply', {draft: draft.draft});
      assert.equal(repeated.outcome, 'not_sent');
      return {result, independently_observed: current.value, neighbours, repeated};
    })) return;
    await suite.test(ids[3], 'Explicitly restore the ordinary working register after a verified successful test', async () => {
      const result = await apply(await preview(original));
      const restored = await session.command('evaluate', {expression: `$${register}`});
      assert.equal(number(restored.value), number(original));
      assert.deepEqual(await read(Object.keys(neighbours)), neighbours);
      return {result, restored: restored.value};
    });
  } catch (error) {
    await suite.test('WRITE-H02-CORE-SETUP', 'Initialize the dedicated test session', async () => { throw error; });
  } finally {
    // An unknown/mismatched write is never retried or automatically rolled back.
    if (session) await suite.test('WRITE-H02-CORE-CLEANUP', 'Close only this test session', () => session.close());
    if (hash(project) !== projectHash) await suite.test('WRITE-H02-CORE-CONFIG', 'Original project remains unchanged', async () => { throw Error('Project was modified'); });
    for (const id of ids) if (!suite.results.some(item => item.id === id)) suite.skip(id, 'Deferred fixture phase', 'An earlier precondition or write phase failed');
    suite.finish();
  }
})();
