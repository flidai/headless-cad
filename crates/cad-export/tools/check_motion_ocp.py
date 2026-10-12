"""Independent closure for every recorded sample plus imported STEP at drive targets."""
import argparse,importlib.metadata,json,math,pathlib,platform
import OCP
from check_step_metadata_ocp import command
from check_constrained_fixture_ocp import require
from check_four_bar_ocp import case as step_case
ROOT=pathlib.Path(__file__).resolve().parents[3]
def expected_pins(angle,length):
    b=[30*math.cos(angle),30*math.sin(angle)];d=math.hypot(80-b[0],b[1]);u=[(80-b[0])/d,-b[1]/d]
    along=(length**2-2500+d*d)/(2*d);h=math.sqrt(length**2-along**2)
    return dict(a=[0,0],b=b,c=[b[0]+along*u[0]-h*u[1],b[1]+along*u[1]+h*u[0]],d=[80,0])
def main():
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('--cad',type=pathlib.Path,required=True);p.add_argument('--out-dir',type=pathlib.Path,required=True);a=p.parse_args();a.out_dir.mkdir(parents=True,exist_ok=False);cad=a.cad.resolve();results=[];generations=[];reports=[]
    for label,length,high,step,pose in [('short',65,1.1,.8,0),('nominal',70,1.3,2,1),('long',75,1.5,4,2)]:
        directory=a.out_dir/label;directory.mkdir();generation=directory/'generation'
        flags=[item for k,v in dict(coupler_length=length,high_angle=high,max_pin_step=step,pose=pose).items() for item in ['--param',f'{k}={v}']]
        manifest=command([cad,'build',ROOT/'examples/motion_four_bar.star','--out-dir',generation,*flags])
        document=json.loads((generation/'motion-crank_cycle.json').read_text());study=document['study']['solution'];frames=study['frames'];require(study['completed_requested_frames']==4,'requested sequence completed')
        targets=[(frame['requested_index'],frame['coordinates']['input_angle']) for frame in frames if frame['requested_index'] is not None]
        require(targets==[(0,.9),(1,high),(2,.5),(3,.9)],'authored target order preserved')
        require(len(frames)>4,'adaptive intermediate samples present');require(study['attempted_solves']<=512,'native attempt budget')
        previous=None;max_error=0
        for frame in frames:
            coordinates=expected_pins(frame['coordinates']['input_angle'],length)
            for pin,expected in coordinates.items():
                error=math.dist(frame['pins'][pin],expected);max_error=max(max_error,error);require(error<1e-5,'independent sample closure')
            require(frame['status']=='fully_constrained' and frame['degrees_of_freedom']==0,'sample solver acceptance')
            require(frame['max_residual_mm']<=1e-7 and frame['max_angular_residual_radians']<=1e-9,'sample residuals')
            for branch in frame['branches']:require(branch['signed_double_area_mm2']>branch['uncertainty_margin_mm2'],'selected branch margin')
            if previous:
                actual_drive=abs(frame['coordinates']['input_angle']-previous['coordinates']['input_angle']);actual_pin=max(math.dist(frame['pins'][pin],previous['pins'][pin]) for pin in coordinates)
                require(actual_drive<=.08 and actual_pin<=step,'independent recorded step bounds')
                require(abs(actual_drive-frame['max_drive_step_radians'])<1e-12 and abs(actual_pin-frame['max_pin_step_mm'])<1e-10,'step evidence matches sampled coordinates')
            previous=frame
        require(command([cad,'verify',generation])['valid'],'motion generation integrity');generations.append(f'{label}/generation')
        imported=[]
        for name,angle in [('start',.9),('high',high),('low',.5)]:
            sample=directory/name;sample.mkdir();imported.append(step_case(cad,sample,angle,length,pose));generations.append(f'{label}/{name}/generation');reports.append(f'{label}/{name}/report.json')
        result=dict(parameters=dict(length=length,high=high,step=step,pose=pose),frames=len(frames),attempts=study['attempted_solves'],max_independent_position_error_mm=max_error,targets=targets,static_exchange=imported,motion_context_hash=manifest['mechanism_context_hash'])
        (directory/'report.json').write_text(json.dumps(result,indent=2,allow_nan=False)+'\n');reports.append(f'{label}/report.json');results.append(result)
    print(json.dumps(dict(valid=True,cases=results,retained_generations=generations,retained_reports=reports,oracle=dict(python_version=platform.python_version(),ocp_version=str(OCP.__version__),cadquery_ocp_version=importlib.metadata.version('cadquery-ocp'))),indent=2,allow_nan=False))
if __name__=='__main__':main()
