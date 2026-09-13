/**
 * topology.js — the MS Tail Spinner: decks, fieldbuses, hubs and the trunk.
 *
 * The ship is described declaratively in `DECK_PLAN` and laid out into
 * normalised [0,1] canvas coordinates, so a resize is a re-scale rather than a
 * re-layout. Every node ends up with an MQTT topic of the shape
 *
 *     ship/<deck>/<zone>/<type>/<id>
 *
 * which is what makes the selectors in the console read the way they do:
 * `//engine/+` reaches every propulsion node on the ship, `/ship/deck4//+`
 * reaches everything in the galley.
 */

/** Decks, bow to the right. `x0`/`x1` taper to give the hull its profile. */
export const DECKS = [
  { id: 'bridge', num: 'brg', name: 'Navigation Bridge', y: 0.080, x0: 0.400, x1: 0.700 },
  { id: 'deck12', num: '12', name: 'Sun Deck', y: 0.143, x0: 0.250, x1: 0.800 },
  { id: 'deck11', num: '11', name: 'Lido Deck', y: 0.206, x0: 0.190, x1: 0.850 },
  { id: 'deck10', num: '10', name: 'Balcony Cabins', y: 0.269, x0: 0.155, x1: 0.875 },
  { id: 'deck9', num: '09', name: 'Cabins', y: 0.332, x0: 0.135, x1: 0.890 },
  { id: 'deck8', num: '08', name: 'Cabins', y: 0.395, x0: 0.120, x1: 0.900 },
  { id: 'deck7', num: '07', name: 'Cabins', y: 0.458, x0: 0.108, x1: 0.910 },
  { id: 'deck6', num: '06', name: 'Atrium & Shops', y: 0.521, x0: 0.098, x1: 0.918 },
  { id: 'deck5', num: '05', name: 'Promenade & Boats', y: 0.584, x0: 0.090, x1: 0.925 },
  { id: 'deck4', num: '04', name: 'Galley & Dining', y: 0.647, x0: 0.090, x1: 0.925 },
  { id: 'deck3', num: '03', name: 'Crew & Stores', y: 0.710, x0: 0.098, x1: 0.916 },
  { id: 'deck2', num: '02', name: 'Machinery', y: 0.773, x0: 0.115, x1: 0.898 },
  { id: 'deck1', num: '01', name: 'Tank Top & Bilge', y: 0.836, x0: 0.150, x1: 0.852 },
];

/** Longitudinal thirds. The bow is to the right, so `fore` is the right third. */
export const ZONES = {
  aft: { letter: 'a', span: [0.00, 0.315] },
  mid: { letter: 'm', span: [0.345, 0.655] },
  fore: { letter: 'f', span: [0.685, 1.0] },
};

/**
 * Fieldbus families. `kbps` is the raw signalling rate and `overhead` the
 * framing multiplier, so a 60-byte publish costs more than 60 bytes of bus
 * time. The slow buses (KNX, LoRa, M-Bus) are the ones that actually saturate,
 * which is the point: they are why the traffic has to be concentrated at a hub
 * before it ever reaches the broker.
 */
export const BUS_KINDS = {
  rs485: { label: 'Modbus RTU / RS-485', kbps: 115.2, overhead: 1.35, color: '#7dd3fc' },
  can: { label: 'CANopen', kbps: 500, overhead: 1.9, color: '#a78bfa' },
  n2k: { label: 'NMEA 2000', kbps: 250, overhead: 1.9, color: '#38bdf8' },
  knx: { label: 'KNX TP1', kbps: 9.6, overhead: 1.6, color: '#fbbf24' },
  mbus: { label: 'M-Bus', kbps: 38.4, overhead: 1.4, color: '#fb923c' },
  lon: { label: 'LonWorks FT-10', kbps: 78, overhead: 1.5, color: '#f472b6' },
  enet: { label: 'Ethernet segment', kbps: 100000, overhead: 1.08, color: '#4ade80' },
  lora: { label: 'LoRa 868 MHz', kbps: 27, overhead: 2.2, color: '#facc15' },
};

/**
 * Device families. `rate` is the nominal publish rate in messages per second
 * and `bytes` the nominal payload size; both are modulated by voyage phase and
 * by whatever scenarios are running.
 */
