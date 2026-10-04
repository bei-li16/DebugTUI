// Unified functional acceptance; all child suites are sequential and isolated.
const {spawn, spawnSync} = require('node:child_process');
const fs = require('node:fs'), path = require('node:path'), assert = require('node:assert/strict');
const {root, hash, outputDirectory, parseOptions} = require('./test-support/session.cjs');
const options = parseOptions(process.argv.slice(2), ['--binary','--gdb','--cc','--only','--elf','--profile','--source-root','--svd','--project'], ['--hardware','--allow-reset','--allow-download']);
assert(!options['allow-reset'] || options.hardware, '--allow-reset requires --hardware');
assert(!options['allow-download'] || options.hardware, '--allow-download requires --hardware');
const binary = path.resolve(options.binary || path.join(root,'target/debug/debugtui.exe'));
assert(fs.existsSync(binary), 'Build first: powershell -File scripts/build.ps1');
const out = outputDirectory('functional');
const manifest = JSON.parse(fs.readFileSync(path.join(root,'tests/functional-coverage.json')));
const findTool = (explicit, name, candidates) => {
  if (explicit) return explicit;
  for (const candidate of candidates) if (fs.existsSync(candidate)) return candidate;
  const probe = spawnSync(process.platform==='win32'?'where.exe':'which',[name],{encoding:'utf8',windowsHide:true});
  return probe.status===0 ? probe.stdout.trim().split(/\r?\n/)[0] : null;
};
const gdb = findTool(options.gdb || process.env.DEBUGTUI_TEST_GDB, 'gdb', ['C:/MinGW/bin/gdb.exe','C:/Program Files/mingw64/bin/gdb.exe']);
const cc = findTool(options.cc || process.env.DEBUGTUI_TEST_CC, 'gcc', ['C:/MinGW/bin/gcc.exe','C:/Program Files/mingw64/bin/gcc.exe']);
const env = {...process.env, ...(gdb?{DEBUGTUI_TEST_GDB:gdb}:{}), ...(cc?{DEBUGTUI_TEST_CC:cc}:{})};
// Require PS7 for suites with intentional native stderr negative cases.
const powershell = findTool(null,process.platform==='win32'?'pwsh.exe':'pwsh',[]);
const ps = (id,file,args=[],native=false) => ({id,command:powershell,args:['-NoProfile','-ExecutionPolicy','Bypass','-File',path.join(root,file),...args],windows:true,native});
const js = (id,file,args=[],native=false) => ({id,command:process.execPath,args:[path.join(root,file),...args],native});
const suites = [
  ps('unit','scripts/build.ps1',['-Test']),
  js('cli','scripts/test-cli.cjs',['--binary',binary]),
  ps('terminal','scripts/test-terminal.ps1',['-Binary',binary]),
  ps('devices-tui','scripts/test-devices.ps1',['-Binary',binary]),
  js('devices-gdb','scripts/test-devices-gdb.cjs',[binary],true),
  ps('source-remap','scripts/test-source-remap.ps1',['-Binary',binary,'-Gdb',gdb||'gdb','-Compiler',cc||'gcc'],true),
  ps('distribution','scripts/test-distribution.ps1',['-Binary',binary]),
  js('exit','scripts/test-exit-policy.cjs',[binary]),
  ps('pause','scripts/test-pause.ps1',['-Binary',binary]),
  ps('environment','scripts/test-gdb-environment.ps1',['-Binary',binary]),
  ps('native','scripts/test-native-gdb.ps1',['-Binary',binary,'-Gdb',gdb||'gdb','-Compiler',cc||'gcc'],true),
  js('completion','scripts/test-completion-gdb.cjs',[binary],true),
  js('watch-tree','scripts/test-watch-tree-gdb.cjs',[binary],true),
  js('variable-write','scripts/test-variable-write-gdb.cjs',[binary],true),
  js('search','scripts/test-search-gdb.cjs',[binary],true),
  js('breakpoints','scripts/test-breakpoints-gdb.cjs',[binary],true),
  js('memory','scripts/test-memory-access-gdb.cjs',[binary],true),
  js('svd','scripts/test-svd-gdb.cjs',['--native',binary],true),
  js('logs','scripts/test-logs-gdb.cjs',[binary],true),
  js('multicore','scripts/test-multicore-gdb.cjs',[binary],true),
  js('linked','scripts/test-linked-breakpoints-gdb.cjs',[binary],true)
];
if (options.hardware) {
  assert(options.elf&&options.profile&&options['source-root'], '--hardware requires --elf, --profile and --source-root');
  const args=['--binary',binary];
  for (const key of ['elf','profile','source-root','svd','project']) if(options[key]) args.push(`--${key}`,options[key]);
  for (const key of ['allow-reset','allow-download']) if(options[key]) args.push(`--${key}`);
  suites.push(js('hardware','scripts/test-project-hardware.cjs',args));
  const hardwareEnv={...env,DEBUGTUI_TEST_ELF:path.resolve(options.elf),DEBUGTUI_TEST_PROFILE:path.resolve(options.profile)};
  suites.push({...js('hardware-breakpoints','scripts/test-breakpoints-gdb.cjs',[binary,'--hardware']),env:hardwareEnv,depends_on:['hardware']});
  suites.push({...js('hardware-memory','scripts/test-memory-access-gdb.cjs',[binary,'--hardware','--own-service']),env:hardwareEnv,depends_on:['hardware']});
}
const knownIds=new Set([...suites.map(s=>s.id),'hardware','hardware-breakpoints','hardware-memory']);
for(const feature of manifest.features) {
  assert(feature.required_suites.length>0, `Unmapped feature ${feature.id}`);
  for(const id of [...feature.required_suites,...(feature.optional_suites||[])]) assert(knownIds.has(id),`${feature.id}: unknown suite ${id}`);
  for(const source of feature.sources) assert(fs.existsSync(path.join(root,source)),`${feature.id}: missing source ${source}`);
}
const selected=options.only?new Set(options.only.split(',')):null;
if(selected) for(const id of selected) assert(suites.some(s=>s.id===id),`Unknown or disabled --only suite: ${id}`);
async function execute(suite) {
  if(selected&&!selected.has(suite.id)) return {id:suite.id,status:'not-selected'};
  if(suite.windows&&process.platform!=='win32') return {id:suite.id,status:'skipped',reason:'Requires Windows'};
  if(suite.windows&&!powershell) return {id:suite.id,status:'skipped',reason:'Requires PowerShell 7 (pwsh in PATH)'};
  if(suite.native&&(!gdb||!cc)) return {id:suite.id,status:'skipped',reason:'Provide --gdb and --cc for real native GDB acceptance'};
  console.log(`RUN ${suite.id}`);
  const start=Date.now(), logFile=path.join(out,`${suite.id}.log`), before=new Set(fs.readdirSync(path.join(root,'artifacts')));
  return await new Promise(resolve=>{
    const child=spawn(suite.command,suite.args,{cwd:root,env:suite.env||env,windowsHide:true});
    const log=fs.createWriteStream(logFile); let output='',timedOut=false,done=false;
    const add=data=>{output+=data.toString();log.write(data);}; child.stdout.on('data',add); child.stderr.on('data',add);
    const timer=setTimeout(()=>{
      timedOut=true;
      if(process.platform==='win32') spawnSync('taskkill.exe',['/PID',String(child.pid),'/T','/F'],{windowsHide:true,timeout:10000});
      else child.kill('SIGKILL');
    },suite.id==='unit'?300000:180000);
    const finish=async(code,error)=>{
      if(done)return;done=true;clearTimeout(timer); await new Promise(r=>log.end(r));
      const artifacts=fs.readdirSync(path.join(root,'artifacts')).filter(name=>!before.has(name)).map(name=>path.join(root,'artifacts',name));
      const reports=[];
      for(const directory of artifacts) for(const name of ['report.json','verification.json','result.json']) { const file=path.join(directory,name); if(fs.existsSync(file)) reports.push(file); }
      const result={id:suite.id,status:code===0&&!error&&!timedOut?'passed':'failed',exit_code:code,duration_ms:Date.now()-start,log:logFile,reports,error:error?.message,timed_out:timedOut};
      if(suite.id==='unit') result.test_counts=[...output.matchAll(/test result: ok\. (\d+) passed; \d+ failed; (\d+) ignored/g)].map(m=>({passed:Number(m[1]),ignored:Number(m[2])}));
      if(result.status==='failed') console.error(output.slice(-1800));
      console.log(`${result.status.toUpperCase()} ${suite.id} (${result.duration_ms} ms)`); resolve({...result,output});
    };
    child.once('error',error=>finish(null,error)); child.once('close',code=>finish(code));
  });
}
(async()=>{
  const results=[]; for(const suite of suites) {
    const missing=(suite.depends_on||[]).filter(id=>results.find(r=>r.id===id)?.status!=='passed');
    if(selected&&!selected.has(suite.id)) results.push({id:suite.id,status:'not-selected'});
    else if(missing.length) results.push({id:suite.id,status:'skipped',reason:`Prerequisite suite did not pass: ${missing.join(', ')}`});
    else results.push(await execute(suite));
  }
  const featureResults=manifest.features.map(feature=>{
    const required=feature.required_suites.map(id=>results.find(r=>r.id===id));
    const missing=feature.required_suites.filter(id=>results.find(r=>r.id===id)?.status!=='passed');
    const unit=results.find(r=>r.id==='unit');
    const unitEvidence=[...(feature.unit_pattern?[feature.unit_pattern]:[]),...(feature.unit_patterns||[])].map(pattern=>({pattern,tests:(unit?.output||'').split(/\r?\n/).filter(line=>line.includes(pattern)&&line.includes(' ... ok')).map(line=>line.trim())}));
    for(const evidence of unitEvidence) if(!evidence.tests.length) missing.push(`unit-pattern:${evidence.pattern}`);
    return {...feature,status:missing.length===0?'automated-passed':required.some(r=>r?.status==='failed')?'failed':'incomplete',missing,unit_evidence:unitEvidence,optional_results:(feature.optional_suites||[]).map(id=>({id,status:results.find(r=>r.id===id)?.status||'not-selected'}))};
  });
  const cleanResults=results.map(({output,...result})=>result);
  const addedCases=cleanResults.filter(r=>['cli','terminal','distribution','hardware'].includes(r.id)).flatMap(suite=>suite.reports?.filter(file=>path.basename(file)==='report.json').flatMap(file=>JSON.parse(fs.readFileSync(file,'utf8').replace(/^\ufeff/,'' )).cases.map(item=>({...item,suite:suite.id,report:file})))||[]);
  const addedCounts={passed:0,failed:0,skipped:0}; for(const item of addedCases) addedCounts[item.status]++;
  const failed=cleanResults.filter(r=>r.status==='failed').length, skipped=cleanResults.filter(r=>['skipped','not-selected'].includes(r.status)).length;
  const report={created_at:new Date().toISOString(),binary,sha256:hash(binary),gdb,cc,powershell,counts:{passed_suites:cleanResults.filter(r=>r.status==='passed').length,failed_suites:failed,skipped_suites:skipped,feature_groups:featureResults.length,automated_passed_features:featureResults.filter(f=>f.status==='automated-passed').length,added_cases:addedCounts},suites:cleanResults,added_cases:addedCases,features:featureResults,manual_acceptance:manifest.manual_acceptance,meaning:manifest.meaning};
  fs.writeFileSync(path.join(out,'report.json'),JSON.stringify(report,null,2));
  const link=file=>`[${path.basename(file)}](${path.relative(out,file).replaceAll('\\','/').replaceAll(' ','%20')})`;
  const markdown=['# DebugTUI 功能验收结果','',`被测程序：${binary}`,`SHA256：${report.sha256}`,'',`套件通过 ${report.counts.passed_suites}，失败 ${failed}，未执行 ${skipped}。`,`本轮新增用例：通过 ${addedCounts.passed}，失败 ${addedCounts.failed}，跳过 ${addedCounts.skipped}（复位/烧录选项见子报告）。`,'','## 套件和证据','','| 套件 | 状态 | 日志/详细报告 |','|---|---|---|',...cleanResults.map(s=>`| ${s.id} | ${s.status} | ${[...(s.log?[link(s.log)]:[]),...(s.reports||[]).map(link),s.reason||''].filter(Boolean).join(' · ')} |`),'','## 功能组','','| 功能组 | 自动化状态 | 未通过/未执行 | 验证限制 |','|---|---|---|---|',...featureResults.map(f=>`| ${f.id} ${f.name} | ${f.status} | ${[...f.missing,...f.optional_results.filter(r=>r.status!=='passed').map(r=>`${r.id}: ${r.status}`)].join(', ')} | ${f.limits.join('；')} |`),'','此表表示功能验收证据，不是代码覆盖率，也不代表穷举硬件和输入组合。','','## 仍需单独验收','',...manifest.manual_acceptance.map(item=>`- ${item}`)];
  fs.writeFileSync(path.join(out,'report.md'),markdown.join('\n'));
  console.log(`RESULT ${JSON.stringify(report.counts)} ${out}`);
  const selectedSkipped=cleanResults.some(r=>r.status==='skipped');
  if(failed||selectedSkipped||(!selected&&featureResults.some(f=>f.status!=='automated-passed'))) process.exitCode=1;
})().catch(error=>{console.error(error);process.exitCode=1;});
