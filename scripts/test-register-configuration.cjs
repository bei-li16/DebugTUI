#!/usr/bin/env node
'use strict';
// Isolated on-disk projects/profiles and the actual executable. No connect command.
const fs = require('node:fs'), path = require('node:path'), assert = require('node:assert/strict');
const {spawnSync} = require('node:child_process');
const {root, outputDirectory, parseOptions, Cases, hash} = require('./test-support/session.cjs');
const options = parseOptions(process.argv.slice(2), ['--binary']);
const binary = path.resolve(options.binary || path.join(root,'target/debug/debugtui.exe'));
const out = outputDirectory('register-configuration');
const suite = new Cases(out,{layer:'actual executable, isolated project/profile/user configuration; no debugger or board',board_tests_executed:false,binary,binary_sha256:hash(binary)});
const m4 = fs.readFileSync(path.join(root,'profiles/registers/cortex-m4.toml'),'utf8');
const r52 = fs.readFileSync(path.join(root,'profiles/registers/cortex-r52.toml'),'utf8');
let sequence=0;
function run(project, profile='', userPreset) {
  const dir = path.join(out,`case-${++sequence}`), tools=path.join(dir,'tools'), config=path.join(dir,'config');
  fs.mkdirSync(tools,{recursive:true}); fs.mkdirSync(path.join(config,'profiles/registers'),{recursive:true});
  fs.writeFileSync(path.join(dir,'debug.toml'),project);
  fs.writeFileSync(path.join(tools,'debug-env.toml'),profile);
  fs.writeFileSync(path.join(tools,'profile.toml'),m4);
  fs.writeFileSync(path.join(dir,'customer.toml'),r52);
  const devices=path.join(config,'profiles/devices.toml');
  fs.writeFileSync(devices,"version=1\n[devices.matrix]\ncores=[0,2]\nbackend='generic'\ncpu='cortex-r52+'\n");
  if(userPreset !== undefined) {
    const file=path.join(config,'profiles/registers/cortex-r52+.toml');
    if(userPreset === 'directory') fs.mkdirSync(file); else fs.writeFileSync(file,userPreset);
  }
  const files=[path.join(dir,'debug.toml'),path.join(tools,'debug-env.toml'),devices,path.join(tools,'profile.toml'),path.join(dir,'customer.toml')];
  const before=files.map(hash);
  const requests=project.includes("chip='matrix'") ? [
    ['select_core',{index:0}],['registers_list',{}],['select_core',{index:1}],['registers_list',{}],['status',{}],['quit',{}]
  ] : [['registers_list',{}],['status',{}],['quit',{}]];
  const input=requests.map(([method,params],i)=>JSON.stringify({id:i+1,method,params})).join('\n')+'\n';
  const result=spawnSync(binary,['--project',path.join(dir,'debug.toml'),'--headless','--stdio'],{
    input,encoding:'utf8',windowsHide:true,timeout:15000,env:{...process.env,DEBUGTUI_CONFIG_DIR:config}
  });
  assert.ifError(result.error);
  fs.writeFileSync(path.join(dir,'stdout.jsonl'),result.stdout);
  fs.writeFileSync(path.join(dir,'stderr.txt'),result.stderr);
  assert.deepEqual(files.map(hash),before,'reading configuration must not rewrite customer files');
  const events=result.stdout.trim().split(/\r?\n/).filter(Boolean).map(line=>JSON.parse(line));
  assert(!events.some(e=>e.event==='log' && e.channel==='mi>'),'configuration listing must not start debugger IO');
  return {result,events,dir,requests};
}
const base="version=3\n[tools]\nprofile='tools/debug-env.toml'\n[debug]\nchip='matrix'\ncores=[0,2]\n";
const envBase="backend='generic'\n[gdb]\nexecutable='./must-not-start-gdb.exe'\n[core_targets.\"0\"]\nendpoint='localhost:5330'\n[core_targets.\"2\"]\nendpoint='localhost:5332'\n";
function lists(run) {
  assert.equal(run.result.status,0,run.result.stderr);
  const responses=run.events.filter(e=>e.event==='response');
  assert.equal(responses.length,run.requests.length,'every request must complete');
  for(const response of responses) assert.equal(response.ok,true,JSON.stringify(response));
  return responses.filter(e=>run.requests[e.id-1]?.[0]==='registers_list').map(e=>e.result);
}
(async()=>{
  try {
    await suite.test('REG-CONFIG-LEGACY','Legacy project without register settings keeps the GDB list and starts no debugger',async()=>{
      const result=run("version=1\n[gdb]\nexecutable='./must-not-start-gdb.exe'\n[target]\nmode='local'\n");
      const [list]=lists(result); assert.equal(list.catalogue,null); assert.equal(list.source,'gdb'); return {source:list.source};
    });
    for(const [id,project,profile,model,source] of [
      ['CHIP','','','cortex-r52+','builtin:cortex-r52+'],
      ['PROFILE','','[registers]\ncpu=\'cortex-m4\'\n','cortex-m4','builtin:cortex-m4'],
      ['BACKEND','','[registers]\ncpu=\'cortex-r52\'\n[backends.generic.registers]\ncpu=\'cortex-m4\'\n','cortex-m4','builtin:cortex-m4'],
      ['PROJECT','[registers]\ncpu=\'cortex-r52\'\n','[registers]\ncpu=\'cortex-m4\'\n','cortex-r52','builtin:cortex-r52'],
      ['PROFILE-PATH','[registers]\ncpu=\'cortex-r52\'\n','[registers]\ncatalogue=\'profile.toml\'\n','cortex-m4','file:'],
      ['PROJECT-PATH','[registers]\ncatalogue=\'customer.toml\'\n','[registers]\ncatalogue=\'profile.toml\'\n','cortex-r52','file:'],
      ['EMPTY','[registers]\ncpu=\'\'\ncatalogue=\'\'\n','[registers]\ncatalogue=\'profile.toml\'\n',null,'gdb']
    ]) await suite.test(`REG-CONFIG-${id}`,'Check actual directory priority and both physical core names without probe',async()=>{
      const result=run(base+project,envBase+profile); const entries=lists(result); assert.equal(entries.length,2);
      for(const [index,list] of entries.entries()) {
        assert.equal(list.context.core,index ? 'core.2':'core.0'); assert(list.source.startsWith(source));
        assert.equal(list.catalogue?.cpu || null,model); assert.equal(list.probe || null,null);
        if(id==='PROFILE-PATH') assert(list.source.replaceAll('\\','/').endsWith('/tools/profile.toml'));
        if(id==='PROJECT-PATH') assert(list.source.replaceAll('\\','/').endsWith('/customer.toml'));
      }
      assert.notEqual(entries[0].context.session,entries[1].context.session);
      return {sources:entries.map(x=>x.source),cores:entries.map(x=>x.context.core)};
    });
    await suite.test('REG-CONFIG-USER','User CPU preset overrides embedded model and preserves its original bytes',async()=>{
      const custom=r52.replace('cpu = "cortex-r52"','cpu = "customer-model"');
      const result=run(base,envBase,custom); const entries=lists(result);
      assert(entries.every(x=>x.catalogue.cpu==='customer-model' && x.source.startsWith('user:')));
      assert.equal(fs.readFileSync(path.join(result.dir,'config/profiles/registers/cortex-r52+.toml'),'utf8'),custom);
      return {sources:entries.map(x=>x.source)};
    });
    await suite.test('REG-CONFIG-ERRORS','Selected malformed paths and unknown settings fail before debugger startup',async()=>{
      const errors=[];
      for(const [project,profile,preset,expected] of [
        ['[registers]\ncatalogue=\'missing.toml\'\n','',undefined,'missing.toml'],
        ['[registers]\nunknown_setting=true\n','',undefined,'unknown field'],
        ['','[registers]\nunknown_setting=true\n',undefined,'unknown field'],
        ['','[backends.generic.registers]\nunknown_setting=true\n',undefined,'unknown field'],
        ['','','version=999','catalogue'],
        ['','','directory','catalogue']
      ]) {
        const result=run(base+project,envBase+profile,preset);
        assert.notEqual(result.result.status,0); assert(result.result.stderr.toLowerCase().includes(expected.toLowerCase()),result.result.stderr);
        errors.push(result.result.stderr.trim());
      }
      return {errors};
    });
  } catch(error) { await suite.test('REG-CONFIG-SETUP','Run configuration fixture',async()=>{throw error;}); }
  finally { suite.finish(); }
})();
