// Deferred REG-H03. Requires a core-private, independently sampled paused hook.
const fs=require('node:fs'), path=require('node:path'), assert=require('node:assert/strict');
const {root,hash,outputDirectory,parseOptions,Cases,Session}=require('./test-support/session.cjs');
const options=parseOptions(process.argv.slice(2),['--binary','--project','--core','--case'],['--run','--software-fixture']);
const phases=['REG-H03-STOP','REG-H03-EVIDENCE','REG-H03-VIEWS','REG-H03-UNCHANGED'];
const out=outputDirectory('register-vfp-hardware');
const suite=new Cases(out,{todo:'REG-H03 VFP D16/D32, S/D/Q overlap, EN=0, TCP10, physical context and owner',
  layer:options['software-fixture']?'software fixture; no board':options.run?'explicit paused VFP hook':'deferred; no target access',
  board_tests_executed:!!options.run&&!options['software-fixture']});
if(!options.run){for(const id of phases)suite.skip(id,'VFP data and controls','Requires --run and independent per-core firmware reference; startup establishes permissions and FP state');suite.finish();process.exit(0);}
assert(options.project&&options.core&&options.case,'--run requires --project FILE --core NAME --case JSON');
const binary=path.resolve(options.binary||path.join(root,'target/release/debugtui.exe'));
const project=path.resolve(options.project),caseFile=path.resolve(options.case);
for(const file of [binary,project,caseFile])assert(fs.existsSync(file),`Missing ${file}`);
const spec=JSON.parse(fs.readFileSync(caseFile));
assert(!spec.software_example||options['software-fixture'],'Replace software expectations before physical execution');
assert(spec.frame_function&&spec.evidence_source&&spec.expected_midr,'Declare paused hook and independent evidence');
assert.equal(Number(spec.expected_mode),0x1a,'This adapter requires Hyp; EL1/Guest have separate pending cases');
assert(/^[A-Za-z_]\w*$/.test(spec.reference),'Use a core-private unsigned-int firmware array');
assert(['r0','r1','pc','cpsr','hcptr'].every(id=>spec.stable_registers?.includes(id)),'Guard both scratch registers, PC, CPSR and HCPTR');
const controls=['fpsid','mvfr0','mvfr1','mvfr2','fpexc','fpscr'];
const exact=(text,bits=32)=>{assert(new RegExp(`^0x[0-9a-f]{${bits/4}}$`,'i').test(text),`Exact ${bits}-bit raw required: ${text}`);return BigInt(text);};
const projectHash=hash(project);
Object.assign(suite.metadata,{binary,binary_sha256:hash(binary),project,project_sha256:projectHash,
  case_file:caseFile,case_sha256:hash(caseFile),evidence_source:spec.evidence_source,
  physical_scratch_check:'Adapter physically restores and reads back R0/R1; logical GDB controls are supplementary'});
