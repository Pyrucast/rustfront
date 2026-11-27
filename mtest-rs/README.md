# mtest-rs

Test an [MFront] behaviour at one material point: read a `.mtest` file,
impose strains and stresses over time, write the evolution of the strains,
stresses and internal state variables in a `.res` file.

> **mtest-rs is an unofficial, independent reimplementation in Rust of a
> subset of [MTest], which is part of TFEL.** It is built on
> [rustfront](..). It is not affiliated with the TFEL project, CEA or EDF,
> and it is written from MTest's documentation and observed behaviour, not
> from its code.

It reads the same input files as MTest and writes result files with the same
columns, so that both can be compared: see `scripts/try.sh` and
`scripts/compare.py` at the root of the repository.

## Usage

```bash
mtest-rs tension.mtest                       # writes tension.res
mtest-rs tension.mtest --output other.res
```

```cpp
@ModellingHypothesis 'Tridimensional';
@Behaviour<generic> 'src/libBehaviour.so' 'Norton';
@MaterialProperty<constant> 'YoungModulus' 200e9;
@MaterialProperty<constant> 'PoissonRatio' 0.3;
@MaterialProperty<constant> 'NortonCoefficient' 5e-69;
@MaterialProperty<constant> 'NortonExponent' 8.2;
@ExternalStateVariable 'Temperature' 293.15;
@ImposedStress 'SXX' {0:0, 1:60e6};
@Times {0, 1 in 5, 100 in 20};
```

Components that are neither imposed a strain nor a stress are free, at zero
stress; in plane strain, `EZZ` is zero. As in MTest, off-diagonal components
are given and written multiplied by √2 (TFEL's convention).

## Supported keywords

| Keyword | Notes |
|---|---|
| `@ModellingHypothesis` | `Tridimensional` by default |
| `@Behaviour<generic>` | the `generic` interface only, small strain behaviours |
| `@MaterialProperty<constant\|function>` | scalars |
| `@ExternalStateVariable[<evolution\|function>]` | scalars |
| `@ImposedStrain`, `@ImposedStress` `[<evolution\|function>]` | components `EXX`…`EYZ`, `ERR EZZ ETT ERZ`, `S…` |
| `@InternalStateVariable` | initial value, `0` or `{…}` |
| `@Times` | `{0, 1 in 10, 2}` |
| `@Real` | a value or an expression, usable in functions |
| `@OutputFile`, `@OutputFilePrecision` | 6 digits by default |
| `@StrainEpsilon`, `@StressEpsilon` | `1e-12` and `1e-3` by default |
| `@MaximumNumberOfIterations`, `@MaximumNumberOfSubSteps` | `100` and `10` by default |
| `@Author`, `@Date`, `@Description` | ignored |

An evolution is a constant, a table `{t0:v0, t1:v1, …}` (linear in between,
constant outside), or with `<function>` an expression of `t`: numbers, `t`,
`@Real` constants, `+ - * / ^`, parentheses and `sin cos tan asin acos atan
sinh cosh tanh exp log ln log10 sqrt abs`.

## Differences with MTest

- Only small strain behaviours of the `generic` interface.
- No `@Test` (comparison with reference values), no JUnit report, no
  acceleration algorithms, no prediction policies, no rotation matrix, no
  pipe tests.
- Each time step is solved by Newton iterations on the free strain
  components with the consistent tangent operator, halving the step on
  failure. The converged results agree with MTest's to round-off; the number
  of iterations may differ by one.

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or
[MIT license](LICENSE-MIT) at your option.

[MFront]: https://thelfer.github.io/tfel/web/index.html
[MTest]: https://thelfer.github.io/tfel/web/mtest.html
