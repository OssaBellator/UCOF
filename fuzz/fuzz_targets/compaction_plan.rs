#![no_main]

use libfuzzer_sys::fuzz_target;
use ucof_experiments::{CompactionLimits, ObjectGraph};

fuzz_target!(|data: &[u8]| {
    let node_count = data.len().min(64);
    if node_count == 0 {
        return;
    }

    let mut graph = ObjectGraph::new();
    for index in 0..node_count {
        let byte = data[index];
        let dependency_count = usize::from(byte & 0x03);
        let dependencies = (0..dependency_count)
            .map(|delta| {
                let target = (usize::from(byte >> 2) + delta) % node_count;
                u64::try_from(target + 1).expect("bounded object id")
            })
            .collect();
        graph
            .add_object(
                u64::try_from(index + 1).expect("bounded object id"),
                dependencies,
            )
            .expect("unique generated object");
    }

    let root = u64::from(data[0]) % u64::try_from(node_count).expect("node count") + 1;
    let _ = graph.plan(
        &[root],
        CompactionLimits {
            max_nodes: 64,
            max_edges: 256,
            max_depth: 64,
        },
    );
});
