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
function run(project, profile='', userPreset, explicitEnvironment) {
  const dir = path.join(out,`case-${++sequence}`), tools=path.join(dir,'tools'), config=path.join(dir,'config');
  fs.mkdirSync(tools,{recursive:true}); fs.mkdirSync(path.join(config,'profiles/registers'),{recursive:true});
  fs.writeFileSync(path.join(dir,'debug.toml'),project);
  fs.writeFileSync(path.join(tools,'debug-env.toml'),profile);
  fs.writeFileSync(path.join(tools,'profile.toml'),m4);
  fs.writeFileSync(path.join(dir,'customer.toml'),r52);
  fs.writeFileSync(path.join(dir,'m7.toml'),m4.replace('cpu = "cortex-m4"','cpu = "cortex-m7"'));
  const devices=path.join(config,'profiles/devices.toml');
  fs.writeFileSync(devices,"version=1\n[devices.matrix]\ncores=[0,2]\nbackend='generic'\ncpu='cortex-r52+'\n");
  if(userPreset !== undefined) {
    const file=path.join(config,'profiles/registers/cortex-r52+.toml');
    if(userPreset === 'directory') fs.mkdirSync(file); else fs.writeFileSync(file,userPreset);
  }
  const files=[path.join(dir,'debug.toml'),path.join(tools,'debug-env.toml'),devices,path.join(tools,'profile.toml'),path.join(dir,'customer.toml'),path.join(dir,'m7.toml')];
  const environmentArgs=[];
  if (explicitEnvironment !== undefined) {
    const selected=path.join(tools,'explicit-environment.toml');
    fs.writeFileSync(selected,explicitEnvironment); files.push(selected);
    environmentArgs.push('--environment',selected);
  }
  const before=files.map(hash);
  const requests=project.includes("chip='matrix'") ? [
    ['select_core',{index:0}],['registers_list',{}],['select_core',{index:1}],['registers_list',{}],['status',{}],['quit',{}]
  ] : [['registers_list',{}],['status',{}],['quit',{}]];
  const input=requests.map(([method,params],i)=>JSON.stringify({id:i+1,method,params})).join('\n')+'\n';
  // Two validated catalogues (up to 4 MiB each) include JSON schema/default
  // fields in the response. Keep this finite bound aligned with distribution
  // tests; Node's 1 MiB default truncates otherwise valid register listings.
  const result=spawnSync(binary,['--project',path.join(dir,'debug.toml'),...environmentArgs,'--headless','--stdio'],{
    input,encoding:'utf8',windowsHide:true,timeout:15000,maxBuffer:32*1024*1024,env:{...process.env,DEBUGTUI_CONFIG_DIR:config}
  });
  fs.writeFileSync(path.join(dir,'stdout.jsonl'),result.stdout);
  fs.writeFileSync(path.join(dir,'stderr.txt'),result.stderr);
  assert.ifError(result.error);
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
function setting(list, ...segments) {
  assert.equal(list.configuration.core,list.context.core);
  const entry=list.configuration.settings.find(entry=>JSON.stringify(entry.path)===JSON.stringify(segments));
  assert(entry,`missing configuration setting ${segments.join('.')}`);
  return entry;
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
    await suite.test('REG-CONFIG-SOURCES','Actual workers report profile/backend/project origins and core map replacement without debugger I/O',async()=>{
      const profile=envBase+"[registers]\ncpu='cortex-r52'\ntcl_endpoint='localhost:6666'\n[registers.facts]\ninherited=4\ncapacity=1\n[backends.generic.registers]\ncpu='cortex-m4'\n[backends.generic.registers.facts]\ncapacity=2\n";
      const project=base+"[registers.facts]\ncapacity=3\n[[cores]]\nname='core.0'\n[cores.registers]\ncpu='cortex-m7'\ncatalogue='m7.toml'\n[cores.registers.facts]\nselected=7\n[[cores]]\nname='core.2'\n[cores.registers.facts]\n";
      const result=run(project,profile), entries=lists(result);
      assert.equal(setting(entries[0],'cpu').value,'cortex-m7');
      assert(setting(entries[0],'cpu').source.endsWith('[cores.registers for core.0]'));
      assert.deepEqual(setting(entries[0],'cpu').overrides.map(entry=>entry.value),['','cortex-r52','cortex-m4']);
      assert(setting(entries[0],'cpu').overrides[2].source.endsWith('[backends.generic.registers]'));
      assert.equal(setting(entries[0],'facts','selected').value,7);
      assert.deepEqual(setting(entries[1],'facts').value,{});
      for(const entry of entries) {
        assert(!entry.configuration.settings.some(setting=>setting.path[0]==='facts' && setting.path[1]==='inherited'));
        assert(entry.configuration.replacements.find(replacement=>replacement.field==='facts').removed_paths.some(path=>JSON.stringify(path)==='["facts","inherited"]'));
        assert(!entry.configuration.settings.some(setting=>setting.source.includes('origin unavailable')));
      }
      const inherited=lists(run(base+"[registers.facts]\ncapacity=3\n",profile));
      assert.equal(setting(inherited[0],'facts','capacity').value,3);
      assert.deepEqual(setting(inherited[0],'facts','capacity').overrides.map(entry=>entry.value),[1,2]);
      assert(setting(inherited[0],'facts','inherited').source.endsWith('debug-env.toml [registers]'));
      return {configurations:entries.map(entry=>entry.configuration),project:result.dir};
    });
    await suite.test('REG-CONFIG-CORE-ARRAY-SOURCE','Unsupported environment core arrays are rejected before IO rather than attributed to the project',async()=>{
      const errors=[];
      for(const [section,expected] of [['cores','Unsupported environment section: cores'],['backends.generic.cores','Unsupported backend section: cores']]) {
        const result=run(base,envBase+`[[${section}]]\nname='core.0'\n[${section}.registers]\ncpu='cortex-m7'\n`);
        assert.notEqual(result.result.status,0);
        assert(result.result.stderr.includes(expected),result.result.stderr);
        errors.push(result.result.stderr.trim());
      }
      return {errors};
    });
    await suite.test('REG-CONFIG-DEFAULT-SOURCE','Chip association and explicit core clears retain their real declaration sources',async()=>{
      const associated=lists(run(base,envBase));
      assert.equal(setting(associated[0],'cpu').value,'cortex-r52+');
      assert(setting(associated[0],'cpu').source.startsWith('chip association:matrix (user:'));
      const cleared=lists(run(base+"[registers]\ncpu='cortex-m4'\n[[cores]]\nname='core.0'\n[cores.registers]\ncpu=''\ncatalogue=''\n",envBase));
      assert.equal(cleared[0].catalogue,null);
      assert.equal(setting(cleared[0],'cpu').value,'');
      assert(setting(cleared[0],'cpu').source.endsWith('[cores.registers for core.0]'));
      assert.equal(setting(cleared[1],'cpu').value,'cortex-m4');
      assert(!setting(cleared[1],'cpu').source.includes('core.0'));
      return {association:setting(associated[0],'cpu'),cleared:cleared.map(entry=>entry.configuration)};
    });
    await suite.test('REG-CONFIG-ENVIRONMENT-SOURCE','Explicit CLI environment supplies origin instead of the unselected missing profile',async()=>{
      const result=run(base.replace("profile='tools/debug-env.toml'","profile='missing-profile.toml'"),envBase,undefined,envBase+"[registers]\ncpu='cortex-m4'\ntcl_endpoint='localhost:6670'\n");
      const entries=lists(result);
      for(const entry of entries) {
        assert.equal(setting(entry,'tcl_endpoint').value,'localhost:6670');
        assert(setting(entry,'tcl_endpoint').source.endsWith('explicit-environment.toml [registers]'));
        assert(!JSON.stringify(entry.configuration).includes('missing-profile.toml'));
      }
      return {configurations:entries.map(entry=>entry.configuration)};
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
    await suite.test('REG-CONFIG-PER-CORE','Each actual worker selects its own catalogue, facts and explicit empty fallback',async()=>{
      const overrides="[registers]\ncpu='cortex-m4'\n[[cores]]\nname='core.0'\n[cores.registers]\ncpu='cortex-m7'\ncatalogue='m7.toml'\n[cores.registers.facts]\nroute_capacity=7\n[[cores]]\nname='core.2'\n[cores.registers.facts]\nroute_capacity=4\n";
      const result=run(base+overrides,envBase); assert.equal(result.result.status,0,result.result.stderr);
      const entries=lists(result); assert.equal(entries.length,2);
      assert.deepEqual(entries.map(x=>x.catalogue.cpu),['cortex-m7','cortex-m4']);
      assert.deepEqual(entries.map(x=>x.facts.route_capacity),[7,4]);
      assert(entries[0].source.replaceAll('\\','/').endsWith('/m7.toml'));
      assert.equal(entries[1].source,'builtin:cortex-m4');
      assert.notEqual(entries[0].context.session,entries[1].context.session);
      const empty=run(base+"[registers]\ncpu='cortex-m4'\n[[cores]]\nname='core.0'\n[cores.registers]\ncpu=''\ncatalogue=''\n",envBase);
      assert.equal(empty.result.status,0,empty.result.stderr);
      assert.equal(lists(empty)[0].catalogue,null); assert.equal(lists(empty)[0].source,'gdb');
      assert.equal(lists(empty)[1].catalogue.cpu,'cortex-m4');
      return {cores:entries.map(x=>x.context.core),models:entries.map(x=>x.catalogue.cpu),sources:entries.map(x=>x.source),empty_source:lists(empty)[0].source};
    });
    await suite.test('REG-CONFIG-PER-CORE-ERRORS','Per-core selection errors cannot fall back to the project preset',async()=>{
      const errors=[];
      for(const [override,expected] of [["catalogue='missing.toml'",'missing.toml'],["unknown_setting=true",'unknown field'],["cp15_command='unsafe mrc'",'core.0 registers']]) {
        const result=run(base+"[[cores]]\nname='core.0'\n[cores.registers]\n"+override+'\n',envBase);
        assert.notEqual(result.result.status,0); assert(result.result.stderr.toLowerCase().includes(expected.toLowerCase()),result.result.stderr);
        errors.push(result.result.stderr.trim());
      }
      return {errors};
    });
  } catch(error) { await suite.test('REG-CONFIG-SETUP','Run configuration fixture',async()=>{throw error;}); }
  finally { suite.finish(); }
})();
