const { test } = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const vm = require('node:vm');

const source = fs.readFileSync(require('node:path').join(__dirname, '../ui/island.js'), 'utf8');
class Node {
  constructor(tag, cls, text) {
    this.tag = tag; this.className = cls || ''; this.textContent = text || '';
    this.children = []; this.dataset = {}; this.style = { setProperty() {} };
  }
  append(...nodes) { nodes.forEach(n => { n.parent = this; n.isConnected = true; this.children.push(n); }); }
  replaceChildren(...nodes) { this.children.forEach(n => { n.parent = null; }); this.children = []; this.append(...nodes); }
  remove() { if (this.parent) this.parent.children = this.parent.children.filter(n => n !== this); this.isConnected = false; }
  querySelector(selector) { return this.children.find(n => n.className.split(' ').includes(selector.slice(1))) || null; }
}
function harness() {
  const calls = [], timers = [];
  const c = vm.createContext({
    snap: { settings: { providers: 'both' }, pending: [], missed: [] },
    cards: new Map(), tab: 'requests',
    el: (tag, cls, text) => new Node(tag, cls, text),
    glyph: provider => new Node('span', provider),
    btn: (text, cls, click) => Object.assign(new Node('button', cls, text), { click }),
    invoke: (command, args) => { calls.push([command, args]); return Promise.resolve(); },
    setTimeout: fn => timers.push(fn),
    leave: node => { node.remove(); return Promise.resolve(); },
    makeReq: a => ({ el: new Node('div', 'req', 'Live approval'), a, clock: { release() {} } }),
  });
  vm.runInContext(source.slice(source.indexOf('const wanted ='), source.indexOf('const SVGNS =')), c);
  vm.runInContext(source.slice(source.indexOf('function renderTerminalRequests('), source.indexOf('function tickCountdowns(')), c);
  c.pane = new Node('div');
  c.renderPane = () => { c.pane.replaceChildren(); c.renderRequests(c.pane, true); };
  return { c, calls, timers };
}

test('terminal handoff stays in Waiting and provides terminal actions', () => {
  const { c, calls } = harness();
  c.snap.pending = [{ id: 1, provider: 'codex' }];
  c.renderRequests(c.pane, true);
  assert.equal(c.pane.children[0].textContent, 'Live approval');
  c.snap.pending = [];
  c.snap.missed = [{ id: 1, provider: 'codex', session_key: 'codex:s', folder: 'G', kind: 'permission' }];
  c.renderRequests(c.pane, false);
  assert.equal(vm.runInContext('waiting().length', c), 1);
  assert.equal(c.pane.querySelector('.empty'), null);
  const row = c.pane.querySelector('.terminal-requests').children[0];
  assert.equal(row.children[1].textContent, 'Permission waiting in terminal');
  assert.deepEqual(row.children.filter(n => n.tag === 'button').map(n => n.textContent), ['Open terminal', 'Dismiss']);
  row.children[3].click(); row.children[4].click();
  assert.equal(calls[0][0], 'jump_session'); assert.equal(calls[0][1].key, 'codex:s');
  assert.equal(calls[1][0], 'dismiss_missed'); assert.equal(calls[1][1].id, 1);
});

test('answered terminal request disappears and Waiting becomes empty', () => {
  const { c, timers } = harness();
  c.snap.missed = [{ id: 1, provider: 'claude', kind: 'question' }];
  c.renderRequests(c.pane, true);
  assert.equal(c.pane.querySelector('.empty'), null);
  c.snap.missed = [];
  c.renderRequests(c.pane, false);
  timers.forEach(fn => fn());
  assert.equal(c.pane.querySelector('.terminal-requests'), null);
  assert.equal(c.pane.querySelector('.empty').textContent, 'Nothing is waiting for you.');
});

test('Waiting applies provider filtering to terminal handoffs', () => {
  const { c } = harness();
  c.snap.settings.providers = 'codex';
  c.snap.missed = [{ id: 1, provider: 'claude' }, { id: 2, provider: 'codex' }];
  assert.equal(vm.runInContext('waiting().length', c), 1);
  assert.equal(vm.runInContext('waiting()[0].id', c), 2);
});

test('new requests open the notch; handoff keeps its count; answers clear it', () => {
  const { c } = harness();
  const nodes = new Map();
  c.$ = id => {
    if (!nodes.has(id)) nodes.set(id, { textContent: '', classList: { toggle() {} } });
    return nodes.get(id);
  };
  Object.assign(c, {
    usageRefresh: null, seen: new Set(), edge: 'top', requestsUp: false,
    setEdge() {}, fetchOffset() {}, moveHl() {}, renderCompact() {}, decide() {},
    tabs: { querySelector() {} }, renderPane() {},
    setTab: tab => { c.tab = tab; }, armFold: () => { c.opened = (c.opened || 0) + 1; },
  });
  vm.runInContext(source.slice(source.indexOf('function onState(s)'), source.indexOf('listen("state",')), c);
  const state = { settings: { providers: 'both', edge: 'top' }, sessions: [], pending: [{ id: 1, provider: 'codex' }], missed: [] };
  c.onState(state);
  assert.equal(c.tab, 'requests'); assert.equal(c.requestsUp, true); assert.equal(c.opened, 1);
  assert.equal(nodes.get('tn-requests').textContent, 1);
  state.missed = state.pending; state.pending = [];
  c.onState(state);
  assert.equal(nodes.get('tn-requests').textContent, 1); assert.equal(c.opened, 1);
  state.missed = [];
  c.onState(state);
  assert.equal(nodes.get('tn-requests').textContent, ''); assert.equal(c.requestsUp, false);
});
