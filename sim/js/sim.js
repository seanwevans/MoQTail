/**
 * sim.js — the shipboard network, from sensor drop to satellite dish.
 *
 * The chain each message walks is the point of the whole demo:
 *
 *     node -> fieldbus -> zone hub -> (concentrator) -> trunk riser
 *          -> core broker -> MoQTail matcher -> shore uplink
 *
 * Every link has a finite budget. A KNX segment carrying twelve cabin
 * controllers runs out of bus time long before the ship's backbone runs out of
 * bits, so the hubs exist to concentrate; and the satellite uplink is so much
 * narrower than the ship's own traffic that the only way to get telemetry
 * ashore is to filter it at the broker. That is the argument for MoQTail, drawn
 * to scale.
 */

import { compile, display, Matcher } from './moqtail.js';
import { buildTopology, NODE_TYPES } from './topology.js';
import { createShip, stepShip, phaseOf, samplePayload, INCIDENTS } from './telemetry.js';

/** Fixed MQTT overhead per publish: fixed header, packet id, property block. */
const MQTT_OVERHEAD = 14;
/** How often the rolling counters are folded into the displayed rates. */
const ROLL_MS = 250;
/** How much history the throughput chart keeps. */
const HISTORY_MS = 45000;
const TAIL_MAX = 220;
const PACKET_MAX = 460;
const PACKET_SPAWN_PER_S = 150;

/** How busy each device family is, by voyage phase. */
const PHASE_RATE = {
  moored: { engine: 0.25, nav: 0.4, pos: 2.2, pax: 2.6, door: 3.2, lift: 2.0, fuel: 2.4 },
  departure: { engine: 1.3, nav: 1.6, pos: 0.5, pax: 2.2, door: 2.4, lift: 1.6 },
  cruise: {},
  maneuver: { engine: 1.5, nav: 1.8, door: 1.3 },
  arrival: { engine: 0.9, nav: 1.7, pos: 1.4, pax: 1.8, door: 1.9, lift: 1.5 },
};

export class Simulation {
  constructor() {
    this.ship = createShip();
    this.topology = buildTopology();
    this.simTimeMs = 0;
    this.speed = 1;
    this.running = true;
    this.shoreBudgetKbps = 256;

    this.selectorSource = '';
    this.selector = null;
    this.matcher = null;
    this.selectorError = null;
    this.canonical = '';
    this.stageValue = null;
    this.stageLabel = '';

    this.incidents = [];
    this.packets = [];
    this.packetBudget = 0;
    this.tail = [];
    this.tailSeq = 0;
    this.tailBudget = 0;
    this.log = [];

    this.window = { offered: 0, delivered: 0, matched: 0, dropped: 0, bits: 0, matchedBits: 0, ms: 0 };
    this.stats = {
      offeredRate: 0, deliveredRate: 0, matchedRate: 0, droppedRate: 0,
      fieldKbps: 0, brokerKbps: 0, matchedKbps: 0, shoreUtil: 0,
      busPeak: 0, hubPeak: 0, savedPct: 0,
    };
    this.history = [];

    this.setSelector('/msg[qos=2]//#');
  }

  // -- selector -------------------------------------------------------------

  /** Compiles `source`; on failure the previous matcher keeps running. */
  setSelector(source) {
    this.selectorSource = source;
    try {
      const selector = compile(source);
      this.selector = selector;
      this.matcher = new Matcher(selector);
      this.canonical = display(selector);
      this.selectorError = null;
      this.stageValue = null;
      this.stageLabel = stageLabelFor(selector);
      this.stats.matchedRate = 0;
      this.stats.matchedKbps = 0;
      this.stats.shoreUtil = 0;
      this.stats.savedPct = 0;
      this.window.matched = 0;
      this.window.matchedBits = 0;
      this.history = [];
      this.tail = [];
      for (const node of this.topology.nodes) {
        node.matched = 0;
        node.lastMatchAt = -1e9;
      }
      return true;
    } catch (e) {
      this.selectorError = e;
      return false;
    }
  }

