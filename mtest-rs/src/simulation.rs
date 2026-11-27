//! The time loop: at each step, Newton iterations on the strain components
//! that are not imposed, until their stresses reach the imposed ones (zero by
//! default).

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

    // What is imposed on each component. In plane strain, `EZZ` is zero.
    let zero = Evolution::Constant(0.0);
    let mut imposed_strain: Vec<Option<&Evolution>> = vec![None; n];
    let mut imposed_stress: Vec<Option<&Evolution>> = vec![None; n];
    if test.hypothesis == Hypothesis::PlaneStrain {
        imposed_strain[2] = Some(&zero);
    }
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
        if imposed_strain[i].is_some() || imposed_stress[i].is_some() {
            return Err(format!(
                "`{component}`: this component is already imposed (in plane strain, EZZ is zero)"
            ));
        }
        match kind {
            Imposed::Strain => imposed_strain[i] = Some(evolution),
            Imposed::Stress => imposed_stress[i] = Some(evolution),
        }
    }
    let free: Vec<usize> = (0..n).filter(|&i| imposed_strain[i].is_none()).collect();

    let mut strain0 = vec![0.0; n];
    let mut stress0 = vec![0.0; n];
    let (mut stored_energy, mut dissipated_energy) = (0.0, 0.0);
    let mut k = vec![0.0; d.tangent_operator_size];
    let mut rows = Vec::new();
    for (step, &end) in test.times.iter().enumerate() {
        // From the previous time to `end`, halving the step on failure.
        let mut start = if step == 0 { end } else { test.times[step - 1] };
        let mut target = end;
        let mut sub_steps = 0;
        while start < end {
            let (material0, material1) = (values(material, start), values(material, target));
            let (external0, external1) = (values(external, start), values(external, target));
            let mut strain1 = strain0.clone();
            for (i, imposed) in imposed_strain.iter().enumerate() {
                if let Some(evolution) = imposed {
                    strain1[i] = evolution.value(target);
                }
            }
            let target_stress: Vec<f64> = imposed_stress
                .iter()
                .map(|imposed| imposed.map_or(0.0, |e| e.value(target)))
                .collect();
            let mut stress1 = vec![0.0; n];
            let mut isv1 = isv0.clone();
            // The last correction of the free strains: none needed when every
            // component is imposed.
            let mut correction = if free.is_empty() { 0.0 } else { f64::INFINITY };
            let mut iterations = 0;
            let outcome = loop {
                iterations += 1;
                let s0 = InitialState {
                    gradients: &strain0,
                    thermodynamic_forces: &stress0,
                    material_properties: &material0,
                    internal_state_variables: &isv0,
                    external_state_variables: &external0,
                    stored_energy,
                    dissipated_energy,
                };
                let mut s1 = FinalState {
                    gradients: &strain1,
                    thermodynamic_forces: &mut stress1,
                    material_properties: &material1,
                    internal_state_variables: &mut isv1,
                    external_state_variables: &external1,
                };
                let tangent = Some((TangentOperator::ConsistentTangent, k.as_mut_slice()));
                let integration = match behaviour.integrate(target - start, &s0, &mut s1, tangent) {
                    Ok(integration) => integration,
                    Err(e) => break Err(e.to_string()),
                };
                let residual: Vec<f64> = free
                    .iter()
                    .map(|&i| stress1[i] - target_stress[i])
                    .collect();
                let largest = residual.iter().fold(0.0_f64, |m, r| m.max(r.abs()));
                if largest < test.stress_epsilon && correction < test.strain_epsilon {
                    break Ok(integration);
                }
                if iterations == test.maximum_iterations {
                    break Err(format!(
                        "no convergence after {iterations} iterations (residual {largest:e})"
                    ));
                }
                // Solve K·δ = −r on the free components: Gauss elimination
                // with partial pivoting on the free rows and columns of the
                // tangent operator.
                let m = free.len();
                let mut a: Vec<f64> = free
                    .iter()
                    .flat_map(|&i| free.iter().map(|&j| k[n * i + j]).collect::<Vec<_>>())
                    .collect();
                let mut delta: Vec<f64> = residual.iter().map(|r| -r).collect();
                let mut singular = false;
                for c in 0..m {
                    let pivot = (c..m)
                        .max_by(|&x, &y| a[m * x + c].abs().total_cmp(&a[m * y + c].abs()))
                        .unwrap_or(c);
                    if a[m * pivot + c] == 0.0 {
                        singular = true;
                        break;
                    }
                    for j in 0..m {
                        a.swap(m * c + j, m * pivot + j);
                    }
                    delta.swap(c, pivot);
                    for row in c + 1..m {
                        let factor = a[m * row + c] / a[m * c + c];
                        for j in c..m {
                            a[m * row + j] -= factor * a[m * c + j];
                        }
                        delta[row] -= factor * delta[c];
                    }
                }
                if singular {
                    break Err("singular tangent operator".to_owned());
                }
                for row in (0..m).rev() {
                    let known: f64 = (row + 1..m).map(|j| a[m * row + j] * delta[j]).sum();
                    delta[row] = (delta[row] - known) / a[m * row + row];
                }
                correction = delta.iter().fold(0.0_f64, |m, d| m.max(d.abs()));
                for (&i, d) in free.iter().zip(&delta) {
                    strain1[i] += d;
                }
            };
            match outcome {
                Ok(integration) => {
                    println!("from {start} to {target}: {iterations} iteration(s)");
                    (strain0, stress0, isv0) = (strain1, stress1, isv1);
                    (stored_energy, dissipated_energy) =
                        (integration.stored_energy, integration.dissipated_energy);
                    start = target;
                    target = end;
                }
                Err(e) if sub_steps < test.maximum_sub_steps => {
                    println!("from {start} to {target}: {e}, halving the time step");
                    sub_steps += 1;
                    target = start + (target - start) / 2.0;
                }
                Err(e) => {
                    return Err(format!(
                        "from {start} to {target}: {e}, after {sub_steps} sub-steps"
                    ));
                }
            }
        }
        let mut row = vec![end];
        row.extend(&strain0);
        row.extend(&stress0);
        row.extend(&isv0);
        row.extend([stored_energy, dissipated_energy]);
        rows.push(row);
    }
    Ok(rows)
}
