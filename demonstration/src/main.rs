mod matrix;
mod message;
mod root;
mod worker;

use mpi::traits::*;

fn main() {
    let universe = mpi::initialize().expect("failed to initialize mpi");
    let world = universe.world();

    // Rank 0 drives the demo (the "root"); everyone else is a "worker".
    match world.rank() {
        0 => root::root(&world),
        _ => worker::worker(&world),
    }
}
