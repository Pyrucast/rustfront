//! The conversions MTest makes for a finite strain behaviour loaded with
//! another stress measure or tangent operator than the Cauchy stress and its
//! derivative with respect to the deformation gradient, those its algorithm
//! works with. Written from the formulas, in the global frame (they are
//! objective): with J = det F and τ = J·σ the Kirchhoff stress, the second
//! Piola-Kirchhoff stress is S = F⁻¹·τ·F⁻ᵀ, the first one P = τ·F⁻ᵀ, and
//! dσ = dτ/J − σ·tr(F⁻¹·dF).

use std::f64::consts::SQRT_2;
use std::ops::{Add, Mul, Sub};

use rustfront::{FiniteStrainTangentOperator, StressMeasure};

/// The (row, column) of the components of an unsymmetric tensor, in TFEL's
/// order `XX YY ZZ XY YX XZ ZX YZ ZY`: the first 3, 5 or 9 of them, as the
/// space dimension.
const TENSOR: [(usize, usize); 9] = [
    (0, 0),
    (1, 1),
    (2, 2),
    (0, 1),
    (1, 0),
    (0, 2),
    (2, 0),
    (1, 2),
    (2, 1),
];

/// Those of a symmetric tensor, `xx yy zz xy xz yz`: the first 3, 4 or 6,
/// TFEL storing the shear components multiplied by √2.
const STENSOR: [(usize, usize); 6] = [(0, 0), (1, 1), (2, 2), (0, 1), (0, 2), (1, 2)];

/// A tensor, as a 3×3 matrix.
#[derive(Clone, Copy)]
struct Tensor([[f64; 3]; 3]);

impl Tensor {
    /// From the values of an unsymmetric tensor, in TFEL's order.
    fn unsymmetric(values: &[f64]) -> Tensor {
        let mut m = [[0.0; 3]; 3];
        for (v, &(i, j)) in values.iter().zip(&TENSOR) {
            m[i][j] = *v;
        }
        Tensor(m)
    }

    /// From the values of a symmetric tensor, in TFEL's storage.
    fn symmetric(values: &[f64]) -> Tensor {
        let mut m = [[0.0; 3]; 3];
        for (v, &(i, j)) in values.iter().zip(&STENSOR) {
            let v = if i == j { *v } else { v / SQRT_2 };
            m[i][j] = v;
            m[j][i] = v;
        }
        Tensor(m)
    }

    /// The `size` values of the tensor, in TFEL's order.
    fn unsymmetric_values(self, size: usize) -> Vec<f64> {
        TENSOR[..size].iter().map(|&(i, j)| self.0[i][j]).collect()
    }

    /// The `size` values of the symmetric part of the tensor, in TFEL's
    /// storage.
    fn symmetric_values(self, size: usize) -> Vec<f64> {
        STENSOR[..size]
            .iter()
            .map(|&(i, j)| {
                if i == j {
                    self.0[i][i]
                } else {
                    (self.0[i][j] + self.0[j][i]) / SQRT_2
                }
            })
            .collect()
    }

    fn transpose(self) -> Tensor {
        Tensor([0, 1, 2].map(|i| [0, 1, 2].map(|j| self.0[j][i])))
    }

    /// The inverse and the determinant.
    fn inverse(self) -> (Tensor, f64) {
        let m = self.0;
        // The cofactors, transposed.
        let c = [0, 1, 2].map(|i| {
            [0, 1, 2].map(|j| {
                let (r0, r1) = ((j + 1) % 3, (j + 2) % 3);
                let (c0, c1) = ((i + 1) % 3, (i + 2) % 3);
                m[r0][c0] * m[r1][c1] - m[r0][c1] * m[r1][c0]
            })
        });
        let determinant = m[0][0] * c[0][0] + m[0][1] * c[1][0] + m[0][2] * c[2][0];
        (Tensor(c) * (1.0 / determinant), determinant)
    }
}

impl Mul for Tensor {
    type Output = Tensor;
    fn mul(self, b: Tensor) -> Tensor {
        Tensor([0, 1, 2].map(|i| [0, 1, 2].map(|j| (0..3).map(|k| self.0[i][k] * b.0[k][j]).sum())))
    }
}

impl Mul<f64> for Tensor {
    type Output = Tensor;
    fn mul(self, s: f64) -> Tensor {
        Tensor(self.0.map(|row| row.map(|v| v * s)))
    }
}

impl Add for Tensor {
    type Output = Tensor;
    fn add(self, b: Tensor) -> Tensor {
        Tensor([0, 1, 2].map(|i| [0, 1, 2].map(|j| self.0[i][j] + b.0[i][j])))
    }
}

impl Sub for Tensor {
    type Output = Tensor;
    fn sub(self, b: Tensor) -> Tensor {
        self + b * -1.0
    }
}

