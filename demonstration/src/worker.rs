use mpi::traits::*;

use crate::matrix::matmul;
use crate::message::{ResultBlock, Task};

pub fn worker<C: Communicator>(world: &C) {
    let root = world.process_at_rank(0);

    // Receive the packed Task and unpack it back into a struct.
    let (bytes, _status) = root.receive_vec::<u8>();
    let task = Task::unpack(world, &bytes);

    let rows = task.rows as usize;
    let k = task.k as usize;
    let n = task.n as usize;

    // Multiply the local row-block: (rows x k) * (k x n) -> (rows x n).
    let c_block = matmul(&task.a_block, &task.b, rows, k, n);

    // Pack the block (carrying its absolute row offset) and send it back.
    let result = ResultBlock {
        start: task.start,
        rows: task.rows,
        n: task.n,
        c_block,
    };
    let packed = result.pack(world);
    root.send(&packed[..]);
}
