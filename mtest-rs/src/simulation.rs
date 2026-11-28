//! The time loop, MTest's: at each step, Newton iterations on the strain
//! components and one Lagrange multiplier per imposed strain, until the
//! stresses of the free components reach the imposed ones (zero by default).

use rustfront::{
    Behaviour, FinalState, Frame, Hypothesis, InitialState, MaterialProperty, TangentOperator,
    TimeStep, VariableType,
};

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
    if finite_strain
        && (d.thermodynamic_forces.list[0].name != "Stress"
            || d.tangent_operator_blocks
                != [("Stress".to_owned(), "DeformationGradient".to_owned())])
    {
        return Err(format!(
            "`{}`: mtest-rs does not convert other stress measures and tangent operators \
             than CAUCHY and DSIG_DF yet",
            d.name
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
    let (n, m) = (d.gradients.size, d.thermodynamic_forces.size);

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
    if let Some(name) = unknown.into_iter().find(|name| !declared(name)) {
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
    let zero = Evolution::Constant(0.0);
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
    let rest = Evolution::Constant(if finite_strain { 1.0 } else { 0.0 });
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
    let mut u0 = vec![0.0; unknowns];
    if finite_strain {
        u0[..3].fill(1.0);
    }
    let mut stress0 = vec![0.0; m];
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
    let mut k = vec![0.0; d.tangent_operator_size];
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
        let (external0, external1) = (values(&external, t), values(&external, target));
        // No prediction: the iterations start from the previous state.
        let mut u1 = u0.clone();
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
            let (mut g0, mut g1, mut sig0) = (u0[..n].to_vec(), u1[..n].to_vec(), stress0.clone());
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
            let mut s1 = FinalState {
                gradients: &g1,
                thermodynamic_forces: &mut stress1,
                material_properties: &material1,
                internal_state_variables: &mut isv1,
                external_state_variables: &external1,
            };
            let tangent = Some((TangentOperator::ConsistentTangent, k.as_mut_slice()));
            // As MGIS by default: the behaviour may not propose a larger step.
            let step = TimeStep {
                dt: target - t,
                max_scaling_factor: 1.0,
            };
            let integration = match behaviour.integrate(step, &s0, &mut s1, tangent) {
                Ok(integration) => integration,
                Err(e) => break Err(e.to_string()),
            };
            if let Some(r) = &test.rotation {
                behaviour
                    .rotate_thermodynamic_forces(r, Frame::Global, &mut stress1)
                    .and_then(|()| behaviour.rotate_tangent_operator(r, Frame::Global, &mut k))
                    .map_err(|e| e.to_string())?;
            }
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
