/**
 * telemetry.js — what the ship actually says.
 *
 * Every device family gets a small stateful model so the payloads drift like
 * real instruments rather than flickering randomly: a cabin warms towards its
 * setpoint, a bilge well fills with the sea state, an engine's exhaust
 * temperature tracks its load. That matters for the demo, because a selector
 * such as `//refrig/+[json$.c>4]` is only interesting if the fridge takes a
 * plausible amount of time to fail.
 */

const clamp = (v, lo, hi) => (v < lo ? lo : v > hi ? hi : v);
const r1 = (v) => Math.round(v * 10) / 10;
const r2 = (v) => Math.round(v * 100) / 100;
const jitter = (scale) => (Math.random() - 0.5) * scale;

/** Voyage phases, in the order the auto-pilot cycles them. */
export const PHASES = [
  { id: 'moored', label: 'Moored', throttle: 0.04, sog: 0.0, seconds: 35 },
  { id: 'departure', label: 'Departure', throttle: 0.42, sog: 7.5, seconds: 30 },
  { id: 'cruise', label: 'Cruise', throttle: 0.82, sog: 19.5, seconds: 90 },
  { id: 'maneuver', label: 'Manoeuvring', throttle: 0.55, sog: 11.0, seconds: 30 },
  { id: 'arrival', label: 'Arrival', throttle: 0.18, sog: 3.5, seconds: 30 },
];

export function createShip() {
  return {
    clock: 6 * 3600 + 40 * 60, // ship's time, seconds past midnight
    elapsed: 0,
    phaseIndex: 2,
    phaseT: 0,
    autoVoyage: true,
    throttle: 0.82,
    sog: 19.5,
    hdg: 127,
    lat: 25.7214,
    lon: -79.3102,
    seaState: 2,
    roll: 0,
    pitch: 0,
  };
}

export const phaseOf = (ship) => PHASES[ship.phaseIndex];

export function stepShip(ship, dt) {
  const phase = phaseOf(ship);
  ship.elapsed += dt;
  ship.clock = (ship.clock + dt * 12) % 86400; // ship's clock runs 12x
  ship.phaseT += dt;
  if (ship.autoVoyage && ship.phaseT > phase.seconds) {
    ship.phaseT = 0;
    ship.phaseIndex = (ship.phaseIndex + 1) % PHASES.length;
  }
  // Machinery does not step; it ramps.
  ship.throttle += (phase.throttle - ship.throttle) * Math.min(1, dt * 0.35);
  ship.sog += (phase.sog - ship.sog) * Math.min(1, dt * 0.3);
  ship.hdg = (ship.hdg + dt * 0.12 * (1 + ship.seaState * 0.1)) % 360;
  const rad = (ship.hdg * Math.PI) / 180;
  ship.lat += (Math.cos(rad) * ship.sog * dt) / 216000;
  ship.lon += (Math.sin(rad) * ship.sog * dt) / 216000;
  const swell = 0.55 + ship.seaState * 0.52;
  ship.roll = swell * Math.sin(ship.elapsed * 0.62) + swell * 0.35 * Math.sin(ship.elapsed * 1.31 + 1.1);
  ship.pitch = swell * 0.4 * Math.sin(ship.elapsed * 0.83 + 0.4);
}

// ---------------------------------------------------------------------------
// Incidents
// ---------------------------------------------------------------------------

/**
 * Things that go wrong at sea. `targets` picks the affected nodes, `rate`
 * multiplies their publish rate, `distort` bends their payloads, and `suggest`
 * is the selector that catches the incident — the console offers it when the
 * incident fires, which is the fastest way to learn the DSL.
 */
