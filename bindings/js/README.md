# JavaScript bindings for MoQTail

These bindings expose `moqtail-core` to Node.js using [napi-rs](https://napi.rs/).

## Building

`@napi-rs/cli` is listed under `devDependencies`. Running `npm install` pulls it in and builds the addon:

```bash
npm install
```

You can also install the CLI globally if you prefer:

```bash
npm install -g @napi-rs/cli
```

The resulting `moqtail-js.node` binary will be placed next to `index.js` and can be required as:

```javascript
const { compile } = require('moqtail-js');
```
