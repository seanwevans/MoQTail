/**
 * Runs the golden conformance corpus against the browser engine.
 *
 * The corpus lives with the Rust crate, where `tests/conformance.rs` runs the
 * same cases against `moqtail-core`. Both runners have to agree, so a drift in
 * either engine shows up as a failure on one side.
 *
 *     node --test sim/tests/
 */

import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';

import { compile, display, Matcher, MoqtailError } from '../js/moqtail.js';

const CORPUS_URL = new URL('../../crates/moqtail-core/tests/conformance/corpus.json', import.meta.url);
const corpus = JSON.parse(readFileSync(fileURLToPath(CORPUS_URL), 'utf8'));

const messageOf = (c) => ({
  topic: c.topic,
  headers: c.headers ?? {},
  payload: c.payload ?? null,
});

const approx = (a, b) => Math.abs(a - b) <= 1e-9 * Math.max(Math.abs(a), Math.abs(b), 1);

test('compile cases match the Rust engine', () => {
  for (const c of corpus.compile) {
    let compiled;
    try {
      compiled = compile(c.selector);
    } catch (e) {
      assert.ok(e instanceof MoqtailError, `${c.selector}: threw a non-MoqtailError: ${e}`);
      assert.equal(c.ok, false, `${JSON.stringify(c.selector)} failed to compile: ${e.message}`);
      if (c.error !== undefined) {
        assert.equal(e.message, c.error, `wrong error for ${JSON.stringify(c.selector)}`);
      }
      continue;
    }
    assert.equal(c.ok, true, `${JSON.stringify(c.selector)} compiled but the corpus expects a failure`);
    if (c.display !== undefined) {
      assert.equal(display(compiled), c.display, `${JSON.stringify(c.selector)} did not round-trip through display()`);
      // The canonical form has to compile to itself, or it is useless for
      // round-tripping the selector back into the console.
      assert.equal(display(compile(c.display)), c.display, `display() is not idempotent for ${JSON.stringify(c.selector)}`);
    }
  }
});

test('match cases agree with the Rust engine', () => {
  for (const c of corpus.match) {
    const matcher = new Matcher(compile(c.selector));
    assert.equal(
      matcher.matches(messageOf(c)),
      c.expect,
      `${JSON.stringify(c.selector)} against topic ${JSON.stringify(c.topic)}`,
    );
  }
});

test('pipeline stages agree with the Rust engine', () => {
  for (const c of corpus.process) {
    const matcher = new Matcher(compile(c.selector));
    c.steps.forEach((step, index) => {
      const got = matcher.process(messageOf(step), step.at_ms);
      if (step.expect === null) {
        assert.equal(got, undefined, `${c.selector} step ${index} returned ${got}, expected none`);
      } else {
        assert.notEqual(got, undefined, `${c.selector} step ${index} returned none, expected ${step.expect}`);
        assert.ok(approx(got, step.expect), `${c.selector} step ${index} returned ${got}, expected ${step.expect}`);
      }
    });
  }
});

test('the corpus has not quietly shrunk', () => {
  assert.ok(corpus.compile.length >= 40);
  assert.ok(corpus.match.length >= 50);
  assert.ok(corpus.process.length >= 8);
});
