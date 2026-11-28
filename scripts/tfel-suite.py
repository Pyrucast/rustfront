#!/usr/bin/env python3
"""Run the tests of TFEL's generic interface with MTest and with mtest-rs.

usage: tfel-suite.py <TFEL sources> [--work DIR] [--jobs N] [--filter REGEX]
                     [--skip-build] [--timeout SECONDS]

The TFEL sources are those of the TFEL in the PATH (a checkout of the tag
TFEL-5.1.0 for TFEL 5.1.0). The script reads
mfront/tests/behaviours/generic/CMakeLists.txt: the behaviours of the library
MFrontGenericBehaviours, compiled here with mfront, and the tests
test_generic and test_generic_fs, one .mtest file each. Every test, and a
finite strain one for every stress measure and tangent operator, is run by
MTest and by mtest-rs once its placeholders (@library@, ...) are replaced
and its results asked with 14 digits, then the two result files are compared
by compare.py, the differences below 1e-20 taken as round-off of zeros.

Nothing of TFEL is copied into the repository: everything is written in the
work directory, target/tfel-suite by default. report.txt there gives the
outcome of every test and counts the reasons why mtest-rs fails.

Needs mfront, mtest, make, cargo and python3 in the PATH.
"""

import argparse
import collections
import concurrent.futures
import os
import re
import shutil
import subprocess
import sys
from pathlib import Path

parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
parser.add_argument("tfel", help="the TFEL sources")
parser.add_argument("--work", help="the work directory (target/tfel-suite)")
parser.add_argument("--jobs", type=int, default=os.cpu_count() or 1)
parser.add_argument("--filter", default="", help="only the tests matching this regex")
parser.add_argument("--skip-build", action="store_true", help="keep the library already built")
parser.add_argument("--timeout", type=float, default=300.0, help="per run, in seconds")
args = parser.parse_args()

root = Path(__file__).resolve().parent.parent
tests = Path(args.tfel).resolve() / "mfront" / "tests"
generic = tests / "behaviours" / "generic"
work = Path(args.work).resolve() if args.work else root / "target" / "tfel-suite"
build = work / "build"
library = build / "src" / "libBehaviour.so"
cases = work / "cases"

# The behaviours and the tests of CMakeLists.txt. Those of quantities need a
# TFEL built with them (enable-mfront-quantity-tests): left aside.
cmake = re.sub(r"#.*", "", (generic / "CMakeLists.txt").read_text())
cmake = re.sub(
    r"if\(enable-mfront-quantity-tests\).*?endif\(enable-mfront-quantity-tests\)",
    "",
    cmake,
    flags=re.S,
)
sources = []
for m in re.finditer(r"(?:set|list)\((?:APPEND\s+)?mfront_tests_SOURCES([^)]*)\)", cmake):
    sources += m.group(1).split()
# (case, test, finite strain options, reference file, expected to fail)
STRESS_MEASURES = ["CAUCHY", "PK1", "PK2"]
TANGENT_OPERATORS = ["DPK1_DF", "DSIG_DF", "DS_DEGL", "DTAU_DDF"]
runs = []
for m in re.finditer(r"\btest_generic(_fs)?\(([^)]*)\)", cmake):
    name, *options = m.group(2).split()
    will_fail = "WILL_FAIL" in options
    references = [o for o in options if o not in ("WILL_FAIL", "NO_XML_OUTPUT")]
    reference = references[0] if references else None
    if m.group(1):
        for ss in STRESS_MEASURES:
            for to in TANGENT_OPERATORS:
                runs.append((f"{name}-{ss}-{to}", name, (ss, to), reference, will_fail))
    else:
        runs.append((name, name, None, reference, will_fail))
runs = [r for r in runs if re.search(args.filter, r[0])]

if not args.skip_build:
    print(f"== Compiling the {len(sources)} behaviours with mfront, in {build}")
    shutil.rmtree(build, ignore_errors=True)
    build.mkdir(parents=True)
    search = [f"--search-path={tests / d}" for d in ("behaviours", "models", "properties")]
    files = [str(tests / "behaviours" / f"{s}.mfront") for s in sources]
    with open(build / "mfront.log", "w") as log:
        steps = [
            ["mfront", "--omake", "--interface=generic", *search, *files],
            ["make", "-C", "src", "-f", "Makefile.mfront", f"-j{args.jobs}"],
        ]
        for step in steps:
            if subprocess.run(step, cwd=build, stdout=log, stderr=subprocess.STDOUT).returncode:
                sys.exit(f"`{step[0]}` failed, see {build / 'mfront.log'}")