export const NODE_TYPES = {
  engine: { abbr: 'eng', label: 'Propulsion telemetry', rate: 20, bytes: 120, qos: 0, color: '#fb7185' },
  nav: { abbr: 'nav', label: 'Navigation', rate: 10, bytes: 112, qos: 0, color: '#60a5fa' },
  cctv: { abbr: 'cam', label: 'Camera health', rate: 4, bytes: 82, qos: 0, color: '#94a3b8' },
  power: { abbr: 'pwr', label: 'Power meter', rate: 2, bytes: 96, qos: 0, color: '#fde047' },
  hvac: { abbr: 'ahu', label: 'HVAC controller', rate: 1, bytes: 88, qos: 1, color: '#5eead4' },
  bilge: { abbr: 'blg', label: 'Bilge & ingress', rate: 1, bytes: 58, qos: 2, color: '#38bdf8' },
  temp: { abbr: 'tmp', label: 'Space temperature', rate: 0.5, bytes: 54, qos: 0, color: '#fca5a5' },
  refrig: { abbr: 'rfg', label: 'Refrigeration', rate: 0.5, bytes: 86, qos: 1, color: '#67e8f9' },
  lift: { abbr: 'lft', label: 'Elevator controller', rate: 0.5, bytes: 72, qos: 1, color: '#d8b4fe' },
  pos: { abbr: 'til', label: 'Point of sale', rate: 0.4, bytes: 150, qos: 1, color: '#4ade80' },
  humid: { abbr: 'hum', label: 'Humidity', rate: 0.33, bytes: 46, qos: 0, color: '#93c5fd' },
  water: { abbr: 'wtr', label: 'Potable & grey water', rate: 0.25, bytes: 78, qos: 1, color: '#22d3ee' },
  pax: { abbr: 'pax', label: 'Passenger counter', rate: 0.25, bytes: 42, qos: 0, color: '#f0abfc' },
  door: { abbr: 'dor', label: 'Door & hatch state', rate: 0.2, bytes: 62, qos: 1, color: '#c4b5fd' },
  fire: { abbr: 'fir', label: 'Fire & smoke detector', rate: 0.2, bytes: 66, qos: 2, color: '#f97316' },
  fuel: { abbr: 'fue', label: 'Fuel & tank level', rate: 0.2, bytes: 74, qos: 1, color: '#facc15' },
};

/**
 * The deck plan. Each entry is one (deck, zone) machinery space with a hub and
 * the fieldbuses landing on it. `parent` makes a hub a concentrator that
 * backhauls through another hub instead of straight onto the trunk riser.
 */
