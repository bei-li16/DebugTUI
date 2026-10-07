#!/usr/bin/env node
'use strict';
// Exercise the shipped executable and resources with no hardware connection.
const fs=require('node:fs'), path=require('node:path'), assert=require('node:assert/strict');
const {spawnSync}=require('node:child_process');
const {root,outputDirectory,parseOptions,Cases,hash}=require('./test-support/session.cjs');
const options=parseOptions(process.argv.slice(2),['--binary']);
const binary=path.resolve(options.binary||path.join(root,'target/release/debugtui.exe'));
const out=outputDirectory('bundled-tools'), user=path.join(out,'user settings'), firmware=path.join(out,'固件 工程');
fs.mkdirSync(firmware,{recursive:true});
const suite=new Cases(out,{binary,binary_sha256:hash(binary),board_tests_executed:false});
let sequence=0;
function run(args,cwd=firmware,input='') {
 const r=spawnSync(binary,args,{cwd,input,encoding:'utf8',windowsHide:true,timeout:30000,env:{...process.env,DEBUGTUI_CONFIG_DIR:user}});
 fs.writeFileSync(path.join(out,`run-${++sequence}.json`),JSON.stringify({args,cwd,status:r.status,stdout:r.stdout,stderr:r.stderr},null,2));
 assert.ifError(r.error);return r;
}
function ok(args,cwd,input){const r=run(args,cwd,input);assert.equal(r.status,0,r.stderr);return r;}
function status(chip,cores,probe='cmsis-dap',extra=[],cwd=firmware) {
 const r=ok(['--headless','--stdio','--chip',chip,'--cores',cores.join(','),'--probe',probe,...extra],cwd,'{"id":1,"method":"status"}\n');
 const events=r.stdout.trim().split(/\r?\n/).map(JSON.parse);
 assert(!events.some(e=>e.event==='log'&&e.channel==='mi>'));
 const response=events.find(e=>e.event==='response'&&e.id===1);
 assert.equal(response?.ok,true,JSON.stringify(response));
 assert.deepEqual(response.result.cores.map(c=>[c.name,c.endpoint,c.state]),cores.map(c=>[`core.${c}`,`127.0.0.1:${3333+c}`,'DISCONNECTED']));
}
(async()=>{try{
 await suite.test('BUNDLE-TOOLS','Installed GDB and OpenOCD load their bundled resources without hardware',async()=>{
  const exeDirectory=path.dirname(binary);
  const tools=path.basename(exeDirectory).toLowerCase()==='bin'?path.join(exeDirectory,'../tools'):path.join(exeDirectory,'tools');
  assert(fs.existsSync(path.join(tools,'debug-env.toml')),'Pass the installed npm/portable executable to this suite');
  const before=hash(path.join(tools,'debug-env.toml'));
  const env={...process.env};delete env.PYTHONHOME;delete env.PYTHONPATH;
  const gdb=spawnSync(path.join(tools,'bin/gdb/bin/arm-none-eabi-gdb.exe'),['--nx','--batch','--data-directory='+path.join(tools,'bin/gdb/arm-none-eabi/share/gdb'),'-ex','show data-directory'],{encoding:'utf8',windowsHide:true,timeout:15000,env});
  assert.ifError(gdb.error);assert.equal(gdb.status,0,gdb.stderr);
  const openocd=spawnSync(path.join(tools,'bin/openocd/bin/openocd.exe'),['-s',path.join(tools,'bin/openocd/scripts'),'-f',path.join(tools,'openocd/probes/cmsis-dap.cfg'),'-f',path.join(tools,'openocd/stm32f429.cfg'),'-c','shutdown'],{encoding:'utf8',windowsHide:true,timeout:15000});
  assert.ifError(openocd.error);assert.equal(openocd.status,0,openocd.stderr);assert.equal(hash(path.join(tools,'debug-env.toml')),before);
  return {gdb:gdb.stdout.trim(),openocd:openocd.stderr.trim()};
 });
 await suite.test('BUNDLE-INIT','Startup creates a minimal project in cwd without copying tools',async()=>{
  fs.writeFileSync(path.join(firmware,'Cargo.toml'),'[package]\nname="unrelated"\n');
  fs.writeFileSync(path.join(firmware,'settings.toml'),'version=1\n');
  const r=run([]);assert.notEqual(r.status,0);assert(r.stderr.includes('Interactive mode requires a terminal'));
  const file=path.join(firmware,'debug.toml'), text=fs.readFileSync(file,'utf8');
  assert(text.includes('profile = "builtin:arm-openocd"')&&text.includes('probe = "cmsis-dap"'));
  assert(!/[A-Za-z]:[\\/]|\.vscode/.test(text));
  assert(!fs.existsSync(path.join(firmware,'.vscode'))&&!fs.existsSync(path.join(firmware,'tools')));
  const before=hash(file);ok(['--init-project']);assert.equal(hash(file),before);
 });
 for(const probe of ['cmsis-dap','jlink','stlink']) await suite.test(`BUNDLE-STM32-${probe}`,'Project probe chooses an independent bundled OpenOCD adapter',async()=>status('stm32f429',[0],probe));
 for(const [chip,cores] of [['tha6104',[0]],['tha6206',[1]],['tha6206',[0,1]],['tha6412',[1,3]]]) await suite.test(`BUNDLE-${chip}-${cores.join('')}`,'Bundled R52 templates preserve physical core IDs and fixed ports',async()=>status(chip,cores));
 await suite.test('BUNDLE-PROBE-ERROR','Invalid probe is rejected without modifying the project',async()=>{
  const file=path.join(firmware,'debug.toml'),before=hash(file);const r=run(['--headless','--chip','stm32f429','--cores','0','--probe','invalid']);assert.notEqual(r.status,0);assert(r.stderr.includes('Unknown probe'));assert.equal(hash(file),before);
 });
 await suite.test('BUNDLE-SVD-REFERENCE','CLI saves a portable installed SVD reference and rejects traversal',async()=>{
  ok(['--init-project','--chip','stm32f429','--cores','0','--svd','builtin:svd/STM32F429.svd']);
  const file=path.join(firmware,'debug.toml'),text=fs.readFileSync(file,'utf8'),before=hash(file);
  assert(text.includes('builtin:svd/STM32F429.svd'));
  status('stm32f429',[0]);
  const r=run(['--init-project','--svd','builtin:svd/../../outside.svd']);
  assert.notEqual(r.status,0);assert.equal(hash(file),before);
 });
 await suite.test('BUNDLE-USER-OVERRIDE','User chip overrides load outside npm and project overrides take precedence',async()=>{
  const chips=path.join(user,'profiles/chips');fs.mkdirSync(chips,{recursive:true});
  fs.writeFileSync(path.join(chips,'tha6206.toml'),'broken=[');
  const r=run(['--headless','--chip','tha6206','--cores','1']);assert.notEqual(r.status,0);assert(r.stderr.includes('tha6206.toml'));
  const custom=path.join(firmware,'custom chip.toml');fs.writeFileSync(custom,"extends='builtin:devices/tha6206.toml'\n");
  status('tha6206',[1],'jlink',['--chip-profile',custom]);
  fs.writeFileSync(path.join(chips,'tha6206.toml'),"extends='builtin:devices/tha6206.toml'\n");
  const before=hash(path.join(chips,'tha6206.toml'));ok(['--init-profiles']);assert.equal(hash(path.join(chips,'tha6206.toml')),before);
  status('tha6206',[0,1]);
 });
 await suite.test('BUNDLE-SAVE-MOVE','Saved selection stays portable after moving the project',async()=>{
  ok(['--init-project','--chip','stm32f429','--cores','0','--probe','stlink']);
  const before=fs.readFileSync(path.join(firmware,'debug.toml'),'utf8');assert(before.includes('stlink')&&before.includes('builtin:arm-openocd'));
  const destination=path.join(out,'移动后的工程');for(const p of [firmware,destination])assert(path.resolve(p).startsWith(path.resolve(out)+path.sep));
  fs.renameSync(firmware,destination);
  status('stm32f429',[0],'stlink',[],destination);
  assert.equal(fs.readFileSync(path.join(destination,'debug.toml'),'utf8'),before);
 });
 await suite.test('BUNDLE-DISCOVERY','Unique project is loaded; multiple or malformed projects are preserved',async()=>{
  const cwd=path.join(out,'discovery');fs.mkdirSync(cwd);
  fs.writeFileSync(path.join(cwd,'customer.toml'),"version=2\n[program]\nelf='app.elf'\n");
  assert(ok(['--init-project'],cwd).stdout.includes('customer.toml'));assert(!fs.existsSync(path.join(cwd,'debug.toml')));
  fs.writeFileSync(path.join(cwd,'second.toml'),"version=2\n[program]\nelf='two.elf'\n");
  assert.notEqual(run(['--init-project'],cwd).status,0);assert(!fs.existsSync(path.join(cwd,'debug.toml')));
  fs.writeFileSync(path.join(cwd,'debug.toml'),'broken=[');const before=hash(path.join(cwd,'debug.toml'));
  assert.notEqual(run(['--init-project'],cwd).status,0);assert.equal(hash(path.join(cwd,'debug.toml')),before);
 });
}catch(error){await suite.test('BUNDLE-SETUP','Prepare bundled tools tests',async()=>{throw error;});}finally{suite.finish();}})();
