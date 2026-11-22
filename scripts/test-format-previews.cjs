const {connect} = require("./test-reader.cjs");
const {spawn, execFileSync} = require("node:child_process");
const fs = require("node:fs");
const os = require("node:os");
const path = require("node:path");
const assert = require("node:assert/strict");
const exe = path.resolve(process.argv[2] || "target/release/quickpeek.exe");
const out = path.resolve("target/extended-format-windows");
const profile = fs.mkdtempSync(path.join(os.tmpdir(), "QuickPeek-formats-"));
const pause = ms => new Promise(resolve => setTimeout(resolve, ms));

async function check(extension, first, second, port) {
    const file = path.resolve("tests/fixtures/formats/sample." + extension);
    const child = spawn(exe, [file], {windowsHide:true, stdio:"ignore",
        env:{...process.env, WEBVIEW2_USER_DATA_FOLDER:path.join(profile,extension),
             WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS:"--remote-debugging-port=" + port}});
    let client;
    try {
        client = await connect(port);
        if(extension==="svg"){
            for(let i=0;i<80&&!await client.evaluate("!!document.querySelector('.vector-stage img')?.naturalWidth");i++) await pause(100);
            assert(await client.evaluate("document.querySelector('.vector-stage img').naturalWidth>0"));
            assert(await client.evaluate("(()=>{let stage=document.querySelector('.vector-stage'),w=stage.offsetWidth;stage.dispatchEvent(new WheelEvent('wheel',{deltaY:-160,bubbles:true,cancelable:true}));return stage.offsetWidth>w})()"));
            console.log("svg: render and wheel zoom PASS");return;
        }
        if (["zip","rar","7z"].includes(extension)) {
            for (let i=0;i<80 && !await client.evaluate("!!document.querySelector('.archive-folder')");i++) await pause(100);
            for (const name of ["folder","sub"]) {
                assert(await client.evaluate("Array.from(document.querySelectorAll('.archive-folder')).some(b=>b.textContent==="+JSON.stringify(name)+")"));
                await client.evaluate("Array.from(document.querySelectorAll('.archive-folder')).find(b=>b.textContent==="+JSON.stringify(name)+").click()");
            }
            assert.equal(await client.evaluate("document.querySelector('.archive-file').textContent"),"note.txt");
            assert.match(await client.evaluate("document.querySelector('tbody').textContent"),/<0\.1 KB/);
            await client.evaluate("document.querySelector('.archive-path button').click()");
            assert.equal(await client.evaluate("document.querySelector('.archive-folder').textContent"),"sub");
            await client.evaluate("document.querySelectorAll('.archive-path button')[1].click()");
            assert(await client.evaluate("document.querySelector('.archive-path button').disabled"));
            console.log(extension+": archive navigation PASS");
            return;
        }
        for (const [index, text] of [first, second].filter(Boolean).entries()) {
            if (index) await client.evaluate("document.getElementById('next').click()");
            let ready = false;
            for (let i = 0; i < 80; i++) {
                ready = await client.evaluate("document.getElementById('content')?.dataset.ready === 'true' && document.querySelector('iframe')?.srcdoc.includes(" + JSON.stringify(text) + ")");
                if (ready) break;
                await pause(100);
            }
            assert(ready, extension + ": expected " + text);
            await pause(250);
            const screenshot = await client.send("Page.captureScreenshot", {format:"png"});
            fs.writeFileSync(path.join(out, extension + "-" + (index+1) + ".png"), Buffer.from(screenshot.data,"base64"));
        }
        console.log(extension + ": PASS");
    } finally {
        client?.socket.close();
        execFileSync("pwsh", ["-NoProfile","-Command",
            "$p=Get-Process -Id " + child.pid + " -ErrorAction SilentlyContinue; if($p){[void]$p.CloseMainWindow();[void]$p.WaitForExit(5000)}"], {windowsHide:true});
    }
}
(async () => {
    fs.mkdirSync(out,{recursive:true});
    const cases = [
        ["docx","Table cell"], ["pptx","First slide","Second slide"],
        ["zip","folder/note.txt"], ["7z","note.txt"], ["rar","note.txt"],
        ["svg","data:image/svg+xml;base64,"], ["eml","附件.txt（&lt;0.1 KB）"],
        ["rtf","<strong>"], ["epub","Chapter b","Chapter a"],
    ];
    for (const [i, [extension, first, second]] of cases.entries()) {
        await check(extension, first, second, 19841+i);
    }
})().catch(error => {console.error(error);process.exitCode=1;});
