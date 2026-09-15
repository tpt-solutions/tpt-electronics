// SPDX-License-Identifier: MIT OR Apache-2.0

//! Dense and sparse linear algebra with iterative solvers.
//!
//! `tpt-electronics` keeps its numeric stack in-tree so the whole engine stays
//! dependency-free and `unsafe`-free.
//!
//! * [`DenseMatrix`] — small dense systems (MNA, 2×2 Kalman filters)
//! * [`SparseMatrix`] — CSR matrices for FEM systems (thermal conduction)
//! * [`conjugate_gradient`] — CG for symmetric positive-definite systems

/// Row-major dense matrix.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct DenseMatrix {
    rows: usize,
    cols: usize,
    data: Vec<f64>,
}

impl DenseMatrix {
    /// An `n × m` zero matrix.
    pub fn zeros(n: usize, m: usize) -> Self {
        Self {
            rows: n,
            cols: m,
            data: vec![0.0; n * m],
        }
    }

    /// An `n × n` identity matrix.
    pub fn identity(n: usize) -> Self {
        let mut a = Self::zeros(n, n);
        for i in 0..n {
            a.set(i, i, 1.0);
        }
        a
    }

    /// Number of rows.
    pub fn rows(&self) -> usize {
        self.rows
    }

    /// Number of columns.
    pub fn cols(&self) -> usize {
        self.cols
    }

    /// Element access.
    pub fn at(&self, i: usize, j: usize) -> f64 {
        self.data[i * self.cols + j]
    }

    /// Element mutation.
    pub fn set(&mut self, i: usize, j: usize, v: f64) {
        self.data[i * self.cols + j] = v;
    }

    /// Adds `v` to an element.
    pub fn add(&mut self, i: usize, j: usize, v: f64) {
        self.data[i * self.cols + j] += v;
    }

    /// Matrix–vector product `y = A·x`.
    pub fn mul_vec(&self, x: &[f64]) -> Vec<f64> {
        assert_eq!(self.cols, x.len(), "dimension mismatch");
        let mut y = vec![0.0; self.rows];
        for (i, yi) in y.iter_mut().enumerate() {
            let row = &self.data[i * self.cols..(i + 1) * self.cols];
            *yi = row.iter().zip(x).map(|(a, b)| a * b).sum();
        }
        y
    }

    /// Solves `A·x = b` by Gaussian elimination with partial pivoting.
    ///
    /// Returns `None` if the matrix is singular.
    pub fn solve(&self, b: &[f64]) -> Option<Vec<f64>> {
        assert_eq!(self.rows, self.cols, "solve requires a square matrix");
        let n = self.rows;
        assert_eq!(b.len(), n);
        let mut a = self.data.clone();
        let mut x = b.to_vec();

        for col in 0..n {
            // Partial pivot
            let pivot = (col..n)
                .max_by(|&r1, &r2| a[r1 * n + col].abs().total_cmp(&a[r2 * n + col].abs()))
                .unwrap_or(col);
            if a[pivot * n + col].abs() < 1e-300 {
                return None;
            }
            if pivot != col {
                for j in 0..n {
                    a.swap(col * n + j, pivot * n + j);
                }
                x.swap(col, pivot);
            }
            let d = a[col * n + col];
            for row in (col + 1)..n {
                let factor = a[row * n + col] / d;
                if factor != 0.0 {
                    for j in col..n {
                        a[row * n + j] -= factor * a[col * n + j];
                    }
                    x[row] -= factor * x[col];
                }
            }
        }

        // Back substitution
        for row in (0..n).rev() {
            let mut sum = x[row];
            for j in (row + 1)..n {
                sum -= a[row * n + j] * x[j];
            }
            x[row] = sum / a[row * n + row];
        }
        Some(x)
    }
}

/// Build side for [`SparseMatrix`]: collects (i, j, v) triplets and merges
/// duplicates by summation.
#[derive(Clone, Debug, Default)]
pub struct SparseBuilder {
    n: usize,
    triplets: Vec<(usize, usize, f64)>,
}

impl SparseBuilder {
    /// A builder for an `n × n` matrix.
    pub fn new(n: usize) -> Self {
        Self {
            n,
            triplets: Vec::new(),
        }
    }

