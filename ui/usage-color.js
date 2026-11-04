// Shared usage colour scale (notch, floating ring, ring tooltip).
// Usage colour scale, in OKLCH so brightness stays even across the blend. Pure colours sit at
// 25 % green, 50 % blue, 75 % orange, 100 % red, blending smoothly between them (below 25 %
// stays green). Blue to orange are opposite hues, so the hue keeps turning forward (through
// violet and pink) instead of crossing the grey middle. `light` gives the softer shade used
// for the inner (session) ring; the outer (week) ring uses the full colour.
const STOPS = [
  [25, 0.74, 0.19, 148], // green
  [50, 0.64, 0.19, 258], // blue
  [75, 0.76, 0.17, 412], // orange (52 + 360: keep turning forward)
  [100, 0.64, 0.22, 388], // red (28 + 360)
];
function usageColor(pct, light) {
  const p = Math.max(0, Math.min(100, pct));
  let i = 0;
  while (i < STOPS.length - 1 && p > STOPS[i + 1][0]) i++;
  const [p0, l0, c0, h0] = STOPS[i], [p1, l1, c1, h1] = STOPS[Math.min(i + 1, STOPS.length - 1)];
  const t = p <= p0 ? 0 : p1 === p0 ? 1 : (p - p0) / (p1 - p0);
  const s = t * t * (3 - 2 * t); // smoothstep: eases into and out of each pure colour
  let l = l0 + (l1 - l0) * s, c = c0 + (c1 - c0) * s;
  const h = (h0 + (h1 - h0) * s) % 360;
  if (light) { l = Math.min(0.9, l + 0.1); c *= 0.62; }
  return `oklch(${l.toFixed(3)} ${c.toFixed(3)} ${h.toFixed(1)})`;
}
window.usageColor = usageColor;
