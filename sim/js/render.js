/**
 * render.js — draws the ship, its fieldbuses and the traffic moving on them.
 *
 * Everything is laid out in normalised [0,1] coordinates by topology.js, so
 * this module only scales. The whole ship is drawn inside one rotate/translate
 * so it can heel with the sea state; `toLocal` inverts that transform for hit
 * testing, which keeps the pointer honest while the deck is moving.
 */

import { pointOnPath } from './topology.js';

const WATERLINE = 0.852;
const KEEL = 0.892;

const THEME = {
  skyTop: '#05090f',
  skyBottom: '#0a1524',
};

const COL = {
  hullEdge: '#27405e',
  deckFill: 'rgba(148,197,255,0.035)',
  deckEdge: 'rgba(148,163,184,0.18)',
  label: 'rgba(148,163,184,0.72)',
  labelDim: 'rgba(100,116,139,0.55)',
  accent: '#22d3ee',
  warn: '#fbbf24',
  alarm: '#f87171',
  ok: '#4ade80',
};

/** Green below 60%, amber to 90%, red past it — the same scale everywhere. */
export function loadColor(util) {
  if (!Number.isFinite(util)) return COL.alarm;
  if (util >= 1) return COL.alarm;
  if (util >= 0.9) return '#fb923c';
  if (util >= 0.6) return COL.warn;
  return COL.ok;
}

export class Renderer {
  constructor(canvas, sim) {
    this.canvas = canvas;
    this.ctx = canvas.getContext('2d', { alpha: false });
    this.sim = sim;
    this.dpr = 1;
    this.w = 0;
    this.h = 0;
    this.scale = 1;
    this.tf = { px: 0, py: 0, rot: 0, bob: 0 };
    this.hover = null;
    this.pinned = null;
    this.showLabels = true;
    this.resize();
  }

  resize() {
    const rect = this.canvas.getBoundingClientRect();
    this.dpr = Math.min(window.devicePixelRatio || 1, 2);
    this.w = Math.max(320, rect.width);
    this.h = Math.max(260, rect.height);
    this.canvas.width = Math.round(this.w * this.dpr);
    this.canvas.height = Math.round(this.h * this.dpr);
    this.scale = Math.min(this.w / 1440, this.h / 820);
  }

  x(nx) { return nx * this.w; }
  y(ny) { return ny * this.h; }
  u(v) { return v * Math.max(0.55, this.scale); }

  /** Screen point -> normalised ship coordinates, undoing the heel. */
  toLocal(sx, sy) {
    const { px, py, rot, bob } = this.tf;
    const dx = sx - px;
    const dy = sy - (py + bob);
    const cos = Math.cos(-rot);
    const sin = Math.sin(-rot);
    return [(dx * cos - dy * sin + px) / this.w, (dx * sin + dy * cos + py) / this.h];
  }

  /** Nearest interactive thing to a screen point, or null. */
  pick(sx, sy) {
    const [nx, ny] = this.toLocal(sx, sy);
    const { nodes, hubs, buses } = this.sim.topology;
    let best = null;
    let bestD = Infinity;
    for (const node of nodes) {
      const d = Math.hypot((node.nx - nx) * this.w, (node.ny - ny) * this.h);
      if (d < bestD && d < this.u(9)) { bestD = d; best = { kind: 'node', node }; }
    }
    if (best) return best;
    for (const hub of hubs) {
      const d = Math.hypot((hub.nx - nx) * this.w, (hub.ny - ny) * this.h);
      if (d < bestD && d < this.u(12)) { bestD = d; best = { kind: 'hub', hub }; }
    }
    if (best) return best;
    for (const bus of buses) {
      const [a, b] = bus.line;
      const d = distToSegment(nx * this.w, ny * this.h, a[0] * this.w, a[1] * this.h, b[0] * this.w, b[1] * this.h);
      if (d < bestD && d < this.u(5)) { bestD = d; best = { kind: 'bus', bus }; }
    }
    return best;
  }

