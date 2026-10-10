// Local preferences protocol regression; deliberately provides no usable GDB.
const fs = require('node:fs'), path = require('node:path'), assert = require('node:assert/strict');
const {root, outputDirectory, parseOptions, Cases, Session, hash} = require('./test-support/session.cjs');
const options = parseOptions(process.argv.slice(2), ['--binary'], ['--coordinator']);
const binary = path.resolve(options.binary || path.join(root,'target/debug/debugtui.exe'));
assert(fs.existsSync(binary), `Missing binary: ${binary}`);
const out = outputDirectory('register-display');
const project = path.join(out, 'project.toml');
fs.writeFileSync(project, "version=2\n[gdb]\nexecutable='./must-not-start-gdb.exe'\n[target]\nmode='local'\n" +
  (options.coordinator ? "[[cores]]\nname='core0'\nendpoint='localhost:3333'\n[[cores]]\nname='core1'\nendpoint='localhost:3334'\n" : ''));
const suite = new Cases(out, {layer:'actual binary, local settings protocol; no debugger or board', coordinator:!!options.coordinator, board_tests_executed:false, binary, binary_sha256:hash(binary)});
const scope = core => JSON.stringify(['THA6',core,'cortex-r52','armv8-r-aarch32','builtin:cortex-r52',1]);
const object = JSON.stringify(['d0',null]);
const core0 = {open:['core','simd'], fields:['cpsr'], filter:2, all_definitions:true, query:'d0', formats:{[object]:{kind:'float',bits:64}}};
const core1 = {open:['core','system'], fields:[], filter:3, all_definitions:false, query:'mpu', formats:{[object]:{kind:'vector',lane_bits:32,interpretation:'float'}}};
let first, second, before;
(async () => {
  try {
    first = new Session(binary, project, out); second = new Session(binary, project, out);
    if (!await suite.test('REG-DISPLAY-LOCAL','Save both scopes from independently loaded clients without GDB',async()=>{
      before = await first.command('status'); assert.equal(before.state,'DISCONNECTED');
      await second.command('status');
      let results;
      for(let round=0;round<10;round++) {
        results = await Promise.all([
          first.command('register_preferences',{scope:scope('core0'),preferences:core0}),
          second.command('register_preferences',{scope:scope('core1'),preferences:core1}),
        ]);
        assert(results.every(r=>r.saved));
        const text = fs.readFileSync(project,'utf8'); assert(text.includes('core0')&&text.includes('core1'));
      }
      return {results,concurrent_save_rounds:10};
    })) return;
    if (!await suite.test('REG-DISPLAY-GLOBAL','A stale global settings save preserves both new register scopes',async()=>{
      await first.command('ui_preferences',{animations:'full',unicode:false,formats:{'watch:counter':'binary'}});
      const text = fs.readFileSync(project,'utf8'); assert(text.includes('core0')&&text.includes('core1'));
      assert(text.includes('lane_bits = 32')&&text.includes('bits = 64'));
      assert(text.includes('unicode = false')); return {project_sha256:hash(project)};
    })) return;
    if (!await suite.test('REG-DISPLAY-INVALID','Reject invalid widths, filters, unknown fields and a whole-map overwrite',async()=>{
      const saved = hash(project);
      for (const params of [
        {scope:'',preferences:core0}, {scope:scope('core0'),preferences:{filter:200}},
        {scope:scope('core0'),preferences:{formats:{[object]:{kind:'float',bits:128}}}},
        {scope:scope('core0'),preferences:{formats:{[object]:{kind:'signed',execute:'continue'}}}},
      ]) await first.command('register_preferences',params,false);
      await first.command('ui_preferences',{register_views:{[scope('core0')]:core0}},false);
      assert.equal(hash(project),saved); return {project_sha256:saved};
    })) return;
    await suite.test('REG-DISPLAY-NO-IO','Keep target context unchanged and emit no debugger commands',async()=>{
      const after = await first.command('status');
      assert.equal(after.state,before.state); assert.equal(after.register_session,before.register_session);
      assert.equal(after.generation,before.generation);
      assert(![first,second].some(s=>s.logs('mi>').length)); return {state:after.state,generation:after.generation};
    });
  } catch(error) { await suite.test('REG-DISPLAY-SETUP','Initialize local preference clients',async()=>{throw error;}); }
  finally {
    if(first) await suite.test('REG-DISPLAY-CLOSE-0','Close the first client',()=>first.close());
    if(second) await suite.test('REG-DISPLAY-CLOSE-1','Close the second client',()=>second.close());
    suite.finish();
  }
})();
