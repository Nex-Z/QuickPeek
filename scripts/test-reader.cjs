// Exercise the actual embedded WebView2, using its local DevTools endpoint.
const {spawn, execFileSync} = require('node:child_process');
const fs = require('node:fs');
const path = require('node:path');
const os = require('node:os');
const assert = require('node:assert/strict');
const exe = path.resolve(process.argv[2] || 'target/release/quickpeek.exe');
const temp = fs.mkdtempSync(path.join(os.tmpdir(), 'QuickPeek-reader-test-'));
const csv = path.join(temp, 'rows.csv');
fs.writeFileSync(csv, 'name,note\n'+Array.from({length:5000},(_,i)=>`row${i},"hello, ${i}"`).join('\n'));
const html = path.join(temp, 'page.html');
fs.writeFileSync(html, '<h1>HTML preview</h1><p>Rendered page</p><script>window.injected=true</script>');
const cases = [
  ['log', 'F:/Download/pytorch-2026-06-10_11-06-48.log'],
  ['markdown', 'F:/Download/锤子便签复刻版_短版prd与颜色系统.md'],
  ['json', 'F:/Download/response_body.json'],
  ['excel', 'F:/Download/dmp_emp_info_output.xlsx'],
  ['csv', csv], ['html', html], ['code','tests/fixtures/example.rs'],
];
const sleep = ms => new Promise(resolve=>setTimeout(resolve,ms));
async function connect(port) {
  for(let attempt=0;attempt<100;attempt++) {
    try {
      const pages=await (await fetch(`http://127.0.0.1:${port}/json/list`)).json();
      const page=pages.find(page=>page.url.includes('QuickPeek-previews'));
      if(page) {
        const socket=new WebSocket(page.webSocketDebuggerUrl);
        await new Promise((resolve,reject)=>{socket.onopen=resolve;socket.onerror=reject});
        let id=0; const pending=new Map();
        socket.onmessage=event=>{const reply=JSON.parse(event.data);const callbacks=pending.get(reply.id);if(callbacks){pending.delete(reply.id);reply.error?callbacks.reject(reply.error):callbacks.resolve(reply.result)}};
        const send=(method,params={})=>new Promise((resolve,reject)=>{const key=++id;pending.set(key,{resolve,reject});socket.send(JSON.stringify({id:key,method,params}))});
        return {socket,send,async evaluate(expression){const value=await send('Runtime.evaluate',{expression,returnByValue:true,awaitPromise:true});if(value.exceptionDetails)throw new Error(JSON.stringify(value.exceptionDetails));return value.result.value}};
      }
    } catch {}
    await sleep(100);
  }
  throw new Error('Embedded reader did not load');
}
async function main(){
 const results=[];
 for(const [index,[kind,file]] of cases.entries()) {
  const port=19330+index;
  const start=performance.now();
  const child=spawn(exe,['--classic',path.resolve(file)],{windowsHide:true,stdio:'ignore',env:{...process.env,WEBVIEW2_USER_DATA_FOLDER:path.join(temp,'profile'+index),WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS:`--remote-debugging-port=${port}`}});
  let client;
  try {
   client=await connect(port);
   let ready=false;
   for(let i=0;i<50;i++){if(await client.evaluate("document.getElementById('content')?.dataset.ready === 'true'")){ready=true;break}await sleep(100)}
   assert(ready,'Empty reader');
   const readyMs=Math.round(performance.now()-start);
   if(['excel','csv'].includes(kind)) {
    const before=await client.evaluate("({rows:document.querySelectorAll('tbody tr').length,status:document.getElementById('status').textContent,columns:document.querySelectorAll('thead th').length})");
    assert.equal(before.rows,100);
    assert(before.columns>1);
    await client.evaluate("document.getElementById('next').click()");
    await sleep(150);
    assert.equal(await client.evaluate("document.querySelector('tbody td').textContent"),kind==='csv'?'102':'101');
    await client.evaluate("document.getElementById('previous').click()");
   }
   if(kind==='log') {
    assert((await client.evaluate("document.querySelectorAll('pre').length"))>1);
    assert((await client.evaluate("document.getElementById('content').textContent.length"))>100000);
    const scroll=await client.evaluate("(async()=>{const main=document.getElementById('content'),start=performance.now();main.scrollTop=main.scrollHeight;await new Promise(requestAnimationFrame);return {ms:performance.now()-start,top:main.scrollTop}})()");
    assert(scroll.top>0);assert(scroll.ms<1000);
    await client.evaluate("const range=document.createRange();range.selectNodeContents(document.getElementById('content'));getSelection().removeAllRanges();getSelection().addRange(range)");
    assert((await client.evaluate("getSelection().toString().length"))>100000);
   }
   if(['markdown','html'].includes(kind)) {
    assert.equal(await client.evaluate("document.querySelector('iframe').getAttribute('sandbox')"),'');
    assert.equal(await client.evaluate("document.querySelector('iframe').srcdoc.includes('<h1>')"),true);
   }
   if(kind==='markdown'){
    assert(await client.evaluate("document.getElementById('mode').hidden"));
    await client.evaluate("document.getElementById('mode').click()");
    assert(await client.evaluate("!!document.querySelector('iframe')"));
   }
   if(kind==='json') assert.equal(await client.evaluate("/\\n  \"/.test(document.getElementById('content').textContent)"),true);
   if(['html','csv','json'].includes(kind)) {
    await client.evaluate("document.getElementById('mode').click()");
    assert.equal(await client.evaluate("document.getElementById('mode').textContent"),'预览');
    assert((await client.evaluate("document.querySelectorAll('pre').length"))>0);
    await client.evaluate("document.getElementById('mode').click()");
   }
   if(['html','json','code'].includes(kind)) {
    if(kind==='html')await client.evaluate("document.getElementById('mode').click()");
    await sleep(350);
    assert((await client.evaluate("document.querySelectorAll('[class^=hljs-]').length"))>0,kind+' syntax highlighting');
   }
   if(['excel','csv'].includes(kind)) {
    const gap=await client.evaluate("(async()=>{const m=document.querySelector('main');m.scrollTop=350;await new Promise(requestAnimationFrame);return document.querySelector('thead').getBoundingClientRect().top-m.getBoundingClientRect().top})()");
    assert(Math.abs(gap)<1,'Frozen header gap');
    assert((await client.evaluate("document.querySelector('tbody tr').getBoundingClientRect().height"))<40,'Table row height');
   }
   // Trigger Ctrl+F through the real native hook and inspect its resulting focus.
   execFileSync('pwsh',['-NoProfile','-Command',`Add-Type -TypeDefinition 'using System;using System.Runtime.InteropServices;public class Keys{[DllImport("user32.dll")]public static extern void keybd_event(byte k,byte s,uint f,UIntPtr e);[DllImport("user32.dll")]public static extern bool SetForegroundWindow(IntPtr h);}'; [Keys]::keybd_event(18,0,0,[UIntPtr]::Zero);[Keys]::keybd_event(18,0,2,[UIntPtr]::Zero);[void][Keys]::SetForegroundWindow((Get-Process -Id ${child.pid}).MainWindowHandle);Start-Sleep -Milliseconds 150;[Keys]::keybd_event(17,0,0,[UIntPtr]::Zero);[Keys]::keybd_event(70,0,0,[UIntPtr]::Zero);[Keys]::keybd_event(70,0,2,[UIntPtr]::Zero);[Keys]::keybd_event(17,0,2,[UIntPtr]::Zero)`],{windowsHide:true,stdio:'ignore'});
   await sleep(250);
   assert.equal(await client.evaluate("document.getElementById('find').hidden"),false,'Native Ctrl+F');
   assert.equal(await client.evaluate("document.activeElement.id"),'find','Search focus');
   results.push(`${kind}: PASS; fresh-process first reader ready ${readyMs} ms`);
  } finally {
   client?.socket.close();
   execFileSync('pwsh',['-NoProfile','-Command',`$p=Get-Process -Id ${child.pid} -ErrorAction SilentlyContinue;if($p){[void]$p.CloseMainWindow();[void]$p.WaitForExit(5000)}`],{windowsHide:true,stdio:'ignore'});
  }
 }
 fs.writeFileSync('docs/evidence/reader-tests.txt',results.join('\n')+'\n');
 console.log(results.join('\n'));
}
module.exports={connect};
if(require.main===module)main().catch(error=>{console.error(error);process.exitCode=1});
