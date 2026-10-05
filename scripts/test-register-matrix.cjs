#!/usr/bin/env node
'use strict';
const fs=require('node:fs'),path=require('node:path'),assert=require('node:assert/strict');
const {spawnSync}=require('node:child_process');
const {root,outputDirectory,parseOptions,Cases,hash}=require('./test-support/session.cjs');
const options=parseOptions(process.argv.slice(2),['--binary']);
const binary=path.resolve(options.binary||path.join(root,'target/debug/debugtui.exe'));
const out=outputDirectory('register-matrix');
const suite=new Cases(out,{layer:'actual executable; offline matrix and isolated configuration',board_tests_executed:false,binary,binary_sha256:hash(binary)});
const base="version=3\n[tools]\nprofile='tools/debug-env.toml'\n[debug]\nchip='matrix'\ncores=[0,2]\n";
const profile="backend='generic'\n[gdb]\nexecutable='./must-not-start-gdb.exe'\n[core_targets.\"0\"]\nendpoint='localhost:5330'\n[core_targets.\"2\"]\nendpoint='localhost:5332'\n";
let sequence=0;
function run(settings,custom) {
  const dir=path.join(out,`case-${++sequence}`),config=path.join(dir,'config');
  fs.mkdirSync(path.join(config,'profiles'),{recursive:true});fs.mkdirSync(path.join(dir,'tools'),{recursive:true});
  const files=[path.join(dir,'debug.toml'),path.join(dir,'tools/debug-env.toml'),path.join(config,'profiles/devices.toml')];
  fs.writeFileSync(files[0],base+settings);fs.writeFileSync(files[1],profile);
  fs.writeFileSync(files[2],"version=1\n[devices.matrix]\ncores=[0,2]\nbackend='generic'\ncpu='cortex-r52+'\n");
  if(custom!==undefined){files.push(path.join(dir,'custom.toml'));fs.writeFileSync(files.at(-1),custom);}
  const before=files.map(hash);
  const requests=[['select_core',{index:0}],['registers_matrix',{}],['registers_matrix',{}],['select_core',{index:1}],['registers_matrix',{}],['registers_matrix',{}],['status',{}],['quit',{}]];
  const result=spawnSync(binary,['--project',files[0],'--headless','--stdio'],{encoding:'utf8',windowsHide:true,timeout:15000,maxBuffer:32*1024*1024,
    input:requests.map(([method,params],i)=>JSON.stringify({id:i+1,method,params})).join('\n')+'\n',env:{...process.env,DEBUGTUI_CONFIG_DIR:config}});
  fs.writeFileSync(path.join(dir,'stdout.jsonl'),result.stdout||'');fs.writeFileSync(path.join(dir,'stderr.log'),result.stderr||'');
  assert.ifError(result.error);assert.deepEqual(files.map(hash),before);
  const events=(result.stdout||'').trim().split(/\r?\n/).filter(Boolean).map(line=>JSON.parse(line));
  assert(!events.some(e=>e.event==='log'&&e.channel==='mi>'),'matrix must never start target traffic');
  return {result,events,requests,dir};
}
function matrices(run) {
  assert.equal(run.result.status,0,run.result.stderr);
  const responses=run.events.filter(e=>e.event==='response');assert.equal(responses.length,run.requests.length);
  assert(responses.every(r=>r.ok),JSON.stringify(responses.filter(r=>!r.ok)));
  const matrices=responses.filter(r=>run.requests[r.id-1][0]==='registers_matrix').map(r=>r.result);
  assert.deepEqual(matrices[0],matrices[1]);assert.deepEqual(matrices[2],matrices[3]);
  for(const [index,matrix]of matrices.entries()) {
    assert.equal(matrix.context.core,index<2?'core.0':'core.2');assert.equal(matrix.schema_version,1);
    assert.equal(matrix.probe_current,false);assert.equal(matrix.probe,null);assert.equal(matrix.fact_source,'configuration');
    assert.deepEqual(matrix.observations,[]);assert(matrix.rows.every(r=>r.support!=='observed_value'));
  }
  return [matrices[0],matrices[2]];
}
const item=(matrix,id)=>{const row=matrix.rows.find(r=>r.id===id);assert(row,id);return row;};
(async()=>{
  try {
    await suite.test('MATRIX-CPU','Both built-in R52 variants expose exact declared widths and sparse core identity without probing',async()=>{
      const result=[];
      for(const cpu of ['cortex-r52','cortex-r52+'])for(const matrix of matrices(run(`[registers]\ncpu='${cpu}'\n`))) {
        assert.equal(matrix.catalogue.cpu,cpu);assert.equal(matrix.rows.length,matrix.catalogue.registers.length);
        assert.equal(matrix.planned_classes.length,12);assert(matrix.planned_classes.every(c=>c.hardware_support==='unverified'));
        assert.equal(matrix.planned_classes.find(c=>c.id==='stm').entries,0);
        for(const [id,bits]of [['r0',32],['d0',64],['q0',128],['cntpct',64]])assert.equal(item(matrix,id).bits,bits);
        assert.equal(item(matrix,'d0').support,'route_unavailable');assert.equal(item(matrix,'cntpct').plan.transport,'gdb');
        result.push({cpu,context:matrix.context,rows:matrix.rows.length});
      }
      return result;
    });
    await suite.test('MATRIX-ROUTES','Per-core targets and required native protocols are configuration plans only',async()=>{
      const settings="[registers]\ncpu='cortex-r52'\ntcl_endpoint='localhost:6666'\ncp15_command='arm mrc'\ncp15_64_command='aarch64 mrrc'\ntimer_command='aarch64 timer'\n\n[registers.targets]\n'core.0'='cpu0'\n'core.2'='cpu2'\n";
      const result=matrices(run(settings));
      for(const [index,matrix]of result.entries()) {
        const row=item(matrix,'cntpct');assert.equal(row.plan.target,index?'cpu2':'cpu0');assert.equal(row.plan.endpoint,'localhost:6666');
        assert(row.plan.protocol.startsWith('debugtui-armv8-timer-1 '));assert(row.plan.available);assert.equal(row.support,'unobserved');
        assert.equal(matrix.environment.observed_gdb_endpoint,null);
      }
      return result.map(m=>item(m,'cntpct'));
    });
    await suite.test('MATRIX-CONDITIONS','Configured No excludes a conditional read without claiming hardware absence',async()=>{
      const result=matrices(run("[registers]\ncpu='cortex-r52'\n[registers.facts]\n\"timer.present\"=0\n"));
      for(const matrix of result){assert.equal(item(matrix,'cntpct').support,'conditions_excluded');assert.equal(matrix.effective_facts['timer.present'],0);}
      return result.map(m=>item(m,'cntpct'));
    });
    await suite.test('MATRIX-LEGACY','No catalogue keeps an empty unknown inventory',async()=>{
      for(const matrix of matrices(run("[registers]\ncpu=''\ncatalogue=''\n"))){assert.equal(matrix.source,'gdb');assert.equal(matrix.catalogue,null);assert.deepEqual(matrix.rows,[]);assert.deepEqual(matrix.categories,[]);}
    });
    const custom="version=1\ncpu='custom'\narchitecture='armv8-r-aarch32'\n[[groups]]\nid='root'\nname='Root'\n[[groups]]\nid='child'\nname='Child'\nparent='root'\n[[groups]]\nid='stm'\nname='Undeclared STM'\n[[registers]]\nid='word'\nname='Word'\ngroup='child'\nbits=32\naccess='wo'\nreader={kind='backend',name='word'}\n[[registers]]\nid='slice'\nname='Slice'\ngroup='child'\nbits=16\naccess='ro'\nreader={kind='alias',source='word',offset=0}\n";
    await suite.test('MATRIX-ALIASES','Custom categories retain empty groups and inherit write-only dependency restrictions',async()=>{
      for(const matrix of matrices(run("[registers]\ncatalogue='custom.toml'\n",custom))){assert.equal(item(matrix,'slice').support,'write_only');assert.deepEqual(item(matrix,'slice').dependencies,['slice','word']);assert.deepEqual(item(matrix,'slice').category,['root','child']);assert.equal(matrix.categories.find(c=>c.group==='stm').entries,0);}
    });
    await suite.test('MATRIX-ERROR','Malformed selected catalogue fails without invoking a debugger',async()=>{
      const result=run("[registers]\ncatalogue='custom.toml'\n",'version=999');assert.notEqual(result.result.status,0);assert(result.result.stderr.includes('catalogue'));
    });
  } catch(error){await suite.test('MATRIX-SETUP','Run isolated matrix fixture',async()=>{throw error;});}
  finally{suite.finish();}
})();
