//! Conversions between TFEL's storage of symmetric tensors and the standard
//! Voigt notation. Nothing here allocates.
//!
//! | components | TFEL                                 | Voigt                    |
//! |---|---|---|
//! | 6 (3D)     | `xx, yy, zz, √2·xy, √2·xz, √2·yz`    | `xx, yy, zz, yz, xz, xy` |
//! | 4 (2D)     | `xx, yy, zz, √2·xy`                  | `xx, yy, zz, xy`         |
//! | 3 (1D)     | `xx, yy, zz`                         | `xx, yy, zz`             |
//!
//! In the axisymmetrical hypotheses, `x`, `y`, `z` stand for `r`, `z`, `θ`.
//! Voigt shear components are either the tensor ones (`ε_xy`, `σ_xy`) or, for
//! strains, the engineering ones (`γ_xy = 2·ε_xy`): see [`Shear`].

use core::f64::consts::{FRAC_1_SQRT_2, SQRT_2};

/// How the shear components are written in Voigt notation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Shear {
    /// The tensor components: `ε_xy`, `σ_xy`. Always the case for stresses.
    Tensor,
    /// The engineering shear strains `γ_xy = 2·ε_xy`.
    Engineering,
}

/// For the Voigt component `i` of a tensor of `n` components: the position of
/// the same component in TFEL's storage, and the factor that turns the TFEL
/// value into the Voigt one.
fn component(i: usize, n: usize, shear: Shear) -> (usize, f64) {
    assert!(
        matches!(n, 3 | 4 | 6),
        "a symmetric tensor has 3, 4 or 6 components, not {n}"
    );
    // In 3D, Voigt lists the shears `yz, xz, xy`, TFEL `xy, xz, yz`.
    let tfel = if n == 6 { [0, 1, 2, 5, 4, 3][i] } else { i };
    let factor = match (i < 3, shear) {
        (true, _) => 1.0,
        (false, Shear::Tensor) => FRAC_1_SQRT_2,
        (false, Shear::Engineering) => SQRT_2,
    };
    (tfel, factor)
}

/// Converts a symmetric tensor stored by TFEL into Voigt notation.
///
/// ```
/// use rustfront::voigt::{self, Shear};
///
/// // A pure shear stress σ_xy = 1, as TFEL stores it in 3D.
/// let tfel = [0.0, 0.0, 0.0, 2f64.sqrt(), 0.0, 0.0];
/// let mut sigma = [0.0; 6];
/// voigt::from_tfel(&tfel, Shear::Tensor, &mut sigma);
/// assert!((sigma[5] - 1.0).abs() < 1e-15); // xy is last in Voigt notation
/// ```
///
/// # Panics
///
/// When `tfel` does not have 3, 4 or 6 components, or `voigt` not as many.
pub fn from_tfel(tfel: &[f64], shear: Shear, voigt: &mut [f64]) {
    assert_eq!(tfel.len(), voigt.len(), "the two tensors differ in size");
    for (i, v) in voigt.iter_mut().enumerate() {
        let (t, factor) = component(i, tfel.len(), shear);
        *v = factor * tfel[t];
    }
}

/// Converts a symmetric tensor in Voigt notation into TFEL's storage.
///
/// ```
/// use rustfront::voigt::{self, Shear};
///
/// // An engineering shear strain γ_xy = 2e-3 in 2D: ε_xy = 1e-3.
/// let gamma = [0.0, 0.0, 0.0, 2e-3];
/// let mut tfel = [0.0; 4];
/// voigt::to_tfel(&gamma, Shear::Engineering, &mut tfel);
/// assert!((tfel[3] - 2f64.sqrt() * 1e-3).abs() < 1e-18);
/// ```
///
/// # Panics
///
/// When `voigt` does not have 3, 4 or 6 components, or `tfel` not as many.
pub fn to_tfel(voigt: &[f64], shear: Shear, tfel: &mut [f64]) {
    assert_eq!(tfel.len(), voigt.len(), "the two tensors differ in size");
    for (i, v) in voigt.iter().enumerate() {
        let (t, factor) = component(i, voigt.len(), shear);
        tfel[t] = v / factor;
    }
}

