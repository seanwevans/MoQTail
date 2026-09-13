/**
 * moqtail.js — a browser port of the `moqtail-core` selector engine.
 *
 * This is a hand port of `crates/moqtail-core`: the pest grammar in
 * `selector.pest`, the AST in `ast.rs`, and the matcher and pipeline stages in
 * `matcher.rs`. It exists so the simulator can compile and evaluate real
 * MoQTail selectors in the browser with no server and no WebAssembly step.
 *
 * The two engines are locked together by the golden corpus in
 * `crates/moqtail-core/tests/conformance/`, which `conformance.rs` runs against
 * the Rust engine and `sim/tests/conformance.test.mjs` runs against this one.
 * See `sim/README.md` for the one deliberate deviation (identifiers are atomic
 * here; pest's implicit whitespace lets them swallow spaces).
 */

/** Absolute floor for numeric equality; mirrors `ABS_EPS` in matcher.rs. */
const ABS_EPS = 1e-12;
/** Relative term for numeric equality; mirrors `REL_EPS` in matcher.rs. */
const REL_EPS = 1e-9;

/** A compile failure. `kind` mirrors the `parser::Error` variant name. */
export class MoqtailError extends Error {
  constructor(message, kind, pos = null) {
    super(message);
    this.name = 'MoqtailError';
    this.kind = kind;
    this.pos = pos;
  }
}

const err = (kind, message, pos = null) => new MoqtailError(message, kind, pos);

// ---------------------------------------------------------------------------
// Parser
//
// A recursive-descent reading of selector.pest. Ordered choice is PEG-style:
// once an alternative matches, the parse commits to it and a later failure in
// the enclosing sequence does not retry the earlier alternatives.
// ---------------------------------------------------------------------------

const isIdentChar = (c) =>
  c !== undefined &&
  ((c >= 'a' && c <= 'z') || (c >= 'A' && c <= 'Z') || (c >= '0' && c <= '9') || c === '_' || c === '-');

const isDigit = (c) => c !== undefined && c >= '0' && c <= '9';

class Parser {
  constructor(src) {
    this.s = src;
    this.i = 0;
  }

  /** pest's implicit `WHITESPACE = _{ " " | "\t" }`, skipped between tokens. */
  ws() {
    while (this.i < this.s.length && (this.s[this.i] === ' ' || this.s[this.i] === '\t')) this.i++;
  }

  atEnd() {
    this.ws();
    return this.i >= this.s.length;
  }

  /** Consumes `text` if it is next (after whitespace). */
  lit(text) {
    this.ws();
    if (this.s.startsWith(text, this.i)) {
      this.i += text.length;
      return true;
    }
    return false;
  }

  /** `ident = { (ASCII_ALPHANUMERIC | "_" | "-")+ }` */
  ident() {
    this.ws();
    const start = this.i;
    while (isIdentChar(this.s[this.i])) this.i++;
    return this.i > start ? this.s.slice(start, this.i) : null;
  }

  /** `number = { "-"? ~ ASCII_DIGIT+ ~ ("." ~ ASCII_DIGIT+)? }` */
  number() {
    this.ws();
    const start = this.i;
    if (this.s[this.i] === '-') this.i++;
    if (!isDigit(this.s[this.i])) {
      this.i = start;
      return null;
    }
    while (isDigit(this.s[this.i])) this.i++;
    if (this.s[this.i] === '.' && isDigit(this.s[this.i + 1])) {
      this.i++;
      while (isDigit(this.s[this.i])) this.i++;
    }
    return this.s.slice(start, this.i);
  }

  /** `string = { "\"" ~ ( "\\" ~ ANY | !"\"" ~ ANY )* ~ "\"" }` */
  string() {
    this.ws();
    if (this.s[this.i] !== '"') return null;
    const start = this.i;
    let j = this.i + 1;
    for (;;) {
      if (j >= this.s.length) return null; // unterminated
      if (this.s[j] === '\\') {
        j += 2;
        continue;
      }
      if (this.s[j] === '"') {
        j++;
        break;
      }
      j++;
    }
    this.i = j;
    return this.s.slice(start, j);
  }