const DECK_PLAN = [
  { deck: 'bridge', zone: 'fore', hub: 'HUB-BRG', buses: [
    { kind: 'n2k', nodes: { nav: 6 } },
    { kind: 'enet', nodes: { cctv: 2 } }] },

  { deck: 'deck12', zone: 'aft', hub: 'HUB-12A', buses: [
    { kind: 'knx', nodes: { temp: 3, hvac: 2 } },
    { kind: 'enet', nodes: { cctv: 2 } }] },
  { deck: 'deck12', zone: 'mid', hub: 'HUB-12M', buses: [
    { kind: 'knx', nodes: { temp: 2, hvac: 2 } },
    { kind: 'enet', nodes: { cctv: 2 } }] },
  { deck: 'deck12', zone: 'fore', hub: 'HUB-12F', buses: [
    { kind: 'lora', nodes: { pax: 3 } },
    { kind: 'enet', nodes: { cctv: 1 } }] },

  { deck: 'deck11', zone: 'aft', hub: 'HUB-11A', buses: [
    { kind: 'rs485', nodes: { refrig: 2, temp: 2 } },
    { kind: 'enet', nodes: { pos: 2 } }] },
  { deck: 'deck11', zone: 'mid', hub: 'HUB-11M', buses: [
    { kind: 'knx', nodes: { temp: 3, humid: 2, hvac: 2 } },
    { kind: 'enet', nodes: { pos: 3, cctv: 2 } },
    { kind: 'mbus', nodes: { water: 2 } }] },

  { deck: 'deck11', zone: 'fore', hub: 'HUB-11F', buses: [
    { kind: 'knx', nodes: { temp: 3, humid: 2 } },
    { kind: 'enet', nodes: { pos: 2 } }] },

  { deck: 'deck10', zone: 'mid', hub: 'HUB-10M', buses: [
    { kind: 'lon', nodes: { fire: 3 } },
    { kind: 'can', nodes: { lift: 2 } },
    { kind: 'enet', nodes: { cctv: 2 } }] },
  { deck: 'deck10', zone: 'fore', hub: 'HUB-10F', parent: 'HUB-10M', buses: [
    { kind: 'knx', nodes: { temp: 4, hvac: 2 } },
    { kind: 'can', nodes: { door: 4 } }] },

  { deck: 'deck9', zone: 'aft', hub: 'HUB-09A', buses: [
    { kind: 'knx', nodes: { temp: 4, hvac: 2 } },
    { kind: 'can', nodes: { door: 4 } }] },

  { deck: 'deck9', zone: 'fore', hub: 'HUB-09F', buses: [
    { kind: 'knx', nodes: { temp: 4, hvac: 2 } },
    { kind: 'can', nodes: { door: 3 } }] },

  { deck: 'deck8', zone: 'aft', hub: 'HUB-08A', buses: [
    { kind: 'knx', nodes: { temp: 3, hvac: 2 } },
    { kind: 'can', nodes: { door: 3 } }] },
  { deck: 'deck8', zone: 'mid', hub: 'HUB-08M', buses: [
    { kind: 'knx', nodes: { temp: 4, humid: 2 } },
    { kind: 'lon', nodes: { fire: 3 } },
    { kind: 'can', nodes: { lift: 2 } }] },

  { deck: 'deck7', zone: 'fore', hub: 'HUB-07F', buses: [
    { kind: 'knx', nodes: { temp: 4, hvac: 2 } },
    { kind: 'can', nodes: { door: 4 } }] },

  { deck: 'deck7', zone: 'mid', hub: 'HUB-07M', buses: [
    { kind: 'lon', nodes: { fire: 3 } },
    { kind: 'can', nodes: { lift: 2 } },
    { kind: 'enet', nodes: { cctv: 2 } }] },

  { deck: 'deck6', zone: 'mid', hub: 'HUB-06M', buses: [
    { kind: 'enet', nodes: { pos: 4, cctv: 3 } },
    { kind: 'lora', nodes: { pax: 3 } },
    { kind: 'lon', nodes: { fire: 2 } }] },

  { deck: 'deck6', zone: 'fore', hub: 'HUB-06F', parent: 'HUB-06M', buses: [
    { kind: 'enet', nodes: { pos: 2, cctv: 2 } },
    { kind: 'lon', nodes: { fire: 2 } }] },

  { deck: 'deck5', zone: 'aft', hub: 'HUB-05A', buses: [
    { kind: 'enet', nodes: { pos: 3, cctv: 2 } },
    { kind: 'knx', nodes: { temp: 2, hvac: 2 } }] },

  { deck: 'deck5', zone: 'mid', hub: 'HUB-05M', buses: [
    { kind: 'enet', nodes: { pos: 3, cctv: 2 } },
    { kind: 'knx', nodes: { temp: 2, hvac: 1 } }] },

  { deck: 'deck4', zone: 'mid', hub: 'HUB-04M', buses: [
    { kind: 'rs485', nodes: { refrig: 5, temp: 3 } },
    { kind: 'lon', nodes: { fire: 3 } },
    { kind: 'mbus', nodes: { water: 2, power: 2 } }] },
  { deck: 'deck4', zone: 'fore', hub: 'HUB-04F', parent: 'HUB-04M', buses: [
    { kind: 'enet', nodes: { pos: 2, cctv: 2 } },
    { kind: 'can', nodes: { lift: 2 } }] },

  { deck: 'deck3', zone: 'aft', hub: 'HUB-03A', buses: [
    { kind: 'rs485', nodes: { refrig: 3, temp: 2 } },
    { kind: 'mbus', nodes: { water: 2 } },
    { kind: 'lora', nodes: { pax: 2 } }] },
  { deck: 'deck3', zone: 'mid', hub: 'HUB-03M', buses: [
    { kind: 'lon', nodes: { fire: 3 } },
    { kind: 'can', nodes: { door: 4 } },
    { kind: 'enet', nodes: { cctv: 2 } }] },

  { deck: 'deck2', zone: 'aft', hub: 'HUB-02A', buses: [
    { kind: 'can', nodes: { engine: 6 } },
    { kind: 'rs485', nodes: { power: 4, temp: 3 } },
    { kind: 'mbus', nodes: { fuel: 3 } }] },
  { deck: 'deck2', zone: 'mid', hub: 'HUB-02M', buses: [
    { kind: 'can', nodes: { engine: 4 } },
    { kind: 'rs485', nodes: { power: 4 } },
    { kind: 'lon', nodes: { fire: 2 } }] },
  { deck: 'deck2', zone: 'fore', hub: 'HUB-02F', parent: 'HUB-02M', buses: [
    { kind: 'rs485', nodes: { power: 3, water: 2 } },
    { kind: 'can', nodes: { door: 3 } }] },

  { deck: 'deck1', zone: 'aft', hub: 'HUB-01A', buses: [
    { kind: 'rs485', nodes: { bilge: 4, temp: 2 } },
    { kind: 'mbus', nodes: { fuel: 3 } }] },
  { deck: 'deck1', zone: 'mid', hub: 'HUB-01M', buses: [
    { kind: 'rs485', nodes: { bilge: 4 } },
    { kind: 'mbus', nodes: { fuel: 2, water: 2 } }] },
  { deck: 'deck1', zone: 'fore', hub: 'HUB-01F', buses: [
    { kind: 'rs485', nodes: { bilge: 3 } },
    { kind: 'can', nodes: { door: 3 } }] },
];

