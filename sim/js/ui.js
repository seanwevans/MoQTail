/**
 * ui.js — the console around the canvas.
 *
 * Panels update on a slow clock (4-15 Hz) while the canvas runs on rAF, so a
 * ship with two hundred publishers does not spend its frame budget rebuilding
 * table rows.
 */

import { BUS_KINDS } from './topology.js';
import { display } from './moqtail.js';
import { loadColor } from './render.js';
import { INCIDENTS, phaseOf } from './telemetry.js';

/**
 * The guided tour of the DSL. The button face is the selector itself — reading
 * the buttons is most of the documentation.
 */
export const PRESETS = [
  { group: 'Topic shape', items: [
    { s: '//engine/+', why: 'Every propulsion node, wherever it lives on board.' },
    { s: '//bilge/+', why: 'All eleven bilge wells, across three machinery zones.' },
    { s: '/ship/deck4//+', why: 'Everything on the galley deck, at any depth below it.' },
    { s: '/ship/+/aft//+', why: 'The whole aft third of the ship, on every deck.' },
    { s: '/ship/deck2//+', why: 'The machinery spaces — engines, switchboards, fuel.' },
  ] },
  { group: 'Header & property predicates', items: [
    { s: '/msg[qos=2]//#', why: 'Only the safety-critical publishers: bilge and fire.' },
    { s: '/msg[prio="high"]//#', why: 'A user property carried alongside the payload.' },
    { s: '/msg[qos<=1][retained=false]//pos/+', why: 'Live till transactions, not retained state.' },
    { s: '/msg[zone="aft"]//engine/+', why: 'Header and topic predicates, conjoined.' },
  ] },
  { group: 'Payload predicates', items: [
    { s: '//engine/+[json$.egt>520]', why: 'Exhaust gas over the alarm limit. Try the overheat scenario.' },
    { s: '//bilge/+[json$.mm>120]', why: 'Water above 120 mm in a bilge well.' },
    { s: '//fire/+[json$.smoke>0.35]', why: 'Obscuration past the pre-alarm threshold.' },
    { s: '//refrig/+[json$.c>4]', why: 'Cold-chain excursion in the provisions rooms.' },
    { s: '//temp/+[json$.c>26]', why: 'Spaces the air handlers are losing.' },
    { s: '//door/+[json$.state="open"]', why: 'A string predicate over a payload field.' },
    { s: '//pax/+[json$.muster=true]', why: 'Muster compliance, as a boolean predicate.' },
  ] },
  { group: 'Pipeline stages', items: [
    { s: '//nav/+ |> window(10s) |> avg(json$.sog)', why: 'Speed over ground, smoothed over ten seconds.' },
    { s: '//power/+ |> window(30s) |> sum(json$.kw)', why: 'Total electrical load across every meter.' },
    { s: '//pos/+ |> window(60s) |> count()', why: 'Transactions per minute. Try the port-day scenario.' },
    { s: '//engine/+ |> window(20s) |> avg(json$.rpm)', why: 'Mean shaft speed across both engine rooms.' },
  ] },
];

const fmtRate = (v) => (v >= 1000 ? `${(v / 1000).toFixed(1)}k` : v >= 100 ? v.toFixed(0) : v.toFixed(1));
const fmtKbps = (v) => (v >= 1000 ? `${(v / 1000).toFixed(2)} Mbps` : `${v.toFixed(1)} kbps`);
const pct = (v) => `${Math.round(v * 100)}%`;
const el = (id) => document.getElementById(id);

export class UI {
  constructor(sim, renderer) {
    this.sim = sim;
    this.renderer = renderer;
    this.lastPanel = 0;
    this.lastTail = 0;
    this.spark = el('spark');
    this.sparkCtx = this.spark.getContext('2d');
    this.stageSpark = el('stage-spark');
    this.stageCtx = this.stageSpark.getContext('2d');
    this.build();
    this.wire();
    this.setSelector(sim.selectorSource);
  }

