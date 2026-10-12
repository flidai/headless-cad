// Derived display motion only. Never edits the captured recipe or requests a build.
(()=>{
 'use strict';
 const evidence=JSON.parse(document.getElementById('cad-rigid-rotation-data').textContent),group=evidence.group;
 const panel=document.getElementById('rotation-inspection'),phase=document.getElementById('rotation-phase');
 try{
  if(!scene.is_assembly||evidence.recipe_context_hash!==recipeContext.recipe_context_hash||group.assembly_content_hash!==recipeContext.content_hash)throw Error('Rotation evidence does not match this snapshot');
  const members=new Set(group.members.map(member=>member.instance_id));
  if(members.size!==group.members.length||members.size<1||members.size>64||[...members].some(id=>!scene.instances.some(instance=>instance.id===id)))throw Error('Invalid rotation membership');
  const path=group.rotation_world,vector=v=>Array.isArray(v)&&v.length===3&&v.every(Number.isFinite);
  if(!vector(path.pivot)||!vector(path.axis)||Math.abs(Math.hypot(...path.axis)-1)>1e-10||!Number.isFinite(path.angle_radians)||Math.abs(path.angle_radians)>2*Math.PI)throw Error('Invalid source rotation path');
  const originals=scene.faces.filter(face=>members.has(face.instance_id)).map(face=>({face,points:face.points,normal:face.normal}));
  const initialBounds=scene.bounds;
  document.querySelector('.badge').textContent='READ-ONLY MOTION';
  document.getElementById('rotation-summary').textContent=`${group.declaration.id}: ${members.size} occurrences, ${group.clearance_check_ids.length} named checks. All members share one rigid rotation.`;
  function apply(){
   const value=Number(phase.value);if(!Number.isFinite(value)||value<0||value>1)return;
   const angle=path.angle_radians*value,c=Math.cos(angle),s=Math.sin(angle),axis=path.axis;
   const turn=v=>{const crossProduct=cross(axis,v),projection=dot(axis,v);return v.map((x,i)=>x*c+crossProduct[i]*s+axis[i]*projection*(1-c));};
   for(const original of originals){
    original.face.points=value===0?original.points:original.points.map(point=>turn(point.map((x,i)=>x-path.pivot[i])).map((x,i)=>x+path.pivot[i]));
    original.face.normal=value===0?original.normal:turn(original.normal);
   }
   // Static bounds and engineering summaries describe the authored geometry.
   scene.bounds=initialBounds;
   document.getElementById('rotation-angle').textContent=`${(angle*180/Math.PI).toFixed(1)}°`;
   panel.dataset.phase=String(value);
   const picked=scene.faces.find(face=>face.id===selected);
   if(picked)document.getElementById('selection').textContent=document.getElementById('selection').textContent.replace(/Display patch normal: \[[^\]]*\]$/,`Display patch normal: [${picked.normal.map(fmt).join(', ')}]`);
   draw();
  }
  phase.addEventListener('input',apply);
  document.getElementById('rotation-reset').addEventListener('click',()=>{phase.value='0';apply();});
  apply();
 }catch(error){phase.disabled=true;document.getElementById('rotation-summary').textContent=error.message;panel.dataset.error=error.message;}
})();