  /**
   * When a selector matches nothing, the usual reason is that MoQTail anchors
   * the last step at the end of the topic: `//engine` wants a topic that *ends*
   * in `engine`, and shipboard topics end in the device id. Offer the fix.
   */
  suffixHint() {
    if (this.selectorError || !this.selector || !this.matcher) return null;
    const nodes = this.topology.nodes;
    const probeMessage = (node) => ({
      topic: node.topic,
      headers: node.last?.headers ?? {},
      payload: node.last?.payload ?? null,
    });
    if (nodes.some((node) => this.matcher.matches(probeMessage(node)))) return null;

    const pipe = this.selectorSource.indexOf('|>');
    const head = (pipe === -1 ? this.selectorSource : this.selectorSource.slice(0, pipe)).trimEnd();
    const tail = pipe === -1 ? '' : ` ${this.selectorSource.slice(pipe)}`;
    for (const wildcard of ['/+', '/#']) {
      const candidate = `${head}${wildcard}${tail}`;
      try {
        const probe = new Matcher(compile(candidate));
        const hits = nodes.filter((node) => probe.matches(probeMessage(node))).length;
        if (hits > 0) return { candidate, hits };
      } catch {
        // A candidate that will not compile is simply not offered.
      }
    }
    return null;
  }

  // -- incidents ------------------------------------------------------------

  fireIncident(id) {
    const spec = INCIDENTS.find((i) => i.id === id);
    if (!spec) return null;
    this.incidents = this.incidents.filter((i) => i.spec.id !== id);
    const incident = { spec, startedMs: this.simTimeMs, endsMs: this.simTimeMs + spec.seconds * 1000, k: 0 };

    if (spec.kind === 'bus') {
      incident.bus = this.topology.buses.find(spec.busPick);
    } else if (spec.kind === 'hub') {
      incident.hub = this.topology.hubById.get(spec.hubId);
    }
    this.incidents.push(incident);
    this.pushLog(spec.tone, spec.label, spec.blurb);
    return incident;
  }

  clearIncidents() {
    for (const incident of this.incidents) this.retire(incident);
    this.incidents = [];
    this.ship.seaState = 2;
  }

  retire(incident) {
    if (incident.bus) {
      incident.bus.derate = 1;
      incident.bus.alive = true;
    }
    if (incident.hub) incident.hub.alive = true;
  }

  pushLog(tone, title, detail) {
    this.log.unshift({ tone, title, detail, at: this.simTimeMs });
    if (this.log.length > 30) this.log.pop();
  }

  // -- the loop -------------------------------------------------------------

  tick(dtReal) {
    if (!this.running) return;
    const dt = Math.min(0.25, dtReal) * this.speed;
    this.simTimeMs += dt * 1000;
    stepShip(this.ship, dt);
    this.advanceIncidents();
    this.recomputeLinks();
    this.publish(dt);
    this.rollWindow(dt);
    this.advancePackets(dt);
  }

  advanceIncidents() {
    let sea = 2;
    const still = [];
    for (const incident of this.incidents) {
      if (this.simTimeMs >= incident.endsMs) {
        this.retire(incident);
        this.pushLog('info', `${incident.spec.label} — cleared`, 'Conditions back to normal.');
        continue;
      }
      // Ramp in over the first 35% and back out over the last 25%, so a
      // threshold predicate has something to cross rather than a step change.
      const p = (this.simTimeMs - incident.startedMs) / (incident.endsMs - incident.startedMs);
      incident.k = p < 0.35 ? p / 0.35 : p > 0.75 ? Math.max(0, (1 - p) / 0.25) : 1;
      if (incident.spec.sea) sea = Math.max(sea, 2 + (incident.spec.sea - 2) * incident.k);
      if (incident.bus) incident.bus.derate = 1 - (1 - incident.spec.derate) * incident.k;
      if (incident.hub) incident.hub.alive = incident.k < 0.15;
      still.push(incident);
    }
    this.incidents = still;
    this.ship.seaState = sea;
  }

  /** Publish rate for one node, after voyage phase and any live incidents. */
  rateFor(node) {
    const phase = PHASE_RATE[phaseOf(this.ship).id] ?? {};
    let rate = node.baseRate * (phase[node.type] ?? 1);
    if (node.type === 'engine') rate *= 0.35 + this.ship.throttle * 0.9;
    for (const incident of this.incidents) {
      if (incident.spec.targets?.(node, this.ship)) {
        rate *= 1 + (incident.spec.rate - 1) * incident.k;
      }
    }
    return rate;
  }

