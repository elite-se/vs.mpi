//! Small row-major dense-matrix helpers shared by the root and the workers.

/// Multiply a `rows x k` matrix by a `k x n` matrix, both stored row-major in a
/// flat slice, returning the `rows x n` product (also row-major).
pub fn matmul(a: &[f64], b: &[f64], rows: usize, k: usize, n: usize) -> Vec<f64> {
    let mut c = vec![0.0; rows * n];
    for i in 0..rows {
        for j in 0..n {
            let mut sum = 0.0;
            for p in 0..k {
                sum += a[i * k + p] * b[p * n + j];
            }
            c[i * n + j] = sum;
        }
    }
    c
}

/// Contiguous row range `[start, end)` assigned to `rank` when `m` rows are
/// spread across `size` processes as evenly as possible (the first `m % size`
/// ranks each get one extra row).
pub fn row_range(m: usize, size: usize, rank: usize) -> (usize, usize) {
    let base = m / size;
    let rem = m % size;
    let start = rank * base + rank.min(rem);
    let count = base + if rank < rem { 1 } else { 0 };
    (start, start + count)
}

/// Pretty-print a row-major matrix to stdout.
pub fn print_matrix(name: &str, data: &[f64], rows: usize, cols: usize) {
    println!("{name} ({rows}x{cols}):");
    for i in 0..rows {
        print!("  ");
        for j in 0..cols {
            print!("{:9.1}", data[i * cols + j]);
        }
        println!();
    }
}