  build() {
    el('k-nodes').textContent = this.sim.topology.nodes.length;
    el('k-buses').textContent = this.sim.topology.buses.length;
    el('k-hubs').textContent = this.sim.topology.hubs.length;

    el('legend').innerHTML = Object.entries(BUS_KINDS)
      .map(([, k]) => `<span><i style="background:${k.color}"></i>${k.label} · ${k.kbps >= 1000 ? `${k.kbps / 1000} Mbps` : `${k.kbps} kbps`}</span>`)
      .join('') + '<span><i style="background:#22d3ee"></i>trunk riser · hover to inspect, click to subscribe</span>';

    el('presets').innerHTML = PRESETS.map((g) => `
      <div class="preset-group"><span>${g.group}</span><div>
        ${g.items.map((it) => `<button class="preset" data-sel="${escapeAttr(it.s)}" title="${escapeAttr(it.why)}">${escapeHtml(it.s)}</button>`).join('')}
      </div></div>`).join('');

    const { nodes, buses, hubs, risers } = this.sim.topology;
    const families = new Set(nodes.map((n) => n.type)).size;
    const standards = new Set(buses.map((b) => b.kind)).size;
    el('fleet-facts').textContent = [
      `${nodes.length} nodes`, `${families} device families`, `${buses.length} fieldbuses`,
      `${standards} bus standards`, `${hubs.length} hubs`, `${risers.length} trunk risers`,
    ].join(' · ');

    el('incident-buttons').innerHTML = INCIDENTS
      .map((i) => `<button data-incident="${i.id}" title="${escapeAttr(i.blurb)}">${escapeHtml(i.label)}</button>`)
      .join('');
  }

  wire() {
    const input = el('selector');
    input.addEventListener('input', () => this.setSelector(input.value));
    input.addEventListener('keydown', (e) => {
      if (e.key === 'Enter' && !e.shiftKey) {
        e.preventDefault();
        this.setSelector(input.value);
      }
    });

    el('presets').addEventListener('click', (e) => {
      const button = e.target.closest('.preset');
      if (button) this.setSelector(button.dataset.sel, true);
    });

    el('incident-buttons').addEventListener('click', (e) => {
      const button = e.target.closest('button[data-incident]');
      if (!button) return;
      const incident = this.sim.fireIncident(button.dataset.incident);
      if (incident?.spec.suggest) this.offerSuggestion(incident.spec);
    });
    el('btn-clear').addEventListener('click', () => this.sim.clearIncidents());

    el('btn-play').addEventListener('click', () => this.togglePlay());
    el('btn-labels').addEventListener('click', (e) => {
      this.renderer.showLabels = !this.renderer.showLabels;
      e.currentTarget.classList.toggle('on', this.renderer.showLabels);
    });
    el('btn-labels').classList.add('on');

    const speed = el('speed');
    speed.addEventListener('input', () => {
      this.sim.speed = Number(speed.value);
      el('speed-val').textContent = `${this.sim.speed.toFixed(2).replace(/0$/, '')}×`;
    });

    const budget = el('budget');
    budget.addEventListener('input', () => {
      this.sim.shoreBudgetKbps = Number(budget.value);
      el('val-budget').textContent = `${budget.value} kbps`;
    });

    el('selector-hint').addEventListener('click', (e) => {
      const button = e.target.closest('button[data-fix]');
      if (button) this.setSelector(button.dataset.fix, true);
    });

    document.addEventListener('keydown', (e) => {
      if (e.target.tagName === 'TEXTAREA' || e.target.tagName === 'INPUT') return;
      if (e.code === 'Space') { e.preventDefault(); this.togglePlay(); }
    });

    const canvas = this.renderer.canvas;
    canvas.addEventListener('mousemove', (e) => this.onHover(e));
    canvas.addEventListener('mouseleave', () => {
      this.renderer.hover = null;
      el('tooltip').hidden = true;
    });
    canvas.addEventListener('click', (e) => this.onClick(e));
    // A tooltip positioned against the old canvas rect would hang outside a
    // narrowed viewport, so it goes away with the layout that placed it.
    window.addEventListener('resize', () => {
      this.renderer.hover = null;
      el('tooltip').hidden = true;
    });
  }

  togglePlay() {
    this.sim.running = !this.sim.running;
    el('btn-play').textContent = this.sim.running ? '⏸ Pause' : '▶ Play';
  }

  setSelector(source, fromButton = false) {
    const input = el('selector');
    if (input.value !== source) input.value = source;
    this.sim.setSelector(source);
    for (const button of document.querySelectorAll('.preset')) {
      button.classList.toggle('on', button.dataset.sel === source.trim());
    }
    if (fromButton) input.blur();
    this.renderStatus();
    this.renderAst();
  }

  offerSuggestion(spec) {
    const hint = el('selector-hint');
    hint.hidden = false;
    hint.innerHTML = `${escapeHtml(spec.blurb)} Catch it with
      <code>${escapeHtml(spec.suggest)}</code>
      <button data-fix="${escapeAttr(spec.suggest)}">subscribe</button>`;
    clearTimeout(this.hintTimer);
    this.hintTimer = setTimeout(() => { hint.hidden = true; }, 14000);
  }

