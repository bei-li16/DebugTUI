// Read-only symbol metadata integration test. Local GDB/ELF only; no probe/server.
const {spawn, execFileSync} = require('node:child_process');
const fs = require('node:fs');
const path = require('node:path');
const readline = require('node:readline');
const assert = require('node:assert/strict');
const root = path.dirname(__dirname);
const binary = path.resolve(process.argv[2] || path.join(root, 'target/release/debugtui.exe'));
const elf = process.argv[3] && path.resolve(process.argv[3]);
const output = path.join(root, 'artifacts', `search-${elf ? 'arm-elf' : 'native'}-${Date.now()}`);
fs.mkdirSync(output, {recursive:true});
const quote = p => JSON.stringify(path.resolve(p).replaceAll('\\','/'));
if (!elf) {
  fs.writeFileSync(path.join(output, 'Spi.c'), [
    'typedef struct { int baud; } SpiConfig;',
    'SpiConfig Spi_Config = { 7 };',
    'volatile int SpiCounter = 17;',
    'static volatile int SpiLocal = 19;',
    'static void Spi_Private(void) { SpiLocal++; }',
    'void Spi_Init(void) { Spi_Private(); SpiCounter++; }',
    ...Array.from({length:205}, (_,i) => `int spi_limit_${i} = ${i};`),
    'int main(void) { Spi_Init(); return SpiCounter; }',
  ].join('\n'));
  for (const [file, variable] of [['Spi_Irq.c','SpiIrqCount'],['espi_hal.c','espi_hal_count'],['espi_std.c','espi_std_count']]) {
    fs.writeFileSync(path.join(output,file), `volatile int ${variable} = 0;\n`);
  }
  execFileSync(process.env.DEBUGTUI_TEST_CC || 'gcc', ['-g','-O0', ...['Spi.c','Spi_Irq.c','espi_hal.c','espi_std.c'].map(f => path.join(output,f)), '-o',path.join(output,'sample.exe')], {windowsHide:true});
}
const gdb = process.env.DEBUGTUI_TEST_GDB || (elf ? path.join(root,'tools/bin/gdb/bin/arm-none-eabi-gdb.exe') : 'C:/Program Files/mingw64/bin/gdb.exe');
fs.writeFileSync(path.join(output,'project.toml'), `version=2\n[gdb]\nexecutable=${quote(gdb)}\n[target]\nmode="local"\n[program]\nelf=${quote(elf || path.join(output,'sample.exe'))}\n[session]\non_exit="disconnect"\nlog_dir=${quote(output)}\n`);
const child = spawn(binary,['--project',path.join(output,'project.toml'),'--headless','--stdio'],{windowsHide:true});
const log = fs.createWriteStream(path.join(output,'events.jsonl'));
const pending = new Map();
const miCommands = [];
const exit = new Promise(resolve => child.once('exit',resolve));
let nextId = 1;
readline.createInterface({input:child.stdout}).on('line',line => {
  log.write(line+'\n');
  const event = JSON.parse(line);
  if (event.event === 'log' && event.channel === 'mi>') miCommands.push(event.text);
  if (event.event === 'response') pending.get(event.id)?.(event);
});
child.stderr.on('data', data => fs.appendFileSync(path.join(output,'stderr.log'),data));
async function command(method,params={},ok=true) {
  const id = nextId++;
  const event = await new Promise((resolve,reject) => {
    const timer = setTimeout(() => {pending.delete(id);reject(Error(`Timeout: ${method}`));},30000);
    pending.set(id, event => {clearTimeout(timer);pending.delete(id);resolve(event);});
    child.stdin.write(JSON.stringify({id,method,params})+'\n');
  });
  assert.equal(event.ok,ok,`${method}: ${event.error}`);
  return event.result;
}
(async () => {
  try {
    await command('connect');
    const before = await command('status');
    assert.equal(before.state,'READY');
    const firstCommand = miCommands.length;
    const spi = await command('symbols',{query:'sPi'});
    assert(spi.symbols.some(s => s.kind === 'function' && s.file && s.line > 0));
    assert(spi.symbols.some(s => s.kind === 'variable' && s.file && s.line > 0));
    assert(spi.symbols.length <= 600);
    const subsequence = await command('symbols',{query: elf ? 'spinit' : 'spcnt'});
    assert(subsequence.symbols.length > 0);
    assert.deepEqual((await command('symbols',{query:'no_such_symbol_849283'})).symbols,[]);
    assert.deepEqual((await command('symbols',{query:'.*'})).symbols,[]); // Literal input, not regexp injection.
    await command('symbols',{query:'spi\nrun'},false);
    await command('symbols',{query:'x'.repeat(129)},false);
    if (!elf) {
      const exact = await command('symbols',{query:'Spi_Init'});
      const init = exact.symbols.find(s => s.name === 'Spi_Init');
      assert.equal(init.line,6);
      assert(fs.readFileSync(init.file,'utf8').split(/\r?\n/)[init.line-1].includes('void Spi_Init'));
      assert((await command('symbols',{query:'spilocal'})).symbols.some(s => s.name === 'SpiLocal' && s.kind === 'variable'));
      assert((await command('symbols',{query:'spiconfig'})).symbols.some(s => s.name === 'SpiConfig' && s.kind === 'type'));
      assert.equal((await command('symbols',{query:'spi_limit_'})).truncated,true);
      const files = (await command('files')).files;
      for (const file of ['Spi.c','Spi_Irq.c','espi_hal.c','espi_std.c']) assert(files.some(f => f.endsWith(file)));
    }
    const after = await command('status');
    assert.equal(after.state,'READY');
    assert.deepEqual(after.frame,before.frame);
    assert.deepEqual(after.breakpoints,before.breakpoints);
    assert.deepEqual(after.watches,before.watches);
    const queryCommands = miCommands.slice(firstCommand);
    assert(queryCommands.length > 0);
    assert(queryCommands.every(c => /-symbol-info-(functions|variables|types)|-file-list-exec-source-files/.test(c)), JSON.stringify(queryCommands));
    fs.writeFileSync(path.join(output,'result.json'),JSON.stringify({passed:true,elf:elf || 'native fixture',function:spi.symbols.find(s=>s.kind==='function'),variable:spi.symbols.find(s=>s.kind==='variable'),symbols:spi.symbols.length,subsequence:subsequence.symbols.length,warnings:spi.warnings,metadataCommands:queryCommands.length,state:after.state,noTargetExecution:true},null,2));
    await command('quit');
    await exit;
    console.log(`PASS: source symbol search, bounded metadata queries, no target execution. ${output}`);
  } catch (error) {
    console.error(error);
    child.kill();
    process.exitCode = 1;
  } finally {
    log.end();
  }
})();