    /// Adds (sums into) an entry.
    pub fn add(&mut self, i: usize, j: usize, v: f64) {
        assert!(i < self.n && j < self.n);
        self.triplets.push((i, j, v));
    }

    /// Consumes the builder into a CSR matrix.
    pub fn build(self) -> SparseMatrix {
        let n = self.n;
        let mut triplets = self.triplets;
        triplets.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)));

        let mut indptr = vec![0usize; n + 1];
        let mut indices = Vec::with_capacity(triplets.len());
        let mut values = Vec::with_capacity(triplets.len());
        let mut cur_row = 0usize;
        for &(i, j, v) in &triplets {
            while cur_row < i {
                cur_row += 1;
                indptr[cur_row] = indices.len();
            }
            if let (Some(last_j), Some(last_v)) = (indices.last().copied(), values.last_mut()) {
                if last_j == j && indptr[i] < indices.len() {
                    // duplicate in same row: merge
                    *last_v += v;
                    continue;
                }
            }
            indices.push(j);
            values.push(v);
        }
        for entry in &mut indptr[cur_row + 1..=n] {
            *entry = indices.len();
        }
        SparseMatrix {
            n,
            indptr,
            indices,
            values,
        }
    }
}

/// Compressed sparse row (CSR) matrix.
#[derive(Clone, Debug, PartialEq)]
pub struct SparseMatrix {
    /// Dimension (square matrix).
    pub n: usize,
    /// Row pointers, length `n + 1`.
    pub indptr: Vec<usize>,
    /// Column indices, length `nnz`.
    pub indices: Vec<usize>,
    /// Nonzero values, length `nnz`.
    pub values: Vec<f64>,
}

impl SparseMatrix {
    /// Number of stored nonzeros.
    pub fn nnz(&self) -> usize {
        self.values.len()
    }

    /// Matrix–vector product `y = A·x`.
    pub fn mul_vec(&self, x: &[f64]) -> Vec<f64> {
        assert_eq!(x.len(), self.n);
        let mut y = vec![0.0; self.n];
        for (i, yi) in y.iter_mut().enumerate() {
            let mut s = 0.0;
            for k in self.indptr[i]..self.indptr[i + 1] {
                s += self.values[k] * x[self.indices[k]];
            }
            *yi = s;
        }
        y
    }

    /// Solves `A·x = b` with the Conjugate Gradient method.
    ///
    /// `A` must be symmetric positive definite (true for assembled FEM
    /// conductivity/capacitance matrices once boundary conditions are applied).
    /// Returns the solution and the achieved relative residual.
    pub fn solve_cg(&self, b: &[f64], tol: f64, max_iter: usize) -> Option<(Vec<f64>, f64)> {
        conjugate_gradient(self, b, tol, max_iter)
    }
}

/// Dot product.
pub fn dot(x: &[f64], y: &[f64]) -> f64 {
    x.iter().zip(y).map(|(a, b)| a * b).sum()
}

/// Euclidean norm.
pub fn norm2(x: &[f64]) -> f64 {
    dot(x, x).sqrt()
}

/// Conjugate Gradient solver for `A·x = b` with `A` symmetric positive
/// definite. Returns `(x, relative_residual)`.
pub fn conjugate_gradient<A>(a: &A, b: &[f64], tol: f64, max_iter: usize) -> Option<(Vec<f64>, f64)>
where
    A: SpdOperator,
{
    let n = b.len();
    let mut x = vec![0.0; n];
    let b_norm = norm2(b);
    if b_norm == 0.0 {
        return Some((x, 0.0));
    }
    let mut r = b.to_vec();
    let mut p = r.clone();
    let mut rs_old = dot(&r, &r);

    for _ in 0..max_iter {
        let ap = a.apply(&p);
        let p_ap = dot(&p, &ap);
        if p_ap <= 0.0 || !p_ap.is_finite() {
            return None; // not positive definite / divergence
        }
        let alpha = rs_old / p_ap;
        for i in 0..n {
            x[i] += alpha * p[i];
            r[i] -= alpha * ap[i];
        }
        let rs_new = dot(&r, &r);
        let rel = rs_new.sqrt() / b_norm;
        if rel < tol {
            return Some((x, rel));
        }
        let beta = rs_new / rs_old;
        for i in 0..n {
            p[i] = r[i] + beta * p[i];
        }
        rs_old = rs_new;
    }
    let rel = rs_old.sqrt() / b_norm;
    if rel.is_finite() {
        Some((x, rel))
    } else {
        None
    }
}