  /**
   * `json_field = { "json" ~ ( "$" ~ ("." ~ ident)* | ("." ~ ident)+ ) }`
   *
   * The grammar deliberately admits the `$`-less spelling so that `parse_field`
   * can report a dedicated error for it, so this returns the shape it matched
   * and lets the caller decide.
   */
  jsonField() {
    this.ws();
    const start = this.i;
    if (!this.s.startsWith('json', this.i)) return null;
    this.i += 4;
    if (this.s[this.i] === '$') {
      this.i++;
      const parts = [];
      for (;;) {
        const save = this.i;
        if (this.s[this.i] !== '.') break;
        this.i++;
        const id = this.ident();
        if (id === null) {
          this.i = save;
          break;
        }
        parts.push(id);
      }
      return { dollar: true, parts };
    }
    const parts = [];
    for (;;) {
      const save = this.i;
      if (this.s[this.i] !== '.') break;
      this.i++;
      const id = this.ident();
      if (id === null) {
        this.i = save;
        break;
      }
      parts.push(id);
    }
    if (parts.length === 0) {
      this.i = start;
      return null;
    }
    return { dollar: false, parts };
  }

  /** `field = { json_field | ident }`, then `parse_field`'s validation. */
  field() {
    const save = this.i;
    const jf = this.jsonField();
    if (jf !== null) {
      if (!jf.dollar || jf.parts.length === 0) throw err('MissingField', 'missing field', save);
      return { kind: 'json', path: jf.parts };
    }
    const id = this.ident();
    if (id === null) return null;
    return { kind: 'header', name: id };
  }

  /** `value = { boolean | number | string }` */
  value() {
    this.ws();
    if (this.s.startsWith('true', this.i)) {
      this.i += 4;
      return { kind: 'bool', value: true };
    }
    if (this.s.startsWith('false', this.i)) {
      this.i += 5;
      return { kind: 'bool', value: false };
    }
    const num = this.number();
    if (num !== null) return { kind: 'number', value: Number(num) };
    const str = this.string();
    if (str !== null) {
      let parsed;
      try {
        parsed = JSON.parse(str);
      } catch {
        throw err('InvalidValue', 'invalid value', this.i);
      }
      if (typeof parsed !== 'string') throw err('InvalidValue', 'invalid value', this.i);
      return { kind: 'string', value: parsed };
    }
    return null;
  }

  /** `predicate = { "[" ~ field ~ operator ~ value ~ "]" }` */
  predicate() {
    const save = this.i;
    if (!this.lit('[')) return null;
    const field = this.field();
    if (field === null) {
      this.i = save;
      return null;
    }
    this.ws();
    let op = null;
    for (const cand of ['<=', '>=', '<', '>', '=']) {
      if (this.s.startsWith(cand, this.i)) {
        op = cand;
        this.i += cand.length;
        break;
      }
    }
    if (op === null) {
      this.i = save;
      return null;
    }
    const value = this.value();
    if (value === null) {
      this.i = save;
      return null;
    }
    if (!this.lit(']')) {
      this.i = save;
      return null;
    }
    return { field, op, value };
  }

  /** `path_segment = { slash ~ segment ~ predicate* }` */
  pathSegment() {
    const save = this.i;
    let axis;
    if (this.lit('//')) axis = 'descendant';
    else if (this.lit('/')) axis = 'child';
    else return null;

    this.ws();
    let segment;
    const c = this.s[this.i];
    if (c === '+' || c === '#') {
      this.i++;
      segment = c === '+' ? { kind: 'plus' } : { kind: 'hash' };
    } else {
      const id = this.ident();
      if (id === null) {
        this.i = save;
        return null;
      }
      segment = id === 'msg' ? { kind: 'message' } : { kind: 'literal', name: id };
    }

    const predicates = [];
    for (;;) {
      const p = this.predicate();
      if (p === null) break;
      predicates.push(p);
    }
    return { axis, segment, predicates };
  }

  /** `duration = { number ~ unit }`, `unit = { "s" | "m" | "h" }` */
  duration() {
    const save = this.i;
    const num = this.number();
    if (num === null) return null;
    this.ws();
    const u = this.s[this.i];
    if (u !== 's' && u !== 'm' && u !== 'h') {
      this.i = save;
      return null;
    }
    this.i++;
    return { number: num, unit: u };
  }

  /** `func_arg = _{ duration | function | field }` */
  funcArg() {
    const save = this.i;
    const d = this.duration();
    if (d !== null) return { kind: 'duration', duration: d };
    this.i = save;
    const fn = this.func();
    if (fn !== null) return { kind: 'function', func: fn };
    this.i = save;
    const f = this.field();
    if (f !== null) return { kind: 'field', field: f };
    this.i = save;
    return null;
  }

