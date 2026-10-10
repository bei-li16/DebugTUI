'use strict';
// Exercise the actual Cases.finish -> PowerShell cleanup boundary with child
// processes, disk fixtures and byte-for-byte restoration metadata.
const {spawn, spawnSync} = require('node:child_process');
const fs = require('node:fs'), path = require('node:path'), zlib = require('node:zlib');
const assert = require('node:assert/strict'), test = require('node:test');
const {root, outputDirectory, hash} = require('./session.cjs');
const directory = outputDirectory('storage-hook-tests');
const helper = path.join(__dirname, 'session.cjs');
const source = `
  const fs=require('node:fs'),path=require('node:path');
  const {outputDirectory,Cases}=require(process.argv[1]);
  const out=outputDirectory('hook',process.argv[2]);
  const mode=process.argv[3], suite=new Cases(out,{board_tests_executed:mode==='hardware'});
  const contents='# fixture\\n'+'abc123'.repeat(180000);
  fs.writeFileSync(path.join(out,'a.toml'),contents);
  fs.writeFileSync(path.join(out,'b.toml'),contents);
  fs.writeFileSync(path.join(out,'stdout.jsonl'),'{}\\n'.repeat(400000));
  (async()=>{await suite.test('fixture','finish a real test',async()=>{if(mode==='failed')throw Error('expected failure');});suite.finish();})();
`;
function run(mode, keep = false) {
  const storage = path.join(directory, mode + (keep ? '-keep' : ''));
  fs.mkdirSync(storage, {recursive:true});
  const result = spawnSync(process.execPath, ['-e',source,helper,storage,mode], {
    encoding:'utf8',windowsHide:true,timeout:200000,
    env:{...process.env,DEBUGTUI_KEEP_TEST_PAYLOADS:keep?'1':'0'},
  });
  assert.ifError(result.error);
  fs.writeFileSync(path.join(directory, mode + (keep ? '-keep' : '') + '.stdout.txt'),result.stdout);
  fs.writeFileSync(path.join(directory, mode + (keep ? '-keep' : '') + '.stderr.txt'),result.stderr);
  const out=path.join(storage,fs.readdirSync(storage).find(name=>name.startsWith('hook-')));
  return {result,storage,out};
}
test('passing software run cleans copies and archives logs after writing its report', {skip:process.platform!=='win32'}, () => {
  const {result,storage,out}=run('passed');
  assert.equal(result.status,0,result.stderr);
  assert.equal(JSON.parse(fs.readFileSync(path.join(out,'report.json'))).passed,true);
  assert.equal(JSON.parse(fs.readFileSync(path.join(out,'.test-run.json'))).state,'completed');
  assert.equal(fs.existsSync(path.join(out,'a.toml')),true);
  assert.equal(fs.existsSync(path.join(out,'b.toml')),false);
  assert.equal(fs.existsSync(path.join(out,'stdout.jsonl')),false);
  const records=fs.readFileSync(path.join(storage,'.fixture-store/manifest.jsonl'),'utf8').trim().split('\n').map(JSON.parse);
  assert.equal(records.length,2);
  const config=records.find(entry=>entry.path.endsWith('b.toml'));
  assert.equal(config.sha256,hash(path.join(out,'a.toml')));
  assert.deepEqual(zlib.gunzipSync(fs.readFileSync(config.archive)),fs.readFileSync(path.join(out,'a.toml')));
  const log=records.find(entry=>entry.action==='archive-software-log');
  assert.equal(zlib.gunzipSync(fs.readFileSync(log.archive)).toString(),'{}\n'.repeat(400000));
});
test('failed, hardware and explicitly retained runs keep complete fixtures and logs', {skip:process.platform!=='win32'}, () => {
  for(const [mode,keep] of [['failed',false],['hardware',false],['passed',true]]) {
    const {result,out}=run(mode,keep);
    assert.equal(result.status,mode==='failed'?1:0,result.stderr);
    for(const name of ['a.toml','b.toml','stdout.jsonl','report.json']) assert.equal(fs.existsSync(path.join(out,name)),true,name);
    assert.equal(fs.existsSync(path.join(out,'storage-cleanup.json')),false);
  }
});
test('concurrent successful tests share the archive store without partial indexes or cleanup failures', {skip:process.platform!=='win32'}, async () => {
  const storage=path.join(directory,'concurrent');
  fs.mkdirSync(storage,{recursive:true});
  const execute=()=>new Promise((resolve,reject)=>{
    const child=spawn(process.execPath,['-e',source,helper,storage,'passed'],{windowsHide:true,env:{...process.env,DEBUGTUI_KEEP_TEST_PAYLOADS:'0'}});
    let stderr='';
    child.stdout.resume();child.stderr.on('data',bytes=>{stderr+=bytes;});child.on('error',reject);
    child.on('close',code=>{if(code===0)resolve();else reject(Error(`Concurrent cleanup exit ${code}: ${stderr}`));});
  });
  await Promise.all([execute(),execute()]);
  const runs=fs.readdirSync(storage).filter(name=>name.startsWith('hook-'));
  assert.equal(runs.length,2);
  let removed=0;
  for(const name of runs) {
    const report=JSON.parse(fs.readFileSync(path.join(storage,name,'storage-cleanup.json')));
    removed+=report.entries.filter(entry=>entry.action==='deduplicate').length;
    assert.equal(JSON.parse(fs.readFileSync(path.join(storage,name,'report.json'))).passed,true);
  }
  assert.equal(removed,3,'four identical configs must retain exactly one original');
  const journal=fs.readFileSync(path.join(storage,'.fixture-store/manifest.jsonl'),'utf8').trim().split('\n').map(JSON.parse);
  assert.equal(journal.length,5,'three duplicate configs and two software logs');
});
