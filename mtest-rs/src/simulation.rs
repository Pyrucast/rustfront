//! The time loop, MTest's: at each step, Newton iterations on the strain
//! components and one Lagrange multiplier per imposed strain, until the
//! stresses of the free components reach the imposed ones (zero by default).

use rustfront::{
    Behaviour, FinalState, FiniteStrainTangentOperator, Frame, Hypothesis, InitialState,
    MaterialProperty, StressMeasure, Symmetry, TangentOperator, TimeStep, VariableType,
};

use crate::finite_strain;
use crate::input::{Evolution, Imposed, Test};

/// The suffixes of the components of a symmetric tensor, in TFEL's order:
/// `EXX`, `SRZ`, `ElasticStrainYY`, …
pub fn stensor_suffixes(hypothesis: Hypothesis) -> &'static [&'static str] {
    match hypothesis {
        Hypothesis::AxisymmetricalGeneralisedPlaneStrain
        | Hypothesis::AxisymmetricalGeneralisedPlaneStress => &["RR", "ZZ", "TT"],
        Hypothesis::Axisymmetrical => &["RR", "ZZ", "TT", "RZ"],
        Hypothesis::PlaneStress | Hypothesis::PlaneStrain | Hypothesis::GeneralisedPlaneStrain => {
            &["XX", "YY", "ZZ", "XY"]
        }
        Hypothesis::Tridimensional => &["XX", "YY", "ZZ", "XY", "XZ", "YZ"],
    }
}

/// The suffixes of the components of an unsymmetric tensor, in TFEL's order:
/// `XX YY ZZ XY YX XZ ZX YZ ZY` in 3D, those of `FXX`…
pub fn tensor_suffixes(hypothesis: Hypothesis) -> &'static [&'static str] {
    match hypothesis {
        Hypothesis::AxisymmetricalGeneralisedPlaneStrain
        | Hypothesis::AxisymmetricalGeneralisedPlaneStress => &["RR", "ZZ", "TT"],
        Hypothesis::Axisymmetrical => &["RR", "ZZ", "TT", "RZ", "ZR"],
        Hypothesis::PlaneStress | Hypothesis::PlaneStrain | Hypothesis::GeneralisedPlaneStrain => {
            &["XX", "YY", "ZZ", "XY", "YX"]
        }
        Hypothesis::Tridimensional => &["XX", "YY", "ZZ", "XY", "YX", "XZ", "ZX", "YZ", "ZY"],
    }
}