  /** `function = { ident ~ "(" ~ func_arg? ~ ")" }` */
  func() {
    const save = this.i;
    const name = this.ident();
    if (name === null) {
      this.i = save;
      return null;
    }
    if (!this.lit('(')) {
      this.i = save;
      return null;
    }
    const arg = this.funcArg();
    if (!this.lit(')')) {
      this.i = save;
      return null;
    }
    return { name, arg };
  }

  /** `stage = { pipe ~ function }` */
  stage() {
    const save = this.i;
    if (!this.lit('|>')) return null;
    const fn = this.func();
    if (fn === null) {
      this.i = save;
      return null;
    }
    return buildStage(fn);
  }
}

/** Mirrors `parse_stage` in parser.rs, including which arg shapes it rejects. */
function buildStage(fn) {
  const { name, arg } = fn;
  switch (name) {
    case 'window': {
      if (arg === null) throw err('WindowRequiresDuration', 'window requires duration');
      if (arg.kind !== 'duration') throw err('WindowRequiresDuration', 'window requires duration');
      const { number, unit } = arg.duration;
      // `amount` is parsed as a u64 in Rust, so a fraction or a sign is a
      // ParseIntError rather than a rounded window.
      if (!/^\d+$/.test(number)) throw err('ParseInt', 'invalid digit found in string');
      const amount = Number(number);
      if (!Number.isSafeInteger(amount)) throw err('ParseInt', 'number too large to fit in target type');
      const mult = unit === 's' ? 1 : unit === 'm' ? 60 : 3600;
      const seconds = amount * mult;
      if (!Number.isSafeInteger(seconds)) throw err('WindowRequiresDuration', 'window requires duration');
      return { kind: 'window', ms: seconds * 1000 };
    }
    case 'sum':
    case 'avg': {
      const message = `${name} requires field`;
      const kind = name === 'sum' ? 'SumRequiresField' : 'AvgRequiresField';
      if (arg === null || arg.kind !== 'field') throw err(kind, message);
      return { kind: name, field: arg.field };
    }
    case 'count':
      if (arg !== null) throw err('CountTakesNoArguments', 'count takes no arguments');
      return { kind: 'count' };
    default:
      throw err('UnknownFunction', `unknown function ${name}`);
  }
}

/**
 * Compiles a selector into the AST the matcher walks.
 *
 * Throws a {@link MoqtailError} whose message matches the corresponding
 * `moqtail_core::Error` variant. Pure syntax failures carry kind `"Pest"`; the
 * message text there is this port's own, since pest's rendered diagnostics are
 * not reproducible outside pest.
 */
export function compile(input) {
  const p = new Parser(input);
  const steps = [];
  for (;;) {
    const step = p.pathSegment();
    if (step === null) break;
    steps.push(step);
  }
  if (steps.length === 0) {
    throw err('Pest', `expected a selector to start with "/" or "//"`, 0);
  }
  const stages = [];
  for (;;) {
    const st = p.stage();
    if (st === null) break;
    stages.push(st);
  }
  if (!p.atEnd()) {
    throw err('Pest', `unexpected input at offset ${p.i}: ${JSON.stringify(p.s.slice(p.i, p.i + 12))}`, p.i);
  }
  return { steps, stages };
}

// ---------------------------------------------------------------------------
// Display — the `impl fmt::Display for Selector` in ast.rs
// ---------------------------------------------------------------------------

const displayField = (f) => (f.kind === 'header' ? f.name : `json$${f.path.map((p) => `.${p}`).join('')}`);

const displayValue = (v) => {
  // Rust's `f64: Display` and JS's `Number::toString` agree over the range the
  // `number` rule can produce: no exponent, at most one decimal point.
  if (v.kind === 'number') return String(v.value);
  if (v.kind === 'bool') return String(v.value);
  return JSON.stringify(v.value);
};

const displaySegment = (s) =>
  s.kind === 'literal' ? s.name : s.kind === 'plus' ? '+' : s.kind === 'hash' ? '#' : 'msg';

/** Renders a compiled selector back to its canonical source form. */
export function display(selector) {
  let out = '';
  for (const step of selector.steps) {
    out += step.axis === 'child' ? '/' : '//';
    out += displaySegment(step.segment);
    for (const pred of step.predicates) {
      out += `[${displayField(pred.field)}${pred.op}${displayValue(pred.value)}]`;
    }
  }
  for (const stage of selector.stages) {
    if (stage.kind === 'window') out += ` |> window(${Math.floor(stage.ms / 1000)}s)`;
    else if (stage.kind === 'sum') out += ` |> sum(${displayField(stage.field)})`;
    else if (stage.kind === 'avg') out += ` |> avg(${displayField(stage.field)})`;
    else out += ' |> count()';
  }
  return out;
}

