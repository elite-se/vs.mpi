use std::io::{self, BufRead, Write};
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use mpi::traits::*;

use crate::matrix::{matmul, print_matrix, row_range};
use crate::message::{ResultBlock, Task, K, M, N};

pub fn root<C: Communicator>(world: &C) {
    let size = world.size() as usize;

    // Poll and display the world size until the user presses Enter.
    wait_for_enter(size);

    // Build deterministic, easy-to-check input matrices.
    let a: Vec<f64> = (0..M * K).map(|i| (i + 1) as f64).collect();
    let b: Vec<f64> = (0..K * N).map(|i| (i + 1) as f64).collect();

    print_matrix("A", &a, M, K);
    print_matrix("B", &b, K, N);

    // Send each worker its row-block of A plus B, packed into one message.
    for rank in 1..size {
        let (start, end) = row_range(M, size, rank);
        let task = Task {
            start: start as i32,
            rows: (end - start) as i32,
            k: K as i32,
            n: N as i32,
            a_block: a[start * K..end * K].to_vec(),
            b: b.clone(),
        };
        let packed = task.pack(world);
        world.process_at_rank(rank as i32).send(&packed[..]);
    }

    // The root computes its own block (rank 0) directly.
    let mut c = [0.0f64; M * N];
    let (start, end) = row_range(M, size, 0);
    let local = matmul(&a[start * K..end * K], &b, end - start, K, N);
    c[start * N..end * N].copy_from_slice(&local);

    // Gather each worker's packed ResultBlock and copy its rows into place.
    for rank in 1..size {
        let (bytes, _status) = world.process_at_rank(rank as i32).receive_vec::<u8>();
        let result = ResultBlock::unpack(world, &bytes);
        let start = result.start as usize;
        let end = start + result.rows as usize;
        c[start * N..end * N].copy_from_slice(&result.c_block);
    }

    println!();
    print_matrix("C = A * B", &c, M, N);
}

/// Reprint the world size roughly twice a second until the user hits Enter.
fn wait_for_enter(size: usize) {
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        let mut line = String::new();
        let _ = io::stdin().lock().read_line(&mut line);
        let _ = tx.send(());
    });

    loop {
        print!("\rMPI running with {size} process(es). Press Enter to start the computation... ");
        let _ = io::stdout().flush();
        if rx.recv_timeout(Duration::from_millis(500)).is_ok() {
            break;
        }
    }
    println!();
}