export const INCIDENTS = [
  {
    id: 'bilge',
    label: 'Water ingress, Deck 1 midships',
    blurb: 'A shaft seal weeps. Bilge wells fill and the pumps cut in.',
    tone: 'alarm',
    seconds: 55,
    targets: (n) => n.type === 'bilge' && n.deck === 'deck1' && n.zone === 'mid',
    rate: 4,
    suggest: '//bilge/+[json$.mm>120]',
    distort: (n, p, k) => {
      p.mm = r1(p.mm + 340 * k);
      p.pump = p.mm > 90;
      p.alarm = p.mm > 200;
    },
  },
  {
    id: 'overheat',
    label: 'Main engine 2A exhaust overheat',
    blurb: 'A fouled turbocharger drives exhaust gas temperature past the alarm.',
    tone: 'alarm',
    seconds: 50,
    targets: (n) => n.type === 'engine' && n.zone === 'aft',
    rate: 2,
    suggest: '//engine/+[json$.egt>520]',
    distort: (n, p, k) => {
      p.egt = Math.round(p.egt + 260 * k);
      p.vib = r2(p.vib + 3.1 * k);
      p.oilp = r1(p.oilp - 0.9 * k);
    },
  },
  {
    id: 'galley-fire',
    label: 'Galley fire alarm, Deck 4',
    blurb: 'A fryer flashes over. Detectors on the galley and store decks go to alarm.',
    tone: 'alarm',
    seconds: 45,
    targets: (n) => n.type === 'fire' && (n.deck === 'deck4' || n.deck === 'deck3'),
    rate: 9,
    suggest: '//fire/+[json$.smoke>0.35]',
    distort: (n, p, k) => {
      p.smoke = r2(clamp(p.smoke + 0.85 * k, 0, 1));
      p.heatC = Math.round(p.heatC + 110 * k);
      p.state = p.smoke > 0.55 ? 'alarm' : p.smoke > 0.2 ? 'pre-alarm' : 'normal';
    },
  },
  {
    id: 'cold-chain',
    label: 'Galley chiller compressor failure',
    blurb: 'A provisions chiller loses its compressor and the cold chain drifts.',
    tone: 'warn',
    seconds: 70,
    targets: (n) => n.type === 'refrig' && n.deck === 'deck4',
    rate: 2,
    suggest: '//refrig/+[json$.c>4]',
    distort: (n, p, k) => {
      p.c = r1(p.c + 13 * k);
      p.comp = false;
      p.defrost = false;
    },
  },
  {
    id: 'muster',
    label: 'Muster drill — all passengers',
    blurb: 'Every passenger tag reports at once and the LoRa segments saturate.',
    tone: 'info',
    seconds: 50,
    targets: (n) => n.type === 'pax' || n.type === 'door',
    rate: 26,
    suggest: '//pax/+[json$.muster=true]',
    distort: (n, p, k) => {
      if (n.type === 'pax') {
        p.muster = k > 0.25;
        p.count = Math.round(p.count * (1 + 2.4 * k));
      } else {
        p.state = 'open';
        p.cycles += 1;
      }
    },
  },
  {
    id: 'storm',
    label: 'Heavy weather, sea state 6',
    blurb: 'The ship works in a swell: vibration, bilge levels and roll all rise.',
    tone: 'warn',
    seconds: 80,
    targets: (n) => n.type === 'bilge' || n.type === 'engine' || n.type === 'nav',
    rate: 1.6,
    sea: 6,
    suggest: '//nav/+ |> window(10s) |> avg(json$.sog)',
    distort: (n, p, k) => {
      if (n.type === 'bilge') p.mm = r1(p.mm + 60 * k);
      if (n.type === 'engine') p.vib = r2(p.vib + 2.2 * k);
      if (n.type === 'nav') p.sog = r1(Math.max(0, p.sog - 4.5 * k));
    },
  },
  {
    id: 'port-day',
    label: 'Port day — shops and bars open',
    blurb: 'Every till on board starts ringing. Watch the Ethernet segments.',
    tone: 'info',
    seconds: 60,
    targets: (n) => n.type === 'pos',
    rate: 11,
    suggest: '//pos/+ |> window(60s) |> count()',
    distort: (n, p) => {
      p.items = Math.round(p.items * 1.8);
      p.amount = r2(p.amount * 1.6);
    },
  },
  {
    id: 'hvac-storm',
    label: 'Cabin HVAC chiller trip',
    blurb: 'Chilled water is lost to the cabin decks and every space starts to warm.',
    tone: 'warn',
    seconds: 65,
    targets: (n) => (n.type === 'temp' || n.type === 'hvac') && n.deck.startsWith('deck') && Number(n.deckNum) >= 7,
    rate: 3,
    suggest: '//temp/+[json$.c>26]',
    distort: (n, p, k) => {
      p.c = r1(p.c + 7.5 * k);
      if (n.type === 'hvac') {
        p.valve = 100;
        p.mode = 'fault';
      }
    },
  },
  {
    id: 'bus-fault',
    label: 'KNX segment fault, Deck 11 midships',
    blurb: 'A shorted segment halves the bus and the drops start queueing.',
    tone: 'warn',
    seconds: 45,
    kind: 'bus',
    busPick: (b) => b.kind === 'knx' && b.deck === 'deck11',
    derate: 0.25,
    suggest: '/ship/deck11//+',
  },
  {
    id: 'hub-fault',
    label: 'Zone hub HUB-02M offline',
    blurb: 'A machinery-space gateway drops off and takes its fieldbuses with it.',
    tone: 'alarm',
    seconds: 40,
    kind: 'hub',
    hubId: 'HUB-02M',
    suggest: '/ship/deck2//+',
  },
];

