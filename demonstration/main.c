#include <mpi.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>

/* Problem size: A is (M x K), B is (K x N), so C = A * B is (M x N). */
#define M 6
#define K 4
#define N 5

/* Contiguous row range [*start, *end) for `rank` when distributing `m` rows
   across `size` processes as evenly as possible. */
static void row_range(int m, int size, int rank, int *start, int *end)
{
    int base = m / size;
    int rem  = m % size;
    *start = rank * base + (rank < rem ? rank : rem);
    *end   = *start + base + (rank < rem ? 1 : 0);
}

/* Multiply a (rows x k) matrix by a (k x n) matrix, both row-major.
   Result is written into the pre-allocated (rows x n) buffer c. */
static void matmul(const double *a, const double *b,
                   int rows, int k, int n, double *c)
{
    memset(c, 0, sizeof(double) * rows * n);
    for (int i = 0; i < rows; i++)
        for (int j = 0; j < n; j++) {
            double sum = 0.0;
            for (int p = 0; p < k; p++)
                sum += a[i * k + p] * b[p * n + j];
            c[i * n + j] = sum;
        }
}

static void print_matrix(const char *name, const double *data, int rows, int cols)
{
    printf("%s (%dx%d):\n", name, rows, cols);
    for (int i = 0; i < rows; i++) {
        printf("  ");
        for (int j = 0; j < cols; j++)
            printf("%9.1f", data[i * cols + j]);
        printf("\n");
    }
}

int main(int argc, char **argv)
{
    MPI_Init(&argc, &argv);

    int rank, size;
    MPI_Comm_rank(MPI_COMM_WORLD, &rank);
    MPI_Comm_size(MPI_COMM_WORLD, &size);

    /* Every rank logs to its own /tmp/demo.log so `tail -f` can follow output. */
    freopen("/tmp/demo.log", "a", stdout);
    setvbuf(stdout, NULL, _IOLBF, 0); /* line-buffered: each printf flushes immediately */

    /* Print some debug information */
    char hostname[256];
    gethostname(hostname, sizeof(hostname));
    printf("=== Rank %d / %d  (%s) ===\n\n", rank, size, hostname);

    double* a = NULL;
    double b[K * N];

    if (rank == 0) {
        printf("MPI world size: %d\n\n", size);

        a = malloc(M * K * sizeof(double));
        for (int i = 0; i < M * K; i++) a[i] = (double)(i + 1);
        for (int i = 0; i < K * N; i++) b[i] = (double)(i + 1);

        print_matrix("A", a, M, K);
        print_matrix("B", b, K, N);
    }

    /* Broadcast B to every process. (collective communnication) */
    MPI_Bcast(b, K * N, MPI_DOUBLE, 0, MPI_COMM_WORLD);

    /* Build scatter counts/displacements for A (elements = rows * K). */
    int* sendcounts = malloc(size * sizeof(int));
    int* sdispls    = malloc(size * sizeof(int));
    for (int r = 0; r < size; r++) {
        int s, e;
        row_range(M, size, r, &s, &e);
        sendcounts[r] = (e - s) * K;
        sdispls[r]    = s * K;
    }

    /* Receive this process's row-block of A. */
    int my_start, my_end;
    row_range(M, size, rank, &my_start, &my_end);
    int local_rows = my_end - my_start;

    double* local_a = malloc(local_rows * K * sizeof(double));
    MPI_Scatterv(a, sendcounts, sdispls, MPI_DOUBLE,
                 local_a, local_rows * K, MPI_DOUBLE,
                 0, MPI_COMM_WORLD);

    /* Workers log their received row-block of A. */
    if (rank != 0) {
        printf("Rank %d: rows [%d, %d)\n", rank, my_start, my_end);
        print_matrix("local A", local_a, local_rows, K);
    }

    /* Compute this process's block of C. */
    double *local_c = malloc(local_rows * N * sizeof(double));
    matmul(local_a, b, local_rows, K, N, local_c);

    /* Workers log their computed block of C. */
    if (rank != 0) {
        printf("\n");
        print_matrix("local C", local_c, local_rows, N);
    }

    /* Build gather counts/displacements for C (elements = rows * N). */
    int* recvcounts = malloc(size * sizeof(int));
    int* rdispls    = malloc(size * sizeof(int));
    for (int r = 0; r < size; r++) {
        int s, e;
        row_range(M, size, r, &s, &e);
        recvcounts[r] = (e - s) * N;
        rdispls[r]    = s * N;
    }

    /* Gather every process's C block back to root. */
    double* c = NULL;
    if (rank == 0) c = malloc(M * N * sizeof(double));

    MPI_Gatherv(local_c, local_rows * N, MPI_DOUBLE,
                c, recvcounts, rdispls, MPI_DOUBLE,
                0, MPI_COMM_WORLD);

    if (rank == 0) {
        printf("\n");
        print_matrix("C = A * B", c, M, N);
        free(a);
        free(c);
    }

    free(local_a);
    free(local_c);
    free(sendcounts);
    free(sdispls);
    free(recvcounts);
    free(rdispls);

    MPI_Finalize();
    return 0;
}
