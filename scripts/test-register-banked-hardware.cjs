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
assert(Number.isInteger(spec.expected_debug_el) && [0,1,2].includes(spec.expected_debug_el), 'Declare current Debug EL independently of stopped CPSR');
const el = spec.expected_debug_el;
assert(Array.isArray(spec.stable_registers) && ['r0','pc','cpsr'].every(id => spec.stable_registers.includes(id)), 'Guard scratch, PC and CPSR');
const names = ['sp_irq','lr_irq','spsr_irq','r8_fiq','r9_fiq','r10_fiq','r11_fiq','r12_fiq','sp_fiq','lr_fiq','spsr_fiq','sp_und','lr_und','spsr_und','sp_abt','lr_abt','spsr_abt','sp_svc','lr_svc','spsr_svc','sp_hyp','elr_hyp','spsr_hyp'];
if (spec.banks?.some(entry=>entry.id.endsWith('_usr'))) names.push('r8_usr','r9_usr','r10_usr','r11_usr','r12_usr','sp_usr','lr_usr');
assert(Array.isArray(spec.banks) && spec.banks.length === names.length && new Set(spec.banks.map(e=>e.id)).size === names.length);
for (const entry of spec.banks) {
  assert(names.includes(entry.id));
  const unavailable = (el === 0 && !entry.id.endsWith('_usr')) || (el === 1 && entry.id.endsWith('_hyp'));
  const unknown = el === 1 && !entry.id.endsWith('_hyp');
  assert.equal(!!entry.unavailable, unavailable, `Architecture legality for ${entry.id}`);
  assert.equal(!!entry.unknown, unknown, `Current Debug mode proof for ${entry.id}`);
  if (!unavailable && !unknown) assert(/^[a-zA-Z_]\w*(\[\d+\])?$/.test(entry.reference), 'Reference must be a firmware variable/array element');
}
const raw = text => { assert(/^0x[0-9a-f]{8}$/i.test(text), `Exact 32-bit raw required: ${text}`); return BigInt(text); };
const referenceExpression = reference => {
  const match = /^([a-zA-Z_]\w*)(?:\[(\d+)\])?$/.exec(reference);
  assert(match, 'Reference must be a firmware symbol or array element');
  return `((unsigned int *)&${match[1]})[${match[2] || '0'}]`;
};
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
      if(el !== 2) return {current_debug_el:el,legacy_identity_probe_omitted:true,identity_proof:'external MIDR in each successful bank sample'};
      const response = await session.command('registers_probe',{context});
      assert.equal(response.probe.identity.model,'Cortex-R52');
      assert.equal(response.probe.samples.find(s=>s.id==='midr').value.hex,spec.expected_midr);
      return response;
    })) return;
    if (!await suite.test(phases[2],'Compare all banks with independent firmware samples and legal refusals',async()=>{
      const references = {};
      for(const entry of spec.banks.filter(e=>!e.unavailable && !e.unknown)){
        const result=await session.command('evaluate',{expression:referenceExpression(entry.reference)});
        assert(/^(0x[0-9a-f]+|\d+)$/i.test(result.value));
        references[entry.id]=BigInt(result.value); assert(references[entry.id]<(1n<<32n));
      }
      const result=await session.command('registers_read',{context,ids:names,manual:true});
      for(const entry of spec.banks){
        const sample=result.samples.find(s=>s.id===entry.id);
        assert.equal(sample.owner,`core:${context.core}`);
        if(entry.unavailable){assert.equal(sample.reason,'access_restricted');assert.equal(sample.value,null);}
        else if(entry.unknown){assert.equal(sample.reason,'unknown');assert.equal(sample.value,null);}
        else {
          assert.equal(sample.state,'valid',`${entry.id}: ${sample.detail}`);assert.equal(raw(sample.value.hex),references[entry.id]);assert.equal(sample.source,`openocd:aarch64 banked:${entry.id}`);
          const access=sample.provenance?.access, proof=access?.banked;
          assert.equal(access?.phase,'responded');assert.equal(access?.route?.kind,'tcl_register');assert.deepEqual(access?.context,context);
          assert(Number.isInteger(access.timestamp_ms) && Number.isInteger(access.completed_ms) && access.completed_ms >= access.timestamp_ms);
          for(const field of ['midr','dscr','dspsr','dlr']) { assert.equal(proof?.[field]?.bits,32);raw(proof[field].hex); }
          assert.equal(proof.midr.hex,spec.expected_midr); const dscr=raw(proof.dscr.hex);
          assert.equal(Number((dscr>>8n)&3n),el);assert.equal(dscr&0x1c0000c0n,0n);assert(dscr&(1n<<24n));
          assert.equal(dscr&(1n<<BigInt(10+el)),0n);if(el===2)assert.equal(dscr&(1n<<16n),0n);
          assert.equal(Number(raw(proof.dspsr.hex)&31n),mode,'Saved DSPSR must match independently stopped mode');
          const expectedMethod = el===0 || entry.id==='sp_hyp' || (entry.id.endsWith('_usr') && entry.id!=='sp_usr') ? 'mov32' : entry.id==='spsr_hyp' ? 'mrs32' : 'banked_mrs32';
          assert.equal(proof.read_method,expectedMethod);
        }
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