print("== Building mtest-rs")
subprocess.run(
    ["cargo", "build", "--quiet", "--release", "--manifest-path", str(root / "Cargo.toml"), "-p", "mtest-rs"],
    check=True,
)
mtest_rs = Path(os.environ.get("CARGO_TARGET_DIR", root / "target")) / "release" / "mtest-rs"
cases.mkdir(parents=True, exist_ok=True)


def run(case, test, finite_strain, reference, will_fail):
    """The outcome of one case: (kind, detail)."""
    text = (generic / f"{test}.mtest").read_text(errors="replace")
    substitutions = {"@library@": f'"{library}"', "@xml_output@": f'"{case}.xml"'}
    if reference:
        substitutions["@reference_file@"] = f'"{tests / "behaviours" / "references" / reference}"'
    if finite_strain:
        substitutions["@stress_measure@"], substitutions["@tangent_operator@"] = finite_strain
    missing = sorted(set(re.findall(r"@[a-z_0-9]+@", text)) - substitutions.keys())
    if missing:
        return "not run", f"needs {', '.join(missing)}"
    for placeholder, value in substitutions.items():
        text = text.replace(placeholder, value)
    # 6 digits by default: too few to compare to round-off.
    if "@OutputFilePrecision" not in text:
        text = "@OutputFilePrecision 14;\n" + text
    (cases / f"{case}.mtest").write_text(text)

    def execute(command):
        try:
            p = subprocess.run(command, cwd=cases, capture_output=True, text=True, timeout=args.timeout)
            return p.returncode, (p.stderr.strip().splitlines() or [""])[-1]
        except subprocess.TimeoutExpired:
            return None, "timeout"

    mtest, _ = execute(["mtest", "--verbose=level0", f"{case}.mtest"])
    rs, message = execute([str(mtest_rs), f"{case}.mtest", "--output", f"{case}-rs.res"])
    if will_fail:
        return "expected to fail", "mtest-rs fails too" if rs != 0 else "mtest-rs succeeds"
    if mtest != 0:
        return "MTest fails", "timeout" if mtest is None else f"exit {mtest}"
    if rs != 0:
        return "mtest-rs fails", message
    compare, _ = execute(
        [sys.executable, "-I", str(root / "scripts" / "compare.py"), "--atol", "1e-20", f"{case}.res", f"{case}-rs.res"]
    )
    # The MTest of TFEL 5.1 assembles a finite strain residual of its own:
    # with a loose criterion, both stop at different points.
    return ("agree", "") if compare == 0 else ("differ", "finite strain" if finite_strain else "")


print(f"== Running {len(runs)} cases with MTest and mtest-rs ({args.jobs} at a time)")
with concurrent.futures.ThreadPoolExecutor(args.jobs) as pool:
    outcomes = list(pool.map(lambda r: run(*r), runs))


def reason(message):
    """A failure of mtest-rs, without what changes from one case to another."""
    message = re.sub(r"^mtest-rs: ", "", message)
    message = re.sub(r"^\S+\.mtest: ", "", message)
    message = re.sub(r"line \d+: ", "", message)
    message = re.sub(r"`(?!@)[^`]*`", "`…`", message)
    return re.sub(r"\d+(\.\d+)?(e[-+]?\d+)?", "N", message)


kinds = collections.Counter(kind for kind, _ in outcomes)
reasons = collections.Counter(reason(d) for k, d in outcomes if k == "mtest-rs fails")
tests_by_kind = collections.defaultdict(set)
for (case, test, *_), (kind, _) in zip(runs, outcomes):
    tests_by_kind[kind].add(test)
lines = [f"{len(runs)} cases, {len({r[1] for r in runs})} tests, {len(sources)} behaviours", ""]
for kind, n in kinds.most_common():
    lines.append(f"{kind:<20} {n:>5} cases  {len(tests_by_kind[kind]):>4} tests")
lines += ["", "Why mtest-rs fails:"]
lines += [f"{n:>5}  {r}" for r, n in reasons.most_common()]
lines += ["", "Every case:"]
lines += [f"{case:<60} {kind:<20} {detail}" for (case, *_), (kind, detail) in zip(runs, outcomes)]
(work / "report.txt").write_text("\n".join(lines) + "\n")
print("\n".join(lines[: lines.index("Every case:")]))
print(f"Every case in {work / 'report.txt'}")
