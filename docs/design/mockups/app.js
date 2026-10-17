// Builds the phones from the templates, applies a direction's tokens, runs the live analysis,
// handles taps and screen transitions, and plays the demo flow.

(function () {
  const DIRS = window.PDM_DIRECTIONS;
  const SCREENS = [
    { id: "home", label: "Garden home" },
    { id: "capture", label: "Capture" },
    { id: "live", label: "Live analysis" },
    { id: "result", label: "Diagnosis and plan" },
    { id: "step", label: "Plan, step by step" },
    { id: "activity", label: "Live Activity" },
    { id: "garden3d", label: "3D garden" },
    { id: "insights", label: "Insights" },
  ].filter((s) => document.getElementById("t-" + s.id));

  const prefersReduce = window.matchMedia && matchMedia("(prefers-reduced-motion: reduce)").matches;
  const state = {
    dir: DIRS[0].id,
    view: "dir",
    screen: "live",
    scheme: document.documentElement.getAttribute("data-scheme"),
    motion: prefersReduce ? "reduce" : "full",
  };
  (location.hash.slice(1) || "").split("&").forEach((kv) => {
    const [k, v] = kv.split("=");
    if (k in state && v) state[k] = decodeURIComponent(v);
  });
  if (!DIRS.some((d) => d.id === state.dir)) state.dir = DIRS[0].id;

  const $ = (s, el) => (el || document).querySelector(s);
  const $$ = (s, el) => Array.from((el || document).querySelectorAll(s));
  const kebab = (s) => s.replace(/[A-Z]/g, (m) => "-" + m.toLowerCase());
  const wait = (ms) => new Promise((r) => setTimeout(r, ms));

  // ---------- Tokens ----------

  // A damped spring as a CSS linear() easing, so a direction can specify motion as physics.
  function spring({ stiffness = 300, damping = 26, mass = 1 }) {
    let x = 0, v = 0, t = 0;
    const dt = 1 / 120, pts = [];
    while (t < 3) {
      const a = (-stiffness * (x - 1) - damping * v) / mass;
      v += a * dt; x += v * dt; t += dt;
      pts.push(x);
      if (t > 0.1 && Math.abs(1 - x) < 0.001 && Math.abs(v) < 0.01) break;
    }
    const n = Math.min(40, pts.length), step = (pts.length - 1) / (n - 1);
    const out = [];
    for (let i = 0; i < n; i++) out.push(pts[Math.round(i * step)].toFixed(4));
    out[0] = "0"; out[out.length - 1] = "1";
    return { dur: Math.round(t * 1000), ease: "linear(" + out.join(", ") + ")" };
  }
  const motionOf = (m) => (m.spring ? spring(m.spring) : { dur: m.dur, ease: m.ease });

  const loadedFonts = new Set();
  function loadFonts(dir) {
    (dir.fonts.google || []).forEach((f) => {
      if (loadedFonts.has(f)) return;
      loadedFonts.add(f);
      const l = document.createElement("link");
      l.rel = "stylesheet";
      l.href = "https://fonts.googleapis.com/css2?family=" + f + "&display=swap";
      document.head.appendChild(l);
    });
  }

  function applyTokens(phone, dir) {
    const scheme = state.scheme;
    const c = dir.colors[scheme];
    const set = (k, v) => phone.style.setProperty(k, String(v));
    Object.keys(c).forEach((k) => set("--c-" + kebab(k), c[k]));
    const cap = dir.capture || { accent: dir.colors.dark.accent, accentText: dir.colors.dark.accentText };
    set("--cap-accent", cap.accent);
    set("--c-overlay-accent", dir.colors.dark.accent);
    set("--c-overlay-accent-text", dir.colors.dark.accentText);
    set("--cap-accent-text", cap.accentText);

    const f = dir.fonts;
    set("--font-display", f.display);
    set("--font-body", f.body);
    set("--font-num", f.num === "inherit" ? f.body : f.num);
    set("--display-weight", f.displayWeight);
    set("--display-tracking", f.displayTracking || "0");

    const label = dir.sections === "label";
    set("--sec-font", label || dir.sectionFont === "body" ? "var(--font-body)" : "var(--font-display)");
    set("--sec-size", label ? "12px" : (dir.sectionSize || 20) + "px");
    set("--sec-weight", label ? 600 : dir.sectionWeight || 600);
    set("--sec-transform", label ? "uppercase" : "none");
    set("--sec-tracking", label ? "0.08em" : "0");
    set("--sec-color", label ? "var(--c-text-muted)" : "var(--c-text)");
    set("--chip-transform", dir.shape.chipCase === "upper" ? "uppercase" : "none");
    set("--chip-tracking", dir.shape.chipCase === "upper" ? "0.04em" : "0");

    const s = dir.shape;
    set("--r-card", s.card + "px");
    set("--r-button", s.button + "px");
    set("--r-chip", s.chip + "px");
    set("--r-chip-cap", Math.min(s.chip, 23) + "px");
    set("--r-photo", s.photo + "px");
    set("--r-check", s.check === "square" ? "6px" : "50%");
    set("--icon-stroke", s.iconStroke || 1.8);
    const dark = scheme === "dark";
    const styles = {
      border: ["1px solid var(--c-border)", "none"],
      rule: ["1px solid var(--c-border)", "none"],
      fill: ["0", "none"],
      block: ["0", "none"],
      shadow: [dark ? "1px solid var(--c-border)" : "0", dark ? "none" : s.shadow || "0 1px 2px rgba(20,20,10,.05), 0 10px 26px -8px rgba(20,20,10,.16)"],
    }[s.cardStyle];
    set("--card-border", styles[0]);
    set("--card-shadow", styles[1]);

    ["enter", "tick", "box", "check", "push"].forEach((k) => {
      const m = motionOf(dir.motion[k]);
      set("--m-" + k + "-dur", m.dur + "ms");
      set("--m-" + k + "-ease", m.ease);
    });

    set("--enter-y", (dir.motion.enterY ?? 10) + "px");
    set("--enter-scale", dir.motion.enterScale ?? 1);
    set("--tick-from", dir.motion.tickFrom ?? 1);
    const L = dir.layout;
    phone.dataset.region = L.region;
    phone.dataset.unsure = L.unsure;
    phone.dataset.weather = L.weather;
    phone.dataset.buttons = L.buttons;
    phone.dataset.times = String(!!L.times);
    phone.dataset.illus = String(!!L.illus);
    phone.dataset.italic = String(!!dir.italicUnsure);
    phone.dataset.heroNames = String(!!dir.heroNames);
    phone.dataset.mat = String(!!s.mat);
    phone.dataset.home = dir.layout.home;
    phone.dataset.plants = dir.layout.plants;
    phone.dataset.scan = dir.layout.scan;
    phone.dataset.card = s.cardStyle;
    phone.dataset.sec = dir.sections;
    phone.classList.toggle("reduce", state.motion === "reduce");
  }

  // ---------- Phones ----------

  const SBAR = '<span>9:41</span><span class="r">' +
    '<svg viewBox="0 0 18 12"><rect x="0" y="8" width="3" height="4" rx="1"/><rect x="5" y="5.5" width="3" height="6.5" rx="1"/><rect x="10" y="3" width="3" height="9" rx="1"/><rect x="15" y="0" width="3" height="12" rx="1"/></svg>' +
    '<svg viewBox="0 0 16 12"><path d="M8 2.2c2.4 0 4.6.9 6.2 2.5l1.2-1.3A10.4 10.4 0 0 0 8 .4 10.4 10.4 0 0 0 .6 3.4l1.2 1.3A8.6 8.6 0 0 1 8 2.2zm0 3.6c1.4 0 2.7.5 3.7 1.4l1.2-1.3A7.1 7.1 0 0 0 8 4a7.1 7.1 0 0 0-4.9 1.9l1.2 1.3c1-.9 2.3-1.4 3.7-1.4zm0 3.5c.5 0 1 .2 1.3.5L8 11.6 6.7 9.8c.3-.3.8-.5 1.3-.5z"/></svg>' +
    '<svg viewBox="0 0 27 12"><rect x=".5" y=".5" width="22" height="11" rx="3" fill="none" stroke="currentColor" opacity=".4"/><rect x="2" y="2" width="17" height="8" rx="1.6"/><rect x="24" y="4" width="2" height="4" rx="1" opacity=".4"/></svg></span>';

  let flowToken = 0;
  const phones = [];
  // The screens a deep screen sits on, so Back and Close lead where they would in the app.
  const PARENTS = { live: ["home"], result: ["home"], step: ["home", "result"], garden3d: ["home"], insights: ["home", "garden3d"], capture: ["home"] };

  function buildPhone(dir, screenId, opts) {
    opts = opts || {};
    const fig = document.createElement("figure");
    fig.className = "phone-wrap";
    const phone = document.createElement("div");
    phone.className = "phone";
    phone.innerHTML = '<div class="island"></div><div class="sbar">' + SBAR + '</div><div class="hind"></div>';
    const cap = document.createElement("figcaption");
    fig.append(phone, cap);
    applyTokens(phone, dir);

    const P = { dir, phone, fig, stack: [], layer: null, ctl: null, flow: !!opts.flow };

    function onDark(id) {
      return id === "capture" || id === "activity" || (id === "home" && dir.layout.home === "hero") || ((id === "live" || id === "result") && dir.layout.scan === "hero");
    }
    function caption(id) {
      const s = SCREENS.find((x) => x.id === id);
      cap.innerHTML = "<b>" + dir.name + "</b> · " + (s ? s.label : id);
    }
    function makeLayer(id) {
      const layer = document.createElement("div");
      layer.className = "layer";
      const vp = document.createElement("div");
      vp.className = "viewport";
      layer.appendChild(vp);
      const frag = document.getElementById("t-" + id).content.cloneNode(true);
      Array.from(frag.children).forEach((n) => (n.classList.contains("fixed") ? layer : vp).appendChild(n));
      layer.dataset.screen = id;
      return layer;
    }
    function initLayer(layer) {
      const id = layer.dataset.screen;
      if (id === "live") return liveController(P, layer);
      if (id === "capture") {
        const d = $("[data-dist]", layer);
        const t = setInterval(() => { d.textContent = 30 + Math.round(Math.random() * 4) + " cm"; }, 900);
        return { dispose: () => clearInterval(t) };
      }
      if (id === "step") return stepController(P, layer);
      if (id === "activity") {
        P.phone.classList.add("island-live");
        $(".island", P.phone).innerHTML = '<span class="il-l"><svg class="i sm"><use href="#i-leaf"/></svg></span><span class="il-r num">6/9</span>';
        return { dispose() { P.phone.classList.remove("island-live"); $(".island", P.phone).innerHTML = ""; } };
      }
      if (id === "garden3d") {
        const stage = $("[data-g3d]", layer);
        return window.PDMGarden3D.mount(stage, layer, { dir, scheme: state.scheme, reduced: state.motion === "reduce" });
      }
      return null;
    }
    function show(id, kind) {
      const old = P.layer, oldCtl = P.ctl;
      const layer = makeLayer(id);
      if ((kind === "pop" || kind === "dismiss") && old) phone.insertBefore(layer, old); else phone.insertBefore(layer, $(".island", phone));
      P.layer = layer;
      phone.dataset.screen = id;
      phone.classList.toggle("on-dark", onDark(id));
      caption(id);
      P.ctl = initLayer(layer);
      if (old && kind) {
        const pairs = { push: ["push-in", "push-out"], modal: ["modal-in", "modal-under"], pop: ["pop-in", "pop-out"], dismiss: ["modal-back", "modal-out"] }[kind];
        layer.classList.add(pairs[0]);
        old.classList.add(pairs[1]);
        const dur = parseFloat(getComputedStyle(phone).getPropertyValue("--m-push-dur")) || 350;
        setTimeout(() => { old.remove(); if (oldCtl && oldCtl.dispose) oldCtl.dispose(); layer.classList.remove(pairs[0]); }, state.motion === "reduce" ? 0 : dur + 30);
      } else if (old) {
        old.remove();
        if (oldCtl && oldCtl.dispose) oldCtl.dispose();
      }
    }
    // Each stack entry remembers how it was presented, so a modal is dismissed, not popped.
    P.navigate = (id, kind) => { P.stack.push({ id, kind }); show(id, kind); };
    P.back = () => {
      const top = P.stack.length > 1 ? P.stack.pop() : { kind: "push" };
      if (!P.stack.length) P.stack = [{ id: "home" }];
      show(P.stack[P.stack.length - 1].id, top.kind === "modal" ? "dismiss" : "pop");
    };
    P.reset = (id, kind) => { P.stack = [{ id }]; show(id, kind); };

    phone.addEventListener("click", (e) => {
      const a = e.target.closest("[data-act]");
      if (!a || !phone.contains(a)) return;
      const act = a.dataset.act;
      if (act.startsWith("nav:")) { const [, id, kind] = act.split(":"); P.navigate(id, kind); }
      else if (act === "back") P.back();
      else if (act === "check") a.classList.toggle("done");
      else if (act === "answer" && P.ctl && P.ctl.answer) P.ctl.answer();
      else if ((act === "step-done" || act === "step-next") && P.ctl && P.ctl.advance) P.ctl.advance(act === "step-done");
      else if (act === "shutter") {
        const fl = $(".flash", P.layer);
        fl.classList.remove("go"); void fl.offsetWidth; fl.classList.add("go");
        const seg = $("[data-dial]", P.layer);
        if (seg) { seg.classList.add("done"); $("[data-dial-count]", P.layer).textContent = "2 of 2"; }
        const shot = $("[data-shot]", P.layer);
        if (shot && !shot.querySelector(".ok, .spin")) {
          shot.insertAdjacentHTML("beforeend", '<span class="spin"></span>');
          setTimeout(() => { const sp = shot.querySelector(".spin"); if (sp) sp.outerHTML = '<span class="ok"><svg viewBox="0 0 24 24"><path d="M5 12.5l4.5 4.5L19 7.5"/></svg></span>'; shot.classList.remove("current"); }, 900);
        }
      }
    });

    P.stack = (PARENTS[screenId] || []).map((id) => ({ id })).concat([{ id: screenId, kind: screenId === "capture" ? "modal" : "push" }]);
    show(screenId, null);
    phones.push(P);
    return P;
  }

  // The live analysis: t is the index of the running stage; 9 means every stage is done.
  function liveController(P, layer) {
    const stages = $$(".stage", layer);
    const vp = $(".viewport", layer);
    let t = -1, timer = 0, dead = false;
    const tickMs = state.motion === "reduce" ? 500 : 950;
    const now = $("[data-phase-now]", layer), time = $("[data-phase-time]", layer), next = $("[data-phase-next]", layer);
    let started = performance.now(), total = 0;
    const phaseTimer = setInterval(() => {
      if (t < 9 && t !== 6) time.textContent = Math.floor((performance.now() - started) / 1000 * 2.2) + " s";
    }, 250);
    function phase(n) {
      const label = (i) => $(".sl", stages[i]).textContent;
      total += Math.floor((performance.now() - started) / 1000 * 2.2);
      started = performance.now();
      if (n >= 9) { now.textContent = "Plan ready"; time.textContent = "in " + total + " s"; next.textContent = ""; return; }
      now.textContent = n === 6 ? "Waiting for your answer" : label(n);
      time.textContent = n === 6 ? "" : "0 s";
      next.textContent = n + 1 < 9 ? "Next: " + label(n + 1).toLowerCase() : "Next: your plan";
    }
    function setT(n) {
      t = n;
      phase(n);
      layer.dataset.t = String(n);
      stages.forEach((s, i) => { s.className = "stage " + (i < n ? "done" : i === n ? "running" : "waiting"); });
      $$("[data-at], [data-until]", layer).forEach((el) => {
        const at = el.dataset.at != null ? +el.dataset.at : -Infinity;
        const until = el.dataset.until != null ? +el.dataset.until : Infinity;
        const hide = n < at || n >= until;
        const was = el.classList.contains("hidden");
        el.classList.toggle("hidden", hide);
        if (was && !hide && el.dataset.at != null) {
          el.classList.remove("in"); void el.offsetWidth; el.classList.add("in");
          if (P.flow && el.hasAttribute("data-focus")) {
            const top = el.getBoundingClientRect().top - vp.getBoundingClientRect().top + vp.scrollTop - 140;
            vp.scrollTo({ top, behavior: state.motion === "reduce" ? "auto" : "smooth" });
          }
        }
      });
    }
    function run(to) {
      clearInterval(timer);
      timer = setInterval(() => { if (dead) return; if (t < to) setT(t + 1); else clearInterval(timer); }, tickMs);
    }
    $$("[data-at], [data-until]", layer).forEach((el) => el.classList.add("hidden"));
    setT(0);
    run(6);
    return {
      get t() { return t; },
      answer() { if (t === 6) { setT(7); run(9); } },
      dispose() { dead = true; clearInterval(timer); clearInterval(phaseTimer); },
    };
  }

  // The plan one step per screen (inspiration.md, Crouton's step mode): each step with its timing,
  // why it helps, its source, and any time phrase turned into named tasks.
  const STEPS = [
    { title: "Remove the lowest infected leaves", when: "Today", cat: "Cultural", icon: "i-hand",
      why: "Spores spread from the lower leaves when rain splashes them.", how: "Cut each leaf at its stem with clean pruners. Bag it; don't compost it.", cite: "1", src: "OMAFRA, Early blight of tomato" },
    { title: "Check for new spots", when: "Every 3 days until Nov 1", cat: "Monitoring", icon: "i-repeat",
      why: "New spots on the upper leaves would mean it is still spreading.", tasks: ["Oct 20", "Oct 23", "Oct 26", "Oct 29", "Nov 1"] },
    { title: "After the next rain, remove splashed leaves", when: "When it next rains", cat: "Cultural", icon: "i-rain",
      why: "Rain splashes spores from the soil onto the low leaves.", trigger: "A task appears when the forecast shows rain." },
    { title: "Apply a fungicide registered for early blight", when: "By Oct 22", cat: "Chemical", icon: "i-flask",
      why: "It protects leaves that aren't infected yet. It doesn't cure spots already there.", safety: "Use only a product registered for this use where you live, and follow its label exactly.", cite: "2", src: "Cornell Vegetable MD Online, Early blight" },
    { title: "Tie up the lowest branches", when: "Today", cat: "Cultural", icon: "i-pin",
      why: "Tomato 2 is 40 cm from Tomato 1, which has early blight. More air between them dries the leaves faster.", how: "Lift the branches off the soil and tie them to the stake." },
  ];

  function stepController(P, layer) {
    let i = 0;
    const done = new Set();
    const body = $("[data-step-body]", layer), dots = $("[data-step-dots]", layer), n = $("[data-step-n]", layer);
    function draw(dirn) {
      dots.innerHTML = STEPS.map((_, j) => `<i class="${j === i ? "on" : ""} ${done.has(j) ? "done" : ""}"></i>`).join("");
      n.textContent = i < STEPS.length ? i + 1 + " of " + STEPS.length : "Done";
      if (i >= STEPS.length) {
        body.innerHTML = `<div class="step-card in"><div class="step-when">All set</div><h1 class="step-title">${done.size} of ${STEPS.length} steps done</h1><p class="t-body muted">The rest are on your Tasks tab, with the dates above. We'll remind you on the day.</p></div>`;
        $$(".step-actions .btn", layer).forEach((b) => (b.hidden = true));
        return;
      }
      const s = STEPS[i];
      body.innerHTML = `<div class="step-card ${dirn ? "slide" : "in"}">
        <div class="step-when"><svg class="i sm"><use href="#${s.icon}"/></svg>${s.when} · ${s.cat}</div>
        <h1 class="step-title">${s.title}</h1>
        <p class="t-body">${s.why}</p>
        ${s.how ? `<p class="t-sub muted">${s.how}</p>` : ""}
        ${s.tasks ? `<div class="step-tasks"><div class="t-fn muted">Adds ${s.tasks.length} tasks</div><div class="chips">${s.tasks.map((d) => `<span class="num">${d}</span>`).join("")}</div></div>` : ""}
        ${s.trigger ? `<div class="step-tasks"><div class="t-fn muted">${s.trigger}</div></div>` : ""}
        ${s.safety ? `<div class="safety"><svg class="i"><use href="#i-info"/></svg><span>${s.safety}</span></div>` : ""}
        ${s.src ? `<div class="step-src t-fn"><span class="cite">${s.cite}</span>${s.src}</div>` : ""}
      </div>`;
    }
    draw(false);
    return {
      advance(markDone) { if (markDone) done.add(i); i++; draw(true); },
      dispose() {},
    };
  }

  // ---------- Views ----------

  function brief(dir) {
    const c = dir.colors[state.scheme];
    const sw = ["background", "surface", "text", "accent", "healthy", "watch", "sick", "info"]
      .map((k) => `<div class="swatch"><i style="background:${c[k]}"></i>${k}</div>`).join("");
    const refs = (dir.references || []).map((r) => `<a href="${r.url}" target="_blank" rel="noopener">${r.label}</a>`).join(" · ");
    return `<div><h2>${dir.name}</h2><div class="words">${dir.words}</div><p>${dir.summary}</p><p class="muted" style="color:var(--page-muted)">${dir.philosophy}</p>${refs ? `<p style="font-size:14px">References: ${refs}</p>` : ""}</div>
      <div><dl><dt>Type</dt><dd>${dir.type}</dd><dt>Motion</dt><dd>${dir.motionNote}</dd><dt>3D garden</dt><dd>${dir.garden3dNote}</dd><dt>Palette</dt><dd><div class="swatches">${sw}</div></dd></dl></div>`;
  }

  const FLOW = [
    ["Garden home", "Weather, the frost alert and every plant's health. Today's tasks include the weekly check of Tomato 2, from Tomato 1's plan."],
    ["Capture", "The LiDAR distance hint while framing; each shot uploads as it is taken."],
    ["Live analysis", "Tomato 2's scan: each stage ticks, the box is drawn on the spots, and when the evidence is split between two problems, one question settles it."],
    ["Diagnosis and plan", "Worded from the evidence, with what matched, the risk, the beneficial larva and a cited checklist."],
    ["3D garden", "Built from the map: beds and plants sized by their measurements, coloured by health. Scrub back in time and watch Tomato 1 turn sick on Oct 11, then on into Thursday\'s frost forecast, with covers over the tender plants."],
    ["Insights", "Spots cover less of the leaves at each check since the infected leaves were removed; height with its noise band; the humid days before."],
  ];

  async function playFlow(P, token) {
    const alive = () => token === flowToken && document.body.contains(P.phone);
    const step = (i) => $$(".flow-steps li").forEach((li, j) => li.classList.toggle("on", j === i));
    const touch = document.createElement("div");
    touch.className = "touch";
    touch.style.opacity = "0";
    P.phone.appendChild(touch);
    async function tap(sel) {
      if (!alive()) throw 0;
      const el = $$(sel, P.layer).find((x) => x.getClientRects().length && getComputedStyle(x).visibility !== "hidden");
      if (!el) return;
      const pr = P.phone.getBoundingClientRect(), r = el.getBoundingClientRect();
      touch.style.opacity = "1";
      touch.style.left = r.left - pr.left + r.width / 2 + "px";
      touch.style.top = r.top - pr.top + r.height / 2 + "px";
      await wait(700);
      if (!alive()) throw 0;
      touch.classList.remove("press"); void touch.offsetWidth; touch.classList.add("press");
      await wait(180);
      el.click();
      await wait(250);
      touch.style.opacity = "0";
    }
    async function scrollTo(sel) {
      const vp = $(".viewport", P.layer), el = $(sel, P.layer);
      if (!el) return;
      vp.scrollTo({ top: el.getBoundingClientRect().top - vp.getBoundingClientRect().top + vp.scrollTop - 110, behavior: state.motion === "reduce" ? "auto" : "smooth" });
      await wait(1100);
    }
    async function until(fn) { while (!fn()) { if (!alive()) throw 0; await wait(120); } }
    try {
      step(0); await wait(2200); await tap('[data-act="nav:capture:modal"]');
      step(1); await wait(1700); await tap('[data-act="shutter"]'); await wait(1400); await tap('[data-act="nav:live:push"]');
      step(2); await until(() => P.ctl && P.ctl.t >= 6); await wait(1500); await tap('[data-act="answer"]');
      await until(() => P.ctl && P.ctl.t >= 9); await wait(900); await tap('[data-act="nav:result:push"]');
      step(3); await wait(2000); await scrollTo("[data-plan]"); await wait(700); await tap("[data-plan] .task"); await wait(1500);
      P.reset("home", "pop"); await wait(900);
      step(4); await tap('[data-act="nav:garden3d:push"]'); await wait(2400);
      for (let i = 0; i < 6 && P.ctl && P.ctl.scrubTo; i++) { if (!alive()) throw 0; P.ctl.scrubTo(i); await wait(i === 5 ? 2600 : 650); }
      if (P.ctl && P.ctl.scrubTo) P.ctl.scrubTo(4);
      await wait(900);
      if (P.ctl && P.ctl.select) { P.ctl.select("tomato1"); }
      await wait(800);
      step(5); await tap('[data-act="nav:insights:push"]'); await wait(2600); await scrollTo(".chart"); await wait(3200);
      step(-1);
      $("#c-replay").textContent = "Replay";
    } catch (e) { /* replaced by a newer render */ }
  }

  function render() {
    flowToken++;
    phones.splice(0).forEach((P) => { if (P.ctl && P.ctl.dispose) P.ctl.dispose(); P.ctl = null; });
    const grid = $("#grid");
    grid.innerHTML = "";
    const dir = DIRS.find((d) => d.id === state.dir);
    DIRS.forEach(loadFonts);

    $("#c-screen-wrap").hidden = state.view !== "screen";
    $("#c-replay").hidden = state.view !== "flow";
    $$("#c-dir button").forEach((b) => b.setAttribute("aria-pressed", String(b.dataset.v === state.dir)));
    $$("#c-dir button").forEach((b) => (b.disabled = state.view === "screen"));
    $("#c-dir").style.opacity = state.view === "screen" ? "0.5" : "1";
    [["#c-view", "view"], ["#c-screen", "screen"], ["#c-scheme", "scheme"], ["#c-motion", "motion"]].forEach(([sel, k]) =>
      $$(sel + " button").forEach((b) => b.setAttribute("aria-pressed", String(b.dataset.v === state[k]))));

    if (state.view === "screen") {
      const s = SCREENS.find((x) => x.id === state.screen) || SCREENS[0];
      $("#brief").innerHTML = `<div><h2>${s.label} in every direction</h2><p>The same screen and content, so only the design differs. Each phone stays clickable.</p></div><div><dl>${DIRS.map((d) => `<dt>${d.name}</dt><dd>${d.words}</dd>`).join("")}</dl></div>`;
      DIRS.forEach((d) => grid.appendChild(buildPhone(d, s.id).fig));
    } else if (state.view === "flow") {
      $("#brief").innerHTML = brief(dir);
      const wrap = document.createElement("div");
      wrap.style.cssText = "display:flex;flex-wrap:wrap;gap:32px;align-items:flex-start;justify-content:center";
      const P = buildPhone(dir, "home", { flow: true });
      const steps = document.createElement("ol");
      steps.className = "flow-steps";
      steps.innerHTML = FLOW.map(([h, t]) => `<li><b>${h}</b><span>${t}</span></li>`).join("");
      wrap.append(P.fig, steps);
      grid.appendChild(wrap);
      setTimeout(() => playFlow(P, flowToken), 400);
    } else {
      $("#brief").innerHTML = brief(dir);
      SCREENS.forEach((s) => grid.appendChild(buildPhone(dir, s.id).fig));
    }
    history.replaceState(null, "", "#" + Object.entries(state).map(([k, v]) => k + "=" + encodeURIComponent(v)).join("&"));
  }

  // ---------- Controls ----------

  $("#c-dir").innerHTML = DIRS.map((d) => `<button data-v="${d.id}">${d.name}</button>`).join("");
  $("#c-screen").innerHTML = SCREENS.map((s) => `<button data-v="${s.id}">${s.label}</button>`).join("");
  const bind = (sel, key, after) => $(sel).addEventListener("click", (e) => {
    const b = e.target.closest("button[data-v]");
    if (!b || b.disabled) return;
    state[key] = b.dataset.v;
    if (after) after();
    render();
  });
  bind("#c-dir", "dir");
  bind("#c-view", "view");
  bind("#c-screen", "screen");
  bind("#c-scheme", "scheme", () => document.documentElement.setAttribute("data-scheme", state.scheme));
  bind("#c-motion", "motion");
  $("#c-replay").addEventListener("click", render);

  const style = document.createElement("style");
  style.textContent = `.flow-steps{list-style:none;margin:12px 0 0;padding:0;max-width:340px;display:grid;gap:10px;counter-reset:s}
    .flow-steps li{counter-increment:s;padding:12px 14px;border-radius:12px;border:1px solid var(--page-border);background:var(--page-surface);display:grid;gap:2px;transition:border-color .2s, transform .2s;color:var(--page-muted);font-size:14px}
    .flow-steps li b{color:var(--page-text);font-size:15px}
    .flow-steps li b::before{content:counter(s) ". "}
    .flow-steps li.on{border-color:var(--page-accent);transform:translateX(4px)}`;
  document.head.appendChild(style);

  document.documentElement.setAttribute("data-scheme", state.scheme);
  render();
})();