  /**
   * Walks every link and works out how much of the offered load survives it.
   * A saturated bus does not lose everything, it loses the excess, so the
   * delivered fraction is `capacity / offered` and the rest queues and ages out.
   */
  recomputeLinks() {
    const { buses, hubs, risers } = this.topology;
    let fieldBits = 0;
    let busPeak = 0;

    for (const bus of buses) {
      let bits = 0;
      let msgs = 0;
      for (const node of bus.nodes) {
        const rate = this.rateFor(node);
        node.rate = rate;
        msgs += rate;
        bits += rate * (node.bytes + node.topic.length + MQTT_OVERHEAD) * 8 * bus.overhead;
      }
      bus.offeredKbps = bits / 1000;
      bus.offeredMsgs = msgs;
      const capacity = bus.capacityKbps * bus.derate;
      bus.util = capacity > 0 ? bus.offeredKbps / capacity : 99;
      bus.deliverFrac = bus.util > 1 ? 1 / bus.util : 1;
      bus.carriedKbps = bus.offeredKbps * bus.deliverFrac;
      fieldBits += bits;
      busPeak = Math.max(busPeak, bus.util);
    }

    for (const hub of hubs) {
      hub.inMsgs = 0;
      hub.inKbps = 0;
      for (const busId of hub.buses) {
        const bus = this.topology.busById.get(busId);
        hub.inMsgs += bus.offeredMsgs * bus.deliverFrac;
        hub.inKbps += bus.carriedKbps;
      }
    }
    // Concentrators backhaul through their parent, so they are sized first.
    for (const hub of hubs) {
      if (!hub.parent) continue;
      hub.util = hub.alive ? hub.inMsgs / hub.cpu : 99;
      hub.deliverFrac = hub.alive ? Math.min(1, 1 / Math.max(hub.util, 1e-6)) : 0;
      const parent = this.topology.hubById.get(hub.parent);
      parent.inMsgs += hub.inMsgs * hub.deliverFrac;
      parent.inKbps += hub.inKbps * hub.deliverFrac;
    }
    let hubPeak = 0;
    for (const hub of hubs) {
      if (hub.parent) {
        hubPeak = Math.max(hubPeak, hub.alive ? hub.util : 1);
        continue;
      }
      hub.util = hub.alive ? hub.inMsgs / hub.cpu : 99;
      hub.deliverFrac = hub.alive ? Math.min(1, 1 / Math.max(hub.util, 1e-6)) : 0;
      hubPeak = Math.max(hubPeak, hub.alive ? hub.util : 1);
    }

    for (const node of this.topology.nodes) {
      const bus = this.topology.busById.get(node.bus);
      const hub = this.topology.hubById.get(node.hub);
      const parent = hub.parent ? this.topology.hubById.get(hub.parent) : null;
      node.deliverFrac = bus.deliverFrac * hub.deliverFrac * (parent ? parent.deliverFrac : 1);
    }

    for (const riser of risers) {
      riser.load = hubs
        .filter((h) => h.riser === riser.id && !h.parent)
        .reduce((sum, h) => sum + h.inKbps * h.deliverFrac, 0);
    }

    this.stats.fieldKbps = fieldBits / 1000;
    this.stats.busPeak = busPeak;
    this.stats.hubPeak = hubPeak;
  }

  /** Emits this tick's messages and runs each survivor through the matcher. */
  publish(dt) {
    const { nodes } = this.topology;
    this.packetBudget = Math.min(PACKET_SPAWN_PER_S * dt + this.packetBudget, 40);
    this.tailBudget = Math.min(this.tailBudget + 26 * dt, 8);
    const w = this.window;
    // One headers object, refilled per message: the matcher reads it
    // synchronously and `node.last` takes its own copy, so nothing outlives
    // the loop iteration that wrote it.
    const headers = {};

    for (const node of nodes) {
      node.accum += node.rate * dt;
      const pending = Math.floor(node.accum);
      if (pending <= 0) continue;
      node.accum -= pending;
      // A long frame — a backgrounded tab, say — can accumulate a large
      // backlog. Emitting all of it at once would stall the frame, so the
      // catch-up is capped and the remainder is counted as offered but never
      // delivered rather than quietly forgotten.
      const emitted = Math.min(pending, 8);
      w.offered += pending;
      w.dropped += pending - emitted;

      for (let i = 0; i < emitted; i++) {
        node.sent++;
        if (Math.random() > node.deliverFrac) {
          w.dropped++;
          continue;
        }
        const payload = samplePayload(node, this.ship, dt);
        for (const incident of this.incidents) {
          if (incident.spec.distort && incident.spec.targets?.(node, this.ship)) {
            incident.spec.distort(node, payload, incident.k);
          }
        }
        const retained = node.type === 'door' || node.type === 'lift';
        headers.qos = String(node.qos);
        headers.retained = String(retained);
        headers.deck = node.deckNum;
        headers.zone = node.zone;
        headers.prio = node.qos === 2 ? 'high' : node.qos === 1 ? 'normal' : 'low';

        const bytes = node.bytes + node.topic.length + MQTT_OVERHEAD;
        w.delivered++;
        w.bits += bytes * 8;
        node.lastPublishAt = this.simTimeMs;

        const msg = { topic: node.topic, headers, payload };
        const hit = this.matcher ? this.matcher.matches(msg) : false;
        if (hit) {
          w.matched++;
          w.matchedBits += bytes * 8;
          node.matched++;
          node.lastMatchAt = this.simTimeMs;
          if (this.selector.stages.length > 0) {
            const value = this.matcher.process(msg, this.simTimeMs);
            if (value !== undefined) this.stageValue = value;
          }
          if (this.tailBudget >= 1) {
            this.tailBudget -= 1;
            this.pushTail(node, msg, bytes);
          }
        }
        node.last = { payload, headers: { ...headers }, at: this.simTimeMs, matched: hit };
        if (this.packetBudget >= 1 && this.packets.length < PACKET_MAX) {
          this.packetBudget -= 1;
          this.packets.push({ node, t: 0, speed: 0.55 + Math.random() * 0.25, matched: hit, color: node.color });
        }
      }
    }
  }

