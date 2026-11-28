#!/usr/bin/env python3
"""Compare two result files of MTest or mtest-rs, column by column.

usage: compare.py reference.res candidate.res [--rtol 1e-12] [--atol 0]

The tolerance of a column is rtol times the largest absolute value of its
group: all the components of the strain (or of the deformation gradient)
together, all those of the stress together, all those of an internal state
variable together, all the elements of an array (`a[0]`, `a[1]`) together,
every other column on its own. A free stress component,
zero up to round-off, is thus compared with the scale of the stresses, not
with its own noise. The tolerance is at least atol, for a group whose values
are all zero up to round-off.

mtest-rs follows MTest's algorithm, so both agree to round-off: the default
tolerance, 1e-12, is meant for results written with 14 digits
(`@OutputFilePrecision 14`).

Exit code: 0 when every column agrees, 1 otherwise.
"""

import argparse
import re
import sys

parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
parser.add_argument("reference")
parser.add_argument("candidate")
parser.add_argument("--rtol", type=float, default=1e-12)
parser.add_argument("--atol", type=float, default=0.0)
args = parser.parse_args()

# Each file: the column names of its header, then its rows of numbers.
files = []
for path in (args.reference, args.candidate):
    names, rows = [], []
    with open(path) as f:
        for line in f:
            if line.startswith("#"):
                # "# 2 column: 1th component of the strain (EXX)"
                names.append(line.split(":", 1)[1].strip())
            elif line.strip():
                rows.append([float(v) for v in line.split()])
    if any(len(row) != len(names) for row in rows):
        sys.exit(f"{path}: the rows do not have one value per column of the header")
    files.append((names, rows))
(names, reference), (candidate_names, candidate) = files
if len(names) != len(candidate_names):
    sys.exit(f"{len(names)} columns in the reference, {len(candidate_names)} in the candidate")
if len(reference) != len(candidate):
    sys.exit(f"{len(reference)} rows in the reference, {len(candidate)} in the candidate")


def group(name):
    # "1th component of the deformation gradient (FXX)": the quantity, here
    # the deformation gradient, groups the columns of its components, as
    # "first component of internal variable 'ElasticStrain' (ElasticStrainXX)".
    if "th component of the " in name:
        return name.split("th component of the ", 1)[1].split(" (", 1)[0]
    if " component of internal variable " in name:
        return name.split(" component of ", 1)[1].split(" (", 1)[0]
    array = re.fullmatch(r"(.+)\[\d+\]", name)
    return array.group(1) if array else name


scales = {}
for j, name in enumerate(names):
    largest = max(abs(row[j]) for row in reference)
    scales[group(name)] = max(scales.get(group(name), 0.0), largest)

failures = 0
print(f"{'column':<62} {'max difference':>15} {'tolerance':>12}")
for j, name in enumerate(names):
    difference = max(abs(r[j] - c[j]) for r, c in zip(reference, candidate))
    tolerance = max(args.rtol * scales[group(name)], args.atol)
    ok = difference <= tolerance
    failures += not ok
    print(f"{name:<62} {difference:15.3e} {tolerance:12.3e}  {'OK' if ok else 'FAILED'}")
print(f"{failures} column(s) out of tolerance" if failures else "all columns agree")
sys.exit(1 if failures else 0)