  draw(timeMs) {
    const ctx = this.ctx;
    ctx.save();
    ctx.scale(this.dpr, this.dpr);
    this.drawSky();
    this.drawSea(timeMs);

    const ship = this.sim.ship;
    this.tf = {
      px: this.w * 0.5,
      py: this.h * 0.74,
      rot: (ship.roll * Math.PI) / 180 * 0.35,
      bob: Math.sin(ship.elapsed * 0.85) * this.u(3.2) + ship.pitch * this.u(1.4),
    };
    ctx.save();
    ctx.translate(this.tf.px, this.tf.py + this.tf.bob);
    ctx.rotate(this.tf.rot);
    ctx.translate(-this.tf.px, -this.tf.py);

    this.drawSilhouette();
    this.drawHull();
    this.drawDecks();
    this.drawShipDetails();
    this.drawBuses(timeMs);
    this.drawTrunk(timeMs);
    this.drawNodes(timeMs);
    this.drawHubs();
    this.drawDeckMarks();
    this.drawBroker(timeMs);
    this.drawPackets();
    this.drawSelection();
    ctx.restore();

    this.drawUplink(timeMs);
    this.drawZoneRule();
    ctx.restore();
  }

  drawSky() {
    const ctx = this.ctx;
    const g = ctx.createLinearGradient(0, 0, 0, this.h);
    g.addColorStop(0, THEME.skyTop);
    g.addColorStop(0.7, THEME.skyBottom);
    g.addColorStop(1, '#081726');
    ctx.fillStyle = g;
    ctx.fillRect(0, 0, this.w, this.h);
  }

  drawSea(timeMs) {
    const ctx = this.ctx;
    const y = this.y(WATERLINE);
    const g = ctx.createLinearGradient(0, y, 0, this.h);
    g.addColorStop(0, '#12466b');
    g.addColorStop(1, '#061626');
    ctx.fillStyle = g;
    ctx.fillRect(0, y, this.w, this.h - y);

    const swell = 1 + this.sim.ship.seaState * 0.55;
    ctx.strokeStyle = 'rgba(103,232,249,0.22)';
    ctx.lineWidth = 1;
    for (let row = 0; row < 5; row++) {
      const yy = y + row * (this.h - y) * 0.21 + 3;
      const amp = this.u(1.6) * swell * (1 - row * 0.13);
      const phase = timeMs * 0.00055 * (1 + row * 0.22);
      ctx.beginPath();
      for (let px = 0; px <= this.w; px += 12) {
        const wy = yy + Math.sin(px * 0.019 + phase) * amp + Math.sin(px * 0.007 - phase * 1.7) * amp * 0.6;
        if (px === 0) ctx.moveTo(px, wy); else ctx.lineTo(px, wy);
      }
      ctx.globalAlpha = 0.55 - row * 0.09;
      ctx.stroke();
    }
    ctx.globalAlpha = 1;
  }

  /** The stepped superstructure profile, so the ship reads as a ship. */
  drawSilhouette() {
    const ctx = this.ctx;
    const decks = this.sim.topology.decks;
    const hullTop = decks.find((d) => d.id === 'deck5').y - 0.028;
    const upper = decks.filter((d) => d.y < hullTop);
    if (upper.length === 0) return;
    const pad = 0.0315; // half the deck pitch, so the steps meet flush

    ctx.beginPath();
    ctx.moveTo(this.x(upper[0].x0), this.y(upper[0].y - pad));
    for (const deck of upper) {
      ctx.lineTo(this.x(deck.x0), this.y(deck.y - pad));
      ctx.lineTo(this.x(deck.x0), this.y(deck.y + pad));
    }
    const last = upper[upper.length - 1];
    ctx.lineTo(this.x(last.x0), this.y(hullTop));
    ctx.lineTo(this.x(last.x1), this.y(hullTop));
    for (let i = upper.length - 1; i >= 0; i--) {
      ctx.lineTo(this.x(upper[i].x1), this.y(upper[i].y + pad));
      ctx.lineTo(this.x(upper[i].x1), this.y(upper[i].y - pad));
    }
    ctx.closePath();
    const g = ctx.createLinearGradient(0, this.y(upper[0].y), 0, this.y(hullTop));
    g.addColorStop(0, 'rgba(31, 54, 84, 0.42)');
    g.addColorStop(1, 'rgba(18, 33, 53, 0.42)');
    ctx.fillStyle = g;
    ctx.fill();
    ctx.strokeStyle = 'rgba(125, 180, 240, 0.26)';
    ctx.lineWidth = 1.1;
    ctx.stroke();
  }

