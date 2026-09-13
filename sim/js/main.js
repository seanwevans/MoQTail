/**
 * main.js — bootstrap and the frame loop.
 *
 * The simulation is stepped from rAF with the real frame delta, so it runs at
 * the same wall-clock speed whatever the display refresh rate. A tab that has
 * been in the background gets its delta clamped inside `Simulation.tick`, so
 * coming back does not fast-forward an hour of shipboard traffic at once.
 */

import { Simulation } from './sim.js';
import { Renderer } from './render.js';
import { UI } from './ui.js';

const sim = new Simulation();
const canvas = document.getElementById('ship');
const renderer = new Renderer(canvas, sim);
const ui = new UI(sim, renderer);

// Warm the ship up so the first frame shows a moving fabric rather than an
// empty one: forty seconds of traffic, run as fast as the CPU will take it.
for (let i = 0; i < 40 * 30; i++) sim.tick(1 / 30);

let last = performance.now();
function frame(now) {
  const dt = Math.min(0.2, (now - last) / 1000);
  last = now;
  sim.tick(dt);
  renderer.draw(now);
  ui.frame(now);
  requestAnimationFrame(frame);
}
requestAnimationFrame(frame);

const onResize = () => renderer.resize();
window.addEventListener('resize', onResize);
if (window.ResizeObserver) new ResizeObserver(onResize).observe(canvas);

// Handy for poking at the model from the console.
Object.assign(window, { sim, renderer, ui });
