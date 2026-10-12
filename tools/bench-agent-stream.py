#!/usr/bin/env python3
"""Compare both real MCP delivery paths for one already-built durable candidate.

This is a local process/transport benchmark. It reports this machine and binary,
checks identical bytes independently with SHA-256, and verifies both generations.
It does not establish a hosted-service or commercial assembly-scale gate.
"""
import argparse
import hashlib
import json
import platform
from pathlib import Path
import sys
import tempfile
sys.path.insert(0,str(Path(__file__).resolve().parents[1]/'scripts'))
from agent_drive_demo import Client, run


def inventory(root):
    return {p.name:{'bytes':p.stat().st_size,'sha256':hashlib.sha256(p.read_bytes()).hexdigest()}
            for p in sorted(root.iterdir()) if p.is_file()}


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--cad',type=Path,default=Path('target/release/cad'))
    parser.add_argument('--workspace',type=Path,default=Path('.'))
    parser.add_argument('--state-dir',type=Path,required=True)
    parser.add_argument('--job-id')
    parser.add_argument('--report',type=Path,required=True)
    args=parser.parse_args()
    if args.report.exists():parser.error('choose a new report path')
    cad=args.cad.resolve()
    client=Client(cad,args.workspace.resolve(),args.state_dir.resolve())
    try:
        job_id=args.job_id
        if job_id is None:
            head=client.tool('cad_release',{})['revision']
            assert head,'choose a job ID when the store has no accepted candidate'
            job_id=head['job_id']
        job=client.tool('cad_job',{'job_id':job_id})
        assert job['mode']=='build' and job['worker']['success']
        with tempfile.TemporaryDirectory(prefix='cad-stream-benchmark-') as temp:
            root=Path(temp)
            baseline=client.download(job,root/'baseline')
            streamed=client.download(job,root/'streamed',stream=True)
            first,second=inventory(root/'baseline'),inventory(root/'streamed')
            assert first==second and len(first)==len(job['artifacts'])
            assert run(cad,'verify',root/'baseline')['valid']
            assert run(cad,'verify',root/'streamed')['valid']
        report={'valid':True,'platform':platform.platform(),'machine':platform.machine(),
                'binary_sha256':hashlib.sha256(cad.read_bytes()).hexdigest(),
                'job_id':job_id,'manifest_digest':job['manifest_hash'],
                'baseline':baseline,'streamed':streamed,
                'speedup':baseline['seconds']/streamed['seconds'],
                'identical_sha256_inventory':first,'both_deliveries_verified':True}
        args.report.parent.mkdir(parents=True,exist_ok=True)
        args.report.write_text(json.dumps(report,indent=2)+'\n')
        print(json.dumps({k:v for k,v in report.items() if k!='identical_sha256_inventory'}))
    finally:client.close()


if __name__=='__main__':main()