  drawHull() {
    const ctx = this.ctx;
    const decks = this.sim.topology.decks;
    const main = decks.find((d) => d.id === 'deck5');
    const top = this.y(main.y - 0.028);
    const sheer = this.u(9);
    const x0 = this.x(main.x0);
    const x1 = this.x(main.x1);
    const keel = this.y(KEEL);

    ctx.beginPath();
    // Sheer line: the main deck edge rises towards the bow.
    ctx.moveTo(x0, top - sheer * 0.45);
    ctx.quadraticCurveTo((x0 + x1) / 2, top + sheer * 0.35, x1 - this.u(14), top - sheer);
    // Raked stem down to the forefoot.
    ctx.bezierCurveTo(x1 + this.u(10), top + (keel - top) * 0.42, x1 - this.u(4), keel - this.u(30), x1 - this.u(52), keel);
    ctx.lineTo(x0 + this.u(66), keel);
    // Cruiser stern.
    ctx.bezierCurveTo(x0 + this.u(12), keel, x0 - this.u(6), keel - this.u(52), x0, top - sheer * 0.45);
    ctx.closePath();
    const g = ctx.createLinearGradient(0, top, 0, keel);
    g.addColorStop(0, '#1a2d46');
    g.addColorStop(0.45, '#122238');
    g.addColorStop(1, '#0a1524');
    ctx.fillStyle = g;
    ctx.fill();
    ctx.strokeStyle = 'rgba(125, 180, 240, 0.34)';
    ctx.lineWidth = 1.2;
    ctx.stroke();

    // Bulbous bow, boot-top stripe and the name on the bow.
    ctx.beginPath();
    ctx.ellipse(x1 - this.u(30), keel - this.u(10), this.u(21), this.u(10), 0, 0, Math.PI * 2);
    ctx.fillStyle = '#132539';
    ctx.fill();
    ctx.strokeStyle = 'rgba(125,180,240,0.25)';
    ctx.stroke();

    ctx.save();
    ctx.beginPath();
    ctx.rect(x0, this.y(WATERLINE) - this.u(9), x1 - x0, this.u(9));
    ctx.clip();
    ctx.fillStyle = 'rgba(190, 60, 90, 0.35)';
    ctx.fillRect(x0, this.y(WATERLINE) - this.u(5), x1 - x0, this.u(5));
    ctx.restore();

    if (this.w > 860) {
      ctx.save();
      ctx.fillStyle = 'rgba(203,213,225,0.42)';
      ctx.font = `600 ${Math.round(Math.max(8, this.u(10)))}px ui-monospace, SFMono-Regular, Menlo, monospace`;
      ctx.textAlign = 'right';
      ctx.textBaseline = 'middle';
      ctx.fillText('TAIL SPINNER', x1 - this.u(70), top + this.u(22));
      ctx.restore();
    }
  }