// ---------------------------------------------------------------------------
// Payload models
// ---------------------------------------------------------------------------

const SPACES = ['cabin', 'corridor', 'lounge', 'galley', 'store', 'plant'];
const TILL_NAMES = ['atrium-bar', 'pool-bar', 'boutique', 'spa', 'coffee', 'casino'];

/** Seeds the per-node model state the first time a node publishes. */
function initState(node) {
  const s = node.state;
  s.seed = Math.random();
  switch (node.type) {
    case 'temp':
      s.sp = 21 + Math.round(s.seed * 4);
      s.c = s.sp + jitter(1.5);
      s.space = SPACES[Math.floor(s.seed * SPACES.length)];
      break;
    case 'humid':
      s.rh = 44 + s.seed * 14;
      break;
    case 'hvac':
      s.sp = 21 + Math.round(s.seed * 3);
      s.c = s.sp + jitter(1.2);
      s.valve = 30 + s.seed * 30;
      break;
    case 'engine':
      s.hours = 12000 + Math.floor(s.seed * 9000);
      s.rpm = 0;
      s.egt = 300;
      s.vib = 1.3;
      break;
    case 'bilge':
      s.mm = 4 + s.seed * 14;
      break;
    case 'fire':
      s.smoke = 0.01 + s.seed * 0.03;
      s.batt = 92 + s.seed * 8;
      break;
    case 'pos':
      s.till = TILL_NAMES[Math.floor(s.seed * TILL_NAMES.length)];
      s.takings = 0;
      break;
    case 'door':
      s.open = false;
      s.cycles = Math.floor(s.seed * 400);
      break;
    case 'lift':
      s.floor = 1 + Math.floor(s.seed * 12);
      s.dir = 'idle';
      break;
    case 'pax':
      s.count = Math.floor(40 + s.seed * 180);
      break;
    case 'fuel':
      s.pct = 38 + s.seed * 50;
      break;
    case 'water':
      s.pct = 45 + s.seed * 45;
      s.tank = s.seed > 0.5 ? 'potable' : 'grey';
      break;
    case 'refrig':
      s.sp = s.seed > 0.6 ? -18 : 3;
      s.c = s.sp + jitter(0.8);
      break;
    case 'power':
      s.kw = 40 + s.seed * 180;
      break;
    case 'cctv':
      s.health = 'ok';
      break;
    default:
      break;
  }
  s.ready = true;
}

/**
 * Produces one message payload for `node`. Cheap on purpose: this runs a few
 * hundred times a second and the result is handed straight to the matcher.
 */
