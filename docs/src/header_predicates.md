# Header Predicates

MoQTail selectors can filter on MQTT message headers and properties using the `/msg` axis. Predicates inside `[]` compare metadata fields such as QoS or whether the message was retained.

## Example

```bash
$ moqtail sub "/msg[qos<=1][retained=true]//sensor"
```

This subscription matches any retained sensor message published with QoS 0 or 1.

Common header names:

- `qos` – Quality of Service level (0, 1 or 2)
- `retained` – boolean retained flag
- `dup` – duplicate delivery flag
- `prop.<name>` – MQTT v5 user properties