  /** Funnel, mast, lifeboats and balcony ticks — the cues that say cruise ship. */
  drawShipDetails() {
    const ctx = this.ctx;
    const decks = this.sim.topology.decks;
    const deck12 = decks.find((d) => d.id === 'deck12');
    const bridge = decks[0];

    // Funnel
    const baseY = this.y(deck12.y - 0.024);
    const topY = this.y(0.030);
    const lx = this.x(0.318);
    const rx = this.x(0.382);
    ctx.beginPath();
    ctx.moveTo(lx, baseY);
    ctx.lineTo(lx + this.u(12), topY);
    ctx.lineTo(rx, topY);
    ctx.lineTo(rx + this.u(7), baseY);
    ctx.closePath();
    ctx.fillStyle = '#1b3049';
    ctx.fill();
    ctx.strokeStyle = 'rgba(125,180,240,0.34)';
    ctx.lineWidth = 1;
    ctx.stroke();
    ctx.fillStyle = 'rgba(34,211,238,0.30)';
    ctx.fillRect(lx + this.u(10), topY + this.u(3), rx - lx - this.u(8), this.u(6));

    // Radar mast over the bridge
    const mx = this.x(bridge.x0 + 0.06);
    ctx.strokeStyle = 'rgba(148,163,184,0.55)';
    ctx.lineWidth = 1;
    ctx.beginPath();
    ctx.moveTo(mx, this.y(bridge.y - 0.022));
    ctx.lineTo(mx, this.y(0.018));
    ctx.stroke();
    ctx.beginPath();
    ctx.ellipse(mx, this.y(0.022), this.u(10), this.u(2.2), 0, 0, Math.PI * 2);
    ctx.fillStyle = 'rgba(148,163,184,0.45)';
    ctx.fill();

    // Bridge windows, set into the front of the wheelhouse
    const by = this.y(bridge.y - 0.014);
    ctx.fillStyle = 'rgba(103,232,249,0.26)';
    for (let x = bridge.x0 + 0.014; x < bridge.x1 - 0.012; x += 0.017) {
      ctx.fillRect(this.x(x), by, this.u(8), this.u(3.2));
    }

    // Lifeboats slung on the boat deck
    const boat = decks.find((d) => d.id === 'deck5');
    const boatY = this.y(boat.y - 0.0345);
    ctx.fillStyle = 'rgba(251,191,36,0.34)';
    ctx.strokeStyle = 'rgba(251,191,36,0.45)';
    ctx.lineWidth = 0.7;
    for (let x = boat.x0 + 0.02; x < boat.x1 - 0.05; x += 0.038) {
      const bw = this.u(17);
      const bh = this.u(5);
      roundRect(ctx, this.x(x), boatY, bw, bh, bh / 2);
      ctx.fill();
      ctx.stroke();
    }

    // Balcony ticks on the cabin decks
    ctx.strokeStyle = 'rgba(148,197,255,0.13)';
    ctx.lineWidth = 0.8;
    for (const deck of decks) {
      if (!['deck7', 'deck8', 'deck9', 'deck10'].includes(deck.id)) continue;
      const y0 = this.y(deck.y - 0.0345);
      const y1 = this.y(deck.y - 0.0265);
      ctx.beginPath();
      for (let x = deck.x0 + 0.01; x < deck.x1 - 0.01; x += 0.0125) {
        ctx.moveTo(this.x(x), y0);
        ctx.lineTo(this.x(x), y1);
      }
      ctx.stroke();
    }
  }

  /**
   * Deck plating across the ship, then one compartment box per instrumented
   * (deck, zone) cell. Drawing the cells rather than the whole deck keeps the
   * un-instrumented spaces from reading as empty boxes.
   */
  drawDecks() {
    const ctx = this.ctx;
    const fontSize = Math.max(8, Math.round(this.u(10)));
    ctx.font = `${fontSize}px ui-monospace, SFMono-Regular, Menlo, monospace`;

    for (const deck of this.sim.topology.decks) {
      const x0 = this.x(deck.x0);
      const x1 = this.x(deck.x1);
      const plate = this.y(deck.y + 0.024);
      ctx.strokeStyle = 'rgba(148,197,255,0.20)';
      ctx.lineWidth = 1;
      ctx.beginPath();
      ctx.moveTo(x0, plate);
      ctx.lineTo(x1, plate);
      ctx.stroke();
    }

    for (const cell of this.sim.topology.cells) {
      const x = this.x(cell.x0);
      const w = this.x(cell.x1) - x;
      const y = this.y(cell.y - 0.024);
      const h = this.y(cell.y + 0.024) - y;
      roundRect(ctx, x, y, w, h, this.u(4));
      ctx.fillStyle = 'rgba(11, 27, 47, 0.62)';
      ctx.fill();
      ctx.strokeStyle = 'rgba(125, 180, 240, 0.24)';
      ctx.lineWidth = 0.9;
      ctx.stroke();
    }

  }