/// The stress the behaviour expects at the beginning of the step, in its
/// `measure`, from the Cauchy stress `sigma` and the deformation gradient
/// `f`.
pub fn from_cauchy(measure: StressMeasure, sigma: &[f64], f: &[f64]) -> Vec<f64> {
    let (inverse, j) = Tensor::unsymmetric(f).inverse();
    let tau = Tensor::symmetric(sigma) * j;
    match measure {
        StressMeasure::Cauchy => sigma.to_vec(),
        StressMeasure::SecondPiolaKirchhoff => {
            (inverse * tau * inverse.transpose()).symmetric_values(sigma.len())
        }
        StressMeasure::FirstPiolaKirchhoff => {
            (tau * inverse.transpose()).unsymmetric_values(f.len())
        }
    }
}

/// The Cauchy stress and its derivative with respect to the deformation
/// gradient, from the stress (`forces`, in its `measure`) and the tangent
/// operator (`tangent`, of the `operator`) the behaviour computed from the
/// deformation gradient `f0` to `f1`.
pub fn to_cauchy(
    measure: StressMeasure,
    operator: FiniteStrainTangentOperator,
    forces: &[f64],
    tangent: &[f64],
    f0: &[f64],
    f1: &[f64],
) -> (Vec<f64>, Vec<f64>) {
    // The sizes of F and of a symmetric tensor.
    let n = f1.len();
    let m = match n {
        3 => 3,
        5 => 4,
        _ => 6,
    };
    let f = Tensor::unsymmetric(f1);
    let (inverse, j) = f.inverse();
    let sigma = match measure {
        StressMeasure::Cauchy => Tensor::symmetric(forces),
        StressMeasure::SecondPiolaKirchhoff => {
            f * Tensor::symmetric(forces) * f.transpose() * (1.0 / j)
        }
        StressMeasure::FirstPiolaKirchhoff => {
            Tensor::unsymmetric(forces) * f.transpose() * (1.0 / j)
        }
    };
    let values = sigma.symmetric_values(m);
    if operator == FiniteStrainTangentOperator::CauchyByDeformationGradient {
        return (values, tangent.to_vec());
    }
    let sigma = Tensor::symmetric(&values);
    let tau = sigma * j;
    let (s, p) = (
        inverse * tau * inverse.transpose(),
        tau * inverse.transpose(),
    );
    let (f0_inverse, _) = Tensor::unsymmetric(f0).inverse();
    // Column by column: the derivatives with respect to each component of F.
    let mut k = vec![0.0; m * n];
    for c in 0..n {
        let mut unit = vec![0.0; n];
        unit[c] = 1.0;
        let df = Tensor::unsymmetric(&unit);
        let dtau = match operator {
            // τ = F·S·Fᵀ, dS = (dS/dE) : dE, dE = (dFᵀ·F + Fᵀ·dF) / 2.
            FiniteStrainTangentOperator::SecondPiolaKirchhoffByGreenLagrange => {
                let de = ((df.transpose() * f + f.transpose() * df) * 0.5).symmetric_values(m);
                let ds: Vec<f64> = (0..m)
                    .map(|i| (0..m).map(|l| tangent[i * m + l] * de[l]).sum())
                    .collect();
                df * s * f.transpose()
                    + f * Tensor::symmetric(&ds) * f.transpose()
                    + f * s * df.transpose()
            }
            // τ = P·Fᵀ, dP the column c of dP/dF.
            FiniteStrainTangentOperator::FirstPiolaKirchhoffByDeformationGradient => {
                let dp: Vec<f64> = (0..n).map(|i| tangent[i * n + c]).collect();
                Tensor::unsymmetric(&dp) * f.transpose() + p * df.transpose()
            }
            // dτ = (dτ/dΔF) : dΔF, the spatial increment ΔF = F·F0⁻¹ (the
            // Cauchy operator returned above).
            _ => {
                let increment = (df * f0_inverse).unsymmetric_values(n);
                let dtau: Vec<f64> = (0..m)
                    .map(|i| (0..n).map(|l| tangent[i * n + l] * increment[l]).sum())
                    .collect();
                Tensor::symmetric(&dtau)
            }
        };
        let dj = (inverse * df).0;
        let dsigma = dtau * (1.0 / j) - sigma * (dj[0][0] + dj[1][1] + dj[2][2]);
        for (i, v) in dsigma.symmetric_values(m).into_iter().enumerate() {
            k[i * n + c] = v;
        }
    }
    (values, k)
}

#[cfg(test)]
mod tests {
    use super::*;

    const LAMBDA: f64 = 120e9;
    const MU: f64 = 80e9;

    /// A deformation gradient of no particular symmetry: of 9 components
    /// in 3D, of 5 in 2D (no out-of-plane shear).
    fn gradient(n: usize) -> Vec<f64> {
        [1.05, 0.97, 1.02, 0.03, -0.02, 0.04, 0.01, -0.03, 0.02][..n].to_vec()
    }