let session,context,before,peerBefore,trapped,enabled,count,neon,reference;
const stable=async()=>{
  const ctx=(await session.command('registers_list')).context;
  const read=await session.command('registers_read',{context:ctx,ids:spec.stable_registers,manual:true});
  return Object.fromEntries(spec.stable_registers.map(id=>{const sample=read.samples.find(s=>s.id===id);
    assert.equal(sample?.state,'valid',`${id}: ${sample?.detail}`);assert.equal(sample.owner,`core:${ctx.core}`);exact(sample.value.hex);return [id,sample.value.hex];}));
};
const select=async core=>{
  if(core!=='default'||spec.peer_core)await session.command('select_core',{name:core});
  const status=await session.command('status');assert.equal(status.state,'STOPPED');
  assert.equal(status.frame.level,0);assert.equal(status.frame.function,spec.frame_function);
};
const word=async index=>{
  const result=await session.command('evaluate',{expression:`((unsigned int *)&${spec.reference})[${index}]`});
  assert(/^(0x[0-9a-f]+|\d+)$/i.test(result.value),'Independent firmware word must be an unsigned integer');
  const value=BigInt(result.value);assert(value>=0n&&value<(1n<<32n));return value;
};
(async()=>{
  try{
    session=new Session(binary,project,out);await session.command('connect');
    if(spec.control_scope){assert(['core','all'].includes(spec.control_scope));await session.command('control_scope',{scope:spec.control_scope});}
    if(!await suite.test(phases[0],'Capture physical owner, independent ready hook and peer controls',async()=>{
      if(spec.peer_core){assert.notEqual(spec.peer_core,options.core);await select(spec.peer_core);peerBefore=await stable();}
      await select(options.core);context=(await session.command('registers_list')).context;
      assert.equal(context.core,options.core);before=await stable();assert.equal(Number(exact(before.cpsr)&31n),0x1a);
      const ready=await session.command('evaluate',{expression:`*(unsigned int *)&${spec.reference}_ready`});assert.equal(BigInt(ready.value),1n);
      assert.equal(await word(0),exact(before.hcptr));trapped=(exact(before.hcptr)&(1n<<10n))!==0n;
      assert.equal(trapped,!!spec.expected_trapped);return {context,before,peerBefore,trapped};
    }))return;
    if(!await suite.test(phases[1],'Compare MVFR/FPEXC raw controls and capability provenance with independent samples',async()=>{
      const probe=await session.command('registers_probe',{context});
      assert.equal(probe.probe.identity.model,'Cortex-R52');
      assert.equal(probe.probe.samples.find(s=>s.id==='midr').value.hex,spec.expected_midr);
      reference={};
      if(trapped){
        for(const id of controls.filter(id=>id!=='fpscr')){const sample=probe.probe.samples.find(s=>s.id===id);assert.equal(sample.reason,'access_restricted');assert.equal(sample.value,null);}
        for(const key of ['vfp.present','vfp.d_registers','vfp.neon','vfp.enabled'])assert.equal(probe.probe.facts[key],undefined);
      }else{
        for(let index=0;index<5;index++){const id=controls[index];reference[id]=await word(index+1);
          const sample=probe.probe.samples.find(s=>s.id===id);assert.equal(sample.state,'valid',sample.detail);assert.equal(exact(sample.value.hex),reference[id]);}
        const layout=Number(reference.mvfr0&15n);assert([1,2].includes(layout));count=layout===1?16:32;neon=count===32;
        assert.equal(reference.mvfr0,exact(spec.expected_mvfr0));assert.equal(reference.mvfr1,exact(spec.expected_mvfr1));
        enabled=(reference.fpexc&(1n<<30n))!==0n;assert.equal(enabled,!!spec.expected_enabled);
        for(const [key,value]of [['vfp.present',1],['vfp.d_registers',count],['vfp.double_precision',Number(neon)],['vfp.neon',Number(neon)],['vfp.enabled',Number(enabled)]]){
          assert.equal(probe.probe.facts[key]?.value,value,key);assert(probe.probe.facts[key]?.source);}
        if(enabled)reference.fpscr=await word(6);
      }
      return probe;
    }))return;
    if(!await suite.test(phases[2],'Compare all raw S/D/Q views and typed disabled/absent/restricted states',async()=>{
      const d=[];if(!trapped&&enabled)for(let index=0;index<count;index++)d.push((await word(8+2*index))|((await word(9+2*index))<<32n));
      const ids=[...Array.from({length:32},(_,n)=>`d${n}`),...Array.from({length:32},(_,n)=>`s${n}`),...Array.from({length:16},(_,n)=>`q${n}`),...controls];
      const read=await session.command('registers_read',{context,ids,manual:true});
      for(const sample of read.samples){
        assert.equal(sample.owner,`core:${context.core}`);assert.deepEqual(sample.context,context);
        const id=sample.id,index=Number(id.slice(1)),data=/^[dsq]\d+$/.test(id);
        const absent=!trapped&&data&&((id[0]==='d'&&index>=count)||(id[0]==='q'&&!neon));
        const reason=trapped?'access_restricted':absent?'hardware_not_implemented':!enabled&&(data||id==='fpscr')?'feature_disabled':null;
        if(reason){assert.equal(sample.reason,reason,`${id}: ${sample.detail}`);assert.equal(sample.value,null);if(absent)assert.equal(sample.implementation,'no');continue;}
        assert.equal(sample.state,'valid',`${id}: ${sample.detail}`);
        const expected=!data?reference[id]:id[0]==='d'?d[index]:id[0]==='s'?(d[Math.floor(index/2)]>>BigInt(32*(index%2)))&0xffffffffn:d[index*2]|(d[index*2+1]<<64n);
        assert.equal(exact(sample.value.hex,data?(id[0]==='d'?64:id[0]==='q'?128:32):32),expected,id);
        if(data){
          const access=sample.provenance?.access, proof=access?.vfp_pair;
          assert.equal(access?.phase,'responded');assert.deepEqual(access?.context,context);
          assert.equal(access?.route?.kind,'tcl_register');assert(access.route.operation.startsWith('VFP read '));
          const first=id[0]==='q'?2*index:id[0]==='d'?2*Math.floor(index/2):2*Math.floor(index/4);
          assert.equal(proof?.first_d,first);assert.equal(proof.raw.bits,128);
          assert.equal(exact(proof.raw.hex,128),d[first]|(d[first+1]<<64n));
          for(const field of ['mvfr0','mvfr1','fpexc']){assert.equal(proof[field].bits,32);assert.equal(exact(proof[field].hex),reference[field]);}
          assert(Number.isInteger(access.timestamp_ms)&&Number.isInteger(access.completed_ms)&&access.completed_ms>=access.timestamp_ms);
        }
      }
      return read;
    }))return;
    await suite.test(phases[3],'Verify controls, peer, physical context and customer configuration unchanged',async()=>{
      await select(options.core);assert.deepEqual((await session.command('registers_list')).context,context);assert.deepEqual(await stable(),before);
      if(!trapped){const read=await session.command('registers_read',{context,ids:['fpexc'],manual:true});assert.equal(exact(read.samples[0].value.hex),reference.fpexc);}
      if(spec.peer_core){await select(spec.peer_core);assert.deepEqual(await stable(),peerBefore);await select(options.core);}
      assert(!session.logs('mi>').some(log=>/-exec-|\b-data-write-|\b-var-assign/.test(log.text)));assert.equal(hash(project),projectHash);
      return {controls_unchanged:true,peer_unchanged:!!peerBefore,no_enable_or_mode_write:true};
    });
  }catch(error){await suite.test('REG-H03-DRIVER','Driver failure',async()=>{throw error;});}
  finally{
    if(session)await suite.test('REG-H03-CLEANUP','Close owned debugger session',()=>session.close());
    assert.equal(hash(project),projectHash);
    for(const id of phases)if(!suite.results.some(item=>item.id===id))suite.skip(id,'Deferred phase','Earlier precondition failed');
    suite.finish();
  }
})();