  renderStatus() {
    const status = el('selector-status');
    if (this.sim.selectorError) {
      status.className = 'status err';
      status.textContent = `✕ ${this.sim.selectorError.message}`;
      return;
    }
    status.className = 'status ok';
    const canonical = this.sim.canonical;
    const same = canonical === this.sim.selectorSource.trim();
    status.textContent = same ? '✓ compiled' : `✓ compiled → ${canonical}`;
  }

  renderAst() {
    const box = el('ast');
    const selector = this.sim.selector;
    if (this.sim.selectorError || !selector) {
      box.innerHTML = '<div class="ast-empty">no AST — fix the selector above</div>';
      return;
    }
    const rows = selector.steps.map((step, i) => {
      const axis = step.axis === 'child' ? '/' : '//';
      const seg = step.segment.kind === 'literal' ? step.segment.name
        : step.segment.kind === 'plus' ? '+'
        : step.segment.kind === 'hash' ? '#' : 'msg';
      const preds = step.predicates
        .map((p) => `<span class="ast-tag pred">${escapeHtml(display({ steps: [{ axis: 'child', segment: { kind: 'literal', name: '' }, predicates: [p] }], stages: [] }).slice(1))}</span>`)
        .join('');
      return `<div class="ast-row"><span class="kind">step ${i + 1}</span>
        <span class="ast-tag axis">${axis}</span>
        <span class="ast-tag seg">${escapeHtml(seg)}</span>${preds}</div>`;
    });
    const stages = selector.stages.map((stage, i) => {
      const text = display({ steps: [], stages: [stage] }).trim().replace(/^\|>\s*/, '');
      return `<div class="ast-row"><span class="kind">stage ${i + 1}</span>
        <span class="ast-tag stage">${escapeHtml(text)}</span></div>`;
    });
    box.innerHTML = rows.join('') + stages.join('');
  }

  onHover(e) {
    const rect = this.renderer.canvas.getBoundingClientRect();
    const sx = e.clientX - rect.left;
    const sy = e.clientY - rect.top;
    const pick = this.renderer.pick(sx, sy);
    this.renderer.hover = pick;
    const tip = el('tooltip');
    if (!pick) { tip.hidden = true; return; }
    tip.hidden = false;
    tip.innerHTML = this.tooltipFor(pick);
    const w = tip.offsetWidth;
    const h = tip.offsetHeight;
    tip.style.left = `${Math.min(Math.max(6, sx + 16), rect.width - w - 6)}px`;
    tip.style.top = `${Math.min(Math.max(6, sy - h - 12), rect.height - h - 6)}px`;
  }

  tooltipFor(pick) {
    if (pick.kind === 'node') {
      const n = pick.node;
      const bus = this.sim.topology.busById.get(n.bus);
      const last = n.last;
      const matched = last?.matched;
      return `<div class="t-title">${escapeHtml(n.id)} · ${escapeHtml(n.typeLabel)}</div>
        <div class="t-row"><span>topic</span><b>${escapeHtml(n.topic)}</b></div>
        <div class="t-row"><span>deck / zone</span><b>${n.deckName} · ${n.zone}</b></div>
        <div class="t-row"><span>bus</span><b>${escapeHtml(bus.id)} (${bus.kindLabel})</b></div>
        <div class="t-row"><span>publishes</span><b>${n.rate.toFixed(1)} msg/s · qos ${n.qos}</b></div>
        ${last ? `<div class="t-payload">${escapeHtml(JSON.stringify(last.payload))}</div>` : ''}
        <div class="${matched ? 't-match' : 't-miss'}">${matched ? '● matched by the active selector' : '○ not matched'}</div>
        <div class="t-hint">click to subscribe to //${n.type}/+</div>`;
    }
    if (pick.kind === 'hub') {
      const h = pick.hub;
      return `<div class="t-title">${escapeHtml(h.id)}${h.parent ? ' · concentrator' : ' · zone hub'}</div>
        <div class="t-row"><span>deck / zone</span><b>${h.deckName} · ${h.zone}</b></div>
        <div class="t-row"><span>fieldbuses</span><b>${h.buses.length}</b></div>
        <div class="t-row"><span>backhaul</span><b>${h.parent ? escapeHtml(h.parent) : h.riser}</b></div>
        <div class="t-row"><span>ingress</span><b>${fmtRate(h.inMsgs ?? 0)} msg/s · ${fmtKbps(h.inKbps ?? 0)}</b></div>
        <div class="t-row"><span>gateway load</span><b>${h.alive ? pct(h.util ?? 0) : 'OFFLINE'}</b></div>
        <div class="t-hint">click to subscribe to /ship/${h.deck}//+</div>`;
    }
    const b = pick.bus;
    return `<div class="t-title">${escapeHtml(b.id)}</div>
      <div class="t-row"><span>family</span><b>${b.kindLabel}</b></div>
      <div class="t-row"><span>capacity</span><b>${fmtKbps(b.capacityKbps * b.derate)}${b.derate < 1 ? ' (derated)' : ''}</b></div>
      <div class="t-row"><span>offered</span><b>${fmtKbps(b.offeredKbps ?? 0)}</b></div>
      <div class="t-row"><span>drops</span><b>${b.nodes.length} devices</b></div>
      <div class="t-row"><span>utilisation</span><b>${pct(b.util ?? 0)}</b></div>
      <div class="t-hint">click to subscribe to /ship/${b.deck}/${b.zone}//+</div>`;
  }