  /**
   * Deck marks, drawn after the hubs: a zone hub sits on the same left edge
   * as the mark for its deck, and the mark has to win.
   */
  drawDeckMarks() {
    const ctx = this.ctx;
    const fontSize = Math.max(8, Math.round(this.u(10)));
    ctx.font = `${fontSize}px ui-monospace, SFMono-Regular, Menlo, monospace`;
    // Deck marks last, so they sit above the compartments they label. The
    // number always fits at the deck's left edge; the name only goes in when
    // the first instrumented compartment leaves room for it.
    ctx.textBaseline = 'middle';
    ctx.textAlign = 'left';
    for (const deck of this.sim.topology.decks) {
      const y = this.y(deck.y);
      const x = this.x(deck.x0) + this.u(3);
      const num = deck.num.toUpperCase();
      const numW = ctx.measureText(num).width;
      roundRect(ctx, x - this.u(2), y - fontSize * 0.72, numW + this.u(5), fontSize * 1.45, this.u(3));
      ctx.fillStyle = 'rgba(6, 16, 28, 0.85)';
      ctx.fill();
      ctx.fillStyle = 'rgba(186, 205, 230, 0.80)';
      ctx.fillText(num, x + this.u(0.5), y);

      if (!this.showLabels || this.w < 900 || deck === this.sim.topology.decks[0]) continue;
      const firstCell = this.sim.topology.cells
        .filter((c) => c.deck === deck.id)
        .reduce((min, c) => (c.x0 < min ? c.x0 : min), Infinity);
      const room = (firstCell - deck.x0) * this.w - numW - this.u(14);
      if (room < ctx.measureText(deck.name).width) continue;
      ctx.fillStyle = 'rgba(125, 148, 178, 0.62)';
      ctx.fillText(deck.name, x + numW + this.u(8), y);
    }
  }

  drawBuses(timeMs) {
    const ctx = this.ctx;
    for (const bus of this.sim.topology.buses) {
      const [a, b] = bus.line;
      const hub = this.sim.topology.hubById.get(bus.hub);
      const hot = bus.util >= 0.9;
      const color = bus.util >= 0.6 ? loadColor(bus.util) : bus.color;
      ctx.strokeStyle = color;
      ctx.globalAlpha = hot ? 0.95 : 0.5;
      ctx.lineWidth = this.u(bus.kind === 'enet' ? 2.0 : 1.5);
      ctx.setLineDash(hot ? [this.u(4), this.u(3)] : []);
      ctx.lineDashOffset = hot ? -(timeMs * 0.04) % 1000 : 0;
      ctx.beginPath();
      ctx.moveTo(this.x(a[0]), this.y(a[1]));
      ctx.lineTo(this.x(b[0]), this.y(b[1]));
      ctx.lineTo(this.x(hub.nx), this.y(hub.ny));
      ctx.stroke();
      ctx.setLineDash([]);

      if (bus.util > 1) {
        ctx.globalAlpha = 0.85;
        ctx.fillStyle = COL.alarm;
        ctx.beginPath();
        ctx.arc(this.x(a[0]), this.y(a[1]), this.u(2.6), 0, Math.PI * 2);
        ctx.fill();
      }
      ctx.globalAlpha = 1;
    }
  }

  drawTrunk(timeMs) {
    const ctx = this.ctx;
    const { risers, hubs, broker } = this.sim.topology;

    for (const hub of hubs) {
      const target = hub.parent
        ? this.sim.topology.hubById.get(hub.parent)
        : risers.find((r) => r.id === hub.riser);
      const tx = hub.parent ? this.x(target.nx) : this.x(target.x);
      const ty = hub.parent ? this.y(target.ny) : this.y(hub.ny);
      ctx.strokeStyle = hub.alive ? 'rgba(56,189,248,0.30)' : 'rgba(248,113,113,0.5)';
      ctx.lineWidth = this.u(hub.parent ? 1.1 : 1.4);
      ctx.beginPath();
      ctx.moveTo(this.x(hub.nx), this.y(hub.ny));
      ctx.lineTo(tx, ty);
      ctx.stroke();
    }

    for (const riser of risers) {
      const x = this.x(riser.x);
      ctx.strokeStyle = 'rgba(34,211,238,0.55)';
      ctx.lineWidth = this.u(2.6);
      ctx.beginPath();
      ctx.moveTo(x, this.y(riser.y0));
      ctx.lineTo(x, this.y(riser.y1));
      ctx.stroke();
      ctx.strokeStyle = 'rgba(165,243,252,0.85)';
      ctx.lineWidth = this.u(1.1);
      ctx.setLineDash([this.u(3), this.u(9)]);
      ctx.lineDashOffset = -(timeMs * 0.09) % 1000;
      ctx.stroke();
      ctx.setLineDash([]);
      ctx.beginPath();
      ctx.moveTo(x, this.y(broker.ny));
      ctx.lineTo(this.x(broker.nx), this.y(broker.ny));
      ctx.strokeStyle = 'rgba(34,211,238,0.55)';
      ctx.lineWidth = this.u(2.2);
      ctx.stroke();
    }
  }