    /// The second Piola-Kirchhoff stress of the Saint Venant-Kirchhoff law,
    /// S = λ tr(E) I + 2μ E.
    fn pk2(f: Tensor) -> Tensor {
        let e = (f.transpose() * f - Tensor::unsymmetric(&[1.0, 1.0, 1.0])) * 0.5;
        Tensor::unsymmetric(&[1.0, 1.0, 1.0]) * (LAMBDA * (e.0[0][0] + e.0[1][1] + e.0[2][2]))
            + e * (2.0 * MU)
    }

    /// The Cauchy stress of the law, by its symmetric values.
    fn cauchy(f: &[f64], m: usize) -> Vec<f64> {
        let f = Tensor::unsymmetric(f);
        let (_, j) = f.inverse();
        (f * pk2(f) * f.transpose() * (1.0 / j)).symmetric_values(m)
    }

    /// The derivative of `function` at `x` by centred differences: `rows` ×
    /// `x.len()` values, row major.
    fn derivative(x: &[f64], rows: usize, function: &dyn Fn(&[f64]) -> Vec<f64>) -> Vec<f64> {
        let h = 1e-6;
        let mut d = vec![0.0; rows * x.len()];
        for c in 0..x.len() {
            let (mut plus, mut minus) = (x.to_vec(), x.to_vec());
            plus[c] += h;
            minus[c] -= h;
            let (fp, fm) = (function(&plus), function(&minus));
            for i in 0..rows {
                d[i * x.len() + c] = (fp[i] - fm[i]) / (2.0 * h);
            }
        }
        d
    }

    fn assert_close(found: &[f64], expected: &[f64], rtol: f64) {
        let scale = expected.iter().fold(0.0_f64, |s, v| s.max(v.abs()));
        for (f, e) in found.iter().zip(expected) {
            assert!((f - e).abs() <= rtol * scale, "{found:?}\n{expected:?}");
        }
    }

    #[test]
    fn stresses_go_back_and_forth() {
        for n in [9, 5] {
            let (f, m) = (gradient(n), if n == 9 { 6 } else { 4 });
            let sigma = cauchy(&f, m);
            for measure in [
                StressMeasure::SecondPiolaKirchhoff,
                StressMeasure::FirstPiolaKirchhoff,
            ] {
                let forces = from_cauchy(measure, &sigma, &f);
                let tangent = vec![0.0; m * n];
                let (back, _) = to_cauchy(
                    measure,
                    FiniteStrainTangentOperator::CauchyByDeformationGradient,
                    &forces,
                    &tangent,
                    &f,
                    &f,
                );
                assert_close(&back, &sigma, 1e-14);
            }
        }
    }

    #[test]
    fn tangent_operators_convert_to_the_derivative_of_the_cauchy_stress() {
        for n in [9, 5] {
            let m = if n == 9 { 6 } else { 4 };
            let (f0, f1) = (
                gradient(n),
                gradient(n).iter().map(|v| v * 1.01).collect::<Vec<_>>(),
            );
            let expected = derivative(&f1, m, &|f| cauchy(f, m));
            let sigma = cauchy(&f1, m);
            let operators = [
                FiniteStrainTangentOperator::SecondPiolaKirchhoffByGreenLagrange,
                FiniteStrainTangentOperator::FirstPiolaKirchhoffByDeformationGradient,
                FiniteStrainTangentOperator::KirchhoffBySpatialIncrement,
            ];
            for operator in operators {
                let tangent = match operator {
                    // dS/dE = λ I⊗I + 2μ I, in TFEL's storage.
                    FiniteStrainTangentOperator::SecondPiolaKirchhoffByGreenLagrange => (0..m * m)
                        .map(|k| {
                            let (i, j) = (k / m, k % m);
                            f64::from(u8::from(i < 3 && j < 3)) * LAMBDA
                                + f64::from(u8::from(i == j)) * 2.0 * MU
                        })
                        .collect(),
                    // dP/dF, P = F·S.
                    FiniteStrainTangentOperator::FirstPiolaKirchhoffByDeformationGradient => {
                        derivative(&f1, n, &|f| {
                            let f = Tensor::unsymmetric(f);
                            (f * pk2(f)).unsymmetric_values(n)
                        })
                    }
                    // dτ/dΔF, F = ΔF·F0.
                    _ => {
                        let f0 = Tensor::unsymmetric(&f0);
                        let increment =
                            (Tensor::unsymmetric(&f1) * f0.inverse().0).unsymmetric_values(n);
                        derivative(&increment, m, &|df| {
                            let f = (Tensor::unsymmetric(df) * f0).unsymmetric_values(n);
                            let (_, j) = Tensor::unsymmetric(&f).inverse();
                            cauchy(&f, m).iter().map(|s| s * j).collect()
                        })
                    }
                };
                let (_, k) = to_cauchy(StressMeasure::Cauchy, operator, &sigma, &tangent, &f0, &f1);
                assert_close(&k, &expected, 1e-6);
            }
        }
    }
}
