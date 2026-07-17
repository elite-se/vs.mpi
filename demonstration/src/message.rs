//! Messages exchanged between the root and the workers.
//!
//! Both structs are serialized by hand with `MPI_Pack`: their heterogeneous
//! fields (the `i32` dimensions and the variable-length `f64` matrix data) are
//! packed one after another into a single message buffer. Because a worker's
//! block has a variable number of rows in *both* directions, packing lets each
//! message carry exactly its block and nothing more.

use mpi::traits::*;
use mpi::Count;

// Problem size: A is (M x K), B is (K x N), so C = A * B is (M x N).
pub const M: usize = 6; // rows of A / C
pub const K: usize = 4; // cols of A / rows of B
pub const N: usize = 5; // cols of B / C

/// A unit of work the root sends to a worker, packed into one message.
pub struct Task {
    pub start: i32,        // absolute first row of A / C this worker handles
    pub rows: i32,         // rows in this worker's block
    pub k: i32,            // shared inner dimension
    pub n: i32,            // columns of B / C
    pub a_block: Vec<f64>, // this worker's `rows * k` row-block of A
    pub b: Vec<f64>,       // the full `k * n` matrix B
}

impl Task {
    /// Pack the struct into one message buffer using `MPI_Pack`.
    pub fn pack<C: Communicator>(&self, world: &C) -> Vec<u8> {
        let int_size = world.pack_size(4, &i32::equivalent_datatype());
        let a_size = world.pack_size(self.a_block.len() as Count, &f64::equivalent_datatype());
        let b_size = world.pack_size(self.b.len() as Count, &f64::equivalent_datatype());
        let mut buf = vec![0u8; (int_size + a_size + b_size) as usize];

        let mut pos = 0;
        pos = world.pack_into(&self.start, &mut buf[..], pos);
        pos = world.pack_into(&self.rows, &mut buf[..], pos);
        pos = world.pack_into(&self.k, &mut buf[..], pos);
        pos = world.pack_into(&self.n, &mut buf[..], pos);
        pos = world.pack_into(&self.a_block[..], &mut buf[..], pos);
        world.pack_into(&self.b[..], &mut buf[..], pos);
        buf
    }

    /// Unpack a message buffer produced by [`Task::pack`], reading the fields in
    /// the same order they were packed.
    pub fn unpack<C: Communicator>(world: &C, bytes: &[u8]) -> Task {
        let (mut start, mut rows, mut k, mut n) = (0i32, 0i32, 0i32, 0i32);
        let mut pos = 0;
        // Safe: fields are unpacked in the exact order/sizes the root packed them.
        unsafe {
            pos = world.unpack_into(bytes, &mut start, pos);
            pos = world.unpack_into(bytes, &mut rows, pos);
            pos = world.unpack_into(bytes, &mut k, pos);
            pos = world.unpack_into(bytes, &mut n, pos);
            let mut a_block = vec![0.0f64; (rows * k) as usize];
            pos = world.unpack_into(bytes, &mut a_block[..], pos);
            let mut b = vec![0.0f64; (k * n) as usize];
            world.unpack_into(bytes, &mut b[..], pos);
            Task { start, rows, k, n, a_block, b }
        }
    }
}

/// A worker's computed contribution to the result matrix C, packed into one
/// message. It carries only this worker's `rows * n` block of C.
pub struct ResultBlock {
    pub start: i32,        // absolute first row this block fills
    pub rows: i32,         // number of rows in the block
    pub n: i32,            // columns of C
    pub c_block: Vec<f64>, // this worker's `rows * n` block of C
}

impl ResultBlock {
    /// Pack the struct into one message buffer using `MPI_Pack`.
    pub fn pack<C: Communicator>(&self, world: &C) -> Vec<u8> {
        let int_size = world.pack_size(3, &i32::equivalent_datatype());
        let c_size = world.pack_size(self.c_block.len() as Count, &f64::equivalent_datatype());
        let mut buf = vec![0u8; (int_size + c_size) as usize];

        let mut pos = 0;
        pos = world.pack_into(&self.start, &mut buf[..], pos);
        pos = world.pack_into(&self.rows, &mut buf[..], pos);
        pos = world.pack_into(&self.n, &mut buf[..], pos);
        world.pack_into(&self.c_block[..], &mut buf[..], pos);
        buf
    }

    /// Unpack a message buffer produced by [`ResultBlock::pack`].
    pub fn unpack<C: Communicator>(world: &C, bytes: &[u8]) -> ResultBlock {
        let (mut start, mut rows, mut n) = (0i32, 0i32, 0i32);
        let mut pos = 0;
        // Safe: fields are unpacked in the exact order/sizes the worker packed them.
        unsafe {
            pos = world.unpack_into(bytes, &mut start, pos);
            pos = world.unpack_into(bytes, &mut rows, pos);
            pos = world.unpack_into(bytes, &mut n, pos);
            let mut c_block = vec![0.0f64; (rows * n) as usize];
            world.unpack_into(bytes, &mut c_block[..], pos);
            ResultBlock { start, rows, n, c_block }
        }
    }
}