/** Vertical trunk risers. Hubs backhaul to whichever one is closer. */
const RISERS = [
  { id: 'riser-aft', label: 'Aft riser', x: 0.305 },
  { id: 'riser-fwd', label: 'Forward riser', x: 0.648 },
];

const BROKER = { id: 'core', label: 'Core broker', nx: 0.545, ny: 0.080 };
const VSAT = { id: 'vsat', label: 'VSAT uplink', nx: 0.628, ny: 0.026 };

const BAND = 0.046; // vertical room a deck gives its fieldbuses
const lerp = (a, b, t) => a + (b - a) * t;

/** Deterministic PRNG so the ship looks identical on every reload. */
function mulberry32(seed) {
  let a = seed >>> 0;
  return () => {
    a = (a + 0x6d2b79f5) >>> 0;
    let t = a;
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

/** Cumulative arc lengths, so a packet's position is one scalar lookup. */
function measure(points) {
  const lengths = [0];
  let total = 0;
  for (let i = 1; i < points.length; i++) {
    const dx = points[i][0] - points[i - 1][0];
    const dy = points[i][1] - points[i - 1][1];
    total += Math.hypot(dx, dy);
    lengths.push(total);
  }
  return { points, lengths, total: total || 1e-6 };
}

/** Point at arc length `t` (0..1) along a measured polyline. */
export function pointOnPath(path, t) {
  const target = Math.max(0, Math.min(1, t)) * path.total;
  const { points, lengths } = path;
  let i = 1;
  while (i < lengths.length - 1 && lengths[i] < target) i++;
  const span = lengths[i] - lengths[i - 1] || 1e-6;
  const f = (target - lengths[i - 1]) / span;
  return [lerp(points[i - 1][0], points[i][0], f), lerp(points[i - 1][1], points[i][1], f)];
}

/**
 * Builds the ship: nodes on buses, buses on hubs, hubs on risers, risers on the
 * core broker. Returns everything already laid out in normalised coordinates.
 */
export function buildTopology() {
  const rand = mulberry32(0x5eaf00d);
  const deckById = new Map(DECKS.map((d) => [d.id, d]));
  const nodes = [];
  const buses = [];
  const hubs = [];
  const cells = [];
  const counters = new Map();

  for (const cell of DECK_PLAN) {
    const deck = deckById.get(cell.deck);
    if (!deck) throw new Error(`deck plan references unknown deck ${cell.deck}`);
    const zone = ZONES[cell.zone];
    const width = deck.x1 - deck.x0;
    const cx0 = deck.x0 + width * zone.span[0];
    const cx1 = deck.x0 + width * zone.span[1];

    // The hub sits at whichever end of the cell faces its trunk riser, so the
    // fieldbuses run "inboard" towards it the way cable trays actually do.
    const riser = RISERS.reduce((best, r) =>
      Math.abs(r.x - (cx0 + cx1) / 2) < Math.abs(best.x - (cx0 + cx1) / 2) ? r : best);
    const hubAtRight = riser.x > (cx0 + cx1) / 2;
    const hubX = hubAtRight ? cx1 - 0.008 : cx0 + 0.008;
    const fieldX0 = hubAtRight ? cx0 + 0.006 : cx0 + 0.028;
    const fieldX1 = hubAtRight ? cx1 - 0.028 : cx1 - 0.006;

    const hub = {
      id: cell.hub,
      label: cell.hub,
      deck: deck.id,
      deckName: deck.name,
      zone: cell.zone,
      parent: cell.parent ?? null,
      riser: riser.id,
      nx: hubX,
      ny: deck.y,
      buses: [],
      // A zone hub is a small industrial gateway, not a server: it runs out of
      // message budget long before the ship's backbone runs out of bits.
      cpu: 900,
      uplinkKbps: 8000,
      alive: true,
    };
    hubs.push(hub);
    cells.push({ deck: deck.id, zone: cell.zone, x0: cx0, x1: cx1, y: deck.y, hub: hub.id });

    cell.buses.forEach((busSpec, busIndex) => {
      const kind = BUS_KINDS[busSpec.kind];
      if (!kind) throw new Error(`deck plan references unknown bus kind ${busSpec.kind}`);
      const count = cell.buses.length;
      const busY = deck.y - BAND / 2 + (BAND * (busIndex + 0.5)) / count;
      const busId = `${cell.hub}-${busSpec.kind.toUpperCase()}${busIndex + 1}`;

      const members = [];
      for (const [type, n] of Object.entries(busSpec.nodes)) {
        for (let i = 0; i < n; i++) members.push(type);
      }

      const bus = {
        id: busId,
        kind: busSpec.kind,
        kindLabel: kind.label,
        capacityKbps: kind.kbps,
        overhead: kind.overhead,
        color: kind.color,
        hub: hub.id,
        deck: deck.id,
        zone: cell.zone,
        nodes: [],
        ny: busY,
        // live state
        offeredKbps: 0,
        carriedKbps: 0,
        util: 0,
        dropped: 0,
        faultUntil: 0,
        derate: 1,
      };

      members.forEach((type, i) => {
        const spec = NODE_TYPES[type];
        const t = members.length === 1 ? 0.5 : i / (members.length - 1);
        const nx = lerp(fieldX0, fieldX1, hubAtRight ? t : 1 - t);
        const seq = (counters.get(`${type}-${deck.num}${zone.letter}`) ?? 0) + 1;
        counters.set(`${type}-${deck.num}${zone.letter}`, seq);
        const id = `${spec.abbr}-${deck.num}${zone.letter}${seq}`;
        const node = {
          id,
          type,
          typeLabel: spec.label,
          color: spec.color,
          deck: deck.id,
          deckNum: deck.num,
          deckName: deck.name,
          zone: cell.zone,
          bus: bus.id,
          hub: hub.id,
          topic: `ship/${deck.id}/${cell.zone}/${type}/${id}`,
          baseRate: spec.rate,
          bytes: spec.bytes,
          qos: spec.qos,
          nx,
          ny: busY + (i % 2 === 0 ? -0.0075 : 0.0075),
          stubY: busY,
          // live state
          rate: spec.rate,
          phase: rand(),
          accum: rand(),
          state: {},
          last: null,
          lastMatchAt: -1e9,
          lastPublishAt: -1e9,
          sent: 0,
          matched: 0,
        };
        nodes.push(node);
        bus.nodes.push(node);
      });

      // The bus runs from its far end, past every drop, into the hub.
      const far = hubAtRight ? [fieldX0 - 0.006, busY] : [fieldX1 + 0.006, busY];
      bus.path = measure([far, [hubX, busY], [hub.nx, hub.ny]]);
      bus.line = [far, [hubX, busY]];
      hub.buses.push(bus.id);
      buses.push(bus);
    });
  }

  const hubById = new Map(hubs.map((h) => [h.id, h]));
  const riserById = new Map(RISERS.map((r) => [r.id, r]));

  // Backhaul: concentrator -> zone hub -> riser -> core broker.
  for (const hub of hubs) {
    const riser = riserById.get(hub.riser);
    const chain = [[hub.nx, hub.ny]];
    if (hub.parent) {
      const parent = hubById.get(hub.parent);
      if (!parent) throw new Error(`hub ${hub.id} has unknown parent ${hub.parent}`);
      chain.push([parent.nx, parent.ny]);
      const parentRiser = riserById.get(parent.riser);
      chain.push([parentRiser.x, parent.ny], [parentRiser.x, BROKER.ny]);
    } else {
      chain.push([riser.x, hub.ny], [riser.x, BROKER.ny]);
    }
    chain.push([BROKER.nx, BROKER.ny]);
    hub.uplinkPath = measure(chain);
    hub.uplinkLine = chain;
  }

  // A node's full journey, used to animate one message end to end.
  for (const node of nodes) {
    const bus = buses.find((b) => b.id === node.bus);
    const hub = hubById.get(node.hub);
    const points = [[node.nx, node.ny], [node.nx, node.stubY]];
    const busEnd = bus.line[1];
    points.push([busEnd[0], node.stubY], [hub.nx, hub.ny]);
    for (const p of hub.uplinkPath.points.slice(1)) points.push(p);
    node.path = measure(points);
  }

  const risers = RISERS.map((r) => ({
    ...r,
    y0: BROKER.ny,
    y1: DECKS[DECKS.length - 1].y + 0.018,
    load: 0,
    alive: true,
  }));

  return {
    decks: DECKS,
    cells,
    nodes,
    buses,
    hubs,
    risers,
    broker: { ...BROKER },
    vsat: { ...VSAT },
    nodeById: new Map(nodes.map((n) => [n.id, n])),
    busById: new Map(buses.map((b) => [b.id, b])),
    hubById,
    types: Object.keys(NODE_TYPES),
  };
}
