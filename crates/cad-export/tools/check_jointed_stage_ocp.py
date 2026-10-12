"""Independent prescribed-joint placement, STEP geometry and anchored-section checks."""
import argparse,importlib.metadata,json,math,pathlib,platform,sys
import OCP
from check_constrained_fixture_ocp import body_properties,require,json_output
from check_sections_ocp import close
from check_step_metadata_ocp import command
from check_assembly_ocp import placement_matrix
from check_modular_stage_ocp import assembly_section
ROOT=pathlib.Path(__file__).resolve().parents[3]
RECIPE=ROOT/'examples/jointed_linear_stage.star'

def case(cad,directory,travel,angle,pose):
    flags=[item for k,v in dict(travel=travel,angle=angle,pose=pose).items() for item in ['--param',f'{k}={v}']]
    generation=directory/'generation';manifest=command([cad,'build',RECIPE,'--out-dir',generation,*flags])
    q=math.sqrt(.5);r=[[[1,0,0],[0,1,0],[0,0,1]],[[0,-1,0],[1,0,0],[0,0,1]],[[.5,.5,q],[.5,.5,-q],[-q,q,0]]][pose]
    def world(p):return [sum(r[i][j]*p[j] for j in range(3))+[7,-4,2][i] for i in range(3)]
    c=math.cos(angle);s=math.sin(angle);rz=[[c,-s,0],[s,c,0],[0,0,1]]
    rr=[[sum(r[i][k]*rz[k][j] for k in range(3)) for j in range(3)] for i in range(3)]
    expected={('stage','rail_left'):([0,0,0],r),('stage','rail_right'):([0,70,0],r),('stage','carriage'):([travel,0,20],r),('stage','head','hub'):([travel+20,45,28],rr),('stage','head','arm'):([travel+20-5*c+5*s,45-5*s-5*c,36],rr)}
    assembly=json.loads((generation/'jointed_linear_stage.assembly.json').read_text())
    for instance in assembly['instances']:
        origin,matrix=expected[tuple(assembly['instance_paths'][instance['id']])];actual=placement_matrix(instance['transform'])
        for i in range(3):
            close(actual[i][3],world(origin)[i],'joint origin')
            for j in range(3):close(actual[i][j],matrix[i][j],'joint orientation')
    reports=manifest['outputs']['jointed_linear_stage']['joints'];require(len(reports)==2,'two local prescribed joints')
    for report in reports:
        require(report['coordinate_frame']=='assembly_definition' and report['intrinsic_dof']==1 and report['remaining_dof']==0,'joint report semantics')
        require(report['residual_translation_norm_mm']<1e-7 and report['residual_angle_rad']<1e-7,'joint residuals')
    rv=160*396;cv=(3600-36*math.pi)*8;hv=864*math.pi;av=2000-16*math.pi
    rail_z=(64000*10-640*(20-2/3))/rv;arm_offset=(2000*25-16*math.pi*5)/av-5
    volumes=[2*rv,cv,hv,av];centers=[[80,45,rail_z],[travel+20,45,24],[travel+20,45,31],[travel+20+arm_offset*c,45+arm_offset*s,38]]
    volume=sum(volumes);center=world([sum(v*p[i] for v,p in zip(volumes,centers))/volume for i in range(3)])
    step=generation/'jointed_linear_stage.step';imported=body_properties(step);close(imported[0],volume,'STEP volume')
    for a,b in zip(imported[1],center):close(a,b,'STEP centroid')
    hierarchy=json_output([sys.executable,str(pathlib.Path(__file__).with_name('check_assembly_ocp.py')),str(step),'--instances','5','--definitions','4','--solids','5','--assembly-json',str(generation/'jointed_linear_stage.assembly.json')])
    view=json.loads((generation/'section-moving_carriage.json').read_text());section=assembly_section(step,view['section']['plane'],expected_solids=5)
    close(section['area'],3600-36*math.pi,'moving carriage area')
    for value in section['centroid']:close(value,0,'moving carriage centroid')
    require(command([cad,'verify',generation])['valid'],'generation integrity')
    result=dict(parameters=dict(travel=travel,angle=angle,pose=pose),joint_reports=reports,expected_volume=volume,expected_centroid=center,imported=imported,hierarchy=hierarchy,section=section)
    (directory/'report.json').write_text(json.dumps(result,indent=2,allow_nan=False)+'\n');return result

def main():
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('--cad',type=pathlib.Path,required=True);p.add_argument('--out-dir',type=pathlib.Path,required=True);a=p.parse_args();a.out_dir.mkdir(parents=True,exist_ok=False);cases=[]
    for label,values in [('low',(20,-math.pi/2,0)),('default',(50,.6,1)),('high',(100,math.pi/2,2))]:
        directory=a.out_dir/label;directory.mkdir();cases.append(case(a.cad.resolve(),directory,*values))
    print(json.dumps(dict(valid=True,cases=cases,retained_generations=[f'{label}/generation' for label in ['low','default','high']],retained_reports=[f'{label}/report.json' for label in ['low','default','high']],oracle=dict(python_version=platform.python_version(),ocp_version=str(OCP.__version__),cadquery_ocp_version=importlib.metadata.version('cadquery-ocp'))),indent=2,allow_nan=False))
if __name__=='__main__':main()
