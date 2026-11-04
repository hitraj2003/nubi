// Shared motion: a small spring engine used by every window (island, notch, toast, ring, panel).
// Springs are described the SwiftUI way, by response (seconds per oscillation) and damping
// fraction (1 = no overshoot). Tuning follows jackson-storm/dynamicnotch's "balanced" preset
// (response 0.5, damping 0.70 for content, 0.75 for expanding); only the numbers are used, no code.
// Springs keep their velocity when retargeted, so an animation interrupted midway turns smoothly.
(function () {
  const reduced = matchMedia("(prefers-reduced-motion: reduce)");

  const PRESET = {
    open: { response: 0.5, damping: 0.75 },   // shape growing
    close: { response: 0.42, damping: 0.9 },  // shape shrinking: quicker, barely any overshoot
    content: { response: 0.5, damping: 0.7 }, // content inside
    snappy: { response: 0.3, damping: 0.8 },  // small UI: tabs, switches
    press: { response: 0.22, damping: 0.58 }, // button press bounce
  };

  function coeffs(p) {
    const w = (2 * Math.PI) / p.response;
    return { k: w * w, c: 2 * p.damping * w };
  }

  // One animated number.
  class Spring {
    constructor(value, preset) {
      this.x = value; this.v = 0; this.target = value;
      this.set(preset || PRESET.content);
    }
    set(preset) { const { k, c } = coeffs(preset); this.k = k; this.c = c; return this; }
    to(target, preset) { if (preset) this.set(preset); this.target = target; return this; }
    jump(value) { this.x = this.target = value; this.v = 0; return this; }
    step(dt) {
      // Fixed 4 ms sub-steps keep it stable even when a frame is late.
      let t = Math.min(dt, 0.064);
      while (t > 0) {
        const h = Math.min(t, 0.004);
        const a = -this.k * (this.x - this.target) - this.c * this.v;
        this.v += a * h; this.x += this.v * h; t -= h;
      }
      if (Math.abs(this.v) < 0.01 && Math.abs(this.x - this.target) < 0.01) { this.x = this.target; this.v = 0; return false; }
      return true;
    }
    get done() { return this.v === 0 && this.x === this.target; }
  }

  // Runs a set of springs on animation frames and calls draw(values) until all rest.
  class Animator {
    constructor(springs, draw) { this.s = springs; this.draw = draw; this.raf = 0; this.last = 0; }
    kick() {
      if (reduced.matches) { for (const k in this.s) this.s[k].jump(this.s[k].target); this.draw(this.values()); return; }
      if (this.raf) return;
      this.last = performance.now();
      const tick = (now) => {
        const dt = (now - this.last) / 1000; this.last = now;
        let moving = false;
        for (const k in this.s) if (this.s[k].step(dt)) moving = true;
        this.draw(this.values());
        this.raf = moving ? requestAnimationFrame(tick) : 0;
      };
      this.raf = requestAnimationFrame(tick);
    }
    values() { const o = {}; for (const k in this.s) o[k] = this.s[k].x; return o; }
  }

  // Spring an element's transform/opacity from one state to another (enter, leave, move).
  // from/to: { x, y, scale, opacity, blur }. Resolves when it rests.
  function tween(el, from, to, preset) {
    const keys = ["x", "y", "scale", "opacity", "blur"];
    const def = { x: 0, y: 0, scale: 1, opacity: 1, blur: 0 };
    const springs = {};
    for (const k of keys) springs[k] = new Spring(from[k] ?? def[k], preset || PRESET.content).to(to[k] ?? def[k]);
    // Opacity and blur settle a bit faster than position, like a crossfade riding on the spring.
    springs.opacity.set({ response: (preset || PRESET.content).response * 0.6, damping: 1 });
    springs.blur.set({ response: (preset || PRESET.content).response * 0.6, damping: 1 });
    return new Promise((res) => {
      const a = new Animator(springs, (v) => {
        el.style.transform = `translate(${v.x}px,${v.y}px) scale(${v.scale})`;
        el.style.opacity = String(Math.max(0, Math.min(1, v.opacity)));
        el.style.filter = v.blur > 0.05 ? `blur(${v.blur}px)` : "";
        if (Object.values(springs).every((s) => s.done)) res();
      });
      a.kick();
      if (reduced.matches) res();
    });
  }

  const enter = (el, preset) => tween(el, { y: -6, scale: 0.96, opacity: 0, blur: 6 }, {}, preset);
  const leave = (el, preset) => tween(el, {}, { y: -4, scale: 0.96, opacity: 0, blur: 6 }, preset || PRESET.close);

  // Springy press feedback for any button inside root.
  function pressable(root) {
    root.addEventListener("pointerdown", (e) => {
      const b = e.target.closest("button"); if (!b || b.disabled) return;
      tween(b, { scale: 1 }, { scale: 0.95 }, PRESET.press);
      const up = () => { tween(b, { scale: 0.95 }, { scale: 1 }, PRESET.press); removeEventListener("pointerup", up); };
      addEventListener("pointerup", up);
    });
  }

  // A highlight that slides between items (tabs, segmented controls, nav).
  function slider(hl) {
    const sx = new Spring(0, PRESET.snappy), sw = new Spring(0, PRESET.snappy);
    const a = new Animator({ sx, sw }, (v) => { hl.style.transform = `translateX(${v.sx}px)`; hl.style.width = v.sw + "px"; });
    let first = true;
    return (item) => {
      if (!item) return;
      const x = item.offsetLeft, w = item.offsetWidth;
      if (first) { sx.jump(x); sw.jump(w); first = false; a.draw(a.values()); return; }
      sx.to(x); sw.to(w); a.kick();
    };
  }

  window.Motion = { PRESET, Spring, Animator, tween, enter, leave, pressable, slider, reduced };
})();
