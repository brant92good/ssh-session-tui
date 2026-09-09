"""Compare fresh-process CLI startup; no connections, windows or user data.

This measures launch through JSON response, not tab focus or network latency.
Run alone, with no other benchmark, and retain the raw samples.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import random
import statistics
import subprocess
import sys
import tempfile
import time


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', type=Path, required=True)
    parser.add_argument('--samples', type=int, default=30)
    parser.add_argument('--output', type=Path, required=True)
    options = parser.parse_args()
    if options.samples < 20:
        parser.error('Use at least 20 observations per runtime')
    source = Path(__file__).resolve().parents[1]
    binary = options.binary.resolve(strict=True)
    observations = {'python':[], 'rust':[]}
    flags = getattr(subprocess, 'CREATE_NO_WINDOW', 0)
    with tempfile.TemporaryDirectory(prefix='ssh-startup-') as temporary:
        root = Path(temporary)
        catalog = root/'catalog.json'
        catalog.write_text(json.dumps({'version':1, 'machines':[{'id':'demo', 'name':'Development', 'user':'dev',
            'routes':[{'id':'lan', 'name':'LAN', 'host':'192.0.2.10', 'port':22}]}]}))
        shared = ['--catalog',str(catalog),'--state-dir',str(root/'device'),'list','--json']
        commands = {'python':[sys.executable,'-E','-s',str(source/'app.py'),*shared], 'rust':[str(binary),*shared]}
        expected = None
        def measure(runtime):
            nonlocal expected
            started = time.perf_counter_ns()
            result = subprocess.run(commands[runtime], cwd=root, capture_output=True, timeout=15, creationflags=flags)
            elapsed = (time.perf_counter_ns() - started)/1_000_000
            assert result.returncode == 0, result.stderr
            value = json.loads(result.stdout)
            if expected is None:
                expected = value
            assert value == expected, 'The compared commands must return equivalent data'
            return elapsed
        for _ in range(3):
            for runtime in commands:
                measure(runtime)
        order = ['python','rust'] * options.samples
        random.Random(0).shuffle(order)
        for runtime in order:
            observations[runtime].append(measure(runtime))
    results = {}
    for runtime, values in observations.items():
        results[runtime] = {'median_ms':statistics.median(values),
            'p95_ms':statistics.quantiles(values,n=100,method='inclusive')[94],
            'min_ms':min(values), 'max_ms':max(values), 'samples_ms':values}
    report = {'measurement':'fresh process to complete equivalent list --json response',
              'cache':'3 warmups per runtime; OS file cache not cleared', 'samples_per_runtime':options.samples,
              'order':'seeded interleaved', 'system':platform.platform(), 'architecture':platform.machine(),
              'python':platform.python_version(), 'native_sha256':hashlib.sha256(binary.read_bytes()).hexdigest(),
              'native_version':subprocess.check_output([str(binary),'--version'], text=True, creationflags=flags).strip(),
              'results':results, 'limits':['one computer/run','not UI first paint','not shortcut or tab focus','not SSH network/login time']}
    options.output.parent.mkdir(parents=True, exist_ok=True)
    options.output.write_text(json.dumps(report, indent=2)+'\n',encoding='utf-8')
    print(json.dumps({name:{k:v for k,v in row.items() if k!='samples_ms'} for name,row in results.items()},indent=2))


if __name__ == '__main__':
    main()
