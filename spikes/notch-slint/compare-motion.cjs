// Run the original engine, rather than a JavaScript rewrite of the Rust port.
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const { execFileSync } = require('node:child_process');
const root = __dirname;
fs.mkdirSync(path.join(root, 'checks'), { recursive: true });
const executable = path.join(root, 'checks', 'motion-trace.exe');
execFileSync('rustc', ['--edition', '2024', path.join(root, 'src/motion_trace.rs'), '-o', executable], { stdio: 'inherit' });
const actual = execFileSync(executable, { encoding: 'utf8' }).trim().split(/\r?\n/).map(line => line.split(','));
const context = {
  window: {},
  document: { documentElement: { dataset: {} } },
  matchMedia: () => ({ matches: false }),
};
vm.runInNewContext(fs.readFileSync(path.join(root, '../../ui/motion.js'), 'utf8'), context);
const { Spring, PRESET } = context.window.Motion;
const spring = new Spring(180, PRESET.open).to(620);
let maxError = 0;
for (let frame = 0; frame < 240; frame++) {
  if (frame === 6) spring.to(180, PRESET.close);
  if (frame === 10) spring.to(620, PRESET.open);
  if (frame === 70) spring.to(180, PRESET.close);
  const moving = spring.step([0.016, 0.008, 0.033, 0.100][frame % 4]);
  const [index, x, velocity, running] = actual[frame];
  const error = Math.max(Math.abs(spring.x - Number(x)), Math.abs(spring.v - Number(velocity)));
  maxError = Math.max(maxError, error);
  if (Number(index) !== frame || error > 1e-9 || String(moving) !== running) {
    throw new Error(`Motion differs at frame ${frame}: max error ${error}, running ${running}/${moving}`);
  }
}
console.log(`PASS: 240 frames match ui/motion.js; maximum absolute error ${maxError}; reversal, long frames, settling covered.`);
