//! The C ABI of MFront's `generic` interface, transcribed from the headers
//! `MFront/GenericBehaviour/State.h` and `MFront/GenericBehaviour/BehaviourData.h`
//! of TFEL 5.1 (`mfront_gb_real` is `double`).
//!
//! Nothing here is checked: [`crate::Behaviour::integrate`] validates every
//! array length before building these structures.

use core::ffi::{c_char, c_int};

/// `mfront_gb_State`: the state at the end of the time step.
#[repr(C)]
pub(crate) struct State {
    pub(crate) gradients: *const f64,
    pub(crate) thermodynamic_forces: *mut f64,
    pub(crate) mass_density: *const f64,
    pub(crate) material_properties: *const f64,
    pub(crate) internal_state_variables: *mut f64,
    pub(crate) stored_energy: *mut f64,
    pub(crate) dissipated_energy: *mut f64,
    pub(crate) external_state_variables: *const f64,
}

/// `mfront_gb_InitialState`: the state at the beginning of the time step.
#[repr(C)]
pub(crate) struct InitialState {
    pub(crate) gradients: *const f64,
    pub(crate) thermodynamic_forces: *const f64,
    pub(crate) mass_density: *const f64,
    pub(crate) material_properties: *const f64,
    pub(crate) internal_state_variables: *const f64,
    pub(crate) stored_energy: *const f64,
    pub(crate) dissipated_energy: *const f64,
    pub(crate) external_state_variables: *const f64,
}

/// `mfront_gb_BehaviourData`: everything the integration function reads and
/// writes.
#[repr(C)]
pub(crate) struct BehaviourData {
    /// Buffer of [`ERROR_MESSAGE_SIZE`] bytes, filled on failure.
    pub(crate) error_message: *mut c_char,
    pub(crate) dt: f64,
    /// Tangent operator. On input, `K[0]` selects which operator to compute.
    pub(crate) k: *mut f64,
    /// Time step scaling factor. On input, the largest increase allowed; on
    /// output, the factor the behaviour proposes.
    pub(crate) rdt: *mut f64,
    /// Only written when `K[0] > 50`, which this crate never requests.
    pub(crate) speed_of_sound: *mut f64,
    pub(crate) s0: InitialState,
    pub(crate) s1: State,
}

/// Size of the error message buffer: the behaviour copies at most 511 bytes,
/// then a NUL (`reportError` in `MFront/GenericBehaviour/Integrate.hxx`).
pub(crate) const ERROR_MESSAGE_SIZE: usize = 512;

/// `int <Behaviour>_<Hypothesis>(mfront_gb_BehaviourData* const)`: returns 1
/// on success, 0 on success with a time step scaling factor below 0.99, -1 on
/// failure. The generated code catches every C++ exception: nothing unwinds
/// across this boundary.
pub(crate) type IntegrateFn = unsafe extern "C" fn(*mut BehaviourData) -> c_int;

/// `int <Behaviour>[_<Hypothesis>]_setParameter(const char*, double)`, and
/// its integer and unsigned short versions: return 1 when the parameter is
/// set, 0 when refused (the library then prints why on the standard error).
/// The parameters are static variables of the library.
pub(crate) type SetParameterFn = unsafe extern "C" fn(*const c_char, f64) -> c_int;
pub(crate) type SetIntegerParameterFn = unsafe extern "C" fn(*const c_char, c_int) -> c_int;
pub(crate) type SetUnsignedShortParameterFn = unsafe extern "C" fn(*const c_char, u16) -> c_int;

/// `void <Behaviour>_<Hypothesis>_rotateArrayOf<Variables>(mfront_gb_real*
/// const dest, const mfront_gb_real* const src, const mfront_gb_real* const
/// r, const mfront_gb_size_type n)`: rotates `n` points, the 9 values of `r`
/// by rows. Each variable is copied before being written, so `dest` may be
/// `src`; a scalar variable is not written at all.
pub(crate) type RotateFn = unsafe extern "C" fn(*mut f64, *const f64, *const f64, usize);

/// `void <Behaviour>_setOutOfBoundsPolicy(const int)`: 0 None, 1 Warning, 2
/// Strict. A static variable of the library too; the function does nothing
/// when the behaviour forbids changing its policy.
pub(crate) type SetOutOfBoundsPolicyFn = unsafe extern "C" fn(c_int);

/// `mfront_gmp_OutputStatus` (`MFront/GenericMaterialProperty/OutputStatus.h`):
/// how the evaluation of a material property went.
#[repr(C)]
pub(crate) struct OutputStatus {
    /// 0: evaluated; 1: evaluated, an input out of its bounds; negative: no
    /// value (-1 bounds, -2 C++ exception, -3 error of the C library, -4 value
    /// not finite, -5 wrong number of arguments).
    pub(crate) status: c_int,
    /// `errno`, when `status` is -3.
    pub(crate) c_error_number: c_int,
    /// The rank, from 1, of an input out of its bounds, 0 when there is none.
    /// Its sign is not reliable: TFEL 5.1 sets -1 for a bound crossed with
    /// the strict policy, not only for a physical bound.
    pub(crate) bounds_status: c_int,
    /// The message, NUL-terminated.
    pub(crate) msg: [u8; ERROR_MESSAGE_SIZE],
}

impl OutputStatus {
    /// A status before any evaluation.
    pub(crate) const CLEARED: OutputStatus = OutputStatus {
        status: 0,
        c_error_number: 0,
        bounds_status: 0,
        msg: [0; ERROR_MESSAGE_SIZE],
    };
}

/// `mfront_gmp_real <Name>(mfront_gmp_OutputStatus* const, const
/// mfront_gmp_real* const, const mfront_gmp_size_type, const
/// mfront_gmp_OutOfBoundsPolicy)`: the arguments, their number, and the
/// policy, a C enumeration: 0 None, 1 Warning, 2 Strict. Like the behaviours,
/// it catches every C++ exception.
pub(crate) type MaterialPropertyFn =
    unsafe extern "C" fn(*mut OutputStatus, *const f64, usize, c_int) -> f64;
