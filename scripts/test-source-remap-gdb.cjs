const fs = require('node:fs'), path = require('node:path'), assert = require('node:assert/strict');
const {Session} = require('./test-support/session.cjs');
const [binary, directory, file] = process.argv.slice(2);
const local = file.replaceAll('\\', '/');
(async () => {
  const session = new Session(binary, path.join(directory, 'session.toml'), directory);
  try {
    await session.command('connect');
    await session.command('console', {command: `list "${local}":1`});
    assert(session.logs().some(log => log.text?.includes('volatile int remap_counter')), 'GDB cannot list mapped C source');
    await session.command('break', {location: `${local}:2`});
    const status = await session.command('status');
    assert(status.breakpoints.some(b => b.line === 2 && b.address && !b.address.includes('PENDING')), JSON.stringify(status.breakpoints));
    const commands = session.logs('mi>').map(e => e.text);
    assert(commands.some(c => c.includes('set substitute-path')));
    fs.writeFileSync(path.join(directory, 'gdb-result.json'), JSON.stringify({passed: true, breakpoint:status.breakpoints, sourceListed: true, commands}, null, 2));
    console.log('PASS: GDB lists relocated source and resolves local absolute file:line breakpoint');
  } finally { await session.close(); }
  const disabled = new Session(binary, path.join(directory, 'session-disabled.toml'), directory);
  try {
    await disabled.command('connect');
    assert(!disabled.logs('mi>').some(e => e.text.includes('set substitute-path')), 'Disabled rules still sent to GDB');
    console.log('PASS: disabled remapping sends no GDB substitution rules');
  } finally { await disabled.close(); }
  if (path.basename(directory) === 'posix') {
    const multiFile = path.join(directory, 'multicore.toml');
    fs.writeFileSync(multiFile, fs.readFileSync(path.join(directory, 'session.toml'), 'utf8') +
      '\n[[cores]]\nname="core.0"\nendpoint="local.0"\n[[cores]]\nname="core.1"\nendpoint="local.1"\n');
    const multi = new Session(binary, multiFile, directory);
    try {
      await multi.command('connect');
      for (const name of ['core.0', 'core.1']) {
        await multi.command('select_core', {name});
        await multi.command('console', {command: `list "${local}":1`});
        await multi.command('break', {location: `${local}:2`});
        const state = await multi.command('status');
        assert.equal(state.cores.length, 2);
        assert(state.breakpoints.some(b => b.line === 2 && b.address && !b.address.includes('PENDING')));
      }
      fs.writeFileSync(path.join(directory, 'multicore-result.json'), JSON.stringify({passed:true,cores:['core.0','core.1'],sourceListed:true,lineBreakpointResolved:true}));
      console.log('PASS: both core.0 and core.1 inherit the mapping and resolve source/line breakpoints');
    } finally { await multi.close(); }
  }
})().catch(error => { console.error(error); process.exitCode=1; });