// ---------------------------------------------------------------------------
// Matcher — the port of matcher.rs
// ---------------------------------------------------------------------------

/** `serde_json::Value::get(&str)`: object keys only, never array indices. */
function jsonPath(root, path) {
  let cur = root;
  for (const part of path) {
    if (cur === null || typeof cur !== 'object' || Array.isArray(cur)) return undefined;
    if (!Object.hasOwn(cur, part)) return undefined;
    cur = cur[part];
  }
  return cur;
}

/** Rust's `<f64 as FromStr>` grammar — deliberately stricter than parseFloat. */
const RUST_F64 = /^[+-]?(?:inf(?:inity)?|nan|(?:\d+\.?\d*|\.\d+)(?:[eE][+-]?\d+)?)$/i;

function parseRustF64(text) {
  if (!RUST_F64.test(text)) return undefined;
  const v = Number(text.replace(/^([+-]?)inf(inity)?$/i, '$1Infinity'));
  return Number.isNaN(v) && !/nan/i.test(text) ? undefined : v;
}

/** `f64::total_cmp`, which orders -0.0 below +0.0. NaN is filtered upstream. */
function totalCmp(a, b) {
  if (a < b) return -1;
  if (a > b) return 1;
  const an = Object.is(a, -0);
  const bn = Object.is(b, -0);
  if (an && !bn) return -1;
  if (!an && bn) return 1;
  return 0;
}

function compareNumbers(l, r, op) {
  if (Number.isNaN(l) || Number.isNaN(r)) return false;
  if (!Number.isFinite(l) || !Number.isFinite(r)) {
    switch (op) {
      case '=':
        return l === r;
      case '<':
        return l < r;
      case '>':
        return l > r;
      case '<=':
        return l <= r;
      default:
        return l >= r;
    }
  }
  const diff = Math.abs(l - r);
  const scale = Math.max(Math.abs(l), Math.abs(r));
  const tol = Math.max(ABS_EPS, REL_EPS * scale);
  const eq = diff <= tol;
  const ord = totalCmp(l, r);
  switch (op) {
    case '=':
      return eq;
    case '<':
      return ord === -1 && !eq;
    case '>':
      return ord === 1 && !eq;
    case '<=':
      return ord !== 1 || eq;
    default:
      return ord !== -1 || eq;
  }
}

function compareValues(left, right, op) {
  if (left.kind === 'number' && right.kind === 'number') return compareNumbers(left.value, right.value, op);
  if (left.kind === 'bool' && right.kind === 'bool') return op === '=' && left.value === right.value;
  if (left.kind === 'string' && right.kind === 'string') return op === '=' && left.value === right.value;
  return false;
}

function headerOf(msg, name) {
  const h = msg.headers;
  if (h === undefined || h === null) return undefined;
  if (h instanceof Map) return h.get(name);
  return Object.hasOwn(h, name) ? h[name] : undefined;
}

function predicateMatch(pred, msg) {
  let left;
  if (pred.field.kind === 'header') {
    const raw = headerOf(msg, pred.field.name);
    if (raw === undefined) return false;
    const hv = String(raw);
    if (pred.value.kind === 'number') {
      const num = parseRustF64(hv);
      if (num === undefined) return false;
      left = { kind: 'number', value: num };
    } else if (pred.value.kind === 'bool') {
      if (hv !== 'true' && hv !== 'false') return false;
      left = { kind: 'bool', value: hv === 'true' };
    } else {
      left = { kind: 'string', value: hv };
    }
  } else {
    const payload = msg.payload;
    const cur = payload === undefined || payload === null ? undefined : jsonPath(payload, pred.field.path);
    if (cur === undefined) return false;
    if (typeof cur === 'boolean') left = { kind: 'bool', value: cur };
    else if (typeof cur === 'number') left = { kind: 'number', value: cur };
    else if (typeof cur === 'string') left = { kind: 'string', value: cur };
    else return false;
  }
  return compareValues(left, pred.value, pred.op);
}

const predicatesMatch = (preds, msg) => preds.every((p) => predicateMatch(p, msg));