  onClick(e) {
    const rect = this.renderer.canvas.getBoundingClientRect();
    const pick = this.renderer.pick(e.clientX - rect.left, e.clientY - rect.top);
    this.renderer.pinned = pick;
    if (!pick) return;
    if (pick.kind === 'node') this.setSelector(`//${pick.node.type}/+`, true);
    else if (pick.kind === 'hub') this.setSelector(`/ship/${pick.hub.deck}//+`, true);
    else this.setSelector(`/ship/${pick.bus.deck}/${pick.bus.zone}//+`, true);
  }

  // -- per-frame and per-tick panel updates ---------------------------------

  frame(timeMs) {
    this.drawSpark();
    this.drawStageSpark();
    if (timeMs - this.lastPanel > 240) {
      this.lastPanel = timeMs;
      this.updatePanels();
    }
    if (timeMs - this.lastTail > 130) {
      this.lastTail = timeMs;
      this.updateTail();
    }
  }

  updatePanels() {
    const { sim } = this;
    const s = sim.stats;
    const ship = sim.ship;

    const hh = String(Math.floor(ship.clock / 3600)).padStart(2, '0');
    const mm = String(Math.floor((ship.clock % 3600) / 60)).padStart(2, '0');
    el('v-clock').textContent = `${hh}:${mm} ship`;
    el('v-phase').textContent = phaseOf(ship).label;
    el('v-sea').textContent = `sea state ${ship.seaState.toFixed(1)} · roll ${ship.roll.toFixed(1)}°`;
    el('v-sog').textContent = `${ship.sog.toFixed(1)} kn · ${Math.round(ship.hdg)}°`;
    el('v-pos').textContent = `${ship.lat.toFixed(3)}°N ${Math.abs(ship.lon).toFixed(3)}°W`;

    el('k-rate').textContent = fmtRate(s.deliveredRate);
    el('k-matched').textContent = fmtRate(s.matchedRate);

    const scale = Math.max(s.brokerKbps, sim.shoreBudgetKbps, 1) * 1.05;
    el('bar-total').style.width = `${Math.min(100, (s.brokerKbps / scale) * 100)}%`;
    el('bar-matched').style.width = `${Math.min(100, (s.matchedKbps / scale) * 100)}%`;
    el('bar-budget').style.width = `${Math.min(100, (sim.shoreBudgetKbps / scale) * 100)}%`;
    el('val-total').textContent = fmtKbps(s.brokerKbps);
    el('val-matched').textContent = fmtKbps(s.matchedKbps);
    const saved = el('saved-pct');
    saved.textContent = `${s.savedPct.toFixed(1)}%`;
    saved.style.color = s.shoreUtil > 1 ? 'var(--alarm)' : 'var(--accent)';
    el('spark-scale').textContent = `${fmtKbps(scale)} full scale · 45 s`;

    this.renderHint();
    this.renderStageBox();
    this.renderBreakdown();
    this.renderTables();
    this.renderIncidents();
    this.renderLog();
  }

