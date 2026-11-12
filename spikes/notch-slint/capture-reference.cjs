// No npm dependencies. Render the unmodified production UI through Chrome CDP.
const fs = require('node:fs');
const path = require('node:path');
const http = require('node:http');
const {createHash} = require('node:crypto');
const {spawn} = require('node:child_process');
const root = path.resolve(__dirname, '../..');
const out = path.join(__dirname, 'reference');
fs.mkdirSync(out, {recursive:true});
const fixture = JSON.parse(fs.readFileSync(path.join(__dirname, 'fixture.json')));
const delay = ms => new Promise(resolve => setTimeout(resolve, ms));
const mime = {'.html':'text/html','.js':'text/javascript','.css':'text/css','.svg':'image/svg+xml','.png':'image/png','.woff2':'font/woff2'};
const server = http.createServer((request,response) => {
  const file = path.resolve(root, '.' + decodeURI(request.url.split('?')[0]));
  if (!file.startsWith(root + path.sep)) {response.writeHead(403);response.end();return;}
  fs.readFile(file,(error,data)=>{response.writeHead(error?404:200,{'Content-Type':(mime[path.extname(file)]||'application/octet-stream')+'; charset=utf-8'});response.end(error?'Not found':data);});
});
function stub(theme) {
  return `(() => {
    const state = ${JSON.stringify(fixture)};
    state.settings.theme = state.settings.notch_theme = ${JSON.stringify(theme)};
    const RealDate = Date;
    Date = class extends RealDate { constructor(...args) { super(...(args.length ? args : [state.now])); } static now() { return state.now; } };
    window.__fixture = state; window.__errors = []; window.__calls = [];
    const handlers = {};
    window.__emit = (name,payload) => (handlers[name] || []).forEach(handler => handler({payload}));
    window.__TAURI__ = {event:{listen:async(name,handler)=>{(handlers[name] ||= []).push(handler);return()=>{};}}, core:{invoke:async(name,args)=>{
      if (name === 'log_js') { window.__errors.push(args.msg); return; }
      window.__calls.push({name,args});
      if(name === 'get_state') return state;
      if(name === 'app_version') return '0.1.0';
      if(name === 'monitors') return ['Primary display · 1920 × 1080'];
      if(name === 'list_releases') return [];
      if(name === 'island_offset') return 0;
      if(name === 'desktime_state') return {enabled:false,connected:false,loading:false,extension_dir:'C:\\\\Nubi\\\\extension'};
      if(name === 'check_update') return 'You are up to date';
      if(name === 'save_settings') {state.settings=args.settings;window.__emit('state',state);}
      return null;
    }}};
  })();`;
}
async function main() {
  await new Promise(resolve=>server.listen(0,'127.0.0.1',resolve));
  const port = server.address().port;
  const chrome = spawn('C:/Program Files/Google/Chrome/Application/chrome.exe',[
    '--headless=new','--disable-gpu','--no-sandbox','--disable-dev-shm-usage','--no-first-run','--no-default-browser-check',
    '--remote-debugging-port=0',`--user-data-dir=${path.join(__dirname,'target/chrome-parity')}`,'about:blank'
  ], {windowsHide:true,stdio:['ignore','ignore','pipe']});
  let endpoint;
  chrome.stderr.on('data',data=>{const match=String(data).match(/DevTools listening on (ws:\/\/\S+)/);if(match) endpoint=match[1];});
  let socket;
  try {
    for(let attempt=0;!endpoint && attempt<100;attempt++) await delay(100);
    if(!endpoint) throw Error('Chrome CDP endpoint not available');
    const targets=await (await fetch(endpoint.replace('ws:','http:').replace(/\/devtools\/browser\/.*/, '/json/list'))).json();
    socket = new WebSocket(targets.find(target=>target.type==='page').webSocketDebuggerUrl);
    await new Promise((resolve,reject)=>{const timeout=setTimeout(()=>reject(Error('CDP WebSocket connection timeout')),10000);socket.onopen=()=>{clearTimeout(timeout);resolve();};socket.onerror=reject;});
    let id=0; const requests=new Map();
    socket.onmessage=event=>{const message=JSON.parse(event.data);if(message.id){const pending=requests.get(message.id);requests.delete(message.id);message.error?pending.reject(Error(JSON.stringify(message.error))):pending.resolve(message.result);}};
    const send=(method,params={})=>new Promise((resolve,reject)=>{const requestId=++id;const timeout=setTimeout(()=>reject(Error('CDP timeout: '+method)),10000);requests.set(requestId,{resolve:value=>{clearTimeout(timeout);resolve(value);},reject});socket.send(JSON.stringify({id:requestId,method,params}));});
    const evaluate=async expression=>{const result=await send('Runtime.evaluate',{expression,awaitPromise:true,returnByValue:true});if(result.exceptionDetails)throw Error(JSON.stringify(result.exceptionDetails));return result.result.value;};
    await send('Page.enable');await send('Runtime.enable');
    await send('Emulation.setDefaultBackgroundColorOverride',{color:{r:96,g:96,b:96,a:1}});
    if(process.argv.includes('--probe-fonts')) {
      await send('Page.addScriptToEvaluateOnNewDocument',{source:stub('dark')});
      await send('Page.navigate',{url:`http://127.0.0.1:${port}/ui/panel.html`});
      await delay(1000);
      await evaluate("document.querySelector('.rail [data-view=settings]').click()");
      await delay(1000);
      await send('DOM.enable'); await send('CSS.enable');
      const document=await send('DOM.getDocument');
      for(const selector of ['.pane.on .rl','.pane.on .rd','.pane.on h1']) {
        const node=await send('DOM.querySelector',{nodeId:document.root.nodeId,selector});
        console.log(selector,JSON.stringify(await send('CSS.getPlatformFontsForNode',{nodeId:node.nodeId})));
      }
      return;
    }
    const overlaysOnly=process.argv.includes('--overlays-only');
    const manifest=overlaysOnly ? JSON.parse(fs.readFileSync(path.join(out,'manifest.json'))).filter(row=>!row.filename.startsWith('overlay-')) : [];
    for(const theme of ['dark','light']) for(const scale of [1,1.5]) {
      const source=await send('Page.addScriptToEvaluateOnNewDocument',{source:stub(theme)});
      async function load(surface,width,height) {
        await send('Emulation.setDeviceMetricsOverride',{width,height,deviceScaleFactor:scale,mobile:false});
        await send('Page.navigate',{url:`http://127.0.0.1:${port}/ui/${surface}.html`});
        await delay(900);
        await evaluate('document.fonts.ready');
      }
      async function capture(name) {
        await delay(1300);
        const errors=await evaluate('window.__errors');
        if(errors.length)throw Error(`${name}: ${errors.join('; ')}`);
        const filename=`${name}-${theme}-${scale*100}.png`;
        const result=await send('Page.captureScreenshot',{format:'png',captureBeyondViewport:false,fromSurface:true});
        fs.writeFileSync(path.join(out,filename),Buffer.from(result.data,'base64'));
        const metrics=await evaluate(`({width:innerWidth,height:innerHeight,dpr:devicePixelRatio,shape:document.querySelector('#shape')?.getBoundingClientRect().toJSON(),items:[...document.querySelectorAll('.view.on header,.view.on .meter,.view.on .ses,.pane.on .card,.side button,.rail button,.pr,.srow,.mi,#deck .card,#deck .head,#deck .cmd,#deck .more,#deck .acts,#deck .q,#deck .opt,#deck .other')].filter(x=>x.offsetParent).map(x=>({class:x.className,text:x.innerText,rect:x.getBoundingClientRect().toJSON()}))})`);
        manifest.push({filename,...metrics});
        console.log(filename);
      }
      if(!overlaysOnly) {
      await load('island',720,700);
      if(theme==='dark' && scale===1) {
        const colors=await evaluate(`(() => {const canvas=document.createElement('canvas'),c=canvas.getContext('2d');return Array.from({length:101},(_,percent)=>[false,true].map(light=>{c.fillStyle=usageColor(percent,light);c.fillRect(0,0,1,1);return [...c.getImageData(0,0,1,1).data].slice(0,3)}))})()`);
        fs.writeFileSync(path.join(out,'usage-colors.json'),JSON.stringify(colors));
      }
      await capture('notch-closed');
      await evaluate("window.__emit('island_pointer',true)");
      await capture('notch-sessions');
      await evaluate("document.querySelector('[data-tab=usage]').click()");
      await capture('notch-usage');
      await load('panel',1100,760);
      for(const page of ['inbox','sessions','usage','alerts']) {
        await evaluate(`document.querySelector('.rail [data-view=${page}]').click()`);
        await capture(`panel-${page}`);
      }
      await evaluate("document.querySelector('.rail [data-view=settings]').click()");
      for(const pane of ['general','appearance','display','requests','notify','desktime','hooks','about']) {
        await evaluate(`document.querySelector('.side [data-pane=${pane}]').click()`);
        await capture(`settings-${pane}`);
      }
      }
      for(const [name,fixtureName] of [['approval','approval'],['parameters','parameters'],['expanded','parameters'],['question','question'],['multi','question'],['custom','question']]) {
        await load('overlay',720,700);
        await evaluate(`window.__fixture.pending=[window.__fixture.overlays[${JSON.stringify(fixtureName)}]];window.__emit('state',window.__fixture);window.__emit('show_requests',null)`);
        await delay(500);
        if(name==='expanded') await evaluate("document.querySelector('.more').open=true");
        if(name==='multi') await evaluate("document.querySelector('.opt').click();document.querySelector('.opt').click();document.activeElement.blur()");
        if(name==='custom') await evaluate("const input=document.querySelector('.other');input.value='Claude Code';input.dispatchEvent(new Event('input'))");
        await capture('overlay-'+name);
      }
      await send('Page.removeScriptToEvaluateOnNewDocument',{identifier:source.identifier});
    }
    fs.writeFileSync(path.join(out,'manifest.json'),JSON.stringify(manifest,null,2));
    fs.writeFileSync(path.join(out,'provenance.json'),JSON.stringify({
      browser:await send('Browser.getVersion'),fixtureSha256:createHash('sha256').update(fs.readFileSync(path.join(__dirname,'fixture.json'))).digest('hex'),
      sources:Object.fromEntries(fs.readdirSync(path.join(root,'ui')).filter(file=>/\.(html|css|js)$/.test(file)).map(file=>[file,createHash('sha256').update(fs.readFileSync(path.join(root,'ui',file))).digest('hex')]))
    },null,2));
  } finally {socket?.close();chrome.kill();server.close();}
}
main().catch(error=>{console.error(error);process.exitCode=1;server.close();});
