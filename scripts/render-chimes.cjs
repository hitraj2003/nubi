// Render the original Web Audio graph, without rewriting its synthesis rules.
// Usage: node scripts/render-chimes.cjs <sounds.js> <output-directory> [--check]
const fs = require('node:fs');
const path = require('node:path');
const os = require('node:os');
const {spawn} = require('node:child_process');
const {createHash} = require('node:crypto');
const [sourcePath, outputPath] = process.argv.slice(2);
if (!sourcePath || !outputPath) throw Error('Expected source sounds.js and output directory');
const check = process.argv.includes('--check');
const source = fs.readFileSync(sourcePath, 'utf8');
const hash = bytes => createHash('sha256').update(bytes).digest('hex');
const delay = ms => new Promise(resolve => setTimeout(resolve, ms));

async function main() {
  const profile = fs.mkdtempSync(path.join(os.tmpdir(), 'nubi-chime-render-'));
  const chrome = spawn('C:/Program Files/Google/Chrome/Application/chrome.exe', [
    '--headless=new', '--disable-gpu', '--no-first-run', '--no-default-browser-check',
    '--remote-debugging-port=0', `--user-data-dir=${profile}`, 'about:blank',
  ], {windowsHide: true, stdio: ['ignore', 'ignore', 'pipe']});
  let endpoint;
  chrome.stderr.on('data', data => {
    const match = String(data).match(/DevTools listening on (ws:\/\/\S+)/);
    if (match) endpoint = match[1];
  });
  let socket;
  try {
    for (let attempt = 0; !endpoint && attempt < 100; attempt++) await delay(100);
    if (!endpoint) throw Error('Chrome debugging endpoint unavailable');
    const listUrl = endpoint.replace('ws:', 'http:').replace(/\/devtools\/browser\/.*/, '/json/list');
    const targets = await (await fetch(listUrl)).json();
    socket = new WebSocket(targets.find(target => target.type === 'page').webSocketDebuggerUrl);
    await new Promise((resolve, reject) => { socket.onopen = resolve; socket.onerror = reject; });
    let id = 0;
    const pending = new Map();
    socket.onmessage = event => {
      const message = JSON.parse(event.data);
      if (!message.id) return;
      const waiter = pending.get(message.id);
      pending.delete(message.id);
      message.error ? waiter.reject(Error(JSON.stringify(message.error))) : waiter.resolve(message.result);
    };
    const send = (method, params = {}) => new Promise((resolve, reject) => {
      const request = ++id;
      const timer = setTimeout(() => reject(Error(`Timed out: ${method}`)), 20000);
      pending.set(request, {
        resolve: value => { clearTimeout(timer); resolve(value); },
        reject: error => { clearTimeout(timer); reject(error); },
      });
      socket.send(JSON.stringify({id: request, method, params}));
    });
    const browser = await send('Browser.getVersion');
    const manifest = {source_sha256: hash(source), browser: browser.product, sample_rate: 48000, channels: 1, bits: 16, sounds: {}};
    if (!check) fs.mkdirSync(outputPath, {recursive: true});
    for (const kind of ['done', 'question', 'permission', 'idle', 'info', 'unknown']) {
      const expression = `(async () => {
        const context = new OfflineAudioContext(1, 48000, 48000);
        const wrapper = new Proxy(context, {get(target, key) {
          if (key === 'state') return 'running';
          const value = Reflect.get(target, key, target);
          return typeof value === 'function' ? value.bind(target) : value;
        }});
        const host = {AudioContext: function () { return wrapper; }};
        new Function('window', ${JSON.stringify(source)})(host);
        host.chime(${JSON.stringify(kind)});
        return Array.from((await context.startRendering()).getChannelData(0));
      })()`;
      const result = await send('Runtime.evaluate', {expression, awaitPromise: true, returnByValue: true});
      if (result.exceptionDetails) throw Error(JSON.stringify(result.exceptionDetails));
      const samples = result.result.value;
      const wav = Buffer.alloc(44 + samples.length * 2);
      wav.write('RIFF'); wav.writeUInt32LE(wav.length - 8, 4); wav.write('WAVEfmt ', 8);
      wav.writeUInt32LE(16, 16); wav.writeUInt16LE(1, 20); wav.writeUInt16LE(1, 22);
      wav.writeUInt32LE(48000, 24); wav.writeUInt32LE(96000, 28);
      wav.writeUInt16LE(2, 32); wav.writeUInt16LE(16, 34);
      wav.write('data', 36); wav.writeUInt32LE(samples.length * 2, 40);
      let peak = 0;
      for (let index = 0; index < samples.length; index++) {
        peak = Math.max(peak, Math.abs(samples[index]));
        wav.writeInt16LE(Math.round(Math.max(-1, Math.min(1, samples[index])) * 32767), 44 + index * 2);
      }
      if (kind === 'unknown') {
        if (!wav.equals(fs.readFileSync(path.join(outputPath, 'info.wav')))) throw Error('Unknown-kind fallback differs');
        console.log('PASS unknown kind matches info');
        continue;
      }
      const file = path.join(outputPath, `${kind}.wav`);
      if (check) {
        if (!wav.equals(fs.readFileSync(file))) throw Error(`${kind}: waveform differs from original`);
      } else fs.writeFileSync(file, wav);
      if (!peak || peak >= 1) throw Error(`${kind}: silent or clipped output`);
      manifest.sounds[kind] = {sha256: hash(wav), samples: samples.length, peak};
      console.log(`PASS ${kind}: ${check ? 'identical PCM' : 'rendered original'}, ${samples.length} samples, peak ${peak.toFixed(6)}`);
    }
    if (!check) fs.writeFileSync(path.join(outputPath, 'provenance.json'), JSON.stringify(manifest, null, 2) + '\n');
    else if (JSON.parse(fs.readFileSync(path.join(outputPath, 'provenance.json'))).source_sha256 !== hash(source)) throw Error('Original source hash changed');
    await send('Browser.close');
  } finally {
    if (socket) socket.close();
    if (chrome.exitCode === null) chrome.kill();
  }
}
main().catch(error => { console.error(error); process.exitCode = 1; });
