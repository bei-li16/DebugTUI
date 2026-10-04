// Deferred REG-H02. Run only at the dedicated paused read-only firmware hook.
const fs = require('node:fs'), path = require('node:path'), assert = require('node:assert/strict');
const {root, hash, outputDirectory, parseOptions, Cases, Session} = require('./test-support/session.cjs');
const options = parseOptions(process.argv.slice(2), ['--binary','--project','--core','--case'], ['--run','--software-fixture']);
const phases = ['REG-H02-STOP','REG-H02-IDENTITY','REG-H02-BANKS','REG-H02-UNCHANGED'];
const out = outputDirectory('register-banked-hardware');
const suite = new Cases(out, {todo:'REG-H02 current/noncurrent banks, physical mode, scratch restoration and core ownership',
  layer:options['software-fixture'] ? 'software fixture; no board' : options.run ? 'explicit paused bank fixture' : 'deferred; no target access',
  board_tests_executed:!!options.run && !options['software-fixture']});
if (!options.run) {
  for (const id of phases) suite.skip(id, 'Physical banked register access', 'Requires --run and independent per-core firmware baseline at a dedicated paused hook');
  suite.finish(); process.exit(0);
}
assert(options.project && options.core && options.case, '--run requires --project FILE --core NAME --case JSON');
const binary = path.resolve(options.binary || path.join(root,'target/release/debugtui.exe'));
const project = path.resolve(options.project), caseFile = path.resolve(options.case);
for (const file of [binary,project,caseFile]) assert(fs.existsSync(file), `Missing ${file}`);
const spec = JSON.parse(fs.readFileSync(caseFile));
assert(!spec.software_example || options['software-fixture'], 'Replace software expectations before physical execution');
assert(spec.frame_function && spec.evidence_source && spec.expected_midr, 'Declare dedicated stop hook and independent evidence');
const mode = Number(spec.expected_mode);
assert([0x10,0x11,0x12,0x13,0x17,0x1a,0x1b,0x1f].includes(mode), 'Declare an actual R52 mode');
assert(Array.isArray(spec.stable_registers) && ['r0','pc','cpsr'].every(id => spec.stable_registers.includes(id)), 'Guard scratch, PC and CPSR');
const names = ['sp_irq','lr_irq','spsr_irq','r8_fiq','r9_fiq','r10_fiq','r11_fiq','r12_fiq','sp_fiq','lr_fiq','spsr_fiq','sp_und','lr_und','spsr_und','sp_abt','lr_abt','spsr_abt','sp_svc','lr_svc','spsr_svc','sp_hyp','elr_hyp','spsr_hyp'];
assert(Array.isArray(spec.banks) && spec.banks.length === names.length && new Set(spec.banks.map(e=>e.id)).size === names.length);
for (const entry of spec.banks) {
  assert(names.includes(entry.id));
  const unavailable = mode === 0x10 || (mode !== 0x1a && entry.id.endsWith('_hyp'));
  assert.equal(!!entry.unavailable, unavailable, `Architecture legality for ${entry.id}`);
  if (!unavailable) assert(/^[a-zA-Z_]\w*(\[\d+\])?$/.test(entry.reference), 'Reference must be a firmware variable/array element');
}
const raw = text => { assert(/^0x[0-9a-f]{8}$/i.test(text), `Exact 32-bit raw required: ${text}`); return BigInt(text); };
const projectHash = hash(project);
Object.assign(suite.metadata, {binary,binary_sha256:hash(binary),project,project_sha256:projectHash,
  case_file:caseFile,case_sha256:hash(caseFile),evidence_source:spec.evidence_source,
  physical_scratch_check:'Adapter reads back physical R0; GDB controls are supplementary logical views'});
let session, context, before, peerBefore;
const controls = async () => {
  const current = (await session.command('registers_list')).context;
  const response = await session.command('registers_read',{context:current,ids:spec.stable_registers,manual:true});
  return Object.fromEntries(spec.stable_registers.map(id=>{
    const sample = response.samples.find(s=>s.id===id);
    assert.equal(sample?.state,'valid',`${id}: ${sample?.detail}`);
    assert.equal(sample.owner,`core:${current.core}`); raw(sample.value.hex);
    return [id,sample.value.hex];
  }));
};
const select = async core => {
  if (core !== 'default' || spec.peer_core) await session.command('select_core',{name:core});
  const status = await session.command('status');
  assert.equal(status.state,'STOPPED'); assert.equal(status.frame.level,0);
  assert.equal(status.frame.function,spec.frame_function);
};
(async()=>{
  try {
    session = new Session(binary,project,out); await session.command('connect');
    if (spec.control_scope) { assert(['core','all'].includes(spec.control_scope)); await session.command('control_scope',{scope:spec.control_scope}); }
    if (!await suite.test(phases[0],'Capture current-core controls, mode and optional peer',async()=>{
      if(spec.peer_core){assert.notEqual(spec.peer_core,options.core);await select(spec.peer_core);peerBefore=await controls();}
      await select(options.core); context=(await session.command('registers_list')).context;
      assert.equal(context.core,options.core); before=await controls();
      assert.equal(Number(raw(before.cpsr)&31n),mode);
      return {context,before,peerBefore};
    })) return;
    if (!await suite.test(phases[1],'Validate current-core identity, without privileged probe in User mode',async()=>{
      if(mode === 0x10) return {user_mode:true,identity_probe_omitted:true,expected_access:'unavailable'};
      const response = await session.command('registers_probe',{context});
      assert.equal(response.probe.identity.model,'Cortex-R52');
      assert.equal(response.probe.samples.find(s=>s.id==='midr').value.hex,spec.expected_midr);
      return response;
    })) return;
    if (!await suite.test(phases[2],'Compare all banks with independent firmware samples and legal refusals',async()=>{
      const references = {};
      for(const entry of spec.banks.filter(e=>!e.unavailable)){
        const result=await session.command('evaluate',{expression:`(unsigned int)${entry.reference}`});
        assert(/^(0x[0-9a-f]+|\d+)$/i.test(result.value));
        references[entry.id]=BigInt(result.value); assert(references[entry.id]<(1n<<32n));
      }
      const result=await session.command('registers_read',{context,ids:names,manual:true});
      for(const entry of spec.banks){
        const sample=result.samples.find(s=>s.id===entry.id);
        assert.equal(sample.owner,`core:${context.core}`);
        if(entry.unavailable){assert.equal(sample.reason,'access_restricted');assert.equal(sample.value,null);}
        else {assert.equal(sample.state,'valid',`${entry.id}: ${sample.detail}`);assert.equal(raw(sample.value.hex),references[entry.id]);assert.equal(sample.source,`openocd:aarch64 banked:${entry.id}`);}
      }
      return result;
    })) return;
    await suite.test(phases[3],'Verify PC, CPSR, controls, context, peer and customer configuration unchanged',async()=>{
      await select(options.core); assert.deepEqual((await session.command('registers_list')).context,context);
      assert.deepEqual(await controls(),before);
      if(spec.peer_core){await select(spec.peer_core);assert.deepEqual(await controls(),peerBefore);await select(options.core);}
      assert.equal(hash(project),projectHash);return {controls_unchanged:true,peer_unchanged:!!peerBefore};
    });
  }catch(error){await suite.test('REG-H02-DRIVER','Driver failure',async()=>{throw error;});}
  finally{if(session)await suite.test('REG-H02-CLEANUP','Close owned debugger session',()=>session.close());assert.equal(hash(project),projectHash);suite.finish();}
})();