  drawNodes(timeMs) {
    const ctx = this.ctx;
    const now = this.sim.simTimeMs;
    for (const node of this.sim.topology.nodes) {
      const nx = this.x(node.nx);
      const ny = this.y(node.ny);
      ctx.strokeStyle = 'rgba(148,163,184,0.28)';
      ctx.lineWidth = 0.7;
      ctx.beginPath();
      ctx.moveTo(nx, ny);
      ctx.lineTo(nx, this.y(node.stubY));
      ctx.stroke();

      const sinceMatch = now - node.lastMatchAt;
      const sincePub = now - node.lastPublishAt;
      const matching = sinceMatch < 1400;
      const r = this.u(matching ? 3.6 : 2.7);

      if (matching) {
        const glow = 1 - sinceMatch / 1400;
        ctx.beginPath();
        ctx.arc(nx, ny, r + this.u(3.5) * glow, 0, Math.PI * 2);
        ctx.fillStyle = `rgba(103,232,249,${0.20 * glow})`;
        ctx.fill();
        ctx.beginPath();
        ctx.arc(nx, ny, r + this.u(1.4), 0, Math.PI * 2);
        ctx.strokeStyle = `rgba(165,243,252,${0.55 + 0.4 * glow})`;
        ctx.lineWidth = 1;
        ctx.stroke();
      }

      ctx.beginPath();
      ctx.arc(nx, ny, r, 0, Math.PI * 2);
      ctx.fillStyle = node.color;
      ctx.globalAlpha = matching ? 1 : sincePub < 160 ? 0.95 : 0.58;
      ctx.fill();
      ctx.globalAlpha = 1;
    }
  }

  drawHubs() {
    const ctx = this.ctx;
    for (const hub of this.sim.topology.hubById.values()) {
      const x = this.x(hub.nx);
      const y = this.y(hub.ny);
      const w = this.u(13);
      const h = this.u(9);
      roundRect(ctx, x - w / 2, y - h / 2, w, h, this.u(2.5));
      ctx.fillStyle = hub.alive ? '#0f2236' : '#3b1220';
      ctx.fill();
      ctx.strokeStyle = hub.alive ? loadColor(hub.util ?? 0) : COL.alarm;
      ctx.lineWidth = 1.1;
      ctx.stroke();

      const fill = Math.min(1, hub.util ?? 0);
      ctx.fillStyle = hub.alive ? loadColor(hub.util ?? 0) : COL.alarm;
      ctx.globalAlpha = 0.55;
      ctx.fillRect(x - w / 2 + 1.5, y + h / 2 - this.u(2.4), (w - 3) * (hub.alive ? fill : 1), this.u(1.6));
      ctx.globalAlpha = 1;

      if (hub.parent) {
        ctx.fillStyle = 'rgba(226,232,240,0.55)';
        ctx.beginPath();
        ctx.arc(x, y - h / 2 - this.u(2.4), this.u(1.1), 0, Math.PI * 2);
        ctx.fill();
      }
    }
  }

  drawBroker(timeMs) {
    const ctx = this.ctx;
    const { broker } = this.sim.topology;
    const x = this.x(broker.nx);
    const y = this.y(broker.ny);
    const r = this.u(15);
    const ratio = this.sim.stats.deliveredRate > 0
      ? Math.min(1, this.sim.stats.matchedRate / this.sim.stats.deliveredRate)
      : 0;

    ctx.beginPath();
    ctx.arc(x, y, r + this.u(7) + Math.sin(timeMs * 0.003) * this.u(1.6), 0, Math.PI * 2);
    ctx.fillStyle = 'rgba(34,211,238,0.07)';
    ctx.fill();

    ctx.beginPath();
    for (let i = 0; i < 6; i++) {
      const a = (Math.PI / 3) * i - Math.PI / 2;
      const px = x + Math.cos(a) * r;
      const py = y + Math.sin(a) * r;
      if (i === 0) ctx.moveTo(px, py); else ctx.lineTo(px, py);
    }
    ctx.closePath();
    ctx.fillStyle = '#0b2b3d';
    ctx.fill();
    ctx.strokeStyle = COL.accent;
    ctx.lineWidth = 1.4;
    ctx.stroke();

    ctx.beginPath();
    ctx.arc(x, y, r + this.u(4), -Math.PI / 2, -Math.PI / 2 + Math.PI * 2 * ratio);
    ctx.strokeStyle = '#67e8f9';
    ctx.lineWidth = this.u(2.4);
    ctx.stroke();

    ctx.fillStyle = '#a5f3fc';
    ctx.font = `600 ${Math.max(8, Math.round(this.u(10)))}px ui-monospace, SFMono-Regular, Menlo, monospace`;
    ctx.textAlign = 'center';
    ctx.textBaseline = 'middle';
    ctx.fillText('MoQ', x, y);
    ctx.fillStyle = COL.label;
    ctx.font = `${Math.max(8, Math.round(this.u(9.5)))}px ui-monospace, SFMono-Regular, Menlo, monospace`;
    ctx.fillText('CORE BROKER', x, y + r + this.u(12));
  }

