# Performance Benchmarks

The `benches/runtime.rs` Criterion suite measures the runtime hot paths called out in issue #81:

- graph build
- graph invoke
- transition lookup
- checkpoint save/load
- event dispatch
- state/context packing

To run it locally:

```bash
cargo bench --bench runtime
```

Measured on the current checkout:

| Benchmark | Result |
| --- | --- |
| `graph_build/32` | `9.56 us` |
| `graph_build/256` | `72.7 us` |
| `graph_invoke/32` | `24.9 us` |
| `graph_invoke/256` | `24.9 us` |
| `transition_lookup/naive` | `80.6 ns` |
| `transition_lookup/indexed` | `16.8 ns` |
| `checkpointing/save` | `1.47 us` |
| `checkpointing/load` | `1.44 us` |
| `event_dispatch/publish` | `61.2 ns` |
| `state_packing/clone` | `1.34 us` |
| `state_packing/serialize_context` | `522.8 ns` |

The transition lookup benchmark compares the previous graph-wide scan against the new adjacency-indexed lookup path. That change reduced the measured lookup time by about 4.8x on the benchmark graph.
