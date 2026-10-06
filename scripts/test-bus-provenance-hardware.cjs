// Deferred, stopped RAM checks. The default invocation never starts a debugger.
const fs = require('node:fs'), path = require('node:path'), assert = require('node:assert/strict');
const {root,hash,outputDirectory,parseOptions,Cases,Session} = require('./test-support/session.cjs');
const options = parseOptions(process.argv.slice(2),['--binary','--project','--case'],['--run','--software-fixture']);
const phases = ['BUS-PH01','BUS-PH02','BUS-PH03'];
const out = outputDirectory('bus-provenance');
const suite = new Cases(out,{todo:'BUS binding/receipt subset; full BUS-H acceptance remains separate',
  layer: options['software-fixture'] ? 'software fixture; no board' : options.run ? 'explicit stopped RAM environment' : 'deferred; no target access',
  board_tests_executed: !!options.run && !options['software-fixture']});
if (!options.run) {
  for (const id of phases) suite.skip(id,'Typed RAM binding and observed read origin','Needs --run, an isolated stopped project and independent RAM expectations');
  suite.finish(); process.exit(0);
}
assert(options.project && options.case,'--run needs --project FILE --case JSON');
const binary = path.resolve(options.binary || path.join(root,'target/release/debugtui.exe'));
const project = path.resolve(options.project), caseFile = path.resolve(options.case);
for (const file of [binary,project,caseFile]) assert(fs.existsSync(file),`Missing ${file}`);
const spec = JSON.parse(fs.readFileSync(caseFile));
assert(!spec.software_example || options['software-fixture'],'Replace software expectations before physical execution');
assert.equal(spec.fixture_ram,true); assert.equal(spec.startup_actions_disabled,true);
assert(Array.isArray(spec.cores) && spec.cores.length>0 && spec.cores.length<=4);
const owners = new Set();
for (const core of spec.cores) {
  assert(typeof core.name==='string' && core.name && !owners.has(core.name)); owners.add(core.name);
  assert(/^[A-Za-z_]\w*$/.test(core.expression),'Use a simple, independently known RAM scalar');
  assert([8,16,32,64].includes(core.bits) && typeof core.little_endian==='boolean');
  assert(/^0x[0-9a-f]+$/i.test(core.address) && BigInt(core.address)<=BigInt(Number.MAX_SAFE_INTEGER));
  assert(/^0x[0-9a-f]+$/i.test(core.expected) && BigInt(core.expected)<(1n<<BigInt(core.bits)));
  assert(typeof core.expected_bytes==='string' && /^[0-9a-f]+$/i.test(core.expected_bytes) && core.expected_bytes.length===core.bits/4);
  for (const key of ['channel','target','tcl_endpoint','configuration_source','gdb_endpoint']) assert(typeof core[key]==='string' && core[key]);
}
Object.assign(suite.metadata,{binary,sha256:hash(binary),project,project_sha256:hash(project),case_file:caseFile,case_sha256:hash(caseFile)});
let session;
const bindings = new Map();
const params = binding => ({address:binding.address,bits:binding.bits,little_endian:binding.little_endian,
  context:binding.context,selection_epoch:binding.selection_epoch,watch_binding:binding.binding_id,channel:''});
