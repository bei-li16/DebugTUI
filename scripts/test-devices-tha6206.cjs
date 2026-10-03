// Explicit opt-in board regression. Uses MCAL chipreset; never builds/downloads.
const {spawnSync}=require('node:child_process');
const fs=require('node:fs'),path=require('node:path'),assert=require('node:assert/strict');
const {root,quote,hash,delay,outputDirectory,Cases,Session}=require('./test-support/session.cjs');
const [binaryArg,projectArg,confirmation]=process.argv.slice(2);
assert(confirmation==='--allow-reset','Usage: node scripts/test-devices-tha6206.cjs BINARY MCAL_PROJECT --allow-reset');
const binary=path.resolve(binaryArg),projectRoot=path.resolve(projectArg);
const sourceFile=path.join(projectRoot,'debug-chip.toml'),profile=path.join(projectRoot,'.vscode/debug-env-chip.toml');
const source=fs.readFileSync(sourceFile,'utf8');
const hashes=[sourceFile,profile].map(hash);
const ports=spawnSync('netstat.exe',['-ano','-p','TCP'],{encoding:'utf8',windowsHide:true,timeout:10000});
assert.ifError(ports.error);assert.equal(ports.status,0,ports.stderr);
assert(!ports.stdout.split(/\r?\n/).some(line=>/LISTENING/.test(line)&&/^\s*TCP\s+\S+:(3333|3334|6666)\s/.test(line)),'Close existing probe sessions first');
const out=outputDirectory('devices-tha6206'),suite=new Cases(out,{binary,sha256:hash(binary),projectRoot,layer:'THA6206 physical board; MCAL startup reset allowed; no download'});
const rootSlash=projectRoot.replaceAll('\\','/');
const adjusted=source.replace('profile = ".vscode/debug-env-chip.toml"',`profile = ${quote(profile)}`)
 .replace('source_root = "."',`source_root = ${quote(projectRoot)}`)
 .replaceAll('"DemoWorkspace/',`"${rootSlash}/DemoWorkspace/`).replace('svd = ".vscode/',`svd = "${rootSlash}/.vscode/`);
(async()=>{
 for(const ids of [[0],[1],[0,1]]){
  const passed=await suite.test(ids.join('+'),'chip-selected physical cores: connect, Run, Pause, per-core registers/Watch, Continue',async()=>{
   const directory=path.join(out,ids.join('-'));fs.mkdirSync(directory);
   const config=path.join(directory,'debug.toml');fs.writeFileSync(config,adjusted);
   const s=new Session(binary,config,directory,['--chip','tha6206','--cores',ids.join(',')]);
   try{
    const connected=await s.command('connect',{},true,65000);assert.deepEqual(connected.core_names,ids.map(id=>`core.${id}`));
    await s.command('run');
    if(ids.includes(0)){await s.command('select_core',{name:'core.0'});await s.command('wait_stopped',{timeout_ms:12000});}
    await s.command('pause');
    const frames=[];
    for(const id of ids){await s.command('select_core',{name:`core.${id}`});await s.command('watch',{expression:'uart_cnt'});const st=await s.command('status');assert.equal(st.state,'STOPPED');assert(st.registers.some(r=>r.name==='pc'));assert(st.watches.some(w=>w.name==='uart_cnt'&&!w.error));frames.push(st.frame);}
    await s.command('continue');await delay(250);const running=await s.command('status');assert(running.cores.every(c=>c.state==='RUNNING'));
    await s.command('pause');assert((await s.command('status')).cores.every(c=>c.state==='STOPPED'));
    return {core_names:connected.core_names,frames};
   }finally{await s.close();}
  });
  if(!passed){for(const rest of [[0],[1],[0,1]].slice([[0],[1],[0,1]].findIndex(v=>v.join()===ids.join())+1))suite.skip(rest.join('+'),'remaining physical mode','Earlier board test failed; inspect logs before further resets');break;}
 }
 assert.deepEqual([sourceFile,profile].map(hash),hashes,'Reference configurations changed during test');
 suite.finish();
})().catch(error=>{console.error(error);process.exitCode=1;});
