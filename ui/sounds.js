// One short sound per alert kind, synthesized with Web Audio (no sound files, nothing to license).
// Soft sine tones with a quick attack and exponential fade, through the same 2 kHz low-pass and
// 0.15 gain the Navi sounds used, so they stay gentle.
//   done        rising two-note chime (task finished)
//   question    up-then-up-again "hm?" (it is asking you something)
//   permission  two quick taps on one note (needs a yes/no)
//   idle        one low soft note (waiting, nothing urgent)
//   info        one high soft note
(function () {
  const ctx = new (window.AudioContext || window.webkitAudioContext)();
  const lp = ctx.createBiquadFilter(); lp.type = "lowpass"; lp.frequency.value = 2000;
  const out = ctx.createGain(); out.gain.value = 0.15;
  lp.connect(out); out.connect(ctx.destination);

  // f: Hz, at: seconds from now, len: seconds, vol: 0..1
  function tone(f, at, len, vol) {
    const t = ctx.currentTime + at;
    const o = ctx.createOscillator(), g = ctx.createGain();
    o.type = "sine"; o.frequency.value = f;
    g.gain.setValueAtTime(0.0001, t);
    g.gain.exponentialRampToValueAtTime(vol, t + 0.012);
    g.gain.exponentialRampToValueAtTime(0.0001, t + len);
    o.connect(g); g.connect(lp);
    o.start(t); o.stop(t + len + 0.02);
  }
  const N = { G4: 392, C5: 523.25, E5: 659.25, G5: 783.99, A5: 880, C6: 1046.5, E6: 1318.5 };
  const KIND = {
    done: () => { tone(N.C6, 0, 0.35, 0.9); tone(N.E6, 0.09, 0.5, 0.9); },
    question: () => { tone(N.E5, 0, 0.18, 0.8); tone(N.A5, 0.12, 0.35, 0.8); },
    permission: () => { tone(N.A5, 0, 0.14, 1); tone(N.A5, 0.16, 0.22, 1); },
    idle: () => { tone(N.G4, 0, 0.6, 0.7); },
    info: () => { tone(N.C6, 0, 0.3, 0.6); },
  };
  window.chime = function (kind) {
    if (ctx.state === "suspended") ctx.resume();
    (KIND[kind] || KIND.info)();
  };
})();
