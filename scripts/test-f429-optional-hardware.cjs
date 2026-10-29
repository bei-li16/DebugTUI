// Read-only F429 acceptance of PCSR/LSR and debugger-invisible ETM LAR.
// Use an isolated project with absolute firmware/catalogue paths. Never flash,
// reset, unlock, enable trace or change the user's project in this driver.
const fs=require('node:fs'),path=require('node:path'),net=require('node:net'),assert=require('node:assert/strict');
const {root,hash,outputDirectory,parseOptions,Cases,Session,delay}=require('./test-support/session.cjs');
const options=parseOptions(process.argv.slice(2),['--binary','--project','--original-project','--elf'],['--run']);
const out=outputDirectory('f429-optional-hardware');
const suite=new Cases(out,{board_tests_executed:false,scope:'F429 conditional read-only acceptance; no firmware or register writes'});
const save=(name,value)=>fs.writeFileSync(path.join(out,name+'.json'),JSON.stringify(value,null,2)+'\n');
if(!options.run){suite.skip('BOARD','Physical F429 test','Use --run with explicit project, original-project and elf');suite.finish();process.exit(0);}
for(const key of ['project','original-project','elf'])assert(options[key],`Missing --${key}`);
const binary=path.resolve(options.binary||path.join(root,'target/debug/debugtui.exe'));
const project=path.resolve(options.project),original=path.resolve(options['original-project']),elf=path.resolve(options.elf);
const preserved=Object.fromEntries([binary,project,original,elf].map(p=>[p,hash(p)]));
Object.assign(suite.metadata,{binary,binary_sha256:hash(binary),project,original_project:original,elf,preserved,reset:false,flash:false,register_writes:false});
let session,context,listing,probe;
const transactions=[];
function rpc(command){return new Promise((resolve,reject)=>{
 const socket=net.createConnection({host:'127.0.0.1',port:6666});let response='';
 socket.setTimeout(8000,()=>socket.destroy(Error('Tcl timeout')));socket.on('error',reject);
 socket.on('connect',()=>socket.write(command+'\x1a'));
 socket.on('data',chunk=>{response+=chunk;const end=response.indexOf('\x1a');if(end>=0){socket.end();const value=response.slice(0,end).trim();transactions.push({utc:new Date().toISOString(),command,response:value});resolve(value);}});
});}
async function reference(address){const raw=await rpc(`stm32f4x.cpu read_memory 0x${address.toString(16)} 32 1`);assert(/^(0x[0-9a-f]+|\d+)$/i.test(raw),raw);return BigInt(raw);}
const read=(ids,manual=false)=>session.command('registers_read',{ids,context,manual},true,45000);
const sample=(result,id)=>result.samples.find(s=>s.id===id);
const valid=s=>assert.equal(s?.state,'valid',JSON.stringify(s));
const noIO=s=>assert.equal(s.provenance?.access??null,null,JSON.stringify(s));
async function phase(id,description,body){if(!await suite.test(id,description,body))throw Error(`Failed phase ${id}`);}
(async()=>{try{
 session=new Session(binary,project,out);
 await phase('IDENTITY','Attach existing FreeRTOS firmware and establish physical F429 identity',async()=>{
  await session.command('connect');const status=await session.command('status');save('initial-status',status);
  assert.equal(status.state,'STOPPED');assert.equal(await rpc('stm32f4x.cpu curstate'),'halted');
  listing=await session.command('registers_list');context=listing.context;save('catalogue',listing);
  assert(listing.source.startsWith('file:'));const catalogue=listing.source.slice(5);preserved[catalogue]=hash(catalogue);
  Object.assign(suite.metadata,{catalogue,catalogue_sha256:hash(catalogue)});
  assert.equal(listing.catalogue.registers.length,617);assert.equal(listing.catalogue.registers.reduce((n,r)=>n+r.fields.length,0),1466);
  assert.equal(await reference(0xe000ed00),0x410fc241n);assert.equal((await reference(0xe0042000))&0xfffn,0x419n);
  suite.metadata.board_tests_executed=true;
  return {context,source:listing.source,cpuid:'0x410fc241',device_id:'0x419'};
 });
 await phase('PROBE','Probe applies declarative guards and separates access restriction from reader absence',async()=>{
  probe=await session.command('registers_probe',{context});save('probe',probe);
  assert.equal(probe.statistics.definitions,41);assert.equal(probe.statistics.unique_requests,39);
  assert.equal(probe.statistics.no_io,2);assert.equal(probe.statistics.causes.access_restricted,1);
  for(const id of ['dwt.pcsr','itm.lsr']){const s=probe.probe.samples.find(s=>s.id===id);valid(s);assert.equal(s.implementation,'unknown');}
  const lar=probe.probe.samples.find(s=>s.id==='etm.lar');assert.equal(lar.reason,'access_restricted');noIO(lar);
  noIO(probe.probe.samples.find(s=>s.id==='fpu.mvfr2'));
  for(const key of ['stm32f429.dwt.pcsr.present','stm32f429.itm.software_lock'])assert.equal(probe.probe.facts[key],undefined);
  return probe.statistics;
 });
 await phase('OPTIONAL-READS','PCSR and LSR read values agree with independent stopped OpenOCD word reads',async()=>{
  const results=[];
  for(const manual of [false,true]){
   const result=await read(['dwt.pcsr','itm.lsr','etm.lar','fpu.mvfr2','scs.pid5','scs.pid6','scs.pid7'],false);
   // The unknown four remain automatic; explicit manual testing targets only the proven-safe two and LAR.
   const checked=manual?await read(['dwt.pcsr','itm.lsr','etm.lar'],true):result;
   for(const [id,address] of [['dwt.pcsr',0xe000101c],['itm.lsr',0xe0000fb4]]){
    const s=sample(checked,id);valid(s);assert.equal(s.implementation,'unknown');assert.equal(BigInt(s.value.hex),await reference(address));
   }
   const lar=sample(checked,'etm.lar');assert.equal(lar.state,'unavailable');assert.equal(lar.reason,'access_restricted');assert(lar.detail.includes('UNPREDICTABLE'));noIO(lar);
   assert.equal(checked.statistics.causes.access_restricted,1);assert.equal(checked.statistics.causes.reader_unsupported,undefined);
   for(const id of ['fpu.mvfr2','scs.pid5','scs.pid6','scs.pid7']){const s=sample(result,id);assert.equal(s.state,'not_read');assert.equal(s.implementation,'unknown');noIO(s);}
   results.push({manual,result:checked});
  }
  save('optional-reads',results);return results.map(({manual,result})=>({manual,statistics:result.statistics,values:result.samples.filter(s=>s.state==='valid').map(s=>({id:s.id,value:s.value,implementation:s.implementation}))}));
 });
 await phase('ALL-DEFINITIONS','Read all 617 definitions automatically and verify SCB, NVIC and SysTick regression coverage',async()=>{
  const all=[];for(let i=0;i<listing.catalogue.registers.length;i+=48){const result=await read(listing.catalogue.registers.slice(i,i+48).map(r=>r.id));all.push(...result.samples);save('automatic-all',all);}
  const counts=all.reduce((a,s)=>(a[s.state]=(a[s.state]||0)+1,a),{});
  assert.deepEqual(counts,{valid:425,not_read:16,unsupported:169,unavailable:7});
  const scb=all.filter(s=>s.id.startsWith('scb.'));assert.equal(scb.length,31);scb.forEach(valid);
  for(const id of ['nvic.iser0','nvic.iser1','nvic.iser2','nvic.ipr90','systick.load','systick.val','systick.calib'])valid(all.find(s=>s.id===id));
  const comparisons=[];
  for(const s of [...scb,...all.filter(s=>['nvic.iser0','nvic.iser1','nvic.iser2','systick.load','systick.val','systick.calib'].includes(s.id))]){
   const definition=listing.catalogue.registers.find(r=>r.id===s.id);const raw=await reference(definition.reader.address);
   assert.equal(BigInt(s.value.hex),raw,s.id);comparisons.push({id:s.id,value:s.value.hex,reference:'0x'+raw.toString(16)});
  }
  save('regression-comparisons',comparisons);
  const matrix=await session.command('registers_matrix');save('matrix',matrix);
  const lar=matrix.rows.find(r=>r.id==='etm.lar');assert.equal(lar.support,'access_restricted');assert.equal(lar.plan.available,false);assert.equal(lar.automatic_eligible,false);
  return {states:counts,scb:31,independent_comparisons:comparisons.length};
 });
 await phase('RUNNING-GUARDS','Existing stopped-only PPB route refuses running optional reads and stale contexts',async()=>{
  const old=context;await session.command('continue');await delay(250);
  context=(await session.command('registers_list')).context;
  const running=await read(['dwt.pcsr','itm.lsr','etm.lar'],true);save('running-guards',running);
  for(const s of running.samples){assert.notEqual(s.state,'valid');noIO(s);}
  assert.equal(sample(running,'etm.lar').reason,'access_restricted');
  await session.command('pause');await session.command('wait_stopped',{timeout_ms:8000});
  context=(await session.command('registers_list')).context;assert(context.generation>old.generation);
  const expired=await session.command('registers_read',{ids:['dwt.pcsr'],context:old},false);assert(expired.includes('expired'));
  const before=await read(['dwt.pcsr','itm.lsr'],true);before.samples.forEach(noIO);
  await session.command('registers_probe',{context});const refreshed=await read(['dwt.pcsr','itm.lsr']);refreshed.samples.forEach(valid);save('refreshed',refreshed);
  return {old,current:context,running_no_io:true,stale_evidence_no_io:true};
 });
 await phase('NO-UNAUTHORIZED-IO','Retain MI/Tcl evidence and verify protected addresses never reached a reader',async()=>{
  const logs=session.logs('mi>').map(x=>x.text).join('\n');save('mi-commands',logs);
  for(const line of logs.split('\n')){
   assert(!/-data-write-memory|-data-write-register|-var-assign|\bmcr\b/.test(line),line);
   if(line.includes('-data-read-memory'))assert(!/0xe0041fb0|0xe000ef48|0xe000efd4|0xe000efd8|0xe000efdc/i.test(line),line);
  }
  assert(!transactions.some(t=>/0xe0041fb0|0xe000ef48|0xe000efd4|0xe000efd8|0xe000efdc/i.test(t.command)));
  const values=(await read(['dcb.demcr','etm.itctrl','tpiu.itctrl'])).samples;values.forEach(valid);save('final-controls',values);
  assert.equal(sample({samples:values},'dcb.demcr').value.hex,'0x01000000');
  assert.equal(sample({samples:values},'etm.itctrl').value.hex,'0x00000000');
  assert.equal(sample({samples:values},'tpiu.itctrl').value.hex,'0x00000000');
  return {protected_addresses_not_read:true,feature_enable_writes:false};
 });
}catch(error){save('failure',{message:error.message,stack:error.stack});if(!suite.results.some(r=>r.status==='failed'))await suite.test('ABORT','Finish physical validation',async()=>{throw error;});}
finally{
 save('reference-transactions',transactions);
 if(session)await suite.test('CLEANUP','Detach and close only test-owned processes',()=>session.close());
 await suite.test('PRESERVED','Preserve original project, ELF, test input and binary',async()=>{for(const [file,digest] of Object.entries(preserved))assert.equal(hash(file),digest,file);});
 suite.finish();
}})().catch(error=>{console.error(error);process.exitCode=1;});