export function samplePayload(node, ship, dt) {
  const s = node.state;
  if (!s.ready) initState(node);
  const sea = ship.seaState;

  switch (node.type) {
    case 'temp': {
      const pull = node.deckNum === '02' || node.deckNum === '01' ? 6 : 0;
      s.c += ((s.sp + pull - s.c) * 0.05) + jitter(0.12);
      return { c: r1(s.c), sp: s.sp, space: s.space, rh: r1(48 + Math.sin(ship.elapsed * 0.2 + s.seed) * 6) };
    }
    case 'humid':
      s.rh = clamp(s.rh + jitter(0.6), 25, 95);
      return { rh: r1(s.rh), dew: r1(s.rh * 0.18 + 6) };
    case 'hvac': {
      s.c += (s.sp - s.c) * 0.06 + jitter(0.1);
      s.valve = clamp(s.valve + (s.c - s.sp) * 6 + jitter(2), 0, 100);
      return { mode: 'cool', sp: s.sp, c: r1(s.c), fan: Math.round(40 + s.valve * 0.5), valve: Math.round(s.valve) };
    }
    case 'door': {
      if (Math.random() < 0.08) {
        s.open = !s.open;
        if (s.open) s.cycles++;
      }
      return { state: s.open ? 'open' : 'closed', locked: !s.open && s.seed > 0.7, cycles: s.cycles };
    }
    case 'power': {
      const demand = 0.55 + ship.throttle * 0.6;
      s.kw += (s.kw * 0 + 60 * demand + s.seed * 120 - s.kw) * 0.08 + jitter(3);
      return { kw: r1(s.kw), v: Math.round(438 + jitter(6)), a: r1((s.kw * 1000) / (440 * 1.73)), pf: r2(0.88 + jitter(0.05)), hz: r2(60 + jitter(0.06)) };
    }
    case 'engine': {
      const target = ship.throttle * 104;
      s.rpm += (target - s.rpm) * 0.09 + jitter(0.7);
      const load = clamp(s.rpm / 104, 0, 1.1);
      s.egt += (285 + load * 175 - s.egt) * 0.08 + jitter(3);
      s.vib += (1.1 + load * 2.4 + sea * 0.24 - s.vib) * 0.1 + jitter(0.08);
      s.hours += dt / 3600;
      return {
        rpm: r1(s.rpm), load: Math.round(load * 100), egt: Math.round(s.egt),
        oilp: r1(4.2 + load * 1.1 + jitter(0.12)), vib: r2(Math.max(0, s.vib)), hours: Math.round(s.hours),
      };
    }
    case 'bilge': {
      const inflow = 0.04 + sea * 0.05;
      s.mm = clamp(s.mm + inflow - (s.mm > 60 ? 1.6 : 0.05) + jitter(0.4), 0, 900);
      return { mm: r1(s.mm), pump: s.mm > 60, alarm: s.mm > 200 };
    }
    case 'fire': {
      s.smoke = clamp(s.smoke + jitter(0.006), 0, 1);
      s.batt = clamp(s.batt - dt * 0.0004, 40, 100);
      return { smoke: r2(s.smoke), heatC: Math.round(22 + s.smoke * 40 + jitter(1)), state: 'normal', batt: r1(s.batt) };
    }
    case 'pos': {
      const amount = r2(3 + Math.random() * 96);
      s.takings = r2(s.takings + amount);
      return { till: s.till, amount, items: 1 + Math.floor(Math.random() * 6), card: Math.random() > 0.18, takings: s.takings };
    }
    case 'nav':
      return {
        lat: Math.round(ship.lat * 1e5) / 1e5, lon: Math.round(ship.lon * 1e5) / 1e5,
        sog: r1(Math.max(0, ship.sog + jitter(0.35))), cog: Math.round(ship.hdg),
        hdg: Math.round((ship.hdg + ship.roll * 0.4 + 360) % 360),
        depth: Math.round(40 + Math.sin(ship.elapsed * 0.05) * 30 + jitter(4)),
        wind: r1(8 + sea * 3.5 + jitter(2)),
      };
    case 'cctv':
      return {
        fps: Math.random() < 0.02 ? 12 : 25, kbps: Math.round(1800 + jitter(340)),
        motion: r2(Math.max(0, Math.random() * (node.deckNum === '06' ? 1 : 0.5))), health: s.health,
      };
    case 'lift': {
      if (Math.random() < 0.25) {
        const next = clamp(s.floor + (Math.random() > 0.5 ? 1 : -1), 1, 13);
        s.dir = next > s.floor ? 'up' : next < s.floor ? 'down' : 'idle';
        s.floor = next;
      }
      return { floor: s.floor, dir: s.dir, door: s.dir === 'idle' ? 'open' : 'closed', load: Math.round(Math.random() * 900) };
    }
    case 'pax':
      s.count = Math.round(clamp(s.count + jitter(9), 0, 2400));
      return { count: s.count, muster: false, tags: Math.round(s.count * 0.96) };
    case 'fuel':
      s.pct = clamp(s.pct - ship.throttle * dt * 0.006, 2, 100);
      return { pct: r2(s.pct), l: Math.round(s.pct * 3400), c: r1(28 + jitter(1.5)), lpm: r1(ship.throttle * 62 + jitter(3)) };
    case 'water':
      s.pct = clamp(s.pct + jitter(0.25), 3, 100);
      return { tank: s.tank, pct: r1(s.pct), cl: r2(0.6 + jitter(0.12)), lpm: r1(18 + jitter(7)) };
    case 'refrig': {
      s.c += (s.sp - s.c) * 0.08 + jitter(0.18);
      return { c: r1(s.c), sp: s.sp, door: Math.random() < 0.04, defrost: Math.random() < 0.02, comp: true };
    }
    default:
      return { v: r2(Math.random()) };
  }
}
