'use strict';
const $=id=>document.getElementById(id), content=$('content');
let raw=false,page=0,sheet=0,query='',observer=null,renderId=0,copyAll=false,currentText='';
let richPages=null,richHead='';
const isTable=data.kind==='csv'||data.kind==='spreadsheet';
const richFile=['word','power_point','archive','svg','email','rtf','epub'].includes(data.kind);
const isDocument=richFile||data.kind==='markdown'||data.kind==='html';
$('mode').hidden=richFile||['text','code','spreadsheet','markdown'].includes(data.kind);
$('format').hidden=data.kind!=='json';
$('notice').textContent=data.notice;
data.sheets.forEach((s,i)=>{const option=document.createElement('option');option.value=i;option.textContent=s.name;$('sheets').append(option)});
$('sheets').hidden=data.kind==='csv';
const aliases={rs:'rust',js:'javascript',jsx:'javascript',ts:'typescript',tsx:'typescript',ps1:'powershell',py:'python',cs:'csharp',h:'c',hpp:'cpp',sh:'bash',rb:'ruby',kt:'kotlin',yml:'yaml',toml:'ini',vue:'xml',md:'markdown'};
const language=aliases[data.language.toLowerCase()]||data.language.toLowerCase();
const idle=work=>window.requestIdleCallback?requestIdleCallback(work,{timeout:300}):setTimeout(work,0);
function paintCode(code,text,generation){
 code.textContent=text;
 if(language==='plaintext'||!window.hljs?.getLanguage(language))return;
 idle(()=>{
  if(generation!==renderId||!code.isConnected)return;
  // Restrict expensive grammar work per visible chunk, not per file.
  const limit=16000, snippet=text.slice(0,limit);
  const result=hljs.highlight(snippet,{language,ignoreIllegals:true});
  code.innerHTML=result.value;
  if(text.length>limit)code.append(document.createTextNode(text.slice(limit)));
  code.dataset.highlighted='true';
 });
}
function showText(text){
 currentText=text;
 const allLines=text.split('\n');
 const lines=query?allLines.filter(line=>line.toLocaleLowerCase().includes(query)):allLines;
 const generation=renderId,fragment=document.createDocumentFragment(),chunks=[];
 const mount=pre=>{
  if(pre.dataset.mounted)return;pre.dataset.mounted='true';
  const start=Number(pre.dataset.start),end=Math.min(start+100,lines.length);
  const code=document.createElement('code');pre.append(code);
  paintCode(code,lines.slice(start,end).join('\n')+(end<lines.length?'\n':''),generation);
 };
 for(let i=0;i<lines.length;i+=100){
  const pre=document.createElement('pre');pre.className='chunk';pre.dataset.start=i;
  const height=Math.min(100,lines.length-i)*23;
  pre.style.containIntrinsicSize=`auto ${height}px`;pre.style.minHeight=`${height}px`;
  fragment.append(pre);chunks.push(pre);
 }
 content.append(fragment);
 // Small documents retain complete native text selection. Large files only
 // mount visible chunks; Ctrl+A/C still copies the complete source model.
 if(text.length<512*1024){for(const pre of chunks)mount(pre)}
 else {for(const pre of chunks.slice(0,2))mount(pre)}
 observer=new IntersectionObserver(entries=>{for(const entry of entries){if(entry.isIntersecting)mount(entry.target)}},{root:content,rootMargin:'200px'});
 for(const pre of chunks)observer.observe(pre);
 $('status').textContent=query?`${lines.length.toLocaleString()} 行`:'';
}
const pendingSheets=new Map();
window.loadSheet=(index,rows)=>{
 data.sheets[index].rows=rows;data.sheets[index].loaded=true;
 pendingSheets.get(index)?.resolve();pendingSheets.delete(index);
};
function ensureSheet(index){
 const active=data.sheets[index];if(!active||active.loaded||active.rows.length===active.count)return Promise.resolve();
 if(pendingSheets.has(index))return pendingSheets.get(index).promise;
 let resolve,reject;const promise=new Promise((ok,fail)=>{resolve=ok;reject=fail});
 pendingSheets.set(index,{promise,resolve,reject});
 const script=document.createElement('script');script.src=active.file;
 script.onerror=()=>{pendingSheets.delete(index);reject(new Error('工作表加载失败'))};
 document.head.append(script);return promise;
}
function columnLabel(index){let label='',n=index+1;while(n>0){n--;label=String.fromCharCode(65+n%26)+label;n=Math.floor(n/26)}return label}
function prepareRichPages(){
 if(richPages)return;
 const html=data.kind==='markdown'?data.rendered:data.text;
 richHead=richFile||data.kind==='markdown'?'<style>'+docStyle+'</style>':'';
 if(data.rendered_pages?.length){richPages=data.rendered_pages;return}
 richPages=[html];
 if(html.length<512*1024)return;
 const parsed=new DOMParser().parseFromString(html,'text/html');
 let root=parsed.body;
 // Preserve each complete block and any single enclosing container. Never
 // cut source strings through tags, tables, or CSS rules to create a page.
 while(root.children.length===1&&Array.from(root.childNodes).every(n=>n.nodeType===1||!n.textContent.trim()))root=root.firstElementChild;
 if(root.children.length<300)return;
 const head=parsed.head.innerHTML,groups=[];
 let batch=[],elements=0;
 for(const node of Array.from(root.childNodes)){
  batch.push(node);if(node.nodeType===1)elements++;
  if(elements>=160){groups.push(batch);batch=[];elements=0}
 }if(batch.length)groups.push(batch);
 // Serialize a page only when it is first displayed.
 richPages=new Array(groups.length);
 groups.forEach((_,index)=>Object.defineProperty(richPages,index,{configurable:true,get(){
  const nodes=groups[index];
  let pageRoot=root.cloneNode(false);for(const node of nodes)pageRoot.appendChild(node);
  for(let parent=root.parentElement;root!==parsed.body&&parent;parent=parent.parentElement){
   if(parent===parsed.documentElement)break;
   const wrapper=parent.cloneNode(false);wrapper.append(pageRoot);pageRoot=wrapper;
  }
  const value=pageRoot.outerHTML;
  Object.defineProperty(richPages,index,{value,configurable:true});
  groups[index]=null;
  return value;
 }}));
 // Release the detached DOM in short idle slices after the first paint.
 const pages=richPages;
 let next=1;
 function finishPages(){
  const start=performance.now();
  while(next<pages.length && performance.now()-start<4){void pages[next++];}
  if(next<pages.length)idle(finishPages);
 }
 requestAnimationFrame(()=>requestAnimationFrame(()=>idle(finishPages)));
 richHead+=head;
}
let lastMarkdownQuery='', matchingPages=[];
function markdownPages(){
 if(!query)return richPages;
 if(query===lastMarkdownQuery)return matchingPages;
 lastMarkdownQuery=query;
 matchingPages=[];
 for(const html of richPages){
  const doc=new DOMParser().parseFromString(html,'text/html');
  for(const block of [...doc.body.children]){
   if(!block.textContent.toLocaleLowerCase().includes(query))block.remove();
  }
  if(doc.body.children.length)matchingPages.push(doc.body.innerHTML);
 }
 if(!matchingPages.length)matchingPages.push('<p>未找到匹配内容</p>');
 return matchingPages;
}
function widths(active){
 if(active.widths)return active.widths;
 const measure=document.createElement('canvas').getContext('2d');measure.font='13px Segoe UI';
 active.widths=Array.from({length:active.columns},(_,c)=>{
  let width=96;for(const row of active.rows.slice(0,35)){width=Math.max(width,measure.measureText((row[c]||'').slice(0,80)).width+28)}
  return Math.min(300,Math.ceil(width));
 });return active.widths;
}
async function showTable(generation){
 const active=data.sheets[sheet];if(!active){$('status').textContent='空工作簿';return}
 if(page>0||query){await ensureSheet(sheet);if(generation!==renderId)return}
 const csvHeader=data.kind==='csv'&&active.rows.length>0;
 const offset=csvHeader?1:0;
 const filtered=query?active.rows.map((cells,i)=>({cells,i})).filter(row=>row.i>=offset&&row.cells.some(cell=>cell.toLocaleLowerCase().includes(query))):null;
 const count=filtered?filtered.length:Math.max(0,active.count-offset);
 const pages=Math.max(1,Math.ceil(count/100));page=Math.min(page,pages-1);
 // CSV page one needs the 101st physical row after its header.
 if(csvHeader&&active.rows.length<Math.min(active.count,101)){await ensureSheet(sheet);if(generation!==renderId)return}
 const shown=filtered?filtered.slice(page*100,(page+1)*100):active.rows.slice(page*100+offset,(page+1)*100+offset).map((cells,i)=>({cells,i:page*100+offset+i}));
 $('previous').disabled=page===0;$('next').disabled=page===pages-1;
 $('page-info').textContent=`${page+1} / ${pages} 页`;
 $('status').textContent=query?`${count.toLocaleString()} 行`:'';
 const table=document.createElement('table'),colgroup=document.createElement('colgroup');
 const sizes=[48,...widths(active)];table.style.width=`${sizes.reduce((a,b)=>a+b,0)}px`;
 for(const size of sizes){const col=document.createElement('col');col.style.width=`${size}px`;colgroup.append(col)}table.append(colgroup);
 const thead=document.createElement('thead'),head=document.createElement('tr');
 for(let c=-1;c<active.columns;c++){
  const th=document.createElement('th');th.textContent=c<0?'':csvHeader?(active.rows[0][c]||columnLabel(c)):columnLabel(c);
  if(csvHeader)th.className='csv-heading';th.title=th.textContent;head.append(th);
 }thead.append(head);table.append(thead);
 const tbody=document.createElement('tbody');
 for(const {cells,i} of shown){
  const tr=document.createElement('tr');if(i===0)tr.className='field-row';
  const number=document.createElement('td');number.textContent=i+1;number.className='row-number';tr.append(number);
  for(let c=0;c<active.columns;c++){
   const value=cells[c]||'',td=document.createElement('td');td.textContent=value;td.title=value;td.tabIndex=-1;
   if(/^-?\d+(\.\d+)?$/.test(value))td.className='number';
   td.onclick=()=>{content.querySelector('.selected')?.classList.remove('selected');td.classList.add('selected');$('selection').textContent=`${columnLabel(c)}${i+1}  ${value}`;$('selection').title=value};
   tr.append(td);
  }tbody.append(tr);
 }table.append(tbody);content.replaceChildren(table);
 content.dataset.ready='true';
}
let archiveRoot=null, archiveFolder=null;
function indexArchive(){
 const root={name:'压缩包',parent:null,folders:new Map(),files:[]};
 for(const item of data.archive_entries){
  const names=item.path.split(/[\\/]/).filter(name=>name&&name!=='.');
  if(!names.length)continue;
  const filename=item.directory?null:names.pop();
  let folder=root;
  for(const name of names){
   if(!folder.folders.has(name))folder.folders.set(name,{name,parent:folder,folders:new Map(),files:[]});
   folder=folder.folders.get(name);
  }
  if(filename)folder.files.push({name:filename,entry:item});
 }
 return root;
}
function archiveButton(label,action){
 const button=document.createElement('button');
 button.type='button';button.textContent=label;button.onclick=action;
 return button;
}
function enterArchiveFolder(folder){
 archiveFolder=folder;page=0;render();content.focus();
}
function showArchive(){
 if(!archiveRoot){archiveRoot=indexArchive();archiveFolder=archiveRoot;}
 const folder=archiveFolder;
 content.className='archive-view';
 const trail=document.createElement('nav');trail.className='archive-path';trail.ariaLabel='目录路径';
 const up=archiveButton('↑ 上一级',()=>enterArchiveFolder(folder.parent));
 up.disabled=!folder.parent;trail.append(up);
 const chain=[];for(let node=folder;node;node=node.parent)chain.push(node);
 for(const node of chain.reverse()){
  if(node!==archiveRoot)trail.append(document.createTextNode(' › '));
  const button=archiveButton(node.name,()=>enterArchiveFolder(node));
  if(node===folder)button.setAttribute('aria-current','page');
  trail.append(button);
 }
 content.append(trail);
 const sort=(a,b)=>a.name.localeCompare(b.name,'zh-CN',{numeric:true});
 const items=[...Array.from(folder.folders.values()).sort(sort),...folder.files.slice().sort(sort)];
 const pages=Math.max(1,Math.ceil(items.length/200));
 page=Math.max(0,Math.min(page,pages-1));
 document.querySelector('footer').hidden=pages===1;
 $('sheets').hidden=true;$('previous').disabled=page===0;$('next').disabled=page===pages-1;
 $('page-info').textContent=(page+1)+' / '+pages+' 页';
 if(!items.length){const empty=document.createElement('p');empty.textContent='此目录为空';content.append(empty);}
 else {
  const table=document.createElement('table');const head=table.createTHead().insertRow();
  for(const label of ['名称','大小']){const th=document.createElement('th');th.textContent=label;head.append(th);}
  const tbody=table.createTBody();
  for(const item of items.slice(page*200,(page+1)*200)){
   const row=tbody.insertRow();const file=item.entry;
   const button=archiveButton(item.name,()=>file?window.chrome.webview.postMessage(file.path):enterArchiveFolder(item));
   button.className=file?'archive-file':'archive-folder';
   if(file&&(file.kind==='unknown'||file.size>64*1024*1024)){
    button.disabled=true;button.title=file.kind==='unknown'?'此文件类型暂不支持预览':'文件超过 64 MB 预览上限';
   }
   row.insertCell().append(button);

   row.insertCell().textContent=file?.size_label ?? '';
  }
  content.append(table);
 }
 content.dataset.ready='true';
}
function showVectorImage(){
 const image=new Image();
 const source=new DOMParser().parseFromString(data.rendered_pages[0],'text/html').querySelector('img')?.getAttribute('src');
 if(!source?.startsWith('data:image/svg+xml;base64,'))return;
 content.className='vector-view';
 let zoom=1;
 const stage=document.createElement('div');
 stage.className='vector-stage';stage.append(image);content.append(stage);
 image.alt='SVG';image.onload=()=>{content.dataset.ready='true';};image.src=source;
 stage.onwheel=event=>{
  event.preventDefault();
  const bounds=content.getBoundingClientRect(),x=event.clientX-bounds.left,y=event.clientY-bounds.top;
  const previous=image.getBoundingClientRect();
  const fx=(event.clientX-previous.left)/previous.width,fy=(event.clientY-previous.top)/previous.height;
  const delta=event.deltaY*(event.deltaMode===1?16:event.deltaMode===2?content.clientHeight:1);
  zoom=Math.min(16,Math.max(.1,zoom*Math.exp(-Math.max(-2,Math.min(2,delta*.002)))));
  stage.style.width=stage.style.height=Math.max(100,zoom*100)+'%';
  image.style.width=image.style.height=Math.min(100,zoom*100)+'%';
  const next=image.getBoundingClientRect();
  content.scrollLeft+=next.left-bounds.left+fx*next.width-x;
  content.scrollTop+=next.top-bounds.top+fy*next.height-y;
 };
}
function render(){
 $('status').textContent='';renderId++;const generation=renderId;observer?.disconnect();observer=null;copyAll=false;
 content.replaceChildren();content.scrollTop=0;content.scrollLeft=0;delete content.dataset.ready;
 $('mode').textContent=raw?'预览':'源码';$('mode').setAttribute('aria-pressed',raw);
 document.querySelector('footer').hidden=!isTable||raw;
 if(data.kind==='svg'){showVectorImage();return}
 if(data.kind==='archive'){showArchive();return}
 if(isTable&&!raw){content.className='table-view';showTable(generation).catch(error=>{$('notice').textContent=error.message});return}
 if(isDocument&&!raw&&(!query||data.kind==='markdown')){
  prepareRichPages();const pages=data.kind==='markdown'?markdownPages():richPages;page=Math.min(page,pages.length-1);
  document.querySelector('footer').hidden=pages.length<2;
  $('sheets').hidden=true;
  $('previous').disabled=page===0;$('next').disabled=page===pages.length-1;
  $('page-info').textContent=`${page+1} / ${pages.length} 页`;
  content.className='document-view';
  const frame=document.createElement('iframe');frame.title='文档预览';frame.setAttribute('sandbox','');
  const policy=`<meta http-equiv="Content-Security-Policy" content="default-src 'none'; style-src 'unsafe-inline'; img-src data:; form-action 'none';">`;
  frame.onload=()=>{if(generation===renderId)content.dataset.ready='true'};
  frame.srcdoc='<!doctype html><meta charset="utf-8">'+policy+richHead+pages[page];
  content.append(frame);return;
 }
 content.className='text-view';showText(raw?(data.source??data.text):data.text);content.dataset.ready='true';
}
$('mode').onclick=()=>{if(!$('mode').hidden){raw=!raw;render()}};
$('format').onclick=()=>{raw=false;render()};
$('previous').onclick=()=>{page--;render()};$('next').onclick=()=>{page++;render()};
$('sheets').onchange=()=>{sheet=Number($('sheets').value);page=0;render()};
let timer;$('find').oninput=()=>{clearTimeout(timer);timer=setTimeout(()=>{query=$('find').value.toLocaleLowerCase();page=0;render()},120)};
document.addEventListener('keydown',event=>{
 const editing=['INPUT','TEXTAREA','SELECT'].includes(document.activeElement.tagName);
 if(event.ctrlKey&&event.key.toLowerCase()==='r'){event.preventDefault();if(!$('mode').hidden){raw=!raw;render()}}
 if(event.ctrlKey&&event.key.toLowerCase()==='f'){event.preventDefault();$('find').hidden=false;$('find').focus()}
 if(event.ctrlKey&&event.key.toLowerCase()==='a'&&!editing&&content.classList.contains('text-view')){event.preventDefault();copyAll=true;const range=document.createRange();range.selectNodeContents(content);getSelection().removeAllRanges();getSelection().addRange(range)}
});
content.addEventListener('pointerdown',()=>{copyAll=false});
document.addEventListener('copy',event=>{if(copyAll){event.preventDefault();event.clipboardData.setData('text/plain',query?currentText.split('\n').filter(line=>line.toLocaleLowerCase().includes(query)).join('\n'):currentText)}});
const docStyle=`::-webkit-scrollbar{width:10px;height:10px}::-webkit-scrollbar-thumb{background:#89939d;border:2px solid transparent;background-clip:padding-box;border-radius:6px}::-webkit-scrollbar-thumb:hover{background-color:#636f7b}:root{color-scheme:light dark;font:16px/1.8 'Segoe UI','Microsoft YaHei',sans-serif;color:light-dark(#203047,#e2e8f2);background:light-dark(#fff,#1c2029)}body{max-width:920px;margin:0 auto;padding:28px 38px 80px;overflow-wrap:anywhere}h1,h2,h3,h4,h5,h6{line-height:1.35;font-weight:650}h1{font-size:32px;margin:0 0 28px;padding-bottom:18px;border-bottom:2px solid #8893a344}h2{font-size:25px;margin:40px 0 16px;padding-bottom:10px;border-bottom:1px solid #8893a344}h3{font-size:20px;margin:28px 0 12px}h4{font-size:18px}p{margin:12px 0}li{margin:5px 0}ul,ol{padding-left:28px}blockquote{margin-left:0;border-left:4px solid #7294c3;padding:4px 20px;background:#8893a310}pre{background:#8893a318;padding:18px;overflow:auto;border-radius:7px}code{font:14px/1.65 Consolas,monospace;background:#8893a318;padding:2px 4px}pre code{background:none;padding:0}table{border-collapse:collapse;width:100%}td,th{border:1px solid #8893a344;padding:8px 12px;text-align:left}th{background:#8893a318}img{max-width:100%;height:auto}a{color:light-dark(#2463b8,#9bbfff)}hr{border:0;border-top:1px solid #8893a344;margin:28px 0}@media(max-width:560px){body{padding:16px clamp(10px,4vw,22px) 32px}pre{padding:10px}td,th{padding:4px 6px}}`;

render();
