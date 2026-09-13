/**
 * Tests for the ship model itself: that the topology is wired the way the
 * drawing claims, that the selectors on the buttons all compile, and that
 * backpressure actually bites when a link is oversubscribed.
 */

import { test } from 'node:test';
import assert from 'node:assert/strict';

import { compile, Matcher } from '../js/moqtail.js';
import { buildTopology, NODE_TYPES, BUS_KINDS, pointOnPath } from '../js/topology.js';
import { INCIDENTS, createShip } from '../js/telemetry.js';
import { Simulation } from '../js/sim.js';
import { PRESETS } from '../js/ui.js';

const topo = buildTopology();
const run = (sim, seconds, step = 1 / 30) => {
  for (let i = 0; i < seconds / step; i++) sim.tick(step);
};

test('the ship is at least as large as the brief', () => {
  assert.ok(topo.nodes.length >= 100, `expected 100+ nodes, got ${topo.nodes.length}`);
  const families = new Set(topo.nodes.map((n) => n.type));
  assert.ok(families.size >= 10, `expected 10+ device families, got ${families.size}`);
  assert.ok(topo.buses.length > 0 && topo.hubs.length > 0);
  const busFamilies = new Set(topo.buses.map((b) => b.kind));
  assert.ok(busFamilies.size >= 5, 'the fieldbus mix should span several standards');
});

test('every node, bus and hub is wired to something that exists', () => {
  const ids = new Set();
  for (const node of topo.nodes) {
    assert.ok(!ids.has(node.id), `duplicate node id ${node.id}`);
    ids.add(node.id);
    assert.ok(topo.busById.has(node.bus), `${node.id} is on unknown bus ${node.bus}`);
    assert.ok(topo.hubById.has(node.hub), `${node.id} is behind unknown hub ${node.hub}`);
    assert.equal(topo.busById.get(node.bus).hub, node.hub);
    assert.ok(node.path.points.length >= 4, `${node.id} has no drawable path`);
    assert.ok(Number.isFinite(node.path.total) && node.path.total > 0);
  }
  for (const bus of topo.buses) {
    assert.ok(bus.nodes.length > 0, `${bus.id} has no devices`);
    assert.ok(BUS_KINDS[bus.kind], `${bus.id} has unknown family ${bus.kind}`);
    assert.ok(topo.hubById.get(bus.hub).buses.includes(bus.id));
  }
  for (const hub of topo.hubs) {
    if (hub.parent === null) continue;
    const parent = topo.hubById.get(hub.parent);
    assert.ok(parent, `${hub.id} backhauls through missing hub ${hub.parent}`);
    assert.equal(parent.parent, null, 'concentrator chains should be two deep at most');
  }
});

test('a packet path runs from its node all the way to the broker', () => {
  const node = topo.nodes[0];
  const [sx, sy] = pointOnPath(node.path, 0);
  const [ex, ey] = pointOnPath(node.path, 1);
  assert.ok(Math.abs(sx - node.nx) < 1e-6 && Math.abs(sy - node.ny) < 1e-6);
  assert.ok(Math.abs(ex - topo.broker.nx) < 1e-6 && Math.abs(ey - topo.broker.ny) < 1e-6);
});

test('topics are addressable with the selectors the UI suggests', () => {
  const node = topo.nodes.find((n) => n.type === 'engine');
  const exact = new Matcher(compile(`/${node.topic.split('/').join('/')}`));
  assert.ok(exact.matches({ topic: node.topic, headers: {}, payload: null }));

  for (const type of Object.keys(NODE_TYPES)) {
    const family = new Matcher(compile(`//${type}/+`));
    const hits = topo.nodes.filter((n) => family.matches({ topic: n.topic, headers: {}, payload: null }));
    const expected = topo.nodes.filter((n) => n.type === type);
    assert.deepEqual(
      hits.map((n) => n.id).sort(),
      expected.map((n) => n.id).sort(),
      `//${type}/+ did not select exactly the ${type} family`,
    );
  }

  const deck = new Matcher(compile('/ship/deck4//+'));
  const onDeck4 = topo.nodes.filter((n) => deck.matches({ topic: n.topic, headers: {}, payload: null }));
  assert.ok(onDeck4.length > 0);
  assert.ok(onDeck4.every((n) => n.deck === 'deck4'));
});

test('every preset and every scenario suggestion compiles', () => {
  for (const group of PRESETS) {
    for (const item of group.items) {
      assert.doesNotThrow(() => compile(item.s), `preset ${item.s} does not compile`);
      assert.ok(item.why.length > 10, `preset ${item.s} needs an explanation`);
    }
  }
  for (const incident of INCIDENTS) {
    assert.doesNotThrow(() => compile(incident.suggest), `${incident.id} suggests ${incident.suggest}, which does not compile`);
  }
});

