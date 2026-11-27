//! The time loop, MTest's: at each step, Newton iterations on the strain
//! components and one Lagrange multiplier per imposed strain, until the
//! stresses of the free components reach the imposed ones (zero by default).

use rustfront::{Behaviour, FinalState, Hypothesis, InitialState, TangentOperator, VariableType};

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

/// Runs the test. Each row of the result is a line of the output file: the
/// time, the strain, the stress, the internal state variables, the stored and
/// dissipated energies, at the initial time then at the end of each step.
pub fn run(test: &Test, behaviour: &Behaviour) -> Result<Vec<Vec<f64>>, String> {
    let d = behaviour.description();
    let small_strain = d.gradients.list.len() == 1
        && d.gradients.list[0].variable_type == VariableType::Stensor
        && d.tangent_operator_blocks == [("Stress".to_owned(), "Strain".to_owned())];
    if !small_strain {
        return Err(format!("`{}` is not a small strain behaviour", d.name));
    }
    let n = d.gradients.size;

    // The evolutions of the material properties and external state variables,
    // in the order of the behaviour.
    let mut evolutions = Vec::new();
    for (kind, given, expected) in [
        (
            "material property",
            &test.material_properties,
            &d.material_properties,
        ),
        (
            "external state variable",
            &test.external_state_variables,
            &d.external_state_variables,
        ),
    ] {
        if let Some((name, _)) = given
            .iter()
            .find(|(name, _)| !expected.list.iter().any(|v| v.name == *name))
        {
            return Err(format!("`{}` has no {kind} `{name}`", d.name));
        }
        let mut list = Vec::new();
        for v in &expected.list {
            if v.size != 1 {
                return Err(format!(
                    "the {kind} `{}` is not a scalar, which is not supported",
                    v.name
                ));
            }
            let evolution = given
                .iter()
                .find(|(name, _)| *name == v.name)
                .ok_or_else(|| format!("the {kind} `{}` is not given", v.name))?;
            list.push(&evolution.1);
        }
        evolutions.push(list);
    }
    let (material, external) = (&evolutions[0], &evolutions[1]);
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
    let suffixes = stensor_suffixes(test.hypothesis);
    for (kind, component, evolution) in &test.imposed {
        let prefix = match kind {
            Imposed::Strain => 'E',
            Imposed::Stress => 'S',
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
    // Plane strain: EZZ is zero. Plane stress, and axisymmetrical generalised
    // plane stress: the behaviour handles the out-of-plane strain, MTest
    // imposes a zero gradient and a zero stress on that component.
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
                "`{}` cannot be imposed in the {} hypothesis",
                suffixes[i], test.hypothesis
            ));
        }
    }
    constraints.extend(added.iter().map(|&(kind, i)| (kind, i, &zero)));
    let multipliers = constraints
        .iter()
        .filter(|(kind, _, _)| *kind == Imposed::Strain)
        .count();
    let unknowns = n + multipliers;

    // The unknowns at the beginning of the step: strains, then multipliers.
    let mut u0 = vec![0.0; unknowns];
    let mut stress0 = vec![0.0; n];
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
        let (material0, material1) = (values(material, t), values(material, target));
        let (external0, external1) = (values(external, t), values(external, target));
        // No prediction: the iterations start from the previous state.
        let mut u1 = u0.clone();
        let mut stress1 = vec![0.0; n];
        let mut isv1 = isv0.clone();
        let mut iterations = 0;
        let outcome = loop {
            iterations += 1;
            let s0 = InitialState {
                gradients: &u0[..n],
                thermodynamic_forces: &stress0,
                material_properties: &material0,
                internal_state_variables: &isv0,
                external_state_variables: &external0,
                stored_energy,
                dissipated_energy,
            };
            let mut s1 = FinalState {
                gradients: &u1[..n],
                thermodynamic_forces: &mut stress1,
                material_properties: &material1,
                internal_state_variables: &mut isv1,
                external_state_variables: &external1,
            };
            let tangent = Some((TangentOperator::ConsistentTangent, k.as_mut_slice()));
            let integration = match behaviour.integrate(target - t, &s0, &mut s1, tangent) {
                Ok(integration) => integration,
                Err(e) => break Err(e.to_string()),
            };
            let a = *normalisation.get_or_insert_with(|| k.iter().fold(0.0, |m, v| m.max(v.abs())));

            // MTest's system: the tangent operator bordered by the imposed
            // strains, and the residual: the stress minus the imposed
            // stresses and the multipliers, then the gaps to the imposed
            // strains.
            let mut system = vec![0.0; unknowns * unknowns];
            for i in 0..n {
                system[unknowns * i..unknowns * i + n].copy_from_slice(&k[n * i..n * (i + 1)]);
            }
            let mut residual = vec![0.0; unknowns];
            residual[..n].copy_from_slice(&stress1);
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
                println!("from {t} to {target}: {iterations} iteration(s)");
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
