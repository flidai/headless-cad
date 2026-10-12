#!/usr/bin/env python3
"""Exercise durable source revisions through actual MCP process restarts.

The client alone downloads a new delivery. This verifies local recovery and
retry semantics, not autonomous design, release acceptance or hosted isolation.
"""
import argparse
import hashlib
import json
from pathlib import Path
import tempfile
from agent_drive_demo import Client, run
from evidence import publish_new


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--cad',type=Path,default=Path('target/release/cad'))
    parser.add_argument('--workspace',type=Path,default=Path('.'))
    parser.add_argument('--state-dir',type=Path,required=True)
    parser.add_argument('--out-dir',type=Path,required=True)
    args=parser.parse_args()
    cad,workspace,state=args.cad.resolve(),args.workspace.resolve(),args.state_dir.absolute()
    destination=args.out_dir.absolute()
    if destination.exists():parser.error('choose a new delivery directory')
    if state.exists() and any(state.iterdir()):parser.error('choose an empty separate state directory')
    source_path=workspace/'examples/drive_module_rotation.star'
    source=source_path.read_text();original_hash=hashlib.sha256(source.encode()).hexdigest()
    revision=source.replace('name = "shaft_diameter", default = 12','name = "shaft_diameter", default = 14').replace('name = "mounting_pitch", default = 60','name = "mounting_pitch", default = 72')
    assert revision!=source
    destination.parent.mkdir(parents=True,exist_ok=True)
    stage=Path(tempfile.mkdtemp(prefix='.recovery-demo-',dir=destination.parent))
    first=Client(cad,workspace,state)
    try:
        base=first.tool('cad_snapshot',{'recipe':'examples/drive_module_rotation.star'})
        candidate=first.tool('cad_revise',{'snapshot_id':base['snapshot_id'],'source':revision})
        before_args={'snapshot_id':base['snapshot_id'],'mode':'build','request_id':'drive-before'}
        after_args={'snapshot_id':candidate['snapshot_id'],'mode':'build','request_id':'drive-after'}
        rejected_args={**after_args,'request_id':'drive-rejected-gap','params':{'required_clearance':.3}}
        before=first.tool('cad_evaluate',before_args)
        rejected=first.tool('cad_evaluate',rejected_args,error=True)
        after=first.tool('cad_evaluate',after_args)
        assert before['worker']['success'] and after['worker']['success'] and not rejected['worker']['success']
        first.process.kill();first.process.wait(timeout=5)
    finally:first.close()
    second=Client(cad,workspace,state)
    try:
        discovered=second.tool('cad_snapshots',{})
        assert {s['snapshot_id'] for s in discovered['snapshots']}=={base['snapshot_id'],candidate['snapshot_id']}
        jobs_before=second.tool('cad_jobs',{})
        assert len(jobs_before['jobs'])==3
        assert second.tool('cad_evaluate',before_args)==before
        assert second.tool('cad_evaluate',after_args)==after
        assert second.tool('cad_evaluate',rejected_args,error=True)==rejected
        conflict=second.tool('cad_evaluate',{**after_args,'params':{'shaft_diameter':15}},error=True)
        assert len(second.tool('cad_jobs',{})['jobs'])==3
        for name,job in [('before',before),('after',after)]:
            verified=second.tool('cad_verify',{'job_id':job['job_id'],'manifest_digest':job['manifest_hash']})
            assert verified['success'],verified
            second.download(job,stage/name)
            run(cad,'verify',stage/name)
        trace=first.trace+second.trace
    finally:second.close()
    assert hashlib.sha256(source_path.read_bytes()).hexdigest()==original_hash
    evidence={'valid':True,'source_workspace_unchanged':True,'durable_jobs':3,'recovered_retry_results':3,
              'snapshots':discovered,'before_job':before['job_id'],'after_job':after['job_id'],
              'rejected_job':rejected,'conflicting_retry':conflict,'trace':trace,
              'scope':'One local writer; no accepted-release publication, hosted service or autonomous AI reasoning claim'}
    (stage/'evidence.json').write_text(json.dumps(evidence,indent=2))
    (stage/'index.html').write_text('''<!doctype html><html lang="en"><meta charset="utf-8"><meta name="viewport" content="width=device-width"><title>Recover a drive revision</title><style>body{font:17px system-ui;max-width:1050px;margin:60px auto;padding:24px;color:#23433d;background:#f4f7f6}h1{font-size:42px}section{background:white;padding:28px;border:1px solid #cbdad6;border-radius:12px;margin:20px 0}a{color:#12694f}code{font-size:14px}</style><h1>Recover a drive revision</h1><p>Captured Starlark revisions and verified results survive an actual agent process restart. Repeating a request recovers its recorded outcome.</p><section><h2>Revision A</h2><p>12 mm shaft · 60 mm mounting pitch</p><a href="before/rotation-rotor.html">Inspect assembly, motion and recipe</a></section><section><h2>Revision B</h2><p>14 mm shaft · 72 mm mounting pitch</p><a href="after/rotation-rotor.html">Inspect recovered revision</a></section><section><h2>Recovery evidence</h2><p>Three durable jobs, three recovered retry results, and a rejected conflicting retry. The invalid clearance proposal remains a recorded failure. The source workspace is unchanged.</p><a href="evidence.json">Read protocol evidence</a><p>Experimental local recovery. Release acceptance and hosted operation remain separate work. All inspection views are read-only.</p></section></html>''')
    publish_new(stage,destination)
    print(json.dumps({'valid':True,'directory':str(destination),'index':str(destination/'index.html'),'durable_jobs':3,'protocol_calls':len(trace)}))


if __name__=='__main__':main()
