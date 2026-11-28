//! The rotation between the global frame and the material frame of an
//! orthotropic behaviour.

use crate::Error;

/// The frame [`crate::Behaviour::rotate_gradients`] and its siblings rotate
/// values to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Frame {
    /// The frame of the structure, in which the caller works.
    Global,
    /// The frame of the material, its axes of orthotropy, in which the
    /// behaviour integrates.
    Material,
}

/// A rotation matrix R whose columns are the axes of the material frame,
/// expressed in the global frame: the components of a symmetric tensor T in
/// the material frame are those of Rᵀ·T·R, as for MTest's `@RotationMatrix`.
///
/// In 2D, the material frame keeps the z axis: R must be a rotation about z,
/// such as `Rotation::from_axes([c, s, 0.0], [-s, c, 0.0])`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rotation {
    /// By rows, as MFront reads it.
    pub(crate) matrix: [[f64; 3]; 3],
}

impl Rotation {
    /// The rotation of `matrix`, given by rows: `matrix[i][j]` is the `i`-th
    /// global component of the `j`-th material axis.
    ///
    /// # Errors
    ///
    /// [`Error::InvalidRotation`] when a column is not of unit length or two
    /// columns are not orthogonal, within 100 times the machine epsilon, the
    /// check of MTest.
    pub fn new(matrix: [[f64; 3]; 3]) -> Result<Rotation, Error> {
        let column = |j: usize| [matrix[0][j], matrix[1][j], matrix[2][j]];
        let dot = |a: [f64; 3], b: [f64; 3]| a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
        let tolerance = 100.0 * f64::EPSILON;
        // Written so that a NaN fails the checks.
        if !(0..3).all(|j| (dot(column(j), column(j)).sqrt() - 1.0).abs() <= tolerance) {
            return Err(Error::InvalidRotation {
                reason: "a column is not of unit length",
            });
        }
        if ![(0, 1), (0, 2), (1, 2)]
            .iter()
            .all(|&(a, b)| dot(column(a), column(b)).abs() <= tolerance)
        {
            return Err(Error::InvalidRotation {
                reason: "two columns are not orthogonal",
            });
        }
        Ok(Rotation { matrix })
    }

    /// The rotation to the material frame whose first two axes are `axis1`
    /// and `axis2`, given in the global frame, as MTest's
    /// `@RotationMatrix<Direction>`: `axis1` is normalised, `axis2` made
    /// orthogonal to it and normalised, the third axis is their cross
    /// product.
    ///
    /// # Errors
    ///
    /// [`Error::InvalidRotation`] when `axis1` is zero, `axis2` zero or
    /// parallel to it (up to rounding errors), or a component is not finite.
    pub fn from_axes(axis1: [f64; 3], axis2: [f64; 3]) -> Result<Rotation, Error> {
        let norm = |v: [f64; 3]| (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
        let n1 = norm(axis1);
        if !(n1 > 0.0 && n1.is_finite()) {
            return Err(Error::InvalidRotation {
                reason: "the first axis is zero or not finite",
            });
        }
        let a = axis1.map(|x| x / n1);
        let projection = a[0] * axis2[0] + a[1] * axis2[1] + a[2] * axis2[2];
        let b = [0, 1, 2].map(|i| axis2[i] - projection * a[i]);
        let n2 = norm(b);
        let b = b.map(|x| x / n2);
        let c = [
            a[1] * b[2] - a[2] * b[1],
            a[2] * b[0] - a[0] * b[2],
            a[0] * b[1] - a[1] * b[0],
        ];
        // Orthonormal, unless the second axis is zero, not finite, or so
        // close to the first that only rounding errors remain of it.
        Rotation::new([0, 1, 2].map(|i| [a[i], b[i], c[i]])).map_err(|_| Error::InvalidRotation {
            reason: "the second axis is zero, parallel to the first, or not finite",
        })
    }
}
