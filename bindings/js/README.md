# JavaScript bindings for MoQTail

These bindings expose `moqtail-core` to Node.js using [napi-rs](https://napi.rs/).

## Building

1. Install the `napi` CLI:
   ```bash
   npm install -g @napi-rs/cli
   ```
2. Build the addon (runs automatically on `npm install`):
   ```bash
   npm install
   ```

The resulting `moqtail-js.node` binary will be placed next to `index.js` and can be required as:

```javascript
const { compile } = require('moqtail-js');
```
