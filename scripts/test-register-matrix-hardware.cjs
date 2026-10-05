#!/usr/bin/env node
'use strict';
// Deferred read-only inventory acceptance. Default invocation never starts DebugTUI.
const fs=require('node:fs'),path=require('node:path'),assert=require('node:assert/strict');
const {root,hash,outputDirectory,parseOptions,Cases,Session}=require('./test-support/session.cjs');
const options=parseOptions(process.argv.slice(2),['--binary','--project','--core','--case'],['--run','--software-fixture']);
const phases=['MATRIX-H-CONFIG','MATRIX-H-READ','MATRIX-H-EXPORT','MATRIX-H-PRESERVE'];
const out=outputDirectory('register-matrix-hardware');
const suite=new Cases(out,{layer:options['software-fixture']?'actual executable with software fixture':options.run?'explicit independent read baseline':'deferred; no target access',board_tests_executed:!!options.run&&!options['software-fixture']});
if(!options.run){for(const id of phases)suite.skip(id,'Register capability matrix','Requires --run, a stopped core and independent per-register expectations');suite.finish();process.exit(0);}
assert(options.project&&options.core&&options.case,'--run requires --project FILE --core NAME --case JSON');
const binary=path.resolve(options.binary||path.join(root,'target/release/debugtui.exe')),project=path.resolve(options.project),file=path.resolve(options.case);
for(const name of [binary,project,file])assert(fs.existsSync(name),`Missing ${name}`);
const spec=JSON.parse(fs.readFileSync(file,'utf8'));
assert(!spec.software_example||options['software-fixture'],'Replace software expectations with an independent physical baseline');
assert(typeof spec.evidence_source==='string'&&spec.evidence_source.trim(),'Declare independent baseline source');
assert(Array.isArray(spec.entries)&&spec.entries.length>0&&spec.entries.length<=128,'Use 1..128 explicitly selected read-only entries');
assert.equal(new Set(spec.entries.map(e=>e.id)).size,spec.entries.length,'Duplicate ids');
const supports=['observed_value','reader_unsupported','access_restricted','feature_disabled','not_implemented','conditions_excluded','unknown'];
for(const entry of spec.entries){
  assert(/^[A-Za-z0-9_.:/+-]{1,128}$/.test(entry.id)&&[8,16,32,64,128].includes(entry.bits)&&supports.includes(entry.support),'Declare id, exact width and expected support');
  if(entry.support==='observed_value')assert(new RegExp(`^0x[0-9a-f]{${entry.bits/4}}$`,'i').test(entry.expected_hex),'Exact independently sampled raw hex required');
}
const original=hash(project);Object.assign(suite.metadata,{binary,binary_sha256:hash(binary),project,project_sha256:original,case_file:file,case_sha256:hash(file),evidence_source:spec.evidence_source});
let session,context,read,matrix;
const row=(matrix,id)=>{const row=matrix.rows.find(r=>r.id===id);assert(row,`No declared register ${id}`);return row;};
(async()=>{
  try{
    session=new Session(binary,project,out);
    if(!await suite.test(phases[0],'Export configuration without connect or implicit reads',async()=>{
      const offline=await session.command('registers_matrix');assert.equal(offline.probe_current,false);assert.deepEqual(offline.observations,[]);
      assert(offline.rows.every(r=>r.support!=='observed_value'));assert.equal(session.logs('mi>').length,0);
      fs.writeFileSync(path.join(out,'offline.json'),JSON.stringify(offline,null,2));
      for(const entry of spec.entries)assert.equal(row(offline,entry.id).bits,entry.bits);
      return {declared_cpu:offline.catalogue.cpu,rows:offline.rows.length};
    }))return;
    await session.command('connect');if(options.core!=='default')await session.command('select_core',{name:options.core});
    if(spec.control_scope){assert(['core','all'].includes(spec.control_scope));await session.command('control_scope',{scope:spec.control_scope});}
    if(!await suite.test(phases[1],'Read only selected ids and compare independent full-width values',async()=>{
      const before=await session.command('registers_matrix');context=before.context;assert.equal(context.core,options.core);assert.equal(before.environment.target_state,'STOPPED');
      for(const entry of spec.entries){const r=row(before,entry.id);assert(r.readable&&!r.manual_only,`Use a side-effect-free readable entry: ${entry.id}`);}
      read=await session.command('registers_read',{context,ids:spec.entries.map(e=>e.id),manual:true});
      for(const entry of spec.entries){const sample=read.samples.find(s=>s.id===entry.id);assert(sample,entry.id);
        if(entry.support==='observed_value'){assert.equal(sample.state,'valid',sample.detail);assert.equal(sample.value.bits,entry.bits);assert.equal(BigInt(sample.value.hex),BigInt(entry.expected_hex));}
      }
      return read;
    }))return;
    if(!await suite.test(phases[2],'Export unchanged cached receipts and owner epochs without another read',async()=>{
      const traffic=session.logs('mi>').length;matrix=await session.command('registers_matrix');assert.equal(session.logs('mi>').length,traffic);
      assert.deepEqual(matrix.context,context);
      for(const entry of spec.entries){const r=row(matrix,entry.id);assert.equal(r.support,entry.support,entry.id);
        const sample=matrix.observations[r.observation],previous=read.samples.find(s=>s.id===entry.id);assert(sample,entry.id);
        assert.deepEqual(sample.value,previous.value);assert.deepEqual(sample.provenance,previous.provenance);
        if(entry.support==='observed_value'){assert.equal(sample.provenance.access.phase,'responded');assert.deepEqual(sample.provenance.access.context,sample.context);
          if(sample.owner.startsWith('cluster:')||sample.owner.startsWith('chip:'))assert.equal(sample.owner_generation,matrix.owner_generations[sample.owner]);}
      }
      assert(matrix.planned_classes.every(c=>c.hardware_support==='unverified'),'Inventory never promotes hardware support from samples');
      fs.writeFileSync(path.join(out,'matrix.json'),JSON.stringify(matrix,null,2));return {context,classes:matrix.planned_classes};
    }))return;
    await suite.test(phases[3],'Preserve original project and cached evidence on repeated export',async()=>{
      const traffic=session.logs('mi>').length;assert.deepEqual(await session.command('registers_matrix'),matrix);assert.equal(session.logs('mi>').length,traffic);assert.equal(hash(project),original);
      return {project_unchanged:true,no_new_mi_commands:true};
    });
  }catch(error){await suite.test('MATRIX-H-SETUP','Run explicit register matrix fixture',async()=>{throw error;});}
  finally{
    if(session)try{await session.close();}catch(error){await suite.test('MATRIX-H-CLEANUP','Close owned session',async()=>{throw error;});}
    if(hash(project)!==original)await suite.test('MATRIX-H-PROJECT','Preserve customer project',async()=>{throw Error('Project changed');});
    suite.finish();
  }
})();
