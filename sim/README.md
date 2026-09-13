# MS *Tail Spinner* — the MoQTail simulator

A shipboard telemetry fabric you can point selectors at. It runs entirely in the
browser: no broker, no server, no build step.

**Live:** https://seanwevans.github.io/MoQTail/

```
255 devices · 16 device families · 68 fieldbuses · 8 bus standards · 29 hubs · 2 trunk risers
```

## What it is

A cruise ship is a good argument for MoQTail. It carries thousands of sensors on
slow industrial fieldbuses, concentrates them through zone gateways onto a
shipboard backbone, and then has exactly one narrow, expensive way to get any of
it ashore — a satellite link measured in hundreds of kilobits.

So the simulator draws the whole chain to scale:

```
device ─▶ fieldbus ─▶ zone hub ─▶ concentrator ─▶ trunk riser ─▶ core broker ─▶ VSAT ─▶ shore
```

Every link has a budget. Twelve KNX cabin controllers on one 9.6 kbps segment is
a real constraint, which is why the hubs exist. And the ship publishes something
like half a megabit a second against a shore budget of 256 kbps, which is why
the broker has to filter rather than forward.

The selector you type is compiled and run against **every message the broker
receives**, message by message. The match rate, the bandwidth saved, the
highlighted devices and the `moqtail sub` tail are all measurements of that, not
a scripted animation.

## Things to try

| Do this | And watch |
| --- | --- |
| `/msg[qos=2]//#` | Only bilge and fire publish at QoS 2. 97% of the ship's traffic never reaches the dish. |
| Inject **Main engine 2A exhaust overheat**, then `//engine/+[json$.egt>520]` | Nothing matches at first: EGT has to climb through the threshold. |
| Inject **Cabin HVAC chiller trip** | The KNX segments on the cabin decks go past 100% and start dropping frames. A hub cannot un-drop what the bus never carried. |
| Inject **Muster drill**, then `//pax/+[json$.muster=true]` | The LoRa segments saturate — the drill is a bandwidth event, not just a safety one. |
| `//nav/+ \|> window(10s) \|> avg(json$.sog)` | A pipeline stage, plotted live. Compare it to the speed in the header. |
| Inject **Zone hub HUB-02M offline** | Three fieldbuses and everything behind them go dark at once. |
| Type `//bilge` | The console explains why it matches nothing, and offers the fix. |
| Click any device | Subscribes to that whole family. Click a hub for its deck, a bus for its zone. |

## Running it locally

Any static file server will do — ES modules will not load over `file://`.

```bash
cd sim
python3 -m http.server 8080     # then open http://localhost:8080
node --test                     # the engine and simulator tests
```

## The engine

`js/moqtail.js` is a hand port of [`moqtail-core`](../crates/moqtail-core): the
pest grammar in `selector.pest`, the AST and `Display` impl in `ast.rs`, and the
matcher, the numeric tolerance rules and the pipeline stage machinery in
`matcher.rs`. Parse failures carry the same messages as the corresponding
`moqtail_core::Error` variants.

It is a port rather than a WebAssembly build so the site stays a folder of
static files with nothing to compile, install or version-skew. The obvious risk
is that the two engines drift, so they are pinned to each other:

```
crates/moqtail-core/tests/conformance/corpus.json   ← one file
        ├── crates/moqtail-core/tests/conformance.rs  runs it against the Rust engine
        └── sim/tests/conformance.test.mjs            runs it against this one
```

Both runners are in CI. A behaviour that only one engine has fails on one side
or the other.

### The one deliberate deviation

pest applies its implicit `WHITESPACE` rule between repetitions, and the grammar
does not mark `ident`, `number` or `string` atomic. So `moqtail-core` accepts
`/fo o` and compiles it to a literal segment `"fo o"`, which no MQTT topic can
contain. This port treats those tokens as atomic and rejects the input instead.

Both engines reject the other cases this reaches — `window(1 5s)` and
`json $ . temp` parse in pest and then fail in the semantic pass — so the
divergence is confined to inputs that were never going to match anything. The
corpus covers the sane grammar rather than pinning the quirk.

## How the model works

**Topology** (`js/topology.js`) is declarative. `DECK_PLAN` lists each
instrumented (deck, zone) compartment, the hub in it, the fieldbuses landing on
that hub and the devices on each bus. Everything is laid out into normalised
`[0,1]` coordinates once, so a resize is a re-scale and never a re-layout.
Topics come out as `ship/<deck>/<zone>/<type>/<id>`.

**Telemetry** (`js/telemetry.js`) gives every device family a small stateful
model: a cabin warms towards its setpoint, a bilge well fills with the sea
state, exhaust gas temperature tracks engine load. Incidents ramp in and out
rather than stepping, so a threshold predicate has something to cross.

**Transport** (`js/sim.js`) recomputes every link each tick. A bus offered more
than it can carry delivers `capacity / offered` of it and drops the rest; a hub
past its message budget does the same; a dead hub delivers nothing. Whatever
survives reaches the broker and goes through the matcher.

**Drawing** (`js/render.js`) is one canvas. The ship heels with the sea state
inside a single transform, and `toLocal` inverts it so the pointer stays honest
while the deck is moving.

## Deployment

`.github/workflows/pages.yml` runs the tests, copies `sim/` (minus `tests/` and
`package.json`) into `_site/`, and publishes it to GitHub Pages on every push to
`main` that touches the simulator or the corpus. The repository's
**Settings → Pages → Source** has to be set to **GitHub Actions** for the
workflow to have anywhere to publish to.
