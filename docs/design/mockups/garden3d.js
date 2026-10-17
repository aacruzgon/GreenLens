// The 3D garden, built only from what the garden map stores (architecture 6.8): bed outlines,
// plant pins in metres on a local ground plane, and each plant's measured height and canopy.
// No mesh, photo or coordinates. Uses the three.js global (r128 from cdnjs). Each direction's
// garden3d block picks the form (organic, turned or survey), colouring, camera, light and beds.

(function () {
  // The map: metres, x to the east, z to the south. Heights and canopies are LiDAR measurements;
  // statuses and notes per date follow the mock fixture in analytics.md section 6.
  const BEDS = [
    { id: "east", x: -1.0, z: -0.7, w: 2.6, d: 1.1, h: 0.28 },
    { id: "south", x: 0.9, z: 0.95, w: 2.2, d: 1.0, h: 0.22 },
  ];
  const DATES = ["Sep 20", "Sep 27", "Oct 4", "Oct 11", "Oct 17", "Thu"];
  const FORECAST = 5;
  const PLANTS = [
    { id: "tomato1", name: "Tomato 1", kind: "tomato", bed: "east", x: -1.85, z: -0.7, h: 0.62, c: 0.48, tender: true,
      grow: [0.516, 0.661, 0.806, 0.935, 1],
      hist: [["healthy", "Looks healthy"], ["healthy", "Looks healthy"], ["healthy", "Looks healthy"], ["sick", "Early blight, after 3 humid days in a row"], ["sick", "Early blight, spots shrinking"]] },
    { id: "tomato2", name: "Tomato 2", kind: "tomato", bed: "east", x: -1.45, z: -0.62, h: 0.7, c: 0.5, tender: true,
      grow: [0.5, 0.64, 0.78, 0.9, 1], hist: null, status: "healthy", note: "Looks healthy" },
    { id: "pepper", name: "Pepper", kind: "pepper", bed: "east", x: -0.3, z: -0.72, h: 0.45, c: 0.38, tender: true,
      grow: [0.6, 0.72, 0.84, 0.95, 1],
      hist: [["healthy", "Looks healthy"], ["watch", "Aphids"], ["watch", "Ladybug larva eating the aphids"], ["healthy", "Looks healthy"], ["watch", "Yellow lower leaves"]] },
    { id: "basil", name: "Basil", kind: "basil", pot: true, x: 2.35, z: -0.85, h: 0.3, c: 0.28, tender: true,
      grow: [0.45, 0.6, 0.75, 0.9, 1], hist: null, status: "healthy", note: "Looks healthy" },
    { id: "lettuce", name: "Lettuce", kind: "lettuce", bed: "south", x: 0.25, z: 0.95, h: 0.18, c: 0.32, tender: false,
      grow: [0.35, 0.55, 0.72, 0.88, 1], hist: null, status: "healthy", note: "Looks healthy" },
    { id: "kale", name: "Kale", kind: "kale", bed: "south", x: 1.45, z: 0.95, h: 0.55, c: 0.46, tender: false,
      grow: [0.5, 0.65, 0.8, 0.92, 1], hist: null, status: "healthy", note: "Looks healthy" },
  ];
  const STATUS_WORD = { healthy: "Healthy", watch: "Watch", sick: "Sick", dead: "Dead" };
  const last = (i) => Math.min(i, 4);
  const statusAt = (p, i) => (p.hist ? p.hist[last(i)][0] : p.status);
  const growAt = (p, i) => p.grow[last(i)];
  const noteAt = (p, i) => {
    if (i === FORECAST) return p.tender ? "Frost risk: −1 °C Thursday night. Cover it by evening." : "Hardy to a light frost.";
    return p.hist ? p.hist[i][1] : p.note;
  };

  function mount(stage, screen, opts) {
    if (!window.THREE) {
      stage.innerHTML = '<div class="g3d-fallback">The 3D view needs three.js from cdnjs. Connect to the internet and reload.</div>';
      return { dispose() {}, select() {}, scrubTo() {} };
    }
    const T = window.THREE;
    const g = opts.dir.garden3d;
    const pal = g[opts.scheme];
    const roles = opts.dir.colors[opts.scheme];
    const dark = opts.scheme === "dark";
    const reduced = opts.reduced;
    const col = (hex) => new T.Color(hex);
    const statusColor = (s) => {
      const c = col(roles[s]);
      if (g.desaturate) { const l = c.r * 0.3 + c.g * 0.59 + c.b * 0.11; c.lerp(new T.Color(l, l, l), g.desaturate); }
      return c;
    };

    const renderer = new T.WebGLRenderer({ antialias: true });
    renderer.setPixelRatio(Math.min(window.devicePixelRatio || 1, 2));
    renderer.shadowMap.enabled = !!g.shadows;
    renderer.shadowMap.type = T.PCFSoftShadowMap;
    stage.appendChild(renderer.domElement);

    const scene = new T.Scene();
    scene.background = col(pal.bg);

    const persp = new T.PerspectiveCamera(g.fov || 32, 1, 0.1, 100);
    const ortho = new T.OrthographicCamera(-1, 1, 1, -1, 0.1, 100);
    const camera = g.ortho ? ortho : persp;
    const orbit = { az: g.ortho ? 0.785 : 0.42, polar: g.ortho ? 0.96 : 0.9, r: g.ortho ? 10.8 : 12.6, target: new T.Vector3(0.3, 0.15, 0.1) };
    const intro = { polar: reduced ? orbit.polar : 0.32 };

    const L = g.lights;
    scene.add(new T.HemisphereLight(0xffffff, col(L.hemiGround || pal.ground), dark ? L.hemiDark : L.hemi));
    if (L.sunI > 0) {
      const sun = new T.DirectionalLight(col(L.sun), dark ? L.sunI * 0.8 : L.sunI);
      sun.position.set(...L.sunPos);
      sun.castShadow = !!g.shadows;
      sun.shadow.mapSize.set(1024, 1024);
      Object.assign(sun.shadow.camera, { left: -5, right: 5, top: 5, bottom: -5, near: 0.5, far: 20 });
      sun.shadow.bias = -0.0006;
      sun.shadow.radius = 4;
      scene.add(sun);
    }
    if (L.fill) { const f = new T.DirectionalLight(col(L.fill), L.fillI); f.position.set(-1, 3, -5); scene.add(f); }

    const mat = (c, extra) => new T.MeshStandardMaterial(Object.assign({ color: typeof c === "string" ? col(c) : c, roughness: g.roughness, metalness: 0 }, extra || {}));
    const edgeMat = g.edges ? new T.LineBasicMaterial({ color: col(roles.text), transparent: true, opacity: g.edgeOpacity || 0.6 }) : null;

    function mesh(geo, m, parent, edges) {
      const o = new T.Mesh(geo, m);
      o.castShadow = !!g.shadows;
      o.receiveShadow = !!g.shadows;
      if (edgeMat && edges !== false) o.add(new T.LineSegments(new T.EdgesGeometry(geo, 30), edgeMat));
      parent.add(o);
      return o;
    }

    // Ground: a plinth, so the garden reads as a model of the map rather than a photo.
    if (g.form !== "survey") {
      const base = new T.Mesh(new T.CylinderGeometry(4.3, 4.3, 0.24, 72), mat(pal.base));
      base.position.y = -0.12;
      base.receiveShadow = true;
      scene.add(base);
    }
    const ground = new T.Mesh(new T.CircleGeometry(g.form === "survey" ? 12 : 4.3, 72), mat(pal.ground));
    ground.rotation.x = -Math.PI / 2;
    ground.position.y = 0.001;
    ground.receiveShadow = true;
    scene.add(ground);
    if (g.grid) {
      const fine = new T.GridHelper(8, 16, col(roles.border), col(roles.border));
      fine.position.y = 0.003;
      scene.add(fine);
      const metre = new T.GridHelper(8, 8, col(roles.textMuted), col(roles.textMuted));
      metre.position.y = 0.004;
      metre.material.transparent = true;
      metre.material.opacity = 0.6;
      scene.add(metre);
    }

    const bedTop = {};
    for (const b of BEDS) {
      if (g.beds === "outline") {
        const wall = new T.Mesh(new T.BoxGeometry(b.w + 0.1, 0.1, b.d + 0.1), new T.MeshBasicMaterial({ color: col(roles.text), transparent: true, opacity: 0.12, depthWrite: false }));
        wall.position.set(b.x, 0.05, b.z);
        wall.add(new T.LineSegments(new T.EdgesGeometry(wall.geometry), edgeMat));
        scene.add(wall);
        bedTop[b.id] = 0.1;
      } else {
        const frame = mesh(new T.BoxGeometry(b.w + 0.1, b.h, b.d + 0.1), mat(pal.wood), scene);
        frame.position.set(b.x, b.h / 2, b.z);
        const soil = mesh(new T.BoxGeometry(b.w, 0.02, b.d), mat(pal.soil), scene, false);
        soil.position.set(b.x, b.h + 0.002, b.z);
        bedTop[b.id] = b.h + 0.012;
      }
    }

    const detail = g.flat ? 0 : 2;
    function blob(r, m, parent, x, y, z, sx, sy, sz) {
      const o = mesh(new T.IcosahedronGeometry(r, detail), m, parent, false);
      o.position.set(x, y, z);
      if (sx) o.scale.set(sx, sy, sz);
      return o;
    }
    // Organic plants, about 1 unit tall; the group is scaled by the measured height and canopy.
    // `leaf` is the foliage material: a species green, or the plant's status colour.
    const ORGANIC = {
      tomato(gr, leaf) {
        mesh(new T.CylinderGeometry(0.012, 0.012, 1.12, 6), mat(pal.wood), gr, false).position.set(0.12, 0.56, 0.05);
        mesh(new T.CylinderGeometry(0.02, 0.026, 0.95, 6), leaf, gr, false).position.y = 0.475;
        [[0, 0.3, 0, 0.5], [0.09, 0.5, -0.06, 0.45], [-0.08, 0.7, 0.05, 0.38], [0.03, 0.88, 0, 0.28]].forEach(([x, y, z, r]) => blob(r, leaf, gr, x, y, z, 1, 0.75, 1));
        const red = mat(g.fruit.tomato), green = mat(g.foliage.lettuce);
        [[0.32, 0.36, 0.1, red], [0.24, 0.5, -0.28, red], [-0.3, 0.42, 0.18, green], [0.12, 0.62, 0.34, red], [-0.22, 0.64, -0.18, red]].forEach(([x, y, z, m]) => blob(0.075, m, gr, x, y, z));
      },
      pepper(gr, leaf) {
        mesh(new T.CylinderGeometry(0.02, 0.03, 0.6, 6), leaf, gr, false).position.y = 0.3;
        [[0, 0.42, 0, 0.5], [0.12, 0.66, 0.04, 0.4], [-0.1, 0.82, -0.05, 0.3]].forEach(([x, y, z, r]) => blob(r, leaf, gr, x, y, z, 1, 0.8, 1));
        const o = mat(g.fruit.pepper), gp = mat(g.foliage.basil);
        [[0.34, 0.4, 0.12, o], [-0.3, 0.46, 0.2, gp], [0.18, 0.5, -0.34, o]].forEach(([x, y, z, m]) => blob(0.07, m, gr, x, y, z, 0.8, 1.9, 0.8));
      },
      basil(gr, leaf) {
        [[0, 0.6, 0, 0.42], [0.3, 0.42, 0, 0.32], [-0.3, 0.42, 0.06, 0.32], [0.05, 0.4, 0.3, 0.3], [-0.04, 0.42, -0.3, 0.3], [0, 0.88, 0, 0.26]].forEach(([x, y, z, r]) => blob(r, leaf, gr, x, y, z, 1, 0.8, 1));
      },
      lettuce(gr, leaf) {
        for (let i = 0; i < 9; i++) {
          const a = (i / 9) * Math.PI * 2;
          blob(0.3, leaf, gr, Math.cos(a) * 0.42, 0.32, Math.sin(a) * 0.42, 1, 0.55, 0.8).rotation.y = -a;
        }
        blob(0.34, leaf, gr, 0, 0.5, 0, 1, 0.8, 1);
      },
      kale(gr, leaf) {
        for (let i = 0; i < 7; i++) {
          const a = (i / 7) * Math.PI * 2;
          const o = blob(0.5, leaf, gr, Math.cos(a) * 0.22, 0.5, Math.sin(a) * 0.22, 0.32, 1, 0.16);
          o.rotation.y = -a;
          o.rotation.z = 0.55;
        }
      },
    };
    // Turned forms (Herbarium): a stem to the measured height and a sphere for the canopy.
    function turned(gr, leaf, kind) {
      mesh(new T.CylinderGeometry(0.035, 0.045, 0.7, 12), leaf, gr).position.y = 0.35;
      const low = kind === "lettuce";
      const s = mesh(new T.SphereGeometry(0.5, 20, 14), leaf, gr);
      s.position.y = low ? 0.5 : 0.72;
      s.scale.set(1, low ? 1 : 0.62, 1);
    }
    // Survey forms (Instrument): a height-true cylinder and a canopy disc.
    function survey(gr, leaf) {
      mesh(new T.CylinderGeometry(0.07, 0.07, 1, 16), leaf, gr).position.y = 0.5;
      mesh(new T.CylinderGeometry(0.5, 0.5, 0.025, 40), leaf, gr).position.y = 1;
    }

    const frostGroup = new T.Group();
    frostGroup.visible = false;
    scene.add(frostGroup);
    const frostDisc = new T.Mesh(new T.CircleGeometry(4.3, 72), new T.MeshBasicMaterial({ color: col(dark ? "#9CC7F0" : "#DCEBFA"), transparent: true, opacity: dark ? 0.12 : 0.45, depthWrite: false }));
    frostDisc.rotation.x = -Math.PI / 2;
    frostDisc.position.y = 0.006;
    frostGroup.add(frostDisc);

    const plants = [];
    PLANTS.forEach((p, i) => {
      const root = new T.Group();
      root.position.set(p.x, p.bed ? bedTop[p.bed] : 0, p.z);
      scene.add(root);
      if (p.pot) {
        mesh(new T.CylinderGeometry(0.17, 0.12, 0.24, 28), mat(pal.pot), root).position.y = 0.12;
        mesh(new T.CylinderGeometry(0.155, 0.155, 0.02, 28), mat(pal.soil), root, false).position.y = 0.235;
      }
      const byStatus = g.color === "status";
      const leaf = mat(byStatus ? statusColor(statusAt(p, 4)) : g.foliage[p.kind], g.emissive ? { emissive: statusColor(statusAt(p, 4)), emissiveIntensity: g.emissive } : null);
      const body = new T.Group();
      body.position.y = p.pot ? 0.24 : 0;
      root.add(body);
      if (g.form === "turned") turned(body, leaf, p.kind);
      else if (g.form === "survey") survey(body, leaf);
      else ORGANIC[p.kind](body, leaf);
      body.traverse((o) => { o.userData.plantId = p.id; });

      const ring = new T.Mesh(new T.RingGeometry(p.c * 0.56, p.c * 0.64, 56), new T.MeshBasicMaterial({ color: col(roles[statusAt(p, 4)]), transparent: true, opacity: 0.95, side: T.DoubleSide, depthWrite: false }));
      ring.rotation.x = -Math.PI / 2;
      ring.position.y = (p.pot ? 0.25 : 0) + 0.008;
      root.add(ring);

      if (p.tender) {
        const cover = new T.Mesh(new T.SphereGeometry(0.5, 24, 14, 0, Math.PI * 2, 0, Math.PI / 2), new T.MeshStandardMaterial({ color: 0xffffff, transparent: true, opacity: 0.38, roughness: 1, depthWrite: false }));
        cover.position.set(p.x, root.position.y + (p.pot ? 0.24 : 0), p.z);
        cover.scale.set(p.c * 1.3, p.h * 1.25 * 2, p.c * 1.3);
        frostGroup.add(cover);
      }

      const label = document.createElement("div");
      label.className = "g3d-label";
      stage.appendChild(label);
      plants.push({ p, root, body, ring, label, leaf, byStatus, delay: i * 110, scale: growAt(p, 4), goal: growAt(p, 4) });
    });

    let scaleEl = null, northEl = null;
    if (g.scaleBar) {
      scaleEl = document.createElement("div");
      scaleEl.className = "g3d-scale";
      scaleEl.innerHTML = '<i></i><span>1 m</span>';
      northEl = document.createElement("div");
      northEl.className = "g3d-north";
      northEl.innerHTML = '<svg width="14" height="18" viewBox="0 0 14 18"><path d="M7 0l6 16-6-4-6 4z" fill="currentColor"/></svg><span>N</span>';
      screen.querySelector(".g3d").append(scaleEl, northEl);
    }

    let dateIdx = 4, selected = null;
    function drawLabel(x) {
      const st = statusAt(x.p, dateIdx);
      const h = Math.round(x.p.h * growAt(x.p, dateIdx) * 100);
      x.label.innerHTML = `<span class="sh sh-${st}" style="background:${roles[st]}"></span>${x.p.name}${g.labelHeights ? ` · <span class="num">${h} cm</span>` : ""}`;
      x.label.classList.toggle("frost", dateIdx === FORECAST && x.p.tender);
    }
    function recolor() {
      plants.forEach((x) => {
        const st = statusAt(x.p, dateIdx);
        x.ring.material.color.set(roles[st]);
        if (x.byStatus) {
          x.leaf.color.copy(statusColor(st));
          if (g.emissive) x.leaf.emissive.copy(statusColor(st));
        }
        drawLabel(x);
      });
      frostGroup.visible = dateIdx === FORECAST;
    }

    // Interaction: drag to orbit, wheel to zoom, tap to select.
    let down = null, lastInteract = performance.now();
    const el = renderer.domElement;
    el.addEventListener("pointerdown", (e) => { down = { x: e.clientX, y: e.clientY, moved: 0 }; el.setPointerCapture(e.pointerId); lastInteract = performance.now(); });
    el.addEventListener("pointermove", (e) => {
      if (!down) return;
      const dx = e.clientX - down.x, dy = e.clientY - down.y;
      down.moved += Math.abs(dx) + Math.abs(dy);
      down.x = e.clientX; down.y = e.clientY;
      orbit.az -= dx * 0.008;
      orbit.polar = Math.min(1.32, Math.max(0.05, orbit.polar - dy * 0.006));
      intro.polar = orbit.polar;
      lastInteract = performance.now();
    });
    el.addEventListener("pointerup", (e) => { if (down && down.moved < 6) pick(e); down = null; });
    el.addEventListener("wheel", (e) => { e.preventDefault(); orbit.r = Math.min(14, Math.max(5, orbit.r + e.deltaY * 0.004)); }, { passive: false });

    const ray = new T.Raycaster();
    function pick(e) {
      const r = el.getBoundingClientRect();
      ray.setFromCamera({ x: ((e.clientX - r.left) / r.width) * 2 - 1, y: -((e.clientY - r.top) / r.height) * 2 + 1 }, camera);
      const hit = ray.intersectObjects(plants.map((x) => x.body), true).find((h) => h.object.userData.plantId);
      if (hit) select(hit.object.userData.plantId);
    }

    function updateSheet() {
      if (!selected) return;
      const p = selected.p, q = (s) => screen.querySelector(s);
      q("[data-g-name]").textContent = p.name;
      const st = q("[data-g-status]");
      st.className = "status s-" + statusAt(p, dateIdx);
      st.textContent = STATUS_WORD[statusAt(p, dateIdx)];
      q("[data-g-height]").textContent = Math.round(p.h * growAt(p, dateIdx) * 100) + " cm";
      q("[data-g-canopy]").textContent = Math.round(p.c * Math.min(1, 0.4 + 0.6 * growAt(p, dateIdx)) * 100) + " cm";
      q("[data-g-note]").textContent = noteAt(p, dateIdx);
      q("[data-g-date]").textContent = dateIdx === FORECAST ? "Thu · forecast" : DATES[dateIdx];
    }
    function select(id) {
      selected = plants.find((x) => x.p.id === id) || null;
      plants.forEach((x) => x.label.classList.toggle("sel", x === selected));
      updateSheet();
    }
    function scrubTo(i) {
      dateIdx = i;
      plants.forEach((x) => { x.goal = growAt(x.p, i); if (reduced) x.scale = x.goal; });
      const range = screen.querySelector("[data-g-range]");
      if (range) range.value = String(i);
      recolor();
      updateSheet();
    }
    const range = screen.querySelector("[data-g-range]");
    if (range) range.addEventListener("input", () => scrubTo(Number(range.value)));

    function resize() {
      const w = stage.clientWidth, h = stage.clientHeight;
      if (!w || !h) return;
      renderer.setSize(w, h, false);
      persp.aspect = w / h;
      persp.updateProjectionMatrix();
    }
    const ro = new ResizeObserver(resize);
    ro.observe(stage);
    resize();

    const easeOutBack = (t) => { const c1 = 1.5, c3 = c1 + 1; return 1 + c3 * Math.pow(t - 1, 3) + c1 * Math.pow(t - 1, 2); };
    const t0 = performance.now();
    const v = new T.Vector3(), v2 = new T.Vector3();
    let raf = 0;
    function frame(now) {
      raf = requestAnimationFrame(frame);
      const w = stage.clientWidth, h = stage.clientHeight;
      if (g.autoRotate && !reduced && now - lastInteract > 3500) orbit.az += 0.0016;
      if (!down) intro.polar += (orbit.polar - intro.polar) * (reduced ? 1 : 0.05);
      const s = Math.sin(intro.polar);
      camera.position.set(orbit.target.x + orbit.r * s * Math.sin(orbit.az), orbit.target.y + orbit.r * Math.cos(intro.polar), orbit.target.z + orbit.r * s * Math.cos(orbit.az));
      camera.lookAt(orbit.target);
      if (g.ortho) {
        const half = orbit.r * 0.37, aspect = w / h;
        Object.assign(ortho, { left: -half * aspect, right: half * aspect, top: half, bottom: -half });
        ortho.updateProjectionMatrix();
      }

      for (const x of plants) {
        x.scale += (x.goal - x.scale) * (reduced ? 1 : 0.12);
        const t = reduced ? 1 : Math.min(1, Math.max(0, (now - t0 - 250 - x.delay) / 750));
        const grow = t <= 0 ? 0.001 : easeOutBack(t);
        const across = x.p.c * (0.6 + 0.4 * Math.min(1, t)) * Math.min(1, 0.4 + 0.6 * x.scale);
        x.body.scale.set(across, Math.max(0.001, x.p.h * x.scale * grow), across);
        x.ring.scale.set(1, 1, 1);
        x.ring.material.opacity = 0.95;
        if (statusAt(x.p, dateIdx) === "sick" && !reduced) {
          const k = 1 + 0.12 * (0.5 + 0.5 * Math.sin(now / 420));
          x.ring.scale.set(k, k, 1);
          x.ring.material.opacity = 0.95 - 0.35 * (k - 1) / 0.12;
        }
        v.set(x.p.x, x.root.position.y + x.body.position.y + x.p.h * x.scale + 0.14, x.p.z).project(camera);
        const visible = v.z < 1 && t > 0.6;
        x.label.style.opacity = visible ? "1" : "0";
        x.label.style.transform = `translate(${((v.x + 1) / 2) * w}px, ${((1 - v.y) / 2) * h}px) translate(-50%, -100%)`;
      }
      if (scaleEl) {
        v.set(0, 0, 0).project(camera); v2.set(1, 0, 0).project(camera);
        const px = Math.hypot((v2.x - v.x) * w / 2, (v2.y - v.y) * h / 2);
        scaleEl.firstChild.style.width = Math.round(px) + "px";
        v2.set(0, 0, -1).project(camera);
        const ang = Math.atan2((v2.x - v.x) * w, (v.y - v2.y) * h);
        northEl.firstChild.style.transform = `rotate(${ang}rad)`;
      }
      renderer.render(scene, camera);
    }
    recolor();
    raf = requestAnimationFrame(frame);
    select("tomato1");

    return {
      select,
      scrubTo,
      dates: DATES.length,
      dispose() {
        cancelAnimationFrame(raf);
        ro.disconnect();
        scene.traverse((o) => { if (o.geometry) o.geometry.dispose(); if (o.material) o.material.dispose(); });
        renderer.dispose();
        if (renderer.forceContextLoss) renderer.forceContextLoss();
        stage.innerHTML = "";
        if (scaleEl) { scaleEl.remove(); northEl.remove(); }
      },
    };
  }

  window.PDMGarden3D = { mount };
})();