  renderHint() {
    const hint = el('selector-hint');
    if (!hint.hidden && hint.dataset.kind !== 'suffix') return;
    const suggestion = this.sim.suffixHint();
    if (!suggestion) {
      if (hint.dataset.kind === 'suffix') { hint.hidden = true; hint.dataset.kind = ''; }
      return;
    }
    hint.dataset.kind = 'suffix';
    hint.hidden = false;
    hint.innerHTML = `The last step is anchored at the end of the topic, and shipboard
      topics end in a device id — so this matches nothing. Did you mean
      <code>${escapeHtml(suggestion.candidate)}</code>?
      <button data-fix="${escapeAttr(suggestion.candidate)}">use it (${suggestion.hits} nodes)</button>`;
  }

  renderStageBox() {
    const box = el('stage-box');
    if (!this.sim.stageLabel) { box.hidden = true; return; }
    box.hidden = false;
    el('stage-label').textContent = this.sim.stageLabel;
    const value = this.sim.stageValue;
    el('stage-value').textContent = value === null || value === undefined
      ? '—'
      : Math.abs(value) >= 1000 ? Math.round(value).toLocaleString() : value.toFixed(2);
  }

  renderBreakdown() {
    const rows = this.sim.typeBreakdown();
    const box = el('breakdown');
    if (rows.length === 0) {
      box.innerHTML = '<div class="bd-empty">Nothing matches yet. Pick a preset, or inject a scenario.</div>';
      return;
    }
    const top = rows.slice(0, 8);
    const max = top[0].count || 1;
    box.innerHTML = top.map((r) => `
      <div class="bd-row">
        <i style="background:${r.color}"></i>
        <span title="${escapeAttr(r.label)}">${escapeHtml(r.label)}</span>
        <div class="bd-bar"><i style="width:${(r.count / max) * 100}%;background:${r.color}"></i></div>
        <b>${r.count.toLocaleString()}</b>
      </div>`).join('');
  }

  renderTables() {
    const buses = [...this.sim.topology.buses].sort((a, b) => (b.util ?? 0) - (a.util ?? 0)).slice(0, 12);
    el('bus-table').tBodies[0].innerHTML = buses.map((b) => `
      <tr><td>${escapeHtml(b.id)}</td>
        <td style="color:${b.color}">${escapeHtml(b.kind)}</td>
        <td>${b.deck}/${b.zone}</td>
        <td class="num">${(b.offeredKbps ?? 0).toFixed(1)}</td>
        <td class="num">${(b.capacityKbps * b.derate).toFixed(0)}</td>
        <td class="num load"><b style="color:${loadColor(b.util ?? 0)}">${pct(b.util ?? 0)}</b></td></tr>`).join('');

    const hubs = [...this.sim.topology.hubs].sort((a, b) => (b.util ?? 0) - (a.util ?? 0)).slice(0, 10);
    el('hub-table').tBodies[0].innerHTML = hubs.map((h) => `
      <tr><td>${escapeHtml(h.id)}${h.parent ? ' <span class="tag" style="color:#c4b5fd">conc</span>' : ''}</td>
        <td>${h.deck}</td>
        <td>${h.buses.length} bus${h.buses.length === 1 ? '' : 'es'}</td>
        <td class="num">${fmtRate(h.inMsgs ?? 0)}</td>
        <td class="num load"><b style="color:${h.alive ? loadColor(h.util ?? 0) : 'var(--alarm)'}">${h.alive ? pct(h.util ?? 0) : 'down'}</b></td></tr>`).join('');
  }

  renderIncidents() {
    const live = new Map(this.sim.incidents.map((i) => [i.spec.id, i.spec.tone]));
    for (const button of document.querySelectorAll('#incident-buttons button')) {
      const tone = live.get(button.dataset.incident);
      button.className = tone ? `live ${tone}` : '';
    }
  }

  renderLog() {
    el('log').innerHTML = this.sim.log.length === 0
      ? '<div class="bd-empty">Steady as she goes.</div>'
      : this.sim.log.map((entry) => `
        <div class="log-row ${entry.tone}"><b>${escapeHtml(entry.title)}</b><span>${escapeHtml(entry.detail)}</span></div>`).join('');
  }

  updateTail() {
    const rows = this.sim.tail.slice(0, 70);
    el('tail').innerHTML = rows.length === 0
      ? '<div class="tail-empty">No matching messages. The selector compiles, but nothing on board satisfies it right now.</div>'
      : rows.map((r) => `
        <div class="tail-row">
          <span class="t-meta">q${r.qos}${r.retained ? 'R' : ' '}</span>
          <span class="t-topic" style="color:${r.color}">${escapeHtml(r.topic)}</span>
          <span class="t-body">${escapeHtml(r.payload)}</span>
        </div>`).join('');
  }

  // -- charts ---------------------------------------------------------------

