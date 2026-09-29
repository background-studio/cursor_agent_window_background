const assert = require('node:assert/strict');
const vm = require('node:vm');
const { script, probe } = JSON.parse(require('node:fs').readFileSync(0, 'utf8'));
const STATE = '__CURSOR_AGENT_BACKGROUND_STUDIO__';

function fixture({ agent = true, legacy = false, menu = true, overlayHeight = 35, zoom = 1 } = {}) {
  class Element {
    constructor(tag, classes = '', rect = { width: 1920, height: 1032, x: 0, top: 0, bottom: 1032 }) {
      this.tagName = tag;
      this.className = classes;
      this.nodeType = 1;
      this.children = [];
      this.attributes = new Map();
      this.rect = rect;
      this.classList = { contains: name => classes.split(' ').includes(name) };
      const values = new Map();
      this.style = {
        setProperty: (name, value, priority = '') => values.set(name, { value, priority }),
        getPropertyValue: name => values.get(name)?.value || '',
        getPropertyPriority: name => values.get(name)?.priority || '',
        removeProperty: name => values.delete(name),
      };
    }
    get isConnected() { return this.root || !!this.parent?.isConnected; }
    getBoundingClientRect() { return this.rect; }
    matches(selector) { return selector.includes('glass-in-app-menubar') && this === bar; }
    setAttribute(name, value) { this.attributes.set(name, value); }
    getAttribute(name) { return this.attributes.get(name) ?? null; }
    hasAttribute(name) { return this.attributes.has(name); }
    removeAttribute(name) { this.attributes.delete(name); }
    append(...items) { for (const item of items) { item.parent = this; this.children.push(item); } }
    appendChild(item) { this.append(item); }
    prepend(item) { item.parent = this; this.children.unshift(item); }
    remove() { this.parent.children = this.parent.children.filter(item => item !== this); this.parent = null; }
  }
  const body = new Element('body', legacy ? 'bc-window' : 'cursor-dark');
  const head = new Element('head');
  body.root = head.root = true;
  const bar = menu ? new Element('div', '', { width: 1920, height: 36 * zoom, x: 0, top: 0, bottom: 36 * zoom }) : null;
  const panel = agent ? new Element('div', 'agent-panel') : null;
  if (bar) body.append(bar);
  if (panel) body.append(panel);
  const all = () => { const visit = e => [e, ...e.children.flatMap(visit)]; return [...visit(body), ...visit(head)]; };
  const document = {
    body, head,
    createElement: tag => new Element(tag),
    querySelector: selector => {
      if (selector === '.agent-panel') return panel;
      if (selector.includes('glass-in-app-menubar')) return bar;
      return all().find(e => selector === '#' + e.id) || null;
    },
  };
  let nextFrame = 0;
  const frames = new Map();
  let mutation;
  const context = vm.createContext({
    document, innerWidth: 1920, innerHeight: 1032,
    navigator: { windowControlsOverlay: { visible: true, getTitlebarAreaRect: () => ({ height: overlayHeight }) } },
    getComputedStyle: e => ({ height: e.style.getPropertyValue('height') || '36px', backgroundColor: e.style.getPropertyValue('background-color') || 'oklch(0.2 0 0)' }),
    requestAnimationFrame: callback => { frames.set(++nextFrame, callback); return nextFrame; },
    cancelAnimationFrame: id => frames.delete(id),
    MutationObserver: class { constructor(callback) { mutation = callback; } observe() {} disconnect() { mutation = null; } },
  });
  context.window = context;
  return {
    body, bar, panel, document, context, frames,
    install: () => vm.runInContext(script, context),
    probe: () => vm.runInContext(probe, context),
    state: () => context[STATE],
    mutate: () => mutation?.(),
    frame: () => { const callbacks = [...frames.values()]; frames.clear(); callbacks.forEach(callback => callback()); },
  };
}

// New standalone Agent windows no longer carry bc-window; old ones still work.
for (const options of [{}, { legacy: true, menu: false }]) {
  const f = fixture(options);
  assert.equal(f.probe(), true);
  assert.equal(f.install().installed, true);
  assert.equal(f.state().isHealthy(), true);
  assert(f.body.hasAttribute('data-cbg-cursor-agent-active'));
  assert.equal(f.install().unchanged, true);
  f.state().cleanup();
  assert.equal(f.state(), undefined);
  assert.equal(f.body.hasAttribute('data-cbg-cursor-agent-active'), false);
}

// IDE windows and an embedded agent panel without a standalone marker are untouched.
for (const options of [{ agent: false, menu: false }, { menu: false }]) {
  const f = fixture(options);
  assert.equal(f.probe(), false);
  assert.equal(f.install().installed, false);
  assert.equal(f.body.hasAttribute('data-cbg-cursor-agent-active'), false);
}

// A detached layer must be reinstalled even if configuration revision is unchanged.
{
  const f = fixture();
  f.install();
  f.document.querySelector('#cbg-cursor-agent-layer').remove();
  assert.equal(f.state().isHealthy(), false);
  assert.equal(f.install().installed, true);
  assert.equal(f.state().isHealthy(), true);
}

// Healthy window controls are never resized or recolored.
{
  const f = fixture();
  f.install();
  assert.equal(f.bar.style.getPropertyValue('height'), '');
  assert.equal(f.bar.style.getPropertyValue('background-color'), '');
  assert.equal(f.frames.size, 0);
}

// Bad native height: use CSS pixels (not zoomed rectangle), wait two frames,
// and restore exact inline styles and priorities instead of freezing a height.
{
  const f = fixture({ overlayHeight: 4905, zoom: 1.08 });
  f.bar.style.setProperty('min-height', '36px', 'important');
  f.install();
  assert.equal(f.bar.style.getPropertyValue('height'), '37px');
  f.frame();
  assert.equal(f.bar.style.getPropertyValue('height'), '37px');
  f.frame();
  assert.equal(f.bar.style.getPropertyValue('height'), '');
  assert.equal(f.bar.style.getPropertyValue('min-height'), '36px');
  assert.equal(f.bar.style.getPropertyPriority('min-height'), 'important');
  assert.equal(f.bar.style.getPropertyValue('max-height'), '');
  f.mutate();
  f.frame();
  assert.equal(f.frames.size, 0);
}

// Cleanup while a repair/paint is queued must cancel all mutations and restore styles.
{
  const f = fixture({ overlayHeight: 4905 });
  f.panel.style.setProperty('background-color', 'oklch(0.2 0 0)', 'important');
  f.panel.style.setProperty('box-shadow', '0 0 2px red');
  f.install();
  f.mutate();
  f.state().cleanup();
  f.frame();
  assert.equal(f.bar.style.getPropertyValue('height'), '');
  assert.equal(f.panel.style.getPropertyValue('background-color'), 'oklch(0.2 0 0)');
  assert.equal(f.panel.style.getPropertyPriority('background-color'), 'important');
  assert.equal(f.panel.style.getPropertyValue('box-shadow'), '0 0 2px red');
  assert.equal(f.frames.size, 0);
}

console.log('8 payload lifecycle scenarios passed');