/// Abstraction over operators usable by [`conjugate_gradient`].
pub trait SpdOperator {
    /// Computes `y = A·x`.
    fn apply(&self, x: &[f64]) -> Vec<f64>;
}

impl SpdOperator for SparseMatrix {
    fn apply(&self, x: &[f64]) -> Vec<f64> {
        self.mul_vec(x)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn approx(a: f64, b: f64, eps: f64) -> bool {
        (a - b).abs() <= eps
    }

    #[test]
    fn dense_solve_2x2() {
        // [4 1; 1 3] x = [1 2]  =>  det = 11, x = [1/11, 7/11]
        let mut a = DenseMatrix::zeros(2, 2);
        a.set(0, 0, 4.0);
        a.set(0, 1, 1.0);
        a.set(1, 0, 1.0);
        a.set(1, 1, 3.0);
        let x = a.solve(&[1.0, 2.0]).unwrap();
        assert!(approx(x[0], 1.0 / 11.0, 1e-14));
        assert!(approx(x[1], 7.0 / 11.0, 1e-14));
    }

    #[test]
    fn dense_solve_pivoting() {
        // Requires a row swap; zero in the top-left.
        let mut a = DenseMatrix::zeros(2, 2);
        a.set(0, 0, 0.0);
        a.set(0, 1, 1.0);
        a.set(1, 0, 1.0);
        a.set(1, 1, 0.0);
        let x = a.solve(&[2.0, 3.0]).unwrap();
        assert!(approx(x[0], 3.0, 1e-14));
        assert!(approx(x[1], 2.0, 1e-14));
    }

    #[test]
    fn sparse_builder_merges_duplicates() {
        let mut b = SparseBuilder::new(2);
        b.add(0, 0, 2.0);
        b.add(0, 0, 3.0); // merges to 5
        b.add(1, 1, 4.0);
        let m = b.build();
        assert_eq!(m.nnz(), 2);
        let y = m.mul_vec(&[1.0, 1.0]);
        assert!(approx(y[0], 5.0, 1e-14));
        assert!(approx(y[1], 4.0, 1e-14));
    }

    #[test]
    fn cg_solves_poisson_1d() {
        // 1D Laplacian: -u'' = f, u(0)=u(1)=0, N interior nodes.
        // With f = 1, u_i = x_i(1-x_i)/2.
        let n = 40;
        let h = 1.0 / (n as f64 + 1.0);
        let mut builder = SparseBuilder::new(n);
        for i in 0..n {
            builder.add(i, i, 2.0);
            if i > 0 {
                builder.add(i, i - 1, -1.0);
            }
            if i + 1 < n {
                builder.add(i, i + 1, -1.0);
            }
        }
        let a = builder.build();
        let f: Vec<f64> = vec![h * h; n];
        let (u, rel) = a.solve_cg(&f, 1e-12, 200).unwrap();
        assert!(rel < 1e-10);
        for (i, &ui) in u.iter().enumerate() {
            let x = (i as f64 + 1.0) * h;
            let expected = x * (1.0 - x) / 2.0;
            assert!(
                (ui - expected).abs() < 1e-9,
                "u[{i}] = {ui}, expected {expected}"
            );
        }
    }

    #[test]
    fn cg_rejects_non_spd() {
        // Matrix with a non-positive curvature direction.
        let mut b = SparseBuilder::new(2);
        b.add(0, 0, 1.0);
        b.add(1, 1, -1.0);
        let m = b.build();
        assert!(m.solve_cg(&[1.0, 1.0], 1e-10, 50).is_none());
    }

    #[test]
    fn cg_zero_rhs_returns_zero() {
        let mut b = SparseBuilder::new(2);
        b.add(0, 0, 2.0);
        b.add(1, 1, 2.0);
        let m = b.build();
        let (x, rel) = m.solve_cg(&[0.0, 0.0], 1e-10, 10).unwrap();
        assert!(x.iter().all(|&v| v == 0.0) && rel == 0.0);
    }
}
