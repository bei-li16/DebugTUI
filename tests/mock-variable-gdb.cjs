// Opt-in strict scalar MI fixture. Every accepted inspection/assignment is
// explicitly recognized; the native suite separately exercises real GDB types.
let calls=process.env.DEBUGTUI_TEST_VARIABLE_CALLS_OFF?'off':'on', inspections=0, serial=0, written=false, value=42n;
const objects=new Set();
const env=process.env;
const typeExpression=(env.DEBUGTUI_TEST_VARIABLE_TYPE||'').trim().endsWith('&')?'__typeof__(*(&(counter)))':'__typeof__(counter)';
const store=memory=>{const base=BigInt(env.DEBUGTUI_TEST_VARIABLE_ADDRESS||'0x100000004');for(let i=0;i<4;i++)memory.set(base+BigInt(i),Number((BigInt.asUintN(32,value)>>BigInt(i*8))&255n));};
module.exports.initialize=store;
module.exports.handle=(cmd,{done,error,running,stopped,memory})=>{
  if(cmd==='-gdb-show may-call-functions'){done(`value="${calls}"`);return true;}
  if(cmd==='-gdb-set may-call-functions off'){calls='off';inspections++;done();return true;}
  if(cmd==='-gdb-set may-call-functions on'){
    if(env.DEBUGTUI_TEST_VARIABLE_RESTORE_ERROR){error('Function policy restore failed');return true;}
    calls='on';done();return true;
  }
  if(cmd==='-info-gdb-mi-command var-assign'){done(`command={exists="${env.DEBUGTUI_TEST_NO_VARIABLE_WRITER?'false':'true'}"}`);return true;}
  if(cmd==='-gdb-show may-write-memory'){
    done(`value="${env.DEBUGTUI_TEST_MEMORY_READONLY||(inspections>1&&env.DEBUGTUI_TEST_VARIABLE_PERMISSION_CHANGE)?'off':'on'}"`);return true;
  }
  if(cmd==='-interpreter-exec console "ptype /r counter"'){
    let type=env.DEBUGTUI_TEST_VARIABLE_TYPE||'unsigned int';
    if(inspections>1&&env.DEBUGTUI_TEST_VARIABLE_TYPE_CHANGE)type='int';
    process.stdout.write('~'+JSON.stringify('type = '+type+'\n')+'\n');done();return true;
  }
  if(cmd==='-data-evaluate-expression "sizeof(counter)"'){done('value="4"');return true;}
  if(cmd==='-data-evaluate-expression "counter"'){done(`value="${value}"`);return true;}
  if(cmd==='-data-evaluate-expression "before"'||cmd==='-data-evaluate-expression "after"'){done('value="170"');return true;}
  if(cmd==='-data-evaluate-expression "(unsigned long long)&(counter)"'){
    if(env.DEBUGTUI_TEST_VARIABLE_NO_ADDRESS){error(env.DEBUGTUI_TEST_VARIABLE_NO_ADDRESS);return true;}
    const address=inspections>1&&env.DEBUGTUI_TEST_VARIABLE_ADDRESS_CHANGE?'0x100000008':env.DEBUGTUI_TEST_VARIABLE_ADDRESS||'0x100000004';
    done(`value="${address}"`);return true;
  }
  if(cmd===`-data-evaluate-expression "((${typeExpression})-1) < ((${typeExpression})0)"`){
    done(`value="${(env.DEBUGTUI_TEST_VARIABLE_TYPE||'').replace(/\s*&+$/,'')==='int'?'1':'0'}"`);return true;
  }
  if(cmd==='-var-create - * "counter"'){
    const name='var'+(++serial);objects.add(name);done(`name="${name}",numchild="0",type="unsigned int",value="${value}"`);return true;
  }
  let match=/^-var-(show-attributes|info-path-expression|evaluate-expression -f hexadecimal|delete) "(var\d+)"$/.exec(cmd);
  if(match){
    const [,operation,name]=match;
    if(!objects.has(name)){error('No such variable object');return true;}
    if(operation==='delete'){
      objects.delete(name);
      if(env.DEBUGTUI_TEST_VARIABLE_CLEANUP_ERROR&&written&&calls==='off'){error('Variable cleanup failed');return true;}
      done('ndeleted="1"');return true;
    }
    if(operation==='show-attributes'){done(`attr="${env.DEBUGTUI_TEST_VARIABLE_NOT_EDITABLE?'noneditable':'editable'}"`);return true;}
    if(operation==='info-path-expression'){done('path_expr="counter"');return true;}
    if(written&&env.DEBUGTUI_TEST_VARIABLE_VERIFY_ERROR){error('Variable readback unavailable');return true;}
    done(`value="${env.DEBUGTUI_TEST_VARIABLE_OPTIMIZED?'<optimized out>':'0x'+BigInt.asUintN(32,value).toString(16)}"`);return true;
  }
  match=/^-var-assign "(var\d+)" (".*")$/.exec(cmd);
  if(match){
    const expression=JSON.parse(match[2]),prefix=`(${typeExpression})`;
    const literal=expression.startsWith(prefix)?/^\((0x[0-9a-f]+|-?\d+)\)$/.exec(expression.slice(prefix.length)):null;
    if(!literal){error('Unsupported typed scalar assignment');return true;}
    if(!objects.has(match[1])||calls!=='off'){error('Unbound variable or function calls enabled');return true;}
    written=true;value=env.DEBUGTUI_TEST_VARIABLE_MISMATCH?0n:BigInt(literal[1]);
    store(memory);
    const failure=env.DEBUGTUI_TEST_VARIABLE_WRITE_ERROR;
    if(failure==='closed'){process.exit(7);return true;}
    if(failure==='timeout')return true;
    if(failure==='error'){error('Error after target assignment');return true;}
    if(failure==='error-stop'){error('Error after target assignment');setTimeout(stopped,100);return true;}
    if(env.DEBUGTUI_TEST_VARIABLE_WRITE_RUN)running();
    done(`value="${value}"`);return true;
  }
  return false;
};
