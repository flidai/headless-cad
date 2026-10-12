"""Independent circle-intersection closure and imported STEP checks for solved links."""
import argparse,importlib.metadata,json,math,pathlib,platform,sys
import OCP
from check_constrained_fixture_ocp import body_properties,require,json_output
from check_sections_ocp import close
from check_step_metadata_ocp import command
from check_assembly_ocp import placement_matrix
from check_modular_stage_ocp import assembly_section
ROOT=pathlib.Path(__file__).resolve().parents[3]

def case(cad,directory,angle,length,pose):
    params=dict(angle=angle,coupler_length=length,pose=pose)
    flags=[item for k,v in params.items() for item in ['--param',f'{k}={v}']]
    generation=directory/'generation';manifest=command([cad,'build',ROOT/'examples/solved_four_bar.star','--out-dir',generation,*flags])
    q=math.sqrt(.5);r=[[[1,0,0],[0,1,0],[0,0,1]],[[1,0,0],[0,0,-1],[0,1,0]],[[.5,.5,q],[.5,.5,-q],[-q,q,0]]][pose]
    def world(p):return [sum(r[i][j]*p[j] for j in range(3))+[10,-5,3][i] for i in range(3)]
    b=[30*math.cos(angle),30*math.sin(angle)];delta=[80-b[0],-b[1]];d=math.hypot(*delta);u=[v/d for v in delta]
    along=(length*length-2500+d*d)/(2*d);h=math.sqrt(length*length-along*along)
    c=[b[0]+along*u[0]-h*u[1],b[1]+along*u[1]+h*u[0]]
    pins=[[0,0],b,c,[80,0]]
    specs=[(('ground',),0,3,80,0,6),(('crank_unit','link'),0,1,30,8,4),(('coupler',),1,2,length,14,4),(('rocker',),3,2,50,8,4)]
    expected={};volumes=[];centers=[]
    def placement(path,origin,theta):
        co,si=math.cos(theta),math.sin(theta);rz=[[co,-si,0],[si,co,0],[0,0,1]]
        expected[path]=(world(origin),[[sum(r[i][k]*rz[k][j] for k in range(3)) for j in range(3)] for i in range(3)])
    for path,a,z,l,height,thickness in specs:
        theta=math.atan2(pins[z][1]-pins[a][1],pins[z][0]-pins[a][0]);placement(path,[*pins[a],height],theta)
        volumes.append((12*l+28*math.pi)*thickness)
        centers.append(world([(pins[a][0]+pins[z][0])/2,(pins[a][1]+pins[z][1])/2,height+thickness/2]))
    for path,index,theta in [(('pin_a',),0,0),(('pin_d',),3,0),(('crank_unit','pin_b'),1,angle),(('pin_c',),2,math.atan2(c[1],c[0]-80))]:
        placement(path,[*pins[index],0],theta);volumes.append(math.pi*1.8**2*20);centers.append(world([*pins[index],10]))
    assembly_path=generation/'solved_four_bar.assembly.json';assembly=json.loads(assembly_path.read_text())
    require(len(assembly['instances'])==8,'eight leaf occurrences')
    for instance in assembly['instances']:
        origin,matrix=expected[tuple(assembly['instance_paths'][instance['id']])];actual=placement_matrix(instance['transform'])
        for i in range(3):
            require(abs(actual[i][3]-origin[i])<1e-5,'independent solved origin')
            for j in range(3):require(abs(actual[i][j]-matrix[i][j])<1e-7,'independent solved orientation')
    volume=sum(volumes);center=[sum(v*p[i] for v,p in zip(volumes,centers))/volume for i in range(3)]
    step=generation/'solved_four_bar.step';imported=body_properties(step);close(imported[0],volume,'STEP volume')
    for actual_value,expected_value in zip(imported[1],center):require(abs(actual_value-expected_value)<1e-5,'STEP independent centroid')
    hierarchy=json_output([sys.executable,str(pathlib.Path(__file__).with_name('check_assembly_ocp.py')),str(step),'--instances','8','--definitions','5','--solids','8','--assembly-json',str(assembly_path)])
    view=json.loads((generation/'section-moving_coupler.json').read_text());section=assembly_section(step,view['section']['plane'],expected_solids=8)
    close(section['area'],12*length+(28+4*1.8**2)*math.pi,'coupler and pin section area')
    direction=[(c[i]-b[i])/length for i in range(2)];mid=[(b[i]+c[i])/2 for i in range(2)]
    pin_area=math.pi*1.8**2;total_area=12*length+(28+4*1.8**2)*math.pi
    sum_delta=[sum(point[i]-mid[i] for point in pins) for i in range(2)]
    expected_section_center=[pin_area*(sum_delta[0]*direction[0]+sum_delta[1]*direction[1])/total_area,pin_area*(-sum_delta[0]*direction[1]+sum_delta[1]*direction[0])/total_area]
    for a,b in zip(section['centroid'],expected_section_center):require(abs(a-b)<1e-5,'independent section centroid')
    require(command([cad,'verify',generation])['valid'],'generation integrity')
    result=dict(parameters=params,independent_pins=pins,mechanism_reports=manifest['mechanism_reports'],expected_volume=volume,expected_centroid=center,imported=imported,hierarchy=hierarchy,section=section)
    (directory/'report.json').write_text(json.dumps(result,indent=2,allow_nan=False)+'\n');return result

def main():
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('--cad',type=pathlib.Path,required=True);p.add_argument('--out-dir',type=pathlib.Path,required=True);a=p.parse_args();a.out_dir.mkdir(parents=True,exist_ok=False);cases=[]
    for label,values in [('low',(.5,65,0)),('default',(.9,70,1)),('high',(1.3,75,2))]:
        directory=a.out_dir/label;directory.mkdir();cases.append(case(a.cad.resolve(),directory,*values))
    print(json.dumps(dict(valid=True,cases=cases,retained_generations=[f'{label}/generation' for label in ['low','default','high']],retained_reports=[f'{label}/report.json' for label in ['low','default','high']],oracle=dict(python_version=platform.python_version(),ocp_version=str(OCP.__version__),cadquery_ocp_version=importlib.metadata.version('cadquery-ocp'))),indent=2,allow_nan=False))
if __name__=='__main__':main()