test('every scenario actually has something on board to act on', () => {
  const ship = createShip();
  for (const incident of INCIDENTS) {
    if (incident.kind === 'bus') {
      assert.ok(topo.buses.some(incident.busPick), `${incident.id} targets no bus`);
    } else if (incident.kind === 'hub') {
      assert.ok(topo.hubById.has(incident.hubId), `${incident.id} targets missing hub ${incident.hubId}`);
    } else {
      const hit = topo.nodes.filter((n) => incident.targets(n, ship));
      assert.ok(hit.length > 0, `${incident.id} targets no nodes`);
    }
  }
});

test('a quiet ship delivers everything it offers', () => {
  const sim = new Simulation();
  run(sim, 20);
  assert.ok(sim.stats.deliveredRate > 50, 'the ship should be publishing');
  assert.ok(sim.stats.droppedRate < 1, `nothing should drop at rest, saw ${sim.stats.droppedRate}/s`);
  assert.ok(sim.stats.busPeak < 1, `no bus should be saturated at rest, peak ${sim.stats.busPeak}`);
});

test('an oversubscribed bus sheds the excess rather than everything', () => {
  const sim = new Simulation();
  run(sim, 10);
  sim.fireIncident('muster');
  run(sim, 30);
  const lora = sim.topology.buses.filter((b) => b.kind === 'lora');
  assert.ok(lora.some((b) => b.util > 1), 'the muster drill should saturate the LoRa segments');
  for (const bus of lora) {
    if (bus.util <= 1) continue;
    assert.ok(bus.deliverFrac > 0 && bus.deliverFrac < 1, 'a saturated bus still carries its capacity');
    assert.ok(Math.abs(bus.carriedKbps - bus.capacityKbps * bus.derate) < 1e-6);
  }
  assert.ok(sim.stats.droppedRate > 0, 'saturation should show up as drops');
});

test('a failed hub takes its fieldbuses off the air and comes back', () => {
  const sim = new Simulation();
  run(sim, 5);
  sim.fireIncident('hub-fault');
  run(sim, 12);
  const hub = sim.topology.hubById.get('HUB-02M');
  assert.equal(hub.alive, false);
  assert.equal(hub.deliverFrac, 0);
  const behind = sim.topology.nodes.filter((n) => n.hub === hub.id);
  assert.ok(behind.length > 0);
  assert.ok(behind.every((n) => n.deliverFrac === 0), 'nothing behind a dead hub reaches the broker');
  sim.clearIncidents();
  run(sim, 2);
  assert.equal(hub.alive, true, 'the hub comes back when the incident is cleared');
});

test('filtering at the broker is what keeps the shore link inside budget', () => {
  const sim = new Simulation();
  sim.setSelector('//#');
  run(sim, 20);
  const unfiltered = sim.stats.matchedKbps;
  assert.ok(unfiltered > 100, `the raw firehose should be substantial, saw ${unfiltered} kbps`);

  sim.setSelector('//bilge/+[json$.mm>120]');
  run(sim, 20);
  assert.ok(sim.stats.matchedKbps < unfiltered * 0.1, 'a narrow selector should cut the uplink by an order of magnitude');
  assert.ok(sim.stats.savedPct > 80);
});

test('pipeline stages report over the window the selector asked for', () => {
  const sim = new Simulation();
  sim.setSelector('//nav/+ |> window(10s) |> avg(json$.sog)');
  run(sim, 30);
  assert.equal(sim.stageLabel, 'avg(json$.sog) over 10s');
  assert.ok(Math.abs(sim.stageValue - sim.ship.sog) < 3, 'average SOG should track the ship');

  sim.setSelector('//pos/+ |> window(60s) |> count()');
  run(sim, 30);
  assert.equal(sim.stageLabel, 'count() over 60s');
  assert.ok(sim.stageValue > 0);
});

test('a selector that matches nothing is offered the anchored-suffix fix', () => {
  const sim = new Simulation();
  run(sim, 5);
  sim.setSelector('//bilge');
  run(sim, 2);
  const hint = sim.suffixHint();
  assert.equal(hint.candidate, '//bilge/+');
  assert.equal(hint.hits, sim.topology.nodes.filter((n) => n.type === 'bilge').length);

  sim.setSelector('//bilge/+');
  run(sim, 2);
  assert.equal(sim.suffixHint(), null, 'no hint once the selector matches');

  sim.setSelector('//not-a-device/+');
  run(sim, 2);
  assert.equal(sim.suffixHint(), null, 'no hint when no suffix would help');
});

test('a broken selector leaves the previous subscription running', () => {
  const sim = new Simulation();
  sim.setSelector('//engine/+');
  run(sim, 10);
  const before = sim.canonical;
  assert.equal(sim.setSelector('//engine/+['), false);
  assert.ok(sim.selectorError);
  assert.equal(sim.canonical, before, 'the last good selector stays compiled');
  run(sim, 5);
  assert.ok(sim.stats.matchedRate > 0, 'and keeps matching');
});
