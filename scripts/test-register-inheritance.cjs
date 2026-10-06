#!/usr/bin/env node
'use strict';
// Actual executable/configuration entry. No debugger, target or board is connected.
const fs = require('node:fs'), path = require('node:path'), assert = require('node:assert/strict');
const {spawnSync} = require('node:child_process');
const {root, outputDirectory, parseOptions, Cases, hash} = require('./test-support/session.cjs');
const options = parseOptions(process.argv.slice(2), ['--binary']);
const binary = path.resolve(options.binary || path.join(root, 'target/debug/debugtui.exe'));
const out = outputDirectory('register-inheritance');
const suite = new Cases(out, {layer:'actual executable/configuration/JSONL; no target connection', board_tests_executed:false, binary, binary_sha256:hash(binary)});
const sampleRoot = path.join(root,'profiles/registers/examples');
const common = fs.readFileSync(path.join(sampleRoot,'r52-source-common.toml'),'utf8');
const sample = fs.readFileSync(path.join(sampleRoot,'r52-source-sample.toml'),'utf8');
let sequence = 0;
function run(files, multi=false) {
  const dir = path.join(out,`case-${++sequence}`);
  fs.mkdirSync(dir,{recursive:true});
  for (const [name,text] of Object.entries(files)) fs.writeFileSync(path.join(dir,name),text);
  let project = "version=2\n[gdb]\nexecutable='./must-not-start-gdb.exe'\n[registers]\ncatalogue='leaf.toml'\n";
  if (multi) project += "[[cores]]\nname='core.0'\nendpoint='localhost:3333'\n[[cores]]\nname='core.2'\nendpoint='localhost:3334'\n[cores.registers]\ncatalogue='other.toml'\n";
  fs.writeFileSync(path.join(dir,'debug.toml'),project);
  const tracked = [...Object.keys(files),'debug.toml'].map(name=>path.join(dir,name));
  const before = tracked.map(hash);
  const methods = multi ? [['select_core',{index:0}],['registers_list',{}],['select_core',{index:1}],['registers_list',{}],['quit',{}]] : [['registers_list',{}],['quit',{}]];
  const input = methods.map(([method,params],i)=>JSON.stringify({id:i+1,method,params})).join('\n')+'\n';
  const result = spawnSync(binary,['--project',path.join(dir,'debug.toml'),'--headless','--stdio'],{input,encoding:'utf8',windowsHide:true,timeout:15000,maxBuffer:4*1024*1024,env:{...process.env,DEBUGTUI_CONFIG_DIR:path.join(dir,'config')}});
  assert.ifError(result.error);
  fs.writeFileSync(path.join(dir,'stdout.jsonl'),result.stdout);
  fs.writeFileSync(path.join(dir,'stderr.txt'),result.stderr);
  assert.deepEqual(tracked.map(hash),before,'loading must preserve customer descriptions');
  const events = result.stdout.trim().split(/\r?\n/).filter(Boolean).map(line=>JSON.parse(line));
  assert(!events.some(e=>e.event==='log' && e.channel==='mi>'),'listing must produce zero debugger IO');
  return {result,events,dir,methods};
}
function catalogues(run) {
  assert.equal(run.result.status,0,run.result.stderr);
  const responses = run.events.filter(e=>e.event==='response');
  assert.equal(responses.length,run.methods.length);
  assert(responses.every(e=>e.ok),JSON.stringify(responses));
  return responses.filter(e=>run.methods[e.id-1][0]==='registers_list').map(e=>e.result);
}
(async()=>{
  try {
    await suite.test('INHERIT-ENTRY','Resolve five actual file definitions, explicit override, manual sources and per-core contexts',async()=>{
      const result = run({'r52-source-common.toml':common,'leaf.toml':sample.replace('extends = ["r52-source-common"]','extends = ["r52-source-common.toml"]'),'other.toml':sample.replace('cpu = "cortex-r52"','cpu = "cortex-r52+"')},true);
      const lists = catalogues(result);
      assert.deepEqual(lists.map(x=>x.context.core),['core.0','core.2']);
      assert.deepEqual(lists.map(x=>x.catalogue.cpu),['cortex-r52','cortex-r52+']);
      for (const list of lists) {
        assert.equal(list.probe,null);
        assert.equal(list.catalogue.registers.length,5);
        const midr = list.catalogue.registers.find(x=>x.id==='midr');
        assert.equal(midr.confidence,'high'); assert.equal(midr.source.page,169);
        assert.equal(midr.source.number,'100026_0104_01_en'); assert.equal(midr.fields.length,5);
        assert(list.definition_origins.midr.declared_in.endsWith('r52-source-common.toml'));
        assert.equal(list.definition_origins.midr.inheritance.length,2);
        const mpu = list.catalogue.registers.find(x=>x.id==='prbar0');
        assert(list.definition_origins.prbar0.overrides.endsWith('r52-source-common.toml'));
        assert.equal(mpu.conditions[0].fact,'mpu.el1.regions');
        assert.equal(mpu.reset,undefined,'UNKNOWN reset must not become zero');
        const sticky = list.catalogue.registers.find(x=>x.id==='edprsr');
        assert.equal(sticky.read_side_effect,true); assert.equal(sticky.confidence,'medium');
        assert.equal(sticky.source.version,'M.b'); assert.equal(sticky.source.page,15261);
        assert.equal(list.catalogue.registers.find(x=>x.id==='midr_partnum').reader.kind,'alias');
      }
      return {cores:lists.map(x=>x.context.core),models:lists.map(x=>x.catalogue.cpu),definitions:5};
    });
    await suite.test('INHERIT-COMMON-EDIT','A public parent edit flows into both child models without rewriting them',async()=>{
      const changed = common.replace('Processor identity; variant','Reviewed parent identity; variant');
      const lists = catalogues(run({'r52-source-common.toml':changed,'leaf.toml':sample,'other.toml':sample.replace('cpu = "cortex-r52"','cpu = "cortex-r52+"')},true));
      assert(lists.every(x=>x.catalogue.registers.find(x=>x.id==='midr').description.startsWith('Reviewed parent identity')));
      return {children_updated:lists.length};
    });
    await suite.test('INHERIT-ERRORS','Reject definition/source/reset/cycle/depth errors before any debugger starts',async()=>{
      const header = "version=2\ncpu='test'\narchitecture='armv8-r-aarch32'\n";
      const bad = [
        {'r52-source-common.toml':common,'leaf.toml':sample.replace('override = true','override = false')},
        {'leaf.toml':sample},
        {'r52-source-common.toml':common.replace('page = 169','page = 0'),'leaf.toml':sample},
        {'r52-source-common.toml':common.replace('confidence = "high"','confidence = "verified"'),'leaf.toml':sample},
        {'r52-source-common.toml':common.replace('confidence = "high"','reset = 0x100000000\nconfidence = "high"'),'leaf.toml':sample},
        {'r52-source-common.toml':common,'leaf.toml':sample.replace('source = { section = "4.3.85; Table 4-195 and direct encoding on PDF page 193", page = 192 }','')},
        {'leaf.toml':header+"extends=['cycle']\n",'cycle.toml':header+"extends=['./leaf.toml']\n"},
        {'leaf.toml':header+"extends=['one']\n",'one.toml':header+"extends=['two']\n",'two.toml':header+"extends=['three']\n",'three.toml':header},
      ];
      for (const files of bad) { const result=run(files); assert.notEqual(result.result.status,0,result.result.stdout); assert(result.result.stderr.includes('Register catalogue') || result.result.stderr.includes('file:'),result.result.stderr); }
      return {rejected_before_io:bad.length};
    });
  } finally { suite.finish(); }
})();