  drawPackets() {
    const ctx = this.ctx;
    for (const p of this.sim.packets) {
      const [nx, ny] = pointOnPath(p.node.path, p.t);
      const x = this.x(nx);
      const y = this.y(ny);
      if (p.matched) {
        ctx.beginPath();
        ctx.arc(x, y, this.u(3.4), 0, Math.PI * 2);
        ctx.fillStyle = 'rgba(103,232,249,0.18)';
        ctx.fill();
      }
      ctx.beginPath();
      ctx.arc(x, y, this.u(p.matched ? 1.9 : 1.3), 0, Math.PI * 2);
      ctx.fillStyle = p.matched ? '#a5f3fc' : 'rgba(148,163,184,0.42)';
      ctx.fill();
    }
  }

  drawSelection() {
    const target = this.pinned ?? this.hover;
    if (!target) return;
    const ctx = this.ctx;
    if (target.kind === 'node') {
      const x = this.x(target.node.nx);
      const y = this.y(target.node.ny);
      ctx.beginPath();
      ctx.arc(x, y, this.u(7), 0, Math.PI * 2);
      ctx.strokeStyle = '#fef08a';
      ctx.lineWidth = 1.3;
      ctx.stroke();
    } else if (target.kind === 'hub') {
      const x = this.x(target.hub.nx);
      const y = this.y(target.hub.ny);
      ctx.strokeStyle = '#fef08a';
      ctx.lineWidth = 1.3;
      roundRect(ctx, x - this.u(10), y - this.u(8), this.u(20), this.u(16), this.u(3));
      ctx.stroke();
    } else if (target.kind === 'bus') {
      const [a, b] = target.bus.line;
      ctx.strokeStyle = '#fef08a';
      ctx.lineWidth = this.u(3);
      ctx.globalAlpha = 0.6;
      ctx.beginPath();
      ctx.moveTo(this.x(a[0]), this.y(a[1]));
      ctx.lineTo(this.x(b[0]), this.y(b[1]));
      ctx.stroke();
      ctx.globalAlpha = 1;
    }
  }

