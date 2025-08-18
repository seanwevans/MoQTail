# Pipeline Stages

Selectors can transform and aggregate matched messages using a Unix-like pipeline syntax. Each stage is appended with `|>` and operates on the output of the previous stage.

## `window(duration)`

Groups messages into tumbling time windows. The duration is specified in seconds.

```bash
$ moqtail sub "//sensor |> window(60s)"
```

## `sum(field)`

Adds the numeric values of the given field across all messages in the current window.

```bash
$ moqtail sub "//sensor |> window(60s) |> sum(json$.value)"
```

## `avg(field)`

Computes the average of a numeric field across messages in the window.

```bash
$ moqtail sub "//sensor |> window(60s) |> avg(json$.value)"
```

## `count()`

Counts the number of messages in the window.

```bash
$ moqtail sub "//sensor |> window(5m) |> count()"
```

## Chaining Stages

Stages can be chained to build multi-step analytics.

```bash
$ moqtail sub "//sensor |> window(30s) |> avg(json$.value) |> window(5m) |> count()"
```

This subscription computes 30‑second averages of sensor values and then counts how many such averages fall within each five‑minute window.
