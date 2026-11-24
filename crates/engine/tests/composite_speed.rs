//! Temporary bench (never in a PR): the sample ball's full composite and showing its rusted folder, whose masks are generator stacks.

use std::time::Instant;

use serde_json::json;
use texturecraft_engine::{Session, execute, sample_session};

fn settle(session: &mut Session) -> f64 {
    let start = Instant::now();
    while session.pending_tiles() > 0 {
        session.update();
    }
    start.elapsed().as_secs_f64()
}

#[test]
#[ignore]
fn bench_generator_masks() {
    let mut session = sample_session(2048, 1024).unwrap();
    println!("RESULT initial composite {:.3}s", settle(&mut session));
    let project = &session.doc.project;
    let (set, folder) = project
        .texture_sets
        .iter()
        .enumerate()
        .find_map(|(index, set)| set.stack.iter().copied().find(|id| project.layers.get(id).is_some_and(|layer| layer.name.contains("Rusted"))).map(|id| (index, id)))
        .expect("rusted folder");
    session.active_set = set;
    for round in 1..=3 {
        execute(&mut session, "layer.visible", &json!({"layer": folder.0})).unwrap();
        settle(&mut session);
        execute(&mut session, "layer.visible", &json!({"layer": folder.0})).unwrap();
        println!("RESULT show {round}: {:.3}s", settle(&mut session));
    }
}