  /** The satellite uplink, drawn outside the ship transform so it stays level. */
  drawUplink(timeMs) {
    const ctx = this.ctx;
    const { vsat, broker } = this.sim.topology;
    const dx = this.x(vsat.nx);
    const dy = this.y(vsat.ny) + this.tf.bob;
    const sx = this.x(0.942);
    const sy = this.y(0.022);
    const util = this.sim.stats.shoreUtil;
    const over = util > 1;

    ctx.save();
    const beam = ctx.createLinearGradient(dx, dy, sx, sy);
    const tint = over ? '248,113,113' : '34,211,238';
    beam.addColorStop(0, `rgba(${tint},${0.10 + Math.min(0.42, util * 0.34)})`);
    beam.addColorStop(1, `rgba(${tint},0.015)`);
    ctx.beginPath();
    ctx.moveTo(dx - this.u(4), dy);
    ctx.lineTo(sx - this.u(16), sy + this.u(9));
    ctx.lineTo(sx + this.u(16), sy - this.u(3));
    ctx.lineTo(dx + this.u(4), dy - this.u(3));
    ctx.closePath();
    ctx.fillStyle = beam;
    ctx.fill();

    ctx.font = `${Math.max(9, Math.round(this.u(10)))}px ui-monospace, SFMono-Regular, Menlo, monospace`;
    ctx.textBaseline = 'middle';

    // VSAT radome on the bridge roof
    ctx.beginPath();
    ctx.arc(dx, dy, this.u(11), Math.PI, Math.PI * 2);
    ctx.fillStyle = '#17354f';
    ctx.fill();
    ctx.strokeStyle = over ? COL.alarm : COL.accent;
    ctx.lineWidth = 1.3;
    ctx.stroke();
    ctx.fillRect(dx - this.u(13), dy, this.u(26), this.u(2.5));
    ctx.textAlign = 'center';
    ctx.fillStyle = over ? COL.alarm : 'rgba(148,163,184,0.75)';
    ctx.fillText('VSAT', dx, dy + this.u(13));

    // The satellite it is talking to
    ctx.fillStyle = '#1e3a52';
    ctx.strokeStyle = over ? COL.alarm : 'rgba(103,232,249,0.8)';
    ctx.lineWidth = 1.2;
    roundRect(ctx, sx - this.u(10), sy - this.u(7), this.u(20), this.u(14), this.u(3));
    ctx.fill();
    ctx.stroke();
    ctx.fillStyle = over ? 'rgba(248,113,113,0.6)' : 'rgba(103,232,249,0.55)';
    ctx.fillRect(sx - this.u(30), sy - this.u(4), this.u(17), this.u(8));
    ctx.fillRect(sx + this.u(13), sy - this.u(4), this.u(17), this.u(8));

    ctx.textAlign = 'right';
    ctx.fillStyle = over ? COL.alarm : COL.label;
    ctx.fillText(
      `shore uplink ${Math.round(util * 100)}%${over ? ' · OVER BUDGET' : ''}`,
      sx - this.u(36),
      sy + this.u(17),
    );
    ctx.restore();

    // A dotted feed from the broker up to the dish.
    ctx.strokeStyle = 'rgba(34,211,238,0.4)';
    ctx.setLineDash([this.u(2), this.u(4)]);
    ctx.lineDashOffset = -(timeMs * 0.05) % 1000;
    ctx.beginPath();
    ctx.moveTo(this.x(broker.nx), this.y(broker.ny) + this.tf.bob);
    ctx.lineTo(dx, dy);
    ctx.stroke();
    ctx.setLineDash([]);
  }

  drawZoneRule() {
    if (this.w < 760) return;
    const ctx = this.ctx;
    ctx.font = `${Math.max(8, Math.round(this.u(10)))}px ui-monospace, SFMono-Regular, Menlo, monospace`;
    ctx.textBaseline = 'middle';
    ctx.textAlign = 'center';
    const deck = this.sim.topology.decks.find((d) => d.id === 'deck7');
    const width = deck.x1 - deck.x0;
    const y = this.y(0.945);
    ctx.strokeStyle = 'rgba(125,180,240,0.20)';
    ctx.lineWidth = 1;
    for (const frac of [0, 0.315, 0.345, 0.655, 0.685, 1]) {
      const x = this.x(deck.x0 + width * frac);
      ctx.beginPath();
      ctx.moveTo(x, y - this.u(4));
      ctx.lineTo(x, y + this.u(4));
      ctx.stroke();
    }
    ctx.fillStyle = 'rgba(148,163,184,0.6)';
    ctx.fillText('AFT', this.x(deck.x0 + width * 0.16), y);
    ctx.fillText('MIDSHIP', this.x(deck.x0 + width * 0.5), y);
    ctx.fillText('FORWARD', this.x(deck.x0 + width * 0.84), y);
  }
}

function roundRect(ctx, x, y, w, h, r) {
  const rr = Math.min(r, w / 2, h / 2);
  ctx.beginPath();
  ctx.moveTo(x + rr, y);
  ctx.arcTo(x + w, y, x + w, y + h, rr);
  ctx.arcTo(x + w, y + h, x, y + h, rr);
  ctx.arcTo(x, y + h, x, y, rr);
  ctx.arcTo(x, y, x + w, y, rr);
  ctx.closePath();
}

function distToSegment(px, py, x1, y1, x2, y2) {
  const dx = x2 - x1;
  const dy = y2 - y1;
  const len = dx * dx + dy * dy;
  const t = len === 0 ? 0 : Math.max(0, Math.min(1, ((px - x1) * dx + (py - y1) * dy) / len));
  return Math.hypot(px - (x1 + t * dx), py - (y1 + t * dy));
}
