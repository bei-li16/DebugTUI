// STM32F429 / FreeRTOS acceptance with an isolated project and an existing ELF.
// Reset/download are opt-in. No application build is performed.
const {spawnSync} = require('node:child_process');
const fs = require('node:fs'), path = require('node:path'), assert = require('node:assert/strict');
const {root, quote, delay, hash, outputDirectory, parseOptions, Cases, Session} = require('./test-support/session.cjs');
const options = parseOptions(process.argv.slice(2), ['--binary','--elf','--profile','--source-root','--svd','--project','--break-location','--function'], ['--allow-reset','--allow-download']);
assert(options.elf && options.profile && options['source-root'], 'Required: --elf FILE --profile FILE --source-root DIR');
assert(!options['allow-download'] || options['allow-reset'], '--allow-download also requires --allow-reset');
const binary = path.resolve(options.binary || path.join(root, 'target/debug/debugtui.exe'));
const elf = path.resolve(options.elf), profile = path.resolve(options.profile), source = path.resolve(options['source-root']);
const svd = path.resolve(options.svd || path.join(root, 'resources/svd/stm32/STM32F429.svd'));
const protectedFiles = [elf, profile, svd, ...(options.project ? [path.resolve(options.project)] : [])];
for (const file of protectedFiles) assert(fs.existsSync(file), `Missing input: ${file}`);
if (process.platform === 'win32') {
  const ports = spawnSync('powershell.exe', ['-NoProfile','-Command', "Get-NetTCPConnection -State Listen -ErrorAction SilentlyContinue | Where-Object LocalPort -in @(3333,6666) | Select-Object -ExpandProperty LocalPort"], {encoding:'utf8', windowsHide:true, timeout:15000});
  assert.ifError(ports.error);
  assert.equal(ports.status, 0, ports.stderr);
  assert(!ports.stdout.trim(), 'Ports 3333/6666 are in use. Close the existing debugger before starting an owned-server hardware test.');
}
const out = outputDirectory('project-hardware');
const suite = new Cases(out, {binary, sha256:hash(binary), elf, profile, layer:'STM32F429 physical board', allow_reset:!!options['allow-reset'], allow_download:!!options['allow-download']});
const beforeHashes = Object.fromEntries(protectedFiles.map(file => [file, hash(file)]));
const project = path.join(out, 'project.toml');
const breakLocation = options['break-location'] || 'BSP/USER_TASK/Src/user_task.c:41';
const expectedFunction = options.function || 'Task100ms';
fs.writeFileSync(project, `version=2\nwatch=['xTickCount','Log_Tx_En','g_w25q_jedec_id']\nbreakpoints=[]\n[tools]\nprofile=${quote(profile)}\n[program]\nelf=${quote(elf)}\nsource_root=${quote(source)}\nsvd=${quote(svd)}\n[session]\non_exit='detach'\nlog_dir=${quote(out)}\n`);
let session, connected = false, matched = false, binding, originalWatches, functionLocation;
const ownedProcesses = new Set();
function collectOwnedProcesses() {
  ownedProcesses.add(session.child.pid);
  if (process.platform !== 'win32') return;
  const query = spawnSync('powershell.exe', ['-NoProfile','-Command', `Get-CimInstance Win32_Process | Where-Object ParentProcessId -eq ${Number(session.child.pid)} | Select-Object -ExpandProperty ProcessId | ConvertTo-Json -Compress`], {encoding:'utf8',windowsHide:true,timeout:15000});
  assert.ifError(query.error); assert.equal(query.status,0,query.stderr);
  if(query.stdout.trim()) for(const processId of [JSON.parse(query.stdout)].flat()) ownedProcesses.add(processId);
}
const cmd = (...args) => session.command(...args);
const status = () => cmd('status');
const temporaryStop = async () => {
  await cmd('break', {location:breakLocation, temporary:true});
  await cmd('continue'); await cmd('wait_stopped', {timeout_ms:10000});
  const snapshot = await status(); assert.equal(snapshot.frame.function, expectedFunction); return snapshot;
};
(async () => {
  session = new Session(binary, project, out);
  try {
    connected = await suite.test('HW-01', 'Attach to configured probe and report a real stopped frame', async () => {
      await cmd('connect'); collectOwnedProcesses(); const snapshot = await status(); assert.equal(snapshot.state,'STOPPED'); assert(snapshot.frame.address); return snapshot.frame;
    });
    if (!connected) return;
    matched = await suite.test('HW-02', 'All read-only firmware sections match the supplied ELF', async () => {
      await cmd('console',{command:'compare-sections -r'});
      const logs = session.logs('gdb');
      assert(!logs.some(e => /MIS-MATCH|does not match/i.test(e.text)));
      const sections = logs.filter(e => /Section .*matched\./.test(e.text)).map(e => e.text.trim());
      assert(sections.some(s => s.includes('.text')) && sections.some(s => s.includes('.isr_vector')), 'Missing .text/vector confirmation');
      return sections;
    });
    // Do not execute or interpret variable addresses against mismatched firmware.
    if (!matched) return;
    await suite.test('HW-03', 'Scalar/static Watch values and invalid expressions', async () => {
      const snapshot = await status(); originalWatches = snapshot.watches.map(w => w.name);
      assert.deepEqual(originalWatches,['xTickCount','Log_Tx_En','g_w25q_jedec_id']);
      assert(snapshot.watches.every(w => !w.error));
      const values = {};
      for (const expression of originalWatches) values[expression] = (await cmd('evaluate',{expression})).value;
      assert.match(await cmd('evaluate',{expression:'debugtui_missing_symbol_849283'},false), /symbol|context/i);
      return values;
    });
    await suite.test('HW-04', 'Function/variable search, literal regexp characters and source files', async () => {
      const before = await status();
      const symbols = await cmd('symbols',{query:expectedFunction});
      functionLocation = symbols.symbols.find(s => s.name === expectedFunction);
      assert(functionLocation && functionLocation.line > 0 && fs.existsSync(functionLocation.file));
      assert((await cmd('symbols',{query:'xTickCount'})).symbols.some(s => s.kind === 'variable'));
      assert.equal((await cmd('symbols',{query:'.*'})).symbols.length,0);
      assert((await cmd('files')).files.some(f => f.endsWith('tasks.c')));
      const after = await status(); assert.equal(after.frame.address,before.frame.address);
      return functionLocation;
    });
    await suite.test('HW-05', 'Code breakpoint disable/enable, condition/ignore edits and persistence', async () => {
      await cmd('break',{location:breakLocation,enabled:false,condition:'xTickCount >= 0',ignore_count:2});
      let bp = (await status()).breakpoints[0]; assert(!bp.enabled); assert.equal(bp.ignore_count,2);
      await cmd('enable_break',{number:bp.id,enabled:true}); assert((await status()).breakpoints[0].enabled);
      await cmd('update_break',{number:bp.id,enabled:false,condition:'xTickCount >= 1',ignore_count:3});
      bp = (await status()).breakpoints[0]; assert(!bp.enabled); assert.equal(bp.ignore_count,3);
      assert(fs.readFileSync(project,'utf8').includes('enabled = false'));
      await cmd('delete_break',{number:bp.id}); assert.equal((await status()).breakpoints.length,0);
    });
    await suite.test('HW-06', 'Temporary source breakpoint hits the recurring task and removes itself', async () => {
      const hit = await temporaryStop(); assert.equal(hit.breakpoints.length,0); return hit.frame;
    });
    await suite.test('HW-07', 'Source step-over and instruction step advance real PC', async () => {
      const before = await status(); await cmd('next'); await cmd('wait_stopped');
      const next = await status(); assert.notEqual(next.frame.address,before.frame.address);
      await cmd('stepi'); await cmd('wait_stopped'); const stepped = await status(); assert.notEqual(stepped.frame.address,next.frame.address);
      return {before:before.frame,next:next.frame,step:stepped.frame};
    });
    await suite.test('HW-08', 'Stack/frame navigation, locals, CPU registers and disassembly', async () => {
      const snapshot = await status(); assert(snapshot.stack.length > 0); assert(snapshot.registers.some(r => r.name === 'pc'));
      await cmd('frame',{level:0}); await cmd('disassemble'); const assembly = (await status()).assembly; assert(assembly.length > 0);
      return {stack:snapshot.stack.length,locals:snapshot.locals.length,registers:snapshot.registers.length,assembly:assembly.length};
    });
    await suite.test('HW-09', 'Pointer/structure and array Watch expansion, collapse and removal', async () => {
      for (const expression of ['pxCurrentTCB','pxReadyTasksLists']) {
        await cmd('watch',{expression}); await cmd('watch_expand',{expression,path:[],expanded:true});
        const watch = (await status()).watches.find(w => w.name === expression); assert(watch.tree?.children?.length > 0,expression);
        await cmd('watch_expand',{expression,path:[],expanded:false}); await cmd('unwatch',{expression});
      }
      assert.deepEqual((await status()).watches.map(w => w.name),originalWatches);
    });
    await suite.test('HW-10', 'Resolved Watch address agrees across GDB, AHB and stopped core channels', async () => {
      binding = await cmd('watch_resolve',{expression:'xTickCount'}); assert.equal(binding.bits,32);
      const expected = Number((await cmd('evaluate',{expression:'xTickCount'})).value);
      for (const channel of ['', 'ahb', 'core-tcl']) assert.equal((await cmd('memory_read',{address:binding.address,bits:32,little_endian:true,channel})).value,expected);
      await cmd('memory',{address:'&xTickCount',count:16}); assert((await status()).memory.length > 0); return binding;
    });
    await suite.test('HW-11', 'Safe peripheral read and invalid width/alignment/channel errors', async () => {
      const params = {address:0x40023808,bits:32,little_endian:true};
      const gdb = await cmd('memory_read',params); assert.equal((await cmd('memory_read',{...params,channel:'ahb'})).value,gdb.value);
      await cmd('memory_read',{...params,channel:'missing'},false);
      await cmd('memory_read',{...params,address:params.address+1,channel:'ahb'},false);
      await cmd('memory_read',{...params,bits:24,channel:'ahb'},false);
      return {register:'RCC.CFGR',value:gdb.value};
    });
    await suite.test('HW-12', 'Live AHB samples advance without GDB polling or implicit pause', async () => {
      assert(binding, 'HW-10 must establish the binding');
      await cmd('continue'); const before = await status(); assert.equal(before.state,'RUNNING');
      const mi = session.logs('mi>').length, samples = [];
      for (let i=0;i<8;i++) { samples.push((await cmd('memory_read',{address:binding.address,bits:32,little_endian:true,channel:'ahb'})).value); await delay(100); }
      assert.equal(session.logs('mi>').length,mi); assert(new Set(samples).size > 1);
      const after = await status(); assert.equal(after.state,'RUNNING'); assert.equal(after.generation,before.generation);
      await cmd('memory_read',{address:binding.address,bits:32,little_endian:true,channel:'core-tcl'},false);
      await cmd('symbols',{query:expectedFunction},false); await cmd('pause'); assert.equal((await status()).state,'STOPPED'); return samples;
    });
    await suite.test('HW-13', 'Reconnect and duplicate Connect preserve the existing session', async () => {
      await cmd('reconnect'); collectOwnedProcesses(); assert.equal((await status()).state,'STOPPED');
      await cmd('connect',{},false); assert.equal((await status()).state,'STOPPED');
      assert.deepEqual((await status()).watches.map(w => w.name),originalWatches);
    });
    await suite.test('HW-14', 'Watch preferences survive a full debugger process restart', async () => {
      await session.close(); session = new Session(binary,project,out); await cmd('connect'); collectOwnedProcesses();
      assert.equal((await status()).state,'STOPPED'); assert.deepEqual((await status()).watches.map(w => w.name),originalWatches);
    });
    if (options['allow-reset']) await suite.test('HW-15', 'Configured reset/run actions reach main using a temporary breakpoint', async () => {
      await cmd('restart'); await cmd('break',{location:'main',temporary:true}); await cmd('run'); await cmd('wait_stopped'); assert.equal((await status()).frame.function,'main');
    }); else suite.skip('HW-15','Configured reset/run','Requires --allow-reset');
    if (options['allow-download']) await suite.test('HW-16', 'GDB download and read-only section verification', async () => {
      await cmd('download'); const start = session.logs().length; await cmd('console',{command:'compare-sections -r'});
      const logs = session.logs().slice(start); assert(!logs.some(e => /MIS-MATCH/.test(e.text))); assert(logs.some(e => /Section .text.*matched/.test(e.text)));
    }); else suite.skip('HW-16','Actual firmware download','Requires --allow-reset --allow-download');
  } finally {
    await suite.test('HW-17','Clean debugger exit and owned process cleanup',async () => {
      await session.close();
      if(process.platform==='win32'&&ownedProcesses.size) {
        // Get-Process -Id emits ObjectNotFound (and exit 1) for successfully
        // cleaned-up PIDs, even with SilentlyContinue. Query the inventory.
        const query=spawnSync('powershell.exe',['-NoProfile','-Command',`Get-CimInstance Win32_Process | Where-Object ProcessId -in @(${[...ownedProcesses].join(',')}) | Select-Object -ExpandProperty ProcessId`],{encoding:'utf8',windowsHide:true,timeout:15000});
        assert.ifError(query.error); assert.equal(query.status,0,query.stderr); assert(!query.stdout.trim(),`Owned processes remain: ${query.stdout}`);
      }
      return [...ownedProcesses];
    });
    await suite.test('HW-18','User project, tool profile, SVD and ELF stay byte-for-byte unchanged',() => {
      for (const file of protectedFiles) assert.equal(hash(file),beforeHashes[file],file); return beforeHashes;
    });
    // Aborted prerequisites must remain visibly uncovered in the report.
    for (let n=1;n<=18;n++) { const id=`HW-${String(n).padStart(2,'0')}`; if (!suite.results.some(r=>r.id===id)) suite.skip(id,'Dependent hardware scenario',!connected?'Connection failed':'Firmware comparison failed'); }
    suite.finish();
  }
})().catch(error => { console.error(error); process.exitCode=1; });