  sizeCanvas(canvas, ctx) {
    const dpr = Math.min(window.devicePixelRatio || 1, 2);
    const w = canvas.clientWidth;
    const h = canvas.clientHeight;
    if (canvas.width !== Math.round(w * dpr) || canvas.height !== Math.round(h * dpr)) {
      canvas.width = Math.round(w * dpr);
      canvas.height = Math.round(h * dpr);
    }
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    return [w, h];
  }

  drawSpark() {
    const ctx = this.sparkCtx;
    const [w, h] = this.sizeCanvas(this.spark, ctx);
    ctx.clearRect(0, 0, w, h);
    const history = this.sim.history;
    if (history.length < 2) return;

    const budget = this.sim.shoreBudgetKbps;
    const peak = Math.max(budget, ...history.map((p) => p.total)) * 1.08 || 1;
    const t0 = history[0].t;
    const span = Math.max(1, history[history.length - 1].t - t0);
    const px = (p) => ((p.t - t0) / span) * w;
    const py = (v) => h - (v / peak) * (h - 4) - 2;

    ctx.strokeStyle = 'rgba(90,124,165,0.18)';
    ctx.lineWidth = 1;
    for (let i = 1; i < 4; i++) {
      const y = (h / 4) * i;
      ctx.beginPath();
      ctx.moveTo(0, y);
      ctx.lineTo(w, y);
      ctx.stroke();
    }

    ctx.beginPath();
    ctx.moveTo(0, h);
    for (const p of history) ctx.lineTo(px(p), py(p.total));
    ctx.lineTo(w, h);
    ctx.closePath();
    ctx.fillStyle = 'rgba(148,163,184,0.13)';
    ctx.fill();
    ctx.beginPath();
    history.forEach((p, i) => (i === 0 ? ctx.moveTo(px(p), py(p.total)) : ctx.lineTo(px(p), py(p.total))));
    ctx.strokeStyle = 'rgba(148,163,184,0.75)';
    ctx.lineWidth = 1.2;
    ctx.stroke();

    ctx.beginPath();
    ctx.moveTo(0, h);
    for (const p of history) ctx.lineTo(px(p), py(p.matched));
    ctx.lineTo(w, h);
    ctx.closePath();
    ctx.fillStyle = 'rgba(34,211,238,0.20)';
    ctx.fill();
    ctx.beginPath();
    history.forEach((p, i) => (i === 0 ? ctx.moveTo(px(p), py(p.matched)) : ctx.lineTo(px(p), py(p.matched))));
    ctx.strokeStyle = '#22d3ee';
    ctx.lineWidth = 1.6;
    ctx.stroke();

    const by = py(budget);
    ctx.setLineDash([4, 4]);
    ctx.strokeStyle = 'rgba(251,191,36,0.75)';
    ctx.lineWidth = 1;
    ctx.beginPath();
    ctx.moveTo(0, by);
    ctx.lineTo(w, by);
    ctx.stroke();
    ctx.setLineDash([]);
    ctx.fillStyle = 'rgba(251,191,36,0.9)';
    ctx.font = '9px ui-monospace, monospace';
    ctx.textAlign = 'right';
    ctx.fillText('shore budget', w - 4, Math.max(9, by - 3));
  }

  drawStageSpark() {
    const box = el('stage-box');
    if (box.hidden) return;
    const ctx = this.stageCtx;
    const [w, h] = this.sizeCanvas(this.stageSpark, ctx);
    ctx.clearRect(0, 0, w, h);
    const points = this.sim.history.filter((p) => typeof p.stage === 'number');
    if (points.length < 2) return;
    const values = points.map((p) => p.stage);
    const lo = Math.min(...values);
    const hi = Math.max(...values);
    const pad = (hi - lo) * 0.15 || Math.abs(hi) * 0.1 || 1;
    const t0 = points[0].t;
    const span = Math.max(1, points[points.length - 1].t - t0);
    ctx.beginPath();
    points.forEach((p, i) => {
      const x = ((p.t - t0) / span) * w;
      const y = h - ((p.stage - lo + pad) / (hi - lo + pad * 2)) * (h - 4) - 2;
      i === 0 ? ctx.moveTo(x, y) : ctx.lineTo(x, y);
    });
    ctx.strokeStyle = '#86efac';
    ctx.lineWidth = 1.6;
    ctx.stroke();
  }
}

const escapeHtml = (s) => String(s).replace(/[&<>"']/g, (c) =>
  ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' }[c]));
const escapeAttr = escapeHtml;
