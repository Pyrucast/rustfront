//! Modelling hypotheses: how a 3D problem is reduced, which fixes the number of
//! components of vectors and tensors.

use core::fmt;
use core::str::FromStr;

use crate::Error;

/// A modelling hypothesis of MFront.
///
/// A behaviour is compiled for a list of hypotheses, and one integration
/// function is generated for each of them.
///
/// ```
/// use rustfront::Hypothesis;
///
/// let h: Hypothesis = "PlaneStrain".parse()?;
/// assert_eq!(h, Hypothesis::PlaneStrain);
/// assert_eq!(h.to_string(), "PlaneStrain");
/// assert_eq!(h.space_dimension(), 2);
/// # Ok::<(), rustfront::Error>(())
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Hypothesis {
    /// 1D: axisymmetrical generalised plane strain. Symmetric tensors have
    /// the components `rr`, `zz`, `θθ`.
    AxisymmetricalGeneralisedPlaneStrain,
    /// 1D: axisymmetrical generalised plane stress.
    AxisymmetricalGeneralisedPlaneStress,
    /// 2D: axisymmetrical, in the meridian plane. Symmetric tensors have the
    /// components `rr`, `zz`, `θθ`, `rz`.
    Axisymmetrical,
    /// 2D: plane stress.
    PlaneStress,
    /// 2D: plane strain.
    PlaneStrain,
    /// 2D: generalised plane strain.
    GeneralisedPlaneStrain,
    /// 3D. Symmetric tensors have the components `xx`, `yy`, `zz`, `xy`, `xz`,
    /// `yz`.
    Tridimensional,
}

impl Hypothesis {
    /// Every hypothesis.
    pub const ALL: [Hypothesis; 7] = [
        Hypothesis::AxisymmetricalGeneralisedPlaneStrain,
        Hypothesis::AxisymmetricalGeneralisedPlaneStress,
        Hypothesis::Axisymmetrical,
        Hypothesis::PlaneStress,
        Hypothesis::PlaneStrain,
        Hypothesis::GeneralisedPlaneStrain,
        Hypothesis::Tridimensional,
    ];

    /// The name MFront gives the hypothesis, the one found in symbol names
    /// such as `Norton_Tridimensional`.
    ///
    /// ```
    /// use rustfront::Hypothesis;
    ///
    /// assert_eq!(Hypothesis::Axisymmetrical.name(), "Axisymmetrical");
    /// ```
    pub const fn name(self) -> &'static str {
        match self {
            Hypothesis::AxisymmetricalGeneralisedPlaneStrain => {
                "AxisymmetricalGeneralisedPlaneStrain"
            }
            Hypothesis::AxisymmetricalGeneralisedPlaneStress => {
                "AxisymmetricalGeneralisedPlaneStress"
            }
            Hypothesis::Axisymmetrical => "Axisymmetrical",
            Hypothesis::PlaneStress => "PlaneStress",
            Hypothesis::PlaneStrain => "PlaneStrain",
            Hypothesis::GeneralisedPlaneStrain => "GeneralisedPlaneStrain",
            Hypothesis::Tridimensional => "Tridimensional",
        }
    }

    /// The space dimension: 1, 2 or 3. With it, a vector has `dimension`
    /// components, a symmetric tensor 3, 4 or 6, a tensor 3, 5 or 9.
    ///
    /// ```
    /// use rustfront::Hypothesis;
    ///
    /// assert_eq!(Hypothesis::AxisymmetricalGeneralisedPlaneStrain.space_dimension(), 1);
    /// assert_eq!(Hypothesis::Tridimensional.space_dimension(), 3);
    /// ```
    pub const fn space_dimension(self) -> usize {
        match self {
            Hypothesis::AxisymmetricalGeneralisedPlaneStrain
            | Hypothesis::AxisymmetricalGeneralisedPlaneStress => 1,
            Hypothesis::Axisymmetrical
            | Hypothesis::PlaneStress
            | Hypothesis::PlaneStrain
            | Hypothesis::GeneralisedPlaneStrain => 2,
            Hypothesis::Tridimensional => 3,
        }
    }
}

impl fmt::Display for Hypothesis {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

/// Parses the name MFront gives the hypothesis, see [`Hypothesis::name`].
///
/// ```
/// use rustfront::{Error, Hypothesis};
///
/// assert!(matches!("3D".parse::<Hypothesis>(), Err(Error::UnknownHypothesis(_))));
/// ```
impl FromStr for Hypothesis {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Error> {
        Hypothesis::ALL
            .into_iter()
            .find(|h| h.name() == s)
            .ok_or_else(|| Error::UnknownHypothesis(s.to_owned()))
    }
}