/// Converts a tangent operator `dσ/dε` computed by TFEL (`n × n`, row major)
/// into the Voigt matrix `D` such that `σ = D·ε` in Voigt notation, the
/// stress having tensor shears and the strain the shears given by `shear`.
///
/// ```
/// use rustfront::voigt::{self, Shear};
///
/// // Isotropic elasticity in 2D, as TFEL computes it: λ I⊗I + 2μ I.
/// let (lambda, mu) = (1.0, 0.5);
/// let mut k = [0.0; 16];
/// for i in 0..3 {
///     for j in 0..3 {
///         k[4 * i + j] = lambda;
///     }
/// }
/// for i in 0..4 {
///     k[5 * i] += 2.0 * mu;
/// }
/// let mut d = [0.0; 16];
/// voigt::tangent_from_tfel(&k, Shear::Engineering, &mut d);
/// // With engineering shears, the shear modulus is μ, not 2μ.
/// assert!((d[15] - mu).abs() < 1e-15);
/// assert_eq!(d[0], lambda + 2.0 * mu);
/// ```
///
/// # Panics
///
/// When `tfel` does not have 9, 16 or 36 values, or `voigt` not as many.
pub fn tangent_from_tfel(tfel: &[f64], shear: Shear, voigt: &mut [f64]) {
    assert_eq!(tfel.len(), voigt.len(), "the two operators differ in size");
    let n = tfel.len().isqrt();
    assert_eq!(n * n, tfel.len(), "a tangent operator is a square matrix");
    for i in 0..n {
        // The stress has tensor shears; the strain is converted back to TFEL's
        // storage, hence the inverse of its factor.
        let (ti, fi) = component(i, n, Shear::Tensor);
        for j in 0..n {
            let (tj, fj) = component(j, n, shear);
            voigt[n * i + j] = fi * tfel[n * ti + tj] / fj;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_round_trip_gives_the_tensor_back() {
        for n in [3, 4, 6] {
            for shear in [Shear::Tensor, Shear::Engineering] {
                let original: Vec<f64> = (1..=n).map(|i| i as f64).collect();
                let (mut voigt, mut back) = (vec![0.0; n], vec![0.0; n]);
                from_tfel(&original, shear, &mut voigt);
                to_tfel(&voigt, shear, &mut back);
                for (a, b) in original.iter().zip(&back) {
                    assert!(
                        (a - b).abs() < 1e-14,
                        "{n} {shear:?}: {original:?} → {back:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn the_3d_shears_are_reordered() {
        let tfel = [0.0, 0.0, 0.0, SQRT_2 * 1.0, SQRT_2 * 2.0, SQRT_2 * 3.0];
        let mut voigt = [0.0; 6];
        from_tfel(&tfel, Shear::Tensor, &mut voigt);
        // TFEL xy, xz, yz = 1, 2, 3 → Voigt yz, xz, xy = 3, 2, 1.
        for (found, expected) in voigt[3..].iter().zip([3.0, 2.0, 1.0]) {
            assert!((found - expected).abs() < 1e-14, "{voigt:?}");
        }
    }

    #[test]
    fn the_isotropic_tangent_matches_the_voigt_stiffness() {
        let (lambda, mu) = (3.0, 2.0);
        let mut k = [0.0; 36];
        for i in 0..6 {
            for j in 0..6 {
                k[6 * i + j] = if i < 3 && j < 3 { lambda } else { 0.0 };
            }
            k[7 * i] += 2.0 * mu;
        }
        let mut d = [0.0; 36];
        tangent_from_tfel(&k, Shear::Engineering, &mut d);
        for i in 3..6 {
            assert!((d[7 * i] - mu).abs() < 1e-14, "{d:?}");
        }
        tangent_from_tfel(&k, Shear::Tensor, &mut d);
        for i in 3..6 {
            assert!((d[7 * i] - 2.0 * mu).abs() < 1e-14, "{d:?}");
        }
    }
}
