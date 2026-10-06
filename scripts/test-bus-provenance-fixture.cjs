const fs=require('node:fs'),path=require('node:path'),net=require('node:net'),assert=require('node:assert/strict'),{spawn}=require('node:child_process');
const {root,quote,outputDirectory}=require('./test-support/session.cjs');
const binary=path.resolve(process.argv[2]||path.join(root,'target/debug/debugtui.exe'));
(async()=>{
 const out=outputDirectory('bus-driver-software'),sockets=new Set();
 const server=net.createServer(socket=>{sockets.add(socket);socket.on('close',()=>sockets.delete(socket));let buffer='';socket.on('data',data=>{buffer+=data.toString();let index;while((index=buffer.indexOf('\x1a'))>=0){const command=buffer.slice(0,index);buffer=buffer.slice(index+1);fs.appendFileSync(path.join(out,'packets.txt'),command+'\n');const dump=command.includes(' 8 4');socket.write(`0:${dump?'17 0 0 0':'17'}\x1a`);}});});
 await new Promise(resolve=>server.listen(0,'127.0.0.1',resolve));
 try {
 const endpoint=`127.0.0.1:${server.address().port}`,project=path.join(out,'project.toml'),elf=path.join(out,'fixture.exe'),context=path.join(out,'context.json');
 fs.writeFileSync(elf,Buffer.from('MZ\0\0\0\0'));fs.writeFileSync(context,JSON.stringify({thread:'1',frame:0,pc:'0x100000008'}));
 const env={DEBUGTUI_TEST_REGISTERS:'[]',DEBUGTUI_TEST_WATCH_RESOLVE:'1',DEBUGTUI_TEST_MEMORY_BLOCKS:'[{begin="0x100000004",contents="11000000"}]',DEBUGTUI_TEST_CONTEXT_FILE:context,DEBUGTUI_TEST_TRANSCRIPT:path.join(out,'commands.txt')};
 let toml=`version=2\n[program]\nelf=${quote(elf)}\n[gdb]\nexecutable=${quote(process.execPath)}\nargs=[${quote(path.join(root,'tests/mock-gdb.cjs'))}]\n[gdb.env]\n`;
 for(const [key,value] of Object.entries(env))toml+=`${key}=${JSON.stringify(value)}\n`;
 toml+='[target]\nmode="remote"\nendpoint="localhost:3333"\n[session]\non_exit="disconnect"\n';
 for(let i=0;i<4;i++)toml+=`[[cores]]\nname="core${i}"\nendpoint="localhost:${3333+i}"\n`;
 toml+=`[[memory_access]]\nid="bus"\ntarget="soc.bus"\ntcl_endpoint="${endpoint}"\nwhile_running=false\ncores=["core0","core1","core2","core3"]\n`;
 fs.writeFileSync(project,toml);
 const spec={software_example:true,fixture_ram:true,startup_actions_disabled:true,cores:Array.from({length:4},(_,i)=>({name:`core${i}`,expression:'counter',address:'0x100000004',bits:32,little_endian:true,expected:'0x11',expected_bytes:'11000000',channel:'bus',target:'soc.bus',tcl_endpoint:endpoint,gdb_endpoint:`localhost:${3333+i}`,configuration_source:`project:${project.replaceAll('\\','/')}`}))};
 const caseFile=path.join(out,'case.json');fs.writeFileSync(caseFile,JSON.stringify(spec,null,2));
 const child=spawn(process.execPath,[path.join(root,'scripts/test-bus-provenance-hardware.cjs'),'--run','--software-fixture','--binary',binary,'--project',project,'--case',caseFile],{cwd:root,windowsHide:true,env:process.env});
 let output='';child.stdout.on('data',data=>{output+=data;process.stdout.write(data);});child.stderr.on('data',data=>{output+=data;process.stderr.write(data);});
 const code=await new Promise((resolve,reject)=>{child.on('close',resolve);child.on('error',reject);});fs.writeFileSync(path.join(out,'driver-output.log'),output);assert.equal(code,0);
 console.log(`SOFTWARE driver fixture: ${out}`);
 }finally{for(const socket of sockets)socket.destroy();await new Promise(resolve=>server.close(resolve));}
})().catch(error=>{console.error(error);process.exitCode=1;});
