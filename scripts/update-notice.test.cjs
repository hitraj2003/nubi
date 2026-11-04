const { test } = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');

function harness() {
  const nodes = new Map(), calls = [], events = {}, timers = [];
  function node(id) {
    if (!nodes.has(id)) nodes.set(id, {
      textContent: '', hidden: false, disabled: false, scrollTop: 0, listeners: {},
      addEventListener(event, fn) { this.listeners[event] = fn; },
      contains(other) { return [...nodes.values()].includes(other); },
    });
    return nodes.get(id);
  }
  const context = vm.createContext({
    window: { __TAURI__: {
      core: { invoke: async (name, args) => { calls.push([name, args]); return null; } },
      event: { listen: async (name, fn) => { events[name] = fn; } },
    }, addEventListener: (event, fn) => { events[event] = fn; } },
    document: { getElementById: node, addEventListener: (event, fn) => { events[event] = fn; } },
    Date: { now: () => 1_000 },
    setInterval: fn => timers.push(fn),
  });
  vm.runInContext(fs.readFileSync(path.join(__dirname, '../ui/update.js'), 'utf8'), context);
  const ready = new Promise(resolve => setImmediate(resolve));
  return { context, node, calls, events, timers, ready };
}

const notice = (changes = {}) => ({ id: 4, version: '0.2.0', notes: '- A useful fix',
  phase: 'available', automatic: false, expires_ms: 11_000, held: false, message: '', ...changes });

test('manual and automatic notices display notes as text and acknowledge readiness', async () => {
  const h = harness(); await h.ready;
  assert.ok(h.events.update_notice);
  assert.ok(h.calls.some(([name]) => name === 'get_update_notice'));
  h.context.render(notice({ notes: '<img src=x onerror=alert(1)>' }));
  assert.equal(h.node('notes').textContent, '<img src=x onerror=alert(1)>');
  assert.equal(h.node('notes').innerHTML, undefined);
  assert.equal(h.node('version').textContent, 'Version 0.2.0');
  assert.equal(h.node('countdown').textContent, 'Dismisses in 10s');
  assert.ok(h.calls.some(([name, args]) => name === 'update_notice_ready' && args.id === 4));
  h.context.render(notice({ automatic: true, expires_ms: 6_000 }));
  assert.equal(h.node('countdown').textContent, 'Updates in 5s');
  h.timers[0]();
  assert.ok(!h.calls.some(([name]) => name === 'install_update'));
});

test('hover pauses expiry and cancellation sends the current offer identity', async () => {
  const h = harness(); await h.ready;
  h.context.render(notice());
  h.node('notice').listeners.mouseenter();
  assert.ok(h.calls.some(([name, args]) => name === 'hold_update' && args.on === true));
  h.context.render(notice({ held: true }));
  assert.equal(h.node('countdown').textContent, 'Paused while you read');
  h.node('notice').listeners.mouseleave();
  assert.ok(h.calls.some(([name, args]) => name === 'hold_update' && args.on === false));
  h.node('cancel').listeners.click();
  assert.ok(h.calls.some(([name, args]) => name === 'dismiss_update' && args.id === 4));
});

test('download permits cancellation; installation disables both actions', async () => {
  const h = harness(); await h.ready;
  h.context.render(notice({ phase: 'downloading', expires_ms: 0 }));
  assert.equal(h.node('install').disabled, true);
  assert.equal(h.node('cancel').disabled, false);
  h.events.keydown({ key: 'Escape' });
  assert.ok(h.calls.some(([name]) => name === 'dismiss_update'));
  h.calls.length = 0;
  h.context.render(notice({ phase: 'installing', expires_ms: 0 }));
  assert.equal(h.node('install').disabled, true);
  assert.equal(h.node('cancel').disabled, true);
  h.events.keydown({ key: 'Escape' });
  assert.ok(!h.calls.some(([name]) => name === 'dismiss_update'));
});

test('Update requires a click and missing notes offer a release link', async () => {
  const h = harness(); await h.ready;
  h.context.render(notice({ notes: '' }));
  assert.match(h.node('notes').textContent, /release page/);
  await h.node('install').listeners.click();
  assert.ok(h.calls.some(([name, args]) => name === 'install_update' && args.id === 4));
  h.node('release-notes').listeners.click();
  assert.ok(h.calls.some(([name, args]) => name === 'open_release_link' && args.url.endsWith('/v0.2.0')));
  h.context.render(null);
  assert.equal(h.node('notice').hidden, true);
});