/** Port of `Matcher::match_child` — expands the traversal stack for one step. */
function matchChild(stack, step, nextStep, topic, idx, visited, stride) {
  const push = (si, ti) => {
    const key = si * stride + ti;
    if (!visited.has(key)) {
      visited.add(key);
      stack.push(si, ti);
    }
  };
  switch (step.segment.kind) {
    case 'literal':
      if (idx < topic.length && topic[idx] === step.segment.name) push(nextStep, idx + 1);
      break;
    case 'plus':
      if (idx < topic.length) push(nextStep, idx + 1);
      break;
    case 'hash':
      for (let i = idx; i <= topic.length; i++) push(nextStep, i);
      break;
    default: // message: consumes no segment
      push(nextStep, idx);
  }
}

/** Port of `Matcher::match_steps` — an explicit stack, no recursion. */
function matchSteps(steps, topic, msg) {
  const stride = topic.length + 1;
  const stack = [0, 0];
  const visited = new Set([0]);
  while (stack.length > 0) {
    const topicIdx = stack.pop();
    const stepIdx = stack.pop();
    if (stepIdx === steps.length) {
      if (topicIdx === topic.length) return true;
      continue;
    }
    const step = steps[stepIdx];
    if (!predicatesMatch(step.predicates, msg)) continue;
    if (step.axis === 'child') {
      matchChild(stack, step, stepIdx + 1, topic, topicIdx, visited, stride);
    } else {
      for (let start = topicIdx; start <= topic.length; start++) {
        matchChild(stack, step, stepIdx + 1, topic, start, visited, stride);
      }
    }
  }
  return false;
}

function extractField(field, msg) {
  if (field.kind === 'header') {
    const raw = headerOf(msg, field.name);
    if (raw === undefined) return undefined;
    return parseRustF64(String(raw));
  }
  if (msg.payload === undefined || msg.payload === null) return undefined;
  const v = jsonPath(msg.payload, field.path);
  return typeof v === 'number' ? v : undefined;
}

/**
 * A compiled selector plus the per-subscription state the pipeline stages keep.
 *
 * `matches` is stateless; `process` advances the window state and returns the
 * value of the last stage, exactly as `Matcher::process` does.
 */
export class Matcher {
  constructor(selector) {
    this.selector = selector;
    this.stageStates = [];
    let windowMs = null;
    for (const stage of selector.stages) {
      if (stage.kind === 'window') {
        windowMs = stage.ms;
      } else if (stage.kind === 'sum' || stage.kind === 'avg') {
        this.stageStates.push({ kind: 'window', ms: windowMs, values: [], head: 0, sum: 0 });
      } else {
        this.stageStates.push({ kind: 'counter', ms: windowMs, times: [], head: 0 });
      }
    }
  }

  /** True when `msg` satisfies every step and predicate of the selector. */
  matches(msg) {
    const topic = msg.topic === '' ? [] : msg.topic.split('/');
    return matchSteps(this.selector.steps, topic, msg);
  }

  /** Runs the pipeline stages for a matching message; `now` is milliseconds. */
  process(msg, now) {
    if (!this.matches(msg)) return undefined;
    let result;
    let idx = 0;
    for (const stage of this.selector.stages) {
      if (stage.kind === 'window') continue;
      const state = this.stageStates[idx];
      idx++;
      if (stage.kind === 'sum' || stage.kind === 'avg') {
        const v = extractField(stage.field, msg);
        if (v === undefined) return undefined;
        if (state.ms === null) {
          state.values = [];
          state.head = 0;
          state.values.push([now, v]);
          state.sum = v;
          result = v;
        } else {
          state.values.push([now, v]);
          state.sum += v;
          while (state.head < state.values.length) {
            const ts = state.values[state.head][0];
            if (Math.max(0, now - ts) > state.ms) {
              state.sum -= state.values[state.head][1];
              state.head++;
            } else break;
          }
          if (state.head > 64 && state.head * 2 > state.values.length) {
            state.values = state.values.slice(state.head);
            state.head = 0;
          }
          const len = state.values.length - state.head;
          result = stage.kind === 'sum' ? state.sum : len === 0 ? 0 : state.sum / len;
        }
      } else {
        if (state.ms === null) {
          state.times = [now];
          state.head = 0;
          result = 1;
        } else {
          state.times.push(now);
          while (state.head < state.times.length && Math.max(0, now - state.times[state.head]) > state.ms) {
            state.head++;
          }
          if (state.head > 64 && state.head * 2 > state.times.length) {
            state.times = state.times.slice(state.head);
            state.head = 0;
          }
          result = state.times.length - state.head;
        }
      }
    }
    return result;
  }
}

/** Convenience: compile and wrap in one call. */
export const matcherFor = (source) => new Matcher(compile(source));
