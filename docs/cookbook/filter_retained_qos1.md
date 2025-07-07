# Filtering Retained QoS≤1 Messages

This recipe demonstrates how to subscribe only to messages that were published with the `retained` flag and with a Quality of Service level of 1 or lower.

## Steps

1. **Start an MQTT broker** (Mosquitto in this example):

   ```bash
   $ mosquitto -c mosquitto.conf
   ```

2. **Publish a retained message** with QoS 1:

   ```bash
   $ mosquitto_pub -t sensors/temp -m '{"value":42}' -r -q 1
   ```

3. **Subscribe using MoQTail** to filter by `retained` and `QoS`:

   ```bash
   $ moqtail sub "/msg[retained=true][qos<=1]//sensors"
   ```

Any retained message with QoS less than or equal to 1 that matches the selector will be delivered to the subscriber.