  pushTail(node, msg, bytes) {
    this.tail.unshift({
      seq: ++this.tailSeq,
      topic: msg.topic,
      qos: msg.headers.qos,
      retained: msg.headers.retained === 'true',
      bytes,
      payload: JSON.stringify(msg.payload),
      color: node.color,
      at: this.simTimeMs,
    });
    if (this.tail.length > TAIL_MAX) this.tail.length = TAIL_MAX;
  }

  rollWindow(dt) {
    const w = this.window;
    w.ms += dt * 1000;
    if (w.ms < ROLL_MS) return;
    const seconds = w.ms / 1000;
    const blend = 0.45;
    const s = this.stats;
    s.offeredRate += (w.offered / seconds - s.offeredRate) * blend;
    s.deliveredRate += (w.delivered / seconds - s.deliveredRate) * blend;
    s.matchedRate += (w.matched / seconds - s.matchedRate) * blend;
    s.droppedRate += (w.dropped / seconds - s.droppedRate) * blend;
    s.brokerKbps += (w.bits / seconds / 1000 - s.brokerKbps) * blend;
    s.matchedKbps += (w.matchedBits / seconds / 1000 - s.matchedKbps) * blend;
    s.shoreUtil = this.shoreBudgetKbps > 0 ? s.matchedKbps / this.shoreBudgetKbps : 0;
    s.savedPct = s.brokerKbps > 0 ? (1 - s.matchedKbps / s.brokerKbps) * 100 : 0;

    this.history.push({
      t: this.simTimeMs,
      total: s.brokerKbps,
      matched: s.matchedKbps,
      rate: s.deliveredRate,
      matchedRate: s.matchedRate,
      stage: this.stageValue,
    });
    const cutoff = this.simTimeMs - HISTORY_MS;
    while (this.history.length > 0 && this.history[0].t < cutoff) this.history.shift();

    w.offered = 0; w.delivered = 0; w.matched = 0; w.dropped = 0;
    w.bits = 0; w.matchedBits = 0; w.ms = 0;
  }

  advancePackets(dt) {
    const live = [];
    for (const p of this.packets) {
      p.t += dt * p.speed;
      if (p.t < 1) live.push(p);
    }
    this.packets = live;
  }

  /** Per-device-family match tally, for the "who is matching" readout. */
  typeBreakdown() {
    const counts = new Map();
    for (const node of this.topology.nodes) {
      if (node.matched === 0) continue;
      counts.set(node.type, (counts.get(node.type) ?? 0) + node.matched);
    }
    return [...counts.entries()]
      .sort((a, b) => b[1] - a[1])
      .map(([type, count]) => ({ type, count, label: NODE_TYPES[type].label, color: NODE_TYPES[type].color }));
  }
}

function stageLabelFor(selector) {
  const last = [...selector.stages].reverse().find((s) => s.kind !== 'window');
  if (!last) return '';
  const windowStage = selector.stages.filter((s) => s.kind === 'window').pop();
  const suffix = windowStage ? ` over ${Math.round(windowStage.ms / 1000)}s` : ' (per message)';
  if (last.kind === 'count') return `count()${suffix}`;
  const field = last.field.kind === 'header' ? last.field.name : `json$.${last.field.path.join('.')}`;
  return `${last.kind}(${field})${suffix}`;
}
