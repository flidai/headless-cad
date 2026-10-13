// Derived display motion only. Captured recipes and native exports remain authoritative.
(()=>{
 'use strict';
 const evidence=JSON.parse(document.getElementById('cad-rigid-translation-data').textContent),group=evidence.group;
 const panel=document.getElementById('translation-inspection'),phase=document.getElementById('translation-phase');
 try{
  if(!scene.is_assembly||evidence.recipe_context_hash!==recipeContext.recipe_context_hash||group.assembly_content_hash!==recipeContext.content_hash)throw Error('Translation evidence does not match this snapshot');
  const members=new Set(group.members.map(member=>member.instance_id));
  if(members.size!==group.members.length||members.size<1||members.size>64||[...members].some(id=>!scene.instances.some(instance=>instance.id===id)))throw Error('Invalid translation membership');
  const path=group.translation_world;
  if(!Array.isArray(path)||path.length!==3||!path.every(Number.isFinite)||Math.hypot(...path)<=0||Math.hypot(...path)>1e6)throw Error('Invalid source translation path');
  const originals=scene.faces.filter(face=>members.has(face.instance_id)).map(face=>({face,points:face.points}));
  document.querySelector('.badge').textContent='READ-ONLY MOTION';
  document.getElementById('translation-summary').textContent=`${group.declaration.id}: ${members.size} occurrences, ${group.clearance_check_ids.length} named checks. All members share one rigid translation.`;
  function apply(){
   const value=Number(phase.value);if(!Number.isFinite(value)||value<0||value>1)return;
   for(const original of originals)original.face.points=value===0?original.points:original.points.map(point=>point.map((x,i)=>x+path[i]*value));
   document.getElementById('translation-distance').textContent=`${(Math.hypot(...path)*value).toFixed(3)} mm`;
   panel.dataset.phase=String(value);
   draw();
  }
  phase.addEventListener('input',apply);
  document.getElementById('translation-reset').addEventListener('click',()=>{phase.value='0';apply();});
  apply();
 }catch(error){phase.disabled=true;document.getElementById('translation-summary').textContent=error.message;panel.dataset.error=error.message;}
})();
