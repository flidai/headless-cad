#!/usr/bin/env python3
"""Independent exact-rational size interval acceptance; not a collision check."""
import argparse
from fractions import Fraction
import hashlib
import itertools
import json
import math
from pathlib import Path
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[1]


def require(condition, message):
    if not condition:
        raise AssertionError(message)


def fraction(value):
    return Fraction.from_float(float(value))


def inventory(directory):
    return {str(p.relative_to(directory)): hashlib.sha256(p.read_bytes()).hexdigest()
            for p in sorted(directory.rglob('*')) if p.is_file()}


def run(cad, recipe, directory, bore, pin):
    return subprocess.run([str(cad), 'build', str(recipe), '--out-dir', str(directory),
                           '--param', f'bore_allowance={bore}', '--param', f'pin_allowance={pin}'],
                          capture_output=True, text=True)


def check_interval(report, low, high):
    actual_low = report['lower_limit_mm']
    actual_high = report['upper_limit_mm']
    require(math.isfinite(actual_low) and math.isfinite(actual_high), 'finite stack limits')
    require(fraction(actual_low) <= low, 'reported lower bound must enclose exact source arithmetic')
    require(fraction(actual_high) >= high, 'reported upper bound must enclose exact source arithmetic')
    # A handful of directed operations can widen by ulps. A coarse epsilon band
    # would conceal an invalid interval engine; bound slack to operand-scale ulps.
    slack = fraction(64 * math.ulp(6.0))
    require(low - fraction(actual_low) <= slack, 'lower bound unreasonably loose')
    require(fraction(actual_high) - high <= slack, 'upper bound unreasonably loose')
    if 'nominal_mm' in report:
        require(abs(report['nominal_mm'] - float(fraction(6) - fraction(5.6))) <= math.ulp(6), 'nominal difference')
        require(len(report['terms']) == 2, 'two source measurements')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--cad', type=Path, default=ROOT / 'target/release/cad')
    parser.add_argument('--recipe', type=Path, default=ROOT / 'examples/bearing_clearance.star')
    parser.add_argument('--out-dir', type=Path)
    args = parser.parse_args()
    temporary = tempfile.TemporaryDirectory(prefix='cad-tolerance-stack-') if args.out_dir is None else None
    directory = Path(temporary.name) if temporary else args.out_dir
    if not temporary:
        directory.mkdir(parents=True, exist_ok=False)
    cases = []
    try:
        output = directory / 'generation'
        baseline = run(args.cad, args.recipe, output, .01, .01)
        require(baseline.returncode == 0, 'valid baseline build: ' + baseline.stderr)
        for bore, pin in itertools.product([.01, .02, .04], repeat=2):
            before = inventory(output)
            low = fraction(6) - fraction(5.6)
            high = (fraction(6) + fraction(bore)) - (fraction(5.6) - fraction(pin))
            expected_pass = low >= fraction(.39) and high <= fraction(.45)
            result = run(args.cad, args.recipe, output, bore, pin)
            require((result.returncode == 0) == expected_pass,
                    f'pass/fail mismatch bore={bore} pin={pin}: {result.stdout} {result.stderr}')
            if expected_pass:
                summary = json.loads(result.stdout)
                report = summary['tolerance_stacks']['diametral_clearance']
                check_interval(report, low, high)
                verified = subprocess.run([str(args.cad), 'verify', str(output)], capture_output=True, text=True)
                require(verified.returncode == 0 and json.loads(verified.stdout)['valid'], 'generation integrity')
            else:
                require(inventory(output) == before, 'failed obligation changed published generation')
                diagnostic = json.loads(result.stderr)
                require(diagnostic['code'] == 'tolerance_stack_failed', 'typed stack failure')
                require(diagnostic['stack_id'] == 'diametral_clearance', 'failure identifies source stack')
                check_interval(diagnostic, low, high)
            cases.append(dict(bore_allowance=bore, pin_allowance=pin, passed=expected_pass,
                              exact_lower=str(low), exact_upper=str(high), failure_preserves_generation=not expected_pass))
        report = dict(valid=True, scope='exact-rational manufacturing size interval; no collision claim',
                      cases=cases, passed=sum(c['passed'] for c in cases), rejected=sum(not c['passed'] for c in cases))
        (directory / 'report.json').write_text(json.dumps(report, indent=2) + '\n')
        print(json.dumps(report, indent=2))
    finally:
        if temporary:
            temporary.cleanup()


if __name__ == '__main__':
    main()
