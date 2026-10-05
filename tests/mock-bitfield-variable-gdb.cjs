// Strict field/type/RAM fault model. Actual GCC/GDB is tested separately.
const scalar=require('./mock-variable-gdb.cjs'),env=process.env;
const base=BigInt(env.DEBUGTUI_TEST_VARIABLE_ADDRESS||'0x100000000'),little=!env.DEBUGTUI_TEST_BITFIELD_BIG;
let calls='on',inspections=0,serial=0,written=false,word=3n|(62n<<5n)|(0x55n<<11n)|(0x2abcn<<18n);
const objects=new Map(),names=['before','low','middle','neighbour','reserved','after'],widths=[32,5,6,7,14,32],offsets=[0,0,5,11,18,0];
const fail=name=>env.DEBUGTUI_TEST_BITFIELD_FAILURE===name&&(!env.DEBUGTUI_TEST_BITFIELD_APPLY_ONLY||inspections>1);
const value=index=>index===0?0x12345678n:index===5?0x87654321n:(word>>BigInt(offsets[index]))&((1n<<BigInt(widths[index]))-1n);
const signed=n=>n&32n?n-64n:n;
const bytes=()=>{const stored=little?word:(value(1)<<27n)|(value(2)<<21n)|(value(3)<<14n)|value(4);return [0x12345678n,stored,0x87654321n].flatMap(n=>Array.from({length:4},(_,i)=>Number((n>>BigInt((little?i:3-i)*8))&255n)));};
const store=memory=>bytes().forEach((n,i)=>memory.set(base+BigInt(i),n));
const type='struct Cells { unsigned before; unsigned low : 5; int middle : 6; unsigned neighbour : 7; unsigned reserved : 14; unsigned after; }';
const layout='/* offset | size */ type = struct Cells {\n/* 0 | 4 */ unsigned before;\n/* 4:0 | 4 */ unsigned low : 5;\n/* 4:5 | 4 */ int middle : 6;\n/* 5:3 | 4 */ unsigned neighbour : 7;\n/* 6:2 | 4 */ unsigned reserved : 14;\n/* 8 | 4 */ unsigned after;\n/* total size (bytes): 12 */\n}';
module.exports.initialize=memory=>{scalar.initialize(memory);store(memory);};
module.exports.handle=(cmd,api)=>{
 const {done,error,running,memory}=api;
 if(cmd==='-gdb-set may-call-functions off'){calls='off';inspections++;if(inspections===2&&fail('fresh-neighbour')){word=(word&~(127n<<11n))|(7n<<11n);store(memory);}}
 if(cmd==='-gdb-set may-call-functions on')calls='on';
 if(cmd.startsWith('-interpreter-exec console ')){
  const command=JSON.parse(cmd.slice('-interpreter-exec console '.length));let output;
  if(command==='ptype /r counter')output='type = '+(fail('parent-change')?type.replace('struct Cells','struct ChangedCells'):type);
  if(command==='ptype /r counter.middle')output='type = int';
  if(command==='ptype /rod counter')output=fail('no-layout')?'type = '+type:fail('layout-change')?layout.replace('4:5','5:5'):layout;
  if(command==='show endian')output=`The target endianness is set automatically (currently ${(little!==fail('endian'))?'little':'big'} endian).`;
  if(output!==undefined){process.stdout.write('~'+JSON.stringify(output+'\n')+'\n');done();return true;}
 }
 if(cmd==='-data-evaluate-expression "sizeof(counter)"'){done('value="12"');return true;}
 if(cmd==='-data-evaluate-expression "sizeof(counter.middle)"'){done('value="4"');return true;}
 if(cmd==='-data-evaluate-expression "counter.middle"'){done(`value="${signed(value(2))}"`);return true;}
 const evaluated=/^-data-evaluate-expression "counter\.(before|low|neighbour|reserved|after)"$/.exec(cmd);
 if(evaluated){done(`value="${value(names.indexOf(evaluated[1]))}"`);return true;}
 if(cmd==='-data-evaluate-expression "((__typeof__(counter.middle))-1) < ((__typeof__(counter.middle))0)"'){done('value="1"');return true;}
 if(cmd.startsWith('-data-read-memory-bytes ')&&(fail('read')||(written&&fail('verify')))){error('Bitfield parent bytes unavailable');return true;}
 if(cmd.startsWith('-data-read-memory-bytes ')&&fail('running'))running();
 if(cmd==='-var-create - * "counter"'){const name='bits'+(++serial);objects.set(name,{root:true});done(`name="${name}",numchild="6",type="struct Cells",value="{...}"`);return true;}
 let match=/^-var-list-children --no-values "(bits\d+)" (\d+) (\d+)$/.exec(cmd);
 if(match&&objects.has(match[1])){const index=Number(match[2]);if(index>=6||Number(match[3])!==index+1){error('Unbounded field path');return true;}const name=match[1]+'.c'+index;objects.set(name,{index});done(`numchild="6",children=[child={name="${name}",exp="${names[index]}",numchild="0",type="${index===2?'int':'unsigned int'}"}]`);return true;}
 match=/^-var-(show-attributes|info-path-expression|evaluate-expression -f hexadecimal|delete) "(bits\d+(?:\.c\d+)?)"$/.exec(cmd);
 if(match){const node=objects.get(match[2]);if(!node){error('Unknown field object');return true;}
  if(match[1]==='delete'){objects.delete(match[2]);if(written&&fail('cleanup'))error('Field cleanup unavailable');else done('ndeleted="1"');return true;}
  if(match[1]==='show-attributes'){done('attr="editable"');return true;}
  if(match[1]==='info-path-expression'){done(`path_expr="counter.${names[node.index]}"`);return true;}
  if(node.root){done('value="{...}"');return true;}
  let n=node.index===2?BigInt.asUintN(32,signed(value(2))):value(node.index);if(fail('extension'))n^=128n;done(`value="0x${n.toString(16)}"`);return true;
 }
 match=/^-var-assign "(bits\d+\.c2)" (".*")$/.exec(cmd);
 if(match){const literal=/^\(__typeof__\(counter.middle\)\)\((-?\d+|0x[0-9a-f]+)\)$/.exec(JSON.parse(match[2]));if(!objects.has(match[1])||calls!=='off'||!literal){error('Unsafe field assignment');return true;}
  const n=BigInt(literal[1]);if(n<-32n||n>31n){error('Out-of-range field assignment');return true;}word=(word&~(63n<<5n))|(BigInt.asUintN(6,n)<<5n);written=true;
  if(fail('neighbour'))word^=1n<<11n;if(fail('field-mismatch'))word^=1n<<5n;store(memory);if(fail('unknown'))error('Error after field write');else done(`value="${signed(value(2))}"`);return true;
 }
 return scalar.handle(cmd,api);
};
