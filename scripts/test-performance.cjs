const {connect}=require('./test-reader.cjs');
const {spawn,execFileSync}=require('node:child_process');
const fs=require('node:fs'),path=require('node:path'),os=require('node:os'),assert=require('node:assert/strict');
const exe=path.resolve(process.argv[2]||'target/release/quickpeek.exe');
const temp=fs.mkdtempSync(path.join(os.tmpdir(),'QuickPeek-perf-'));
const files=process.argv.length>3?process.argv.slice(3):['5mb.log','10mb.log','5mb.js','10mb.js','5mb.json','10mb.json','5mb.csv','10mb.csv','large.xlsx','5mb.md','10mb.md','5mb.html','10mb.html'];
const sleep=ms=>new Promise(resolve=>setTimeout(resolve,ms));
(async()=>{
 const results=[];
 for(const [i,file] of files.entries()){
  const input=path.resolve('target/perf-fixtures',file),port=19440+i,start=performance.now();
  const child=spawn(exe,['--classic',input],{windowsHide:true,stdio:'ignore',env:{...process.env,WEBVIEW2_USER_DATA_FOLDER:path.join(temp,'profile'+i),WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS:`--remote-debugging-port=${port}`}});
  let client;
  try{
   client=await connect(port);
   let ready=false;
   for(let wait=0;wait<100;wait++){if(await client.evaluate("document.querySelector('main')?.dataset.ready==='true'")){ready=true;break}await sleep(25)}
   assert(ready,'Content not ready');
   await client.evaluate('new Promise(resolve=>requestAnimationFrame(()=>requestAnimationFrame(resolve)))');
   const readyMs=Math.round(performance.now()-start);
   const details=await client.evaluate("({textLength:data.text.length,notice:data.notice,rows:document.querySelectorAll('tbody tr').length,chunks:document.querySelectorAll('pre[data-mounted]').length,status:document.getElementById('status').textContent,total:data.sheets[0]?.count})");
   assert(!details.notice.includes('仅'),'Fixture was truncated');
   if(file.endsWith('.log')||file.endsWith('.js')){
    assert.equal(details.textLength,fs.readFileSync(input,'utf8').length,'Full input must be available');
    assert(details.chunks<8,'Only viewport chunks should mount');
    await client.evaluate("document.querySelector('main').scrollTop=document.querySelector('main').scrollHeight");
    await sleep(80);
    assert(await client.evaluate("document.querySelector('pre:last-child')?.dataset.mounted==='true'"),'Last text chunk unavailable');
   }
   if(file.endsWith('.js')){
    await sleep(200);
    assert((await client.evaluate("document.querySelectorAll('.hljs-keyword').length"))>0,'Code missing syntax colors');
   }
   if(file.endsWith('.json'))assert((await client.evaluate("data.text.includes('12345678901234567890')")),'Integer precision lost');
   if(file.endsWith('.csv')||file.endsWith('.xlsx')){
    assert.equal(details.rows,100);
    await client.evaluate("page=Math.ceil((data.sheets[0].count-(data.kind==='csv'?1:0))/100)-1;render()");
    for(let wait=0;wait<100;wait++){if(await client.evaluate("document.querySelector('main')?.dataset.ready==='true'"))break;await sleep(25)}
    const last=await client.evaluate("Number(document.querySelector('tbody tr:last-child td').textContent)");
    assert.equal(last,details.total,'Last row unavailable');
   }
   if(file.endsWith('.md')||file.endsWith('.html')){
    assert.equal(await client.evaluate('data.text.length'),fs.readFileSync(input,'utf8').length);
    assert(await client.evaluate('richPages.length>1'),'Large document must paginate');
    await client.evaluate('page=richPages.length-1;render()');
    for(let wait=0;wait<100;wait++){if(await client.evaluate("document.querySelector('main')?.dataset.ready==='true'"))break;await sleep(25)}
    assert(await client.evaluate("document.querySelector('iframe').srcdoc.includes(richPages.at(-1))"),'Final document page missing');
   }
   const result={file,bytes:fs.statSync(input).size,readyMs,...details};results.push(result);
   console.log(`${file}: ${readyMs} ms, complete input, PASS`);
  }finally{
   client?.socket.close();
   execFileSync('pwsh',['-NoProfile','-Command',`$p=Get-Process -Id ${child.pid} -ErrorAction SilentlyContinue;if($p){[void]$p.CloseMainWindow();[void]$p.WaitForExit(5000)}`],{windowsHide:true,stdio:'ignore'});
  }
 }
 fs.writeFileSync(process.env.QUICKPEEK_PERF_REPORT || 'docs/evidence/performance.json',JSON.stringify(results,null,2)+'\n');
})().catch(error=>{console.error(error);process.exitCode=1});
