//! Run with `cargo run --release -p morf-scene --example reactive_bench`.
//! Measures a panel-wide signal update with increasing numbers of bindings.

use std::time::Instant;

use morf_scene::reactive::Graph;

fn main() {
    for count in [128, 1024, 4096] {
        let mut graph = Graph::default();
        let input = graph.signal("panel", 0_u32);
        for token in 0..count {
            graph.external_effect(format!("binding {token}"), token as u64);
        }
        let evaluate = |_: u64, context: &mut morf_scene::reactive::EffectContext<'_, u32>| {
            context.get(input).map(|_| ()).map_err(|e| e.to_string())
        };
        graph.flush_external(evaluate).unwrap();
        let started = Instant::now();
        for value in 1..=30 {
            graph.write(input, value).unwrap();
            assert_eq!(graph.flush_external(evaluate).unwrap().runs, count);
        }
        println!(
            "{count:4} bindings: {:?} per update",
            started.elapsed() / 30
        );
    }
}
