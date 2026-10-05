#!/usr/bin/env node
'use strict';
// Production packaging in a private checkout copy. All runtime operations are
// initialization or disconnected metadata; neither GDB nor a probe is started.
const fs=require('node:fs'), path=require('node:path'), assert=require('node:assert/strict');
const {spawnSync}=require('node:child_process');
const {root,outputDirectory,parseOptions,Cases,hash}=require('./test-support/session.cjs');
const options=parseOptions(process.argv.slice(2),['--binary','--previous-package','--previous-sha256','--previous-version']);
const binary=path.resolve(options.binary||path.join(root,'target/debug/debugtui.exe'));
const out=outputDirectory('register-distribution'), stage=path.join(out,'project 工程'), config=path.join(out,'npm 客户配置');
const metadata=JSON.parse(fs.readFileSync(path.join(root,'package.json'),'utf8'));
const cpus=['cortex-m4','cortex-r52','cortex-r52+'];
const required=['profiles/install.cjs','profiles/devices.toml',...cpus.map(cpu=>`profiles/registers/${cpu}.toml`)];
const historical=Boolean(options['previous-package']);
assert(historical===Boolean(options['previous-sha256']) && historical===Boolean(options['previous-version']),'Previous package, SHA256 and version must be provided together');
const suite=new Cases(out,{layer:'production npm/EXE/ZIP, private prefix and config roots',board_tests_executed:false,binary,binary_sha256:hash(binary),version:metadata.version,upgrade_baseline:historical?'verified historical package':'synthetic package replacement fixture; not a historical release'});
let assets, packed, oldFile, installed, preserved, oldExe, sequence=0;
const prefix=path.join(out,'private prefix'), cache=path.join(out,'npm-cache');
function environment(directory) { return {...process.env,DEBUGTUI_CONFIG_DIR:directory,npm_config_cache:cache,npm_config_offline:'true',npm_config_update_notifier:'false'}; }
function execute(command,args,extra={}) {
  const result=spawnSync(command,args,{encoding:'utf8',windowsHide:true,timeout:90000,maxBuffer:32*1024*1024,env:environment(config),...extra});
  const number=++sequence;
  fs.writeFileSync(path.join(out,`process-${number}.stdout.txt`),result.stdout||'');
  fs.writeFileSync(path.join(out,`process-${number}.stderr.txt`),result.stderr||'');
  assert.ifError(result.error);
  return result;
}
function success(command,args,extra) { const result=execute(command,args,extra); assert.equal(result.status,0,result.stderr||result.stdout); return result.stdout; }
function powershell(script,args=[],directory=config) { return execute('pwsh',['-NoProfile','-File',script,...args],{env:environment(directory)}); }
function npm(args,directory=config) {
  // Pass paths as single-quoted PowerShell array elements, not shell commands.
  const encoded=Buffer.from(`$ErrorActionPreference='Continue'; & npm.cmd @(${args.map(arg=>"'"+arg.replaceAll("'","''")+"'").join(',')}); exit $LASTEXITCODE`,'utf16le').toString('base64');
  return execute('pwsh',['-NoProfile','-EncodedCommand',encoded],{env:environment(directory)});
}
function npmSuccess(args,directory=config) { const result=npm(args,directory); assert.equal(result.status,0,result.stderr||result.stdout); return result.stdout; }
const lifecycle=['--global','--prefix',prefix,'--offline','--no-audit','--no-fund','--ignore-scripts=false','--foreground-scripts'];
function copyTree(source,destination) {
  const info=fs.lstatSync(source);
  if(info.isDirectory()) {
    fs.mkdirSync(destination,{recursive:true});
    for(const entry of fs.readdirSync(source)) copyTree(path.join(source,entry),path.join(destination,entry));
  } else {assert(info.isFile(),'Unexpected symlink in packaging source');fs.copyFileSync(source,destination);}
}
function snapshot(directory) {
  const entries={};
  function walk(dir) { for(const entry of fs.readdirSync(dir,{withFileTypes:true})) { const file=path.join(dir,entry.name), key=path.relative(directory,file).replaceAll('\\','/'); if(entry.isDirectory()) {entries[key+'/']='directory';walk(file);} else {assert(entry.isFile(),'Fixture contains unexpected non-file');entries[key]=hash(file);} } }
  if(fs.existsSync(directory)) walk(directory); return entries;
}
function profiles(directory) { return path.join(directory,'profiles'); }
function initialize(exe,directory) { success(exe,['--init-profiles'],{env:environment(directory)}); }
function clean(exe,directory) {
  initialize(exe,directory); const device=path.join(profiles(directory),'devices.toml'), before=hash(device);
  initialize(exe,directory); assert.equal(hash(device),before);
  assert.deepEqual(fs.readdirSync(path.join(profiles(directory),'registers')),[],'Do not copy embedded defaults into user overrides');
}
function seed(directory) {
  fs.mkdirSync(path.join(profiles(directory),'registers','客户 子目录'),{recursive:true});
  fs.writeFileSync(path.join(profiles(directory),'devices.toml'),"version=1\n[devices.matrix]\ncores=[0,2]\nbackend='generic'\n");
  fs.writeFileSync(path.join(profiles(directory),'customer-profile.toml'),'# customer tools profile, preserve verbatim\n');
  fs.writeFileSync(path.join(profiles(directory),'registers','cortex-m4.toml'),fs.readFileSync(path.join(root,'profiles/registers/cortex-m4.toml'),'utf8').replace('cpu = "cortex-m4"','cpu = "customer-model"'));
  fs.writeFileSync(path.join(profiles(directory),'registers','broken.toml'),'customer invalid data, also preserve verbatim\n');
  fs.writeFileSync(path.join(profiles(directory),'registers','客户 子目录','客户.toml'),'# nested customer file\n');
  fs.writeFileSync(path.join(directory,'debug.toml'),"version=2\nwatch=['customer-value']\n");
  return snapshot(directory);
}
function query(exe,directory,settings,multicore=false,expectError=false) {
  const project=path.join(out,`query-${sequence}.toml`);
  let raw="version=2\n[gdb]\nexecutable='./must-not-start-gdb.exe'\n[target]\nmode='local'\n[registers]\n"+settings+'\n';
  if(multicore) raw+="[[cores]]\nname='core.0'\nendpoint='localhost:5300'\n[[cores]]\nname='core.2'\nendpoint='localhost:5302'\n";
  fs.writeFileSync(project,raw);
  const requests=multicore?[['select_core',{index:0}],['registers_list',{}],['select_core',{index:1}],['registers_list',{}],['quit',{}]]:[['registers_list',{}],['quit',{}]];
  const result=execute(exe,['--project',project,'--headless','--stdio'],{env:environment(directory),input:requests.map(([method,params],i)=>JSON.stringify({id:i+1,method,params})).join('\n')+'\n'});
  if(expectError) {assert.notEqual(result.status,0);return result;}
  assert.equal(result.status,0,result.stderr);
  const events=result.stdout.split(/\r?\n/).filter(Boolean).map(line=>JSON.parse(line));
  assert(!events.some(event=>event.event==='log'&&event.channel==='mi>'),'No debugger IO during metadata verification');
  const responses=events.filter(event=>event.event==='response');
  assert.equal(responses.length,requests.length);
  for(const response of responses) assert.equal(response.ok,true,JSON.stringify(response));
  const lists=responses.filter(response=>requests[response.id-1][0]==='registers_list').map(response=>response.result);
  for(const list of lists) assert.equal(list.probe||null,null);
  if(multicore) {assert.deepEqual(lists.map(list=>list.context.core),['core.0','core.2']);assert.notEqual(lists[0].context.session,lists[1].context.session);}
  return lists;
}
function tomlPath(file) { return "catalogue='"+file.replaceAll('\\','/')+"'"; }
function parity(exe,directory,templates,multicore=false) {
  const results=[];
  for(const cpu of cpus) {
    const shipped=query(exe,directory,tomlPath(path.join(templates,`${cpu}.toml`)),multicore);
    const embedded=query(exe,directory,`cpu='${cpu}'`,multicore);
    for(let i=0;i<embedded.length;i++) {
      assert.equal(embedded[i].source,`builtin:${cpu}`);
      assert.equal(shipped[i].catalogue.cpu,cpu);
      assert.deepEqual(embedded[i].catalogue,shipped[i].catalogue,'Embedded definitions must match every shipped template field');
    }
    results.push({cpu,registers:embedded[0].catalogue.registers.length,source:embedded[0].source,cores:embedded.map(list=>list.context.core)});
  }
  return results;
}
function extract(file,destination) {
  const entries=success('tar.exe',['-tzf',file]).split(/\r?\n/).filter(Boolean);
  for(const entry of entries) assert(!entry.startsWith('/')&&!entry.includes('\\')&&!entry.split('/').includes('..')&&!/^[A-Za-z]:/.test(entry),'Unsafe package archive path');
  fs.mkdirSync(destination,{recursive:true}); success('tar.exe',['-xzf',file,'-C',destination]);
  return path.join(destination,'package');
}
function copyProject(destination) {
  fs.mkdirSync(path.join(destination,'target','release'),{recursive:true});
  fs.copyFileSync(binary,path.join(destination,'target','release','debugtui.exe'));
  fs.copyFileSync(path.join(root,'package.json'),path.join(destination,'package.json'));
  for(const item of metadata.files) if(item!=='bin/') {
    const relative=item.replace(/[\\/]+$/,'');
    copyTree(path.join(root,relative),path.join(destination,relative));
  }
  fs.mkdirSync(path.join(destination,'scripts'),{recursive:true});
  for(const script of ['package.ps1','release-assets.ps1','build.ps1']) fs.copyFileSync(path.join(root,'scripts',script),path.join(destination,'scripts',script));
}
function assertPreserved() { assert.deepEqual(snapshot(config),preserved,'Install/upgrade must retain all customer files and directories'); }
(async()=>{
  try {
    await suite.test('REG-PKG-PRODUCTION','Run production npm and release packers in an isolated source copy',async()=>{
      copyProject(stage);
      const result=powershell(path.join(stage,'scripts/release-assets.ps1'),['-SkipBuild']); assert.equal(result.status,0,result.stderr||result.stdout);
      assets=JSON.parse(fs.readFileSync(path.join(stage,'artifacts/release-assets.json'),'utf8').replace(/^\uFEFF/,''));
      packed=extract(assets.tgz,path.join(out,'unpacked npm'));
      for(const file of required) assert.equal(hash(path.join(packed,file)),hash(path.join(root,file)));
      assert.equal(hash(path.join(packed,'bin/debugtui.exe')),hash(binary));
      for(const file of assets.assets) assert(fs.readFileSync(assets.sha256sums,'utf8').includes(hash(file)+'  '+path.basename(file)));
      assert(!fs.existsSync(path.join(packed,'tools')));
      return {assets:assets.assets,sha256sums:assets.sha256sums};
    });
    await suite.test('REG-PKG-OMITTED','Production packer rejects missing catalogue/profile payloads',async()=>{
      const bad=path.join(out,'omitted profiles');copyProject(bad);
      const wrong={...metadata,files:metadata.files.filter(item=>item!=='profiles/')};fs.writeFileSync(path.join(bad,'package.json'),JSON.stringify(wrong));
      const result=powershell(path.join(bad,'scripts/package.ps1'),['-SkipBuild']);
      assert.notEqual(result.status,0);assert((result.stderr+result.stdout).includes('Register catalogue/profile asset omitted'));return {rejected:true};
    });
    await suite.test('REG-PKG-DIRECT','Direct EXE initializes empty extension location and contains all built-ins',async()=>{
      const directory=path.join(out,'direct EXE'), user=path.join(out,'direct config');fs.mkdirSync(directory);const exe=path.join(directory,'debugtui.exe');fs.copyFileSync(assets.exe,exe);
      assert.equal(hash(exe),hash(binary));clean(exe,user);return {catalogues:parity(exe,user,path.join(packed,'profiles/registers'))};
    });
    await suite.test('REG-PKG-ZIP','ZIP contains exact templates and independent multicore embedded catalogues',async()=>{
      const directory=path.join(out,'ZIP install'), user=path.join(out,'ZIP config');
      const result=powershell(path.join(root,'scripts/test-support/extract-register-zip.ps1'),['-Archive',assets.zip,'-Destination',directory]);assert.equal(result.status,0,result.stderr);
      for(const file of required) assert.equal(hash(path.join(directory,file)),hash(path.join(root,file)));
      const exe=path.join(directory,'debugtui.exe');assert.equal(hash(exe),hash(binary));clean(exe,user);
      return {catalogues:parity(exe,user,path.join(directory,'profiles/registers'),true)};
    });
    await suite.test('REG-PKG-INIT-PRESERVE','Repeated init preserves arbitrary valid/invalid and nested customer content',async()=>{
      const user=path.join(out,'init 客户');initialize(binary,user);const before=seed(user);initialize(binary,user);initialize(binary,user);assert.deepEqual(snapshot(user),before);return {customer_files:before};
    });
    await suite.test('REG-PKG-OLD','Install verified previous package or explicitly labeled offline fixture',async()=>{
      if(historical) {oldFile=path.resolve(options['previous-package']);assert.equal(hash(oldFile),options['previous-sha256'].toLowerCase());}
      else {
        const old=path.join(out,'old fixture');fs.mkdirSync(path.join(old,'bin'),{recursive:true});fs.copyFileSync(binary,path.join(old,'bin/debugtui.exe'));
        fs.writeFileSync(path.join(old,'package.json'),JSON.stringify({name:metadata.name,version:'0.0.0-fixture',bin:{debugtui:'bin/debugtui.exe'},files:['bin/']}));
        const result=JSON.parse(npmSuccess(['pack',old,'--json','--pack-destination',out,'--offline']));oldFile=path.join(out,result[0].filename);
      }
      npmSuccess(['install',...lifecycle,oldFile]);installed=path.join(prefix,'node_modules',metadata.name);
      const version=JSON.parse(fs.readFileSync(path.join(installed,'package.json'),'utf8')).version;
      assert.equal(version,historical?options['previous-version']:'0.0.0-fixture');
      oldExe=path.join(extract(oldFile,path.join(out,'old package bytes')),'bin/debugtui.exe');
      if(historical) assert.equal(success(oldExe,['--version']).trim(),`debugtui ${version}`);
      initialize(path.join(installed,'bin/debugtui.exe'),config);preserved=seed(config);
      return {kind:historical?'historical':'synthetic',package:oldFile,sha256:hash(oldFile),version,binary_sha256:hash(oldExe)};
    });
    await suite.test('REG-PKG-UPGRADE','npm upgrade executes real hook and preserves customer profiles and directories',async()=>{
      const output=npmSuccess(['install',...lifecycle,assets.tgz]);assert(output.includes('node profiles/install.cjs'),'Real postinstall hook must run');assert.equal(hash(path.join(installed,'bin/debugtui.exe')),hash(binary));assertPreserved();
      assert.equal(JSON.parse(fs.readFileSync(path.join(installed,'package.json'),'utf8')).version,metadata.version);return {installed_binary_sha256:hash(binary)};
    });
    await suite.test('REG-PKG-ENTRIES','Installed CMD/PowerShell entries and per-core source resolution match package',async()=>{
      assert.equal(JSON.parse(fs.readFileSync(path.join(installed,'package.json'),'utf8')).version,metadata.version);
      assert.equal(hash(path.join(installed,'bin/debugtui.exe')),hash(binary));
      for(const entry of ['debugtui.cmd','debugtui.ps1']) {
        const script=`& '${path.join(prefix,entry).replaceAll("'","''")}' --version; exit $LASTEXITCODE`;
        assert.equal(success('pwsh',['-NoProfile','-EncodedCommand',Buffer.from(script,'utf16le').toString('base64')]).trim(),`debugtui ${metadata.version}`);
      }
      const exe=path.join(installed,'bin/debugtui.exe');const builtins=query(exe,config,"cpu='cortex-r52'",true);assert(builtins.every(list=>list.source==='builtin:cortex-r52'));
      const users=query(exe,config,"cpu='cortex-m4'",true);assert(users.every(list=>list.source.startsWith('user:')&&list.catalogue.cpu==='customer-model'));
      assertPreserved();return {builtins:builtins.map(list=>list.source),users:users.map(list=>list.source)};
    });
    await suite.test('REG-PKG-REPEAT','Same-version package reinstall invokes idempotent init',async()=>{npmSuccess(['install',...lifecycle,assets.tgz]);assertPreserved();assert.equal(hash(path.join(installed,'bin/debugtui.exe')),hash(binary));});
    await suite.test('REG-PKG-UNINSTALL','Private-prefix uninstall preserves external user data',async()=>{npmSuccess(['uninstall',...lifecycle,metadata.name]);assert(!fs.existsSync(path.join(prefix,'debugtui.cmd')));assertPreserved();});
    await suite.test('REG-PKG-REINSTALL','Fresh reinstall preserves data and restores exact package contents',async()=>{npmSuccess(['install',...lifecycle,assets.tgz]);assertPreserved();for(const file of required) assert.equal(hash(path.join(installed,file)),hash(path.join(root,file)));});
    await suite.test('REG-PKG-EXE-ZIP-UPGRADE','Direct EXE and ZIP replacements over previous binary preserve external user data',async()=>{
      const upgrades=[];
      for(const kind of ['EXE','ZIP']) {
        const directory=path.join(out,`${kind} upgrade`), user=path.join(out,`${kind} upgrade config`);fs.mkdirSync(directory);const exe=path.join(directory,'debugtui.exe');fs.copyFileSync(oldExe,exe);initialize(exe,user);const before=seed(user);
        if(kind==='EXE') fs.copyFileSync(assets.exe,exe);
        else for(const entry of fs.readdirSync(path.join(out,'ZIP install'))) copyTree(path.join(out,'ZIP install',entry),path.join(directory,entry));
        initialize(exe,user);assert.deepEqual(snapshot(user),before);assert.equal(hash(exe),hash(binary));
        const lists=query(exe,user,"cpu='cortex-r52+'",true);assert(lists.every(list=>list.source==='builtin:cortex-r52+'));
        upgrades.push({kind,previous_binary_sha256:hash(oldExe),current_binary_sha256:hash(exe)});
      }
      return {upgrades};
    });
    await suite.test('REG-PKG-INVALID-OVERRIDE','Init and upgrade preserve broken/directory overrides; reader reports errors without fallback',async()=>{
      const file=path.join(profiles(config),'registers','cortex-r52+.toml'), errors=[];
      for(const kind of ['broken-file','directory']) {
        if(kind==='broken-file') fs.writeFileSync(file,'broken=[');
        else {fs.unlinkSync(file);fs.mkdirSync(file);}
        const before=snapshot(config);initialize(binary,config);npmSuccess(['install',...lifecycle,assets.tgz]);assert.deepEqual(snapshot(config),before);
        const result=query(path.join(installed,'bin/debugtui.exe'),config,"cpu='cortex-r52+'",false,true);assert(result.stderr.includes('cortex-r52+.toml'));errors.push({kind,error:result.stderr.trim()});
      }
      return {errors};
    });
    await suite.test('REG-PKG-HOOK-ERROR','Failed init makes npm installation fail without replacing invalid customer files',async()=>{
      const user=path.join(out,'invalid devices');fs.mkdirSync(path.join(profiles(user),'registers'),{recursive:true});fs.writeFileSync(path.join(profiles(user),'devices.toml'),'version=999\n');const before=snapshot(user);
      const result=npm(['install','--global','--prefix',path.join(out,'failed prefix'),'--offline','--no-audit','--no-fund','--ignore-scripts=false','--foreground-scripts',assets.tgz],user);
      assert.notEqual(result.status,0);assert((result.stderr+result.stdout).includes('Device catalogue initialization failed'));assert.deepEqual(snapshot(user),before);return {error:result.stderr.trim()};
    });
  } catch(error) {await suite.test('REG-PKG-SETUP','Prepare distribution fixture',async()=>{throw error;});}
  finally {suite.finish();}
})();