/// Runs the test, the material properties `laws` loaded from the libraries
/// the file names. Each row of the result is a line of the output file: the
/// time, the strain, the stress, the internal state variables, the stored and
/// dissipated energies, at the initial time then at the end of each step.
pub fn run(
    test: &Test,
    behaviour: &Behaviour,
    laws: &[(String, MaterialProperty)],
) -> Result<Vec<Vec<f64>>, String> {
    let d = behaviour.description();
    // A small strain behaviour, a finite strain one (loaded with the Cauchy
    // stress and its derivative with respect to the deformation gradient), or
    // a point-wise model: no gradient at all.
    let small_strain = d.gradients.list.len() == 1
        && d.gradients.list[0].variable_type == VariableType::Stensor
        && d.tangent_operator_blocks == [("Stress".to_owned(), "Strain".to_owned())];
    let finite_strain =
        d.gradients.list.len() == 1 && d.gradients.list[0].variable_type == VariableType::Tensor;
    // Another stress measure or tangent operator than the Cauchy stress and
    // its derivative is converted to them, as MTest does, but in plane stress.
    if finite_strain
        && matches!(
            test.hypothesis,
            Hypothesis::PlaneStress | Hypothesis::AxisymmetricalGeneralisedPlaneStress
        )
        && (test.stress_measure != StressMeasure::Cauchy
            || test.finite_strain_tangent_operator
                != FiniteStrainTangentOperator::CauchyByDeformationGradient)
    {
        return Err(format!(
            "`{}`: in the {} hypothesis, only the Cauchy stress and its derivative \
             with respect to the deformation gradient are supported, as in MTest",
            d.name, test.hypothesis
        ));
    }
    let model = d.gradients.list.is_empty() && d.thermodynamic_forces.list.is_empty();
    if !small_strain && !finite_strain && !model {
        return Err(format!(
            "`{}` is neither a small strain behaviour, a finite strain one nor a point-wise model",
            d.name
        ));
    }
    // The gradients (strain or deformation gradient), and the stress.
    // The size of the gradients (strain or deformation gradient) and of the
    // stress (the Cauchy stress for finite strain).
    let n = d.gradients.size;
    let m = if finite_strain {
        stensor_suffixes(test.hypothesis).len()
    } else {
        d.thermodynamic_forces.size
    };

    // The external state variables, in the order of the behaviour.
    if let Some((name, _)) = test.external_state_variables.iter().find(|(name, _)| {
        !d.external_state_variables
            .list
            .iter()
            .any(|v| v.name == *name)
    }) {
        return Err(format!(
            "`{}` has no external state variable `{name}`",
            d.name
        ));
    }
    let given = |name: &str| {
        test.external_state_variables
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, evolution)| evolution)
    };
    let mut external = Vec::new();
    for v in &d.external_state_variables.list {
        if v.size != 1 {
            return Err(format!(
                "the external state variable `{}` is not a scalar, which is not supported",
                v.name
            ));
        }
        external.push(
            given(&v.name)
                .ok_or_else(|| format!("the external state variable `{}` is not given", v.name))?,
        );
    }

    // The material properties, in the order of the behaviour: an evolution,
    // or a material property evaluated, as MTest does with the policy None,
    // at the external state variables named after its inputs.
    enum Source<'a> {
        Evolution(&'a Evolution),
        Law(&'a MaterialProperty, Vec<&'a Evolution>),
    }
    let declared = |name: &str| d.material_properties.list.iter().any(|v| v.name == name);
    let unknown = test
        .material_properties
        .iter()
        .map(|(name, _)| name)
        .chain(laws.iter().map(|(name, _)| name));
    // The thermal expansion coefficients are used by mtest-rs, as by MTest.
    let thermal = [
        "ThermalExpansion",
        "ThermalExpansion1",
        "ThermalExpansion2",
        "ThermalExpansion3",
    ];
    if let Some(name) = unknown
        .into_iter()
        .find(|name| !declared(name) && !thermal.contains(&name.as_str()))
    {
        return Err(format!("`{}` has no material property `{name}`", d.name));
    }
    let mut material = Vec::new();
    for v in &d.material_properties.list {
        if let Some((_, evolution)) = test
            .material_properties
            .iter()
            .find(|(name, _)| *name == v.name)
        {
            material.push(Source::Evolution(evolution));
        } else if let Some((_, law)) = laws.iter().find(|(name, _)| *name == v.name) {
            let inputs = law
                .description()
                .inputs
                .iter()
                .map(|input| {
                    given(input).ok_or_else(|| {
                        format!(
                            "the input `{input}` of `{}` is not an external state variable",
                            v.name
                        )
                    })
                })
                .collect::<Result<Vec<_>, String>>()?;
            material.push(Source::Law(law, inputs));
        } else {
            return Err(format!("the material property `{}` is not given", v.name));
        }
    }
    let material_values = |t: f64| -> Result<Vec<f64>, String> {
        material
            .iter()
            .map(|source| match source {
                Source::Evolution(evolution) => Ok(evolution.value(t)),
                Source::Law(law, inputs) => {
                    let arguments: Vec<f64> = inputs.iter().map(|e| e.value(t)).collect();
                    law.evaluate(&arguments)
                        .map(|e| e.value)
                        .map_err(|e| e.to_string())
                }
            })
            .collect()
    };
    let values = |evolutions: &[&Evolution], t: f64| -> Vec<f64> {
        evolutions.iter().map(|e| e.value(t)).collect()
    };

    // The initial internal state variables: zero unless given.
    let mut isv0 = vec![0.0; d.internal_state_variables.size];
    for (name, given) in &test.internal_state_variables {
        let v = d
            .internal_state_variables
            .list
            .iter()
            .find(|v| v.name == *name)
            .ok_or_else(|| format!("`{}` has no internal state variable `{name}`", d.name))?;
        if given.len() != v.size {
            return Err(format!(
                "`{name}` has {} components, {} given",
                v.size,
                given.len()
            ));
        }
        isv0[v.offset..v.offset + v.size].copy_from_slice(given);
    }

    // The constraints, in MTest's order: those of the file, then those the
    // hypothesis adds. Each imposed strain has a Lagrange multiplier, stored
    // after the strain components among the unknowns.
    let zero = Evolution::Number(0.0);
    let mut constraints: Vec<(Imposed, usize, &Evolution)> = Vec::new();
    // The components: `EXX`… or `FXX`…, and `SXX`…
    let gradient = if finite_strain {
        ('F', tensor_suffixes(test.hypothesis))
    } else {
        ('E', stensor_suffixes(test.hypothesis))
    };
    let stress = ('S', stensor_suffixes(test.hypothesis));
    for (kind, component, evolution) in &test.imposed {
        let (prefix, suffixes) = match kind {
            Imposed::Strain => gradient,
            Imposed::Stress => stress,
        };
        let i = suffixes
            .iter()
            .position(|s| component.strip_prefix(prefix) == Some(s))
            .ok_or_else(|| {
                format!(
                    "`{component}` is not a component in the {} hypothesis",
                    test.hypothesis
                )
            })?;
        if constraints.iter().any(|(_, c, _)| *c == i) {
            return Err(format!("`{component}`: this component is already imposed"));
        }
        constraints.push((*kind, i, evolution));
    }
    // Plane strain: EZZ is zero, FZZ one. Plane stress, and axisymmetrical
    // generalised plane stress: the behaviour handles the out-of-plane
    // strain, MTest imposes the gradient at rest and a zero stress on that
    // component.
    let added: &[(Imposed, usize)] = match test.hypothesis {
        Hypothesis::PlaneStrain => &[(Imposed::Strain, 2)],
        Hypothesis::PlaneStress => &[(Imposed::Strain, 2), (Imposed::Stress, 2)],
        Hypothesis::AxisymmetricalGeneralisedPlaneStress => {
            &[(Imposed::Strain, 1), (Imposed::Stress, 1)]
        }
        _ => &[],
    };
    for &(_, i) in added {
        if constraints.iter().any(|(_, c, _)| *c == i) {
            return Err(format!(
                "`{}{}` cannot be imposed in the {} hypothesis",
                gradient.0, gradient.1[i], test.hypothesis
            ));
        }
    }
    let rest = Evolution::Number(if finite_strain { 1.0 } else { 0.0 });
    constraints.extend(added.iter().map(|&(kind, i)| {
        let value = match kind {
            Imposed::Strain => &rest,
            Imposed::Stress => &zero,
        };
        (kind, i, value)
    }));
    let multipliers = constraints
        .iter()
        .filter(|(kind, _, _)| *kind == Imposed::Strain)
        .count();
    let unknowns = n + multipliers;

    // The unknowns at the beginning of the step: strains, then multipliers.
    // A deformation gradient starts from the identity: its first three
    // components are diagonal.
    // As MTest, the thermal strain of a small strain behaviour, subtracted
    // from the strain it is given: α (T - T_ref) on the diagonal, from the
    // evolutions `Temperature` and `ThermalExpansion` (`ThermalExpansion1`
    // to `3` for an orthotropic behaviour, in its material frame), and the
    // constant `ThermalExpansionReferenceTemperature`, 293.15 by default.
    let named = |name: &str| -> Result<Option<Evolution>, String> {
        test.evolutions
            .iter()
            .rev()
            .find(|(n, _)| n == name)
            .map(|(_, e)| {
                e.resolve(&test.evolutions)
                    .map_err(|e| format!("`{name}`: {e}"))
            })
            .transpose()
    };
    let reference = match named("ThermalExpansionReferenceTemperature")? {
        Some(e) => e
            .constant()
            .ok_or("`ThermalExpansionReferenceTemperature` must be a constant")?,
        None => 293.15,
    };
    let coefficients = match d.symmetry {
        Symmetry::Isotropic => &thermal[..1],
        Symmetry::Orthotropic => &thermal[1..],
    }
    .iter()
    .map(|name| named(name))
    .collect::<Result<Option<Vec<_>>, _>>()?;
    let expansion = match (named("Temperature")?, coefficients) {
        (Some(temperature), Some(coefficients))
            if small_strain && test.handle_thermal_expansion =>
        {
            Some((temperature, coefficients))
        }
        _ => None,
    };
    let thermal_strain = |time: f64| -> Result<Vec<f64>, String> {
        let mut e = vec![0.0; n];
        if let Some((temperature, coefficients)) = &expansion {
            let difference = temperature.value(time) - reference;
            for (i, e) in e.iter_mut().take(3).enumerate() {
                *e = coefficients[i.min(coefficients.len() - 1)].value(time) * difference;
            }
            if let (Some(r), 3) = (&test.rotation, coefficients.len()) {
                behaviour
                    .rotate_gradients(r, Frame::Global, &mut e)
                    .map_err(|e| e.to_string())?;
            }
        }
        Ok(e)
    };

    // The state at the first time: at rest, or as the test gives it.
    let mut u0 = vec![0.0; unknowns];
    if finite_strain {
        u0[..3].fill(1.0);
    }
    let mut stress0 = vec![0.0; m];
    for (initial, values, what) in [
        (&test.initial_gradients, &mut u0[..n], "gradients"),
        (&test.initial_stress, &mut stress0[..], "stress"),
    ] {
        if let Some(initial) = initial {
            if initial.len() != values.len() {
                return Err(format!(
                    "the initial {what} has {} values, {} expected",
                    initial.len(),
                    values.len()
                ));
            }
            values.copy_from_slice(initial);
        }
    }
    // Where each stress component goes in the residual, as MTest assembles
    // it: in its own row for a small strain behaviour; for a finite strain
    // one, the diagonal in the rows of the diagonal of F, and each shear in
    // the two rows of its components of F (σxy in FXY and FYX), the rows of
    // the tangent operator likewise. The MTest of TFEL 5.1 and before puts
    // the diagonal (σxx) in the rows of the shear, which only its tangent
    // operator gets right; fixed in TFEL's master branch, followed here.
    // With imposed shear components, the multipliers absorb the difference:
    // the same results, one more iteration for that MTest.
    let stress_rows: Vec<(usize, usize)> = if finite_strain {
        (0..3)
            .map(|i| (i, i))
            .chain((0..m - 3).flat_map(|i| [(3 + 2 * i, 3 + i), (4 + 2 * i, 3 + i)]))
            .collect()
    } else {
        (0..m).map(|i| (i, i)).collect()
    };
    let (mut stored_energy, mut dissipated_energy) = (0.0, 0.0);
    let mut k = vec![0.0; m * n];
    // MTest scales the multipliers by the largest term of the first tangent
    // operator it assembles, and keeps that factor for the whole test.
    let mut normalisation: Option<f64> = None;
    let mut rows = Vec::new();
    // The current time, the index of the next time of `@Times`, the current
    // step: halved on failure and kept afterwards, as MTest does, until the
    // next time of `@Times`.
    let mut t = test.times[0];
    let mut next = 1;
    let mut dt = test.times[1] - t;
    let mut sub_steps = 0;
    // Whether the last attempt moved the state forward, to be written.
    let mut advanced = true;
    // The unknowns at the beginning of the last step that converged, and its
    // length: none before the first one.
    let mut previous: Option<(Vec<f64>, f64)> = None;
    loop {
        if advanced {
            let mut row = vec![t];
            row.extend(&u0[..n]);
            row.extend(&stress0);
            row.extend(&isv0);
            row.extend([stored_energy, dissipated_energy]);
            rows.push(row);
        }
        if next == test.times.len() {
            return Ok(rows);
        }
        let end = test.times[next];
        let target = (t + dt).min(end);
        let (material0, material1) = (material_values(t)?, material_values(target)?);
        let (thermal0, thermal1) = (thermal_strain(t)?, thermal_strain(target)?);
        let (external0, external1) = (values(&external, t), values(&external, target));
        // The iterations start from the previous state, or with a linear
        // prediction, as MTest, from its extrapolation along the last step.
        let mut u1 = match &previous {
            Some((before, length)) if test.linear_prediction => u0
                .iter()
                .zip(before)
                .map(|(u, b)| u + (u - b) * (target - t) / length)
                .collect(),
            _ => u0.clone(),
        };
        let mut stress1 = vec![0.0; m];
        let mut isv1 = isv0.clone();
        let mut iterations = 0;
        let outcome = loop {
            iterations += 1;
            // An orthotropic behaviour integrates in its material frame, as
            // in MTest: the gradients and the stress at the beginning of the
            // step are rotated to it, the stress and the tangent operator it
            // computes back to the global frame. The internal state variables
            // stay in the material frame.
            // For finite strain, the stress the behaviour expects, from the
            // Cauchy stress.
            // The strain the behaviour sees: without the thermal strain.
            let (mut g0, mut g1) = (u0[..n].to_vec(), u1[..n].to_vec());
            for (g, e) in g0
                .iter_mut()
                .zip(&thermal0)
                .chain(g1.iter_mut().zip(&thermal1))
            {
                *g -= e;
            }
            let mut sig0 = if finite_strain {
                finite_strain::from_cauchy(test.stress_measure, &stress0, &g0)
            } else {
                stress0.clone()
            };
            if let Some(r) = &test.rotation {
                behaviour
                    .rotate_gradients(r, Frame::Material, &mut g0)
                    .and_then(|()| behaviour.rotate_gradients(r, Frame::Material, &mut g1))
                    .and_then(|()| {
                        behaviour.rotate_thermodynamic_forces(r, Frame::Material, &mut sig0)
                    })
                    .map_err(|e| e.to_string())?;
            }
            let s0 = InitialState {
                gradients: &g0,
                thermodynamic_forces: &sig0,
                material_properties: &material0,
                internal_state_variables: &isv0,
                external_state_variables: &external0,
                stored_energy,
                dissipated_energy,
            };
            let mut forces = vec![0.0; d.thermodynamic_forces.size];
            let mut tangent = vec![0.0; d.tangent_operator_size];
            let mut s1 = FinalState {
                gradients: &g1,
                thermodynamic_forces: &mut forces,
                material_properties: &material1,
                internal_state_variables: &mut isv1,
                external_state_variables: &external1,
            };
            let consistent = Some((TangentOperator::ConsistentTangent, tangent.as_mut_slice()));
            // As MGIS by default: the behaviour may not propose a larger step.
            let step = TimeStep {
                dt: target - t,
                max_scaling_factor: 1.0,
            };
            let integration = match behaviour.integrate(step, &s0, &mut s1, consistent) {
                Ok(integration) => integration,
                Err(e) => break Err(e.to_string()),
            };
            if let Some(r) = &test.rotation {
                behaviour
                    .rotate_thermodynamic_forces(r, Frame::Global, &mut forces)
                    .and_then(|()| {
                        behaviour.rotate_tangent_operator(r, Frame::Global, &mut tangent)
                    })
                    .map_err(|e| e.to_string())?;
            }
            // For finite strain, the Cauchy stress and its derivative with
            // respect to the deformation gradient, MTest's unknowns.
            (stress1, k) = if finite_strain {
                finite_strain::to_cauchy(
                    test.stress_measure,
                    test.finite_strain_tangent_operator,
                    &forces,
                    &tangent,
                    &u0[..n],
                    &u1[..n],
                )
            } else {
                (forces, tangent)
            };
            // Nothing to solve, as for a point-wise model: MTest integrates
            // once, without Newton iterations.
            if unknowns == 0 {
                break Ok(integration);
            }
            let a = *normalisation.get_or_insert_with(|| k.iter().fold(0.0, |m, v| m.max(v.abs())));

            // MTest's system: the tangent operator bordered by the imposed
            // strains, and the residual: the stress minus the imposed
            // stresses and the multipliers, then the gaps to the imposed
            // strains.
            let mut system = vec![0.0; unknowns * unknowns];
            let mut residual = vec![0.0; unknowns];
            for &(row, s) in &stress_rows {
                system[unknowns * row..unknowns * row + n].copy_from_slice(&k[n * s..n * (s + 1)]);
                residual[row] = stress1[s];
            }
            let mut multiplier = n;
            for &(kind, c, evolution) in &constraints {
                match kind {
                    Imposed::Stress => residual[c] -= evolution.value(target),
                    Imposed::Strain => {
                        system[unknowns * multiplier + c] -= a;
                        system[unknowns * c + multiplier] -= a;
                        residual[c] -= a * u1[multiplier];
                        residual[multiplier] -= a * (u1[c] - evolution.value(target));
                        multiplier += 1;
                    }
                }
            }

            // δu = system⁻¹·residual: Gauss elimination with partial
            // pivoting, then u1 −= δu.
            let m = unknowns;
            let mut delta = residual.clone();
            let mut singular = false;
            for c in 0..m {
                let pivot = (c..m)
                    .max_by(|&x, &y| system[m * x + c].abs().total_cmp(&system[m * y + c].abs()))
                    .unwrap_or(c);
                if system[m * pivot + c] == 0.0 {
                    singular = true;
                    break;
                }
                for j in 0..m {
                    system.swap(m * c + j, m * pivot + j);
                }
                delta.swap(c, pivot);
                for row in c + 1..m {
                    let factor = system[m * row + c] / system[m * c + c];
                    for j in c..m {
                        system[m * row + j] -= factor * system[m * c + j];
                    }
                    delta[row] -= factor * delta[c];
                }
            }
            if singular {
                break Err("singular system".to_owned());
            }
            for row in (0..m).rev() {
                let known: f64 = (row + 1..m).map(|j| system[m * row + j] * delta[j]).sum();
                delta[row] = (delta[row] - known) / system[m * row + row];
            }
            for (u, d) in u1.iter_mut().zip(&delta) {
                *u -= d;
            }

            // MTest's criteria: never at the first iteration, then the
            // correction and the residual of this iteration on the strain
            // components, and every constraint.
            let correction = delta[..n].iter().fold(0.0_f64, |m, d| m.max(d.abs()));
            let largest = residual[..n].iter().fold(0.0_f64, |m, r| m.max(r.abs()));
            let converged = iterations > 1
                && correction <= test.strain_epsilon
                && largest <= test.stress_epsilon
                && constraints.iter().all(|&(kind, c, evolution)| match kind {
                    Imposed::Strain => {
                        (u1[c] - evolution.value(target)).abs() < test.strain_epsilon
                    }
                    Imposed::Stress => {
                        (stress1[c] - evolution.value(target)).abs() < test.stress_epsilon
                    }
                });
            if converged {
                break Ok(integration);
            }
            if iterations == test.maximum_iterations
                || !correction.is_finite()
                || !largest.is_finite()
            {
                break Err(format!(
                    "no convergence after {iterations} iterations (residual {largest:e})"
                ));
            }
        };
        match outcome {
            Ok(integration) => {
                advanced = true;
                if unknowns == 0 {
                    println!("from {t} to {target}: integrated");
                } else {
                    println!("from {t} to {target}: {iterations} iteration(s)");
                }
                previous = Some((u0, target - t));
                (u0, stress0, isv0) = (u1, stress1, isv1);
                (stored_energy, dissipated_energy) =
                    (integration.stored_energy, integration.dissipated_energy);
                t = target;
                if end - t <= 1e-12 * end.abs().max(1.0) {
                    t = end;
                    next += 1;
                    sub_steps = 0;
                    if next < test.times.len() {
                        dt = test.times[next] - t;
                    }
                }
            }
            Err(e) => {
                advanced = false;
                sub_steps += 1;
                if sub_steps == test.maximum_sub_steps {
                    return Err(format!(
                        "from {t} to {target}: {e}, after {sub_steps} sub-steps"
                    ));
                }
                println!("from {t} to {target}: {e}, halving the time step");
                dt /= 2.0;
            }
        }
    }
}