const hex = core => '0x'+BigInt(core.expected).toString(16).padStart(core.bits/4,'0');
const word = text => '"'+text.replace(/[\\"$\[\]]/g,'\\$&')+'"';
async function select(core) {
  const status = await session.command('status');
  if (status.cores?.length) {
    const index = status.cores.findIndex(c=>c.name===core.name); assert(index>=0);
    await session.command('select_core',{index});
    await session.command('control_scope',{scope:'all'});
  } else assert.equal(core.name,'default');
  const selected = await session.command('status'); assert.equal(selected.state,'STOPPED');
  return selected;
}
function receipt(result,binding,core,channel,dump=false) {
  assert.deepEqual(result.context,binding.context); assert.deepEqual(result.access.context,binding.context);
  assert.equal(result.selection_epoch,binding.selection_epoch); assert.equal(result.channel,channel);
  assert.equal(result.state,'STOPPED'); assert.equal(result.atomic,false);
  assert.equal(result.access.phase,'responded'); assert(Number.isSafeInteger(result.access.timestamp_ms));
  assert(Number.isSafeInteger(result.access.completed_ms) && result.access.completed_ms>=result.access.timestamp_ms);
  const route = result.access.route, address = '0x'+BigInt(core.address).toString(16), bytes = core.bits/8;
  assert.equal(route.address,address); assert.equal(route.bits,core.bits);
  assert.equal(route.byte_order,dump || core.little_endian ? 'little' : 'big');
  if (!channel) {
    assert.equal(route.kind,'gdb_memory'); assert.equal(route.configured_endpoint,core.gdb_endpoint);
    assert(route.endpoint===null || route.endpoint===core.gdb_endpoint);
    assert.equal(result.access.command,`-data-read-memory-bytes ${address} ${bytes}`);
  } else {
    assert.equal(route.kind,'tcl_memory'); assert.equal(route.channel,channel); assert.equal(route.target,core.target);
    assert.equal(route.endpoint,core.tcl_endpoint); assert.equal(route.configuration_source,core.configuration_source);
    assert.equal(route.atomic,false); const bus = dump ? 8 : Math.min(core.bits,32);
    assert.equal(route.bus_width,bus); assert.equal(route.count,core.bits/bus);
    assert.equal(result.access.command,`${word(core.target)} read_memory ${address} ${bus} ${core.bits/bus}`);
  }
  if (!dump) assert.deepEqual(result.raw,{bits:core.bits,hex:hex(core)});
  return result.access;
}
(async()=>{
  try {
    session = new Session(binary,project,out);
    if (!await suite.test('BUS-PH-SETUP','Connect the declared isolated project',()=>session.command('connect'))) {
      for (const id of phases) suite.skip(id,'RAM binding/receipt','Connection failed');
      return;
    }
    const first = await suite.test(phases[0],'Each selected core: typed binding, GDB/AP scalar and AP byte range',async()=>{
      const evidence=[];
      for (const core of spec.cores) {
        const status = await select(core);
        await session.command('watch',{expression:core.expression});
        const listed = await session.command('registers_list');
        const binding = await session.command('watch_resolve',{expression:core.expression,context:listed.context,selection_epoch:status.memory_selection_epoch});
        assert.deepEqual(binding.context,listed.context); assert.equal(binding.context.core,core.name);
        assert.equal(binding.address,Number(BigInt(core.address))); assert.equal(binding.bits,core.bits);
        assert.equal(binding.little_endian,core.little_endian); assert(binding.binding_id && binding.thread && binding.frame_address);
        const gdb = await session.command('memory_read',params(binding));
        const ap = await session.command('memory_read',{...params(binding),channel:core.channel});
        const dump = await session.command('memory_dump',{address:'0x'+BigInt(core.address).toString(16),count:core.bits/8,
          channel:core.channel,context:binding.context,selection_epoch:binding.selection_epoch});
        assert(Array.isArray(dump.bytes) && dump.bytes.length===core.bits/8);
        assert(dump.bytes.every(byte=>Number.isInteger(byte) && byte>=0 && byte<=255));
        assert.equal(Buffer.from(dump.bytes).toString('hex'),core.expected_bytes.toLowerCase());
        evidence.push({core:core.name,binding,gdb:receipt(gdb,binding,core,''),ap:receipt(ap,binding,core,core.channel),dump:receipt(dump,binding,core,core.channel,true)});
        bindings.set(core.name,binding);
      }
      assert(!session.logs('mi>').some(event=>/-exec-(?:run|continue|interrupt|step|next|finish)|-data-write-|-var-assign/.test(event.text)));
      return evidence;
    });
    let second=false;
    if (first) second = await suite.test(phases[1],'Modified typed parameters and superseded binding reject before MI',async()=>{
      for (const core of spec.cores) {
        await select(core); const binding=bindings.get(core.name);
        for (const change of [{address:binding.address+core.bits/8},{little_endian:!binding.little_endian},{watch_binding:null}]) {
          const start=session.logs('mi>').length;
          await session.command('memory_read',{...params(binding),...change},false);
          assert.equal(session.logs('mi>').length,start);
        }
        const fresh=await session.command('watch_resolve',{expression:core.expression});
        const start=session.logs('mi>').length;
        await session.command('memory_read',params(binding),false); assert.equal(session.logs('mi>').length,start);
        bindings.set(core.name,fresh);
      }
      return {cores:[...bindings.keys()],retry:false};
    });
    else suite.skip(phases[1],'Modified/superseded binding','Earlier baseline failed');
    if (second) await suite.test(phases[2],'Explicit reconnect creates new contexts; old bindings cannot dispatch',async()=>{
      await session.command('disconnect'); await session.command('connect');
      const evidence=[];
      for (const core of spec.cores) {
        await select(core); const old=bindings.get(core.name), start=session.logs('mi>').length;
        await session.command('memory_read',params(old),false); assert.equal(session.logs('mi>').length,start);
        const fresh=await session.command('watch_resolve',{expression:core.expression});
        assert.notEqual(fresh.context.session,old.context.session);
        const read=await session.command('memory_read',{...params(fresh),channel:core.channel});
        evidence.push({core:core.name,old_context:old.context,new_context:fresh.context,access:receipt(read,fresh,core,core.channel)});
      }
      return evidence;
    });
    else suite.skip(phases[2],'Explicit reconnect','Earlier binding checks failed');
  } finally {
    if (session) await suite.test('BUS-PH-CLEANUP','Close only this test session',()=>session.close());
    suite.finish();
  }
})().catch(error=>{console.error(error);process.exitCode=1;});
