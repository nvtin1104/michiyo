"""Build a lightweight companion rig from the supplied static GLB. Run in Blender."""
import bpy, math, json
from pathlib import Path
from mathutils import Vector, Matrix, Quaternion
root = Path(__file__).resolve().parents[1]
out = root/'public/skins/reze'
work = root/'artifacts/reze'
work.mkdir(parents=True, exist_ok=True)
bpy.ops.wm.read_factory_settings(use_empty=True)
bpy.ops.import_scene.gltf(filepath=str(out/'reze_de_chainsaw_man.glb'))
meshes = [o for o in bpy.data.objects if o.type == 'MESH']
# The duplicate inverted-hull outline is unreliable after decimation/skinning.
# Keep the textured surfaces; the original download remains untouched.
for o in list(meshes):
    if 'Contorno' in o.name: bpy.data.objects.remove(o, do_unlink=True)
meshes = [o for o in bpy.data.objects if o.type == 'MESH']
low = min((o.matrix_world @ Vector(c)).z for o in meshes for c in o.bound_box)
high = max((o.matrix_world @ Vector(c)).z for o in meshes for c in o.bound_box)
normal = Matrix.Scale(2/(high-low),4) @ Matrix.Translation((0,0,-low))
for o in meshes:
    o.data.transform(normal @ o.matrix_world)
    o.parent=None
    o.matrix_world=Matrix.Identity(4)
# Join split primitives before welding, so seams remain connected during deformation.
for label in ('Ropa','reze','Cabello','Contorno'):
    group=[o for o in meshes if label in o.name]
    if not group: continue
    bpy.ops.object.select_all(action='DESELECT')
    for o in group: o.select_set(True)
    bpy.context.view_layer.objects.active=group[0]
    bpy.ops.object.join()
    o=bpy.context.object
    o.name='Reze_'+label
    bpy.ops.object.mode_set(mode='EDIT')
    bpy.ops.mesh.select_all(action='SELECT')
    bpy.ops.mesh.remove_doubles(threshold=0.00001)
    bpy.ops.object.mode_set(mode='OBJECT')
    dec=o.modifiers.new('Desktop optimization','DECIMATE'); dec.ratio=0.18
    bpy.ops.object.modifier_apply(modifier=dec.name)
    meshes=[o for o in bpy.data.objects if o.type=='MESH']
for img in bpy.data.images:
    if img.size[0]>1024 or img.size[1]>1024:
        ratio=1024/max(img.size);img.scale(max(1,int(img.size[0]*ratio)),max(1,int(img.size[1]*ratio)))
    if img.has_data: img.pack()
arm=bpy.data.armatures.new('RezeCompanionRig'); rig=bpy.data.objects.new('RezeRig',arm)
bpy.context.collection.objects.link(rig)
bpy.context.view_layer.objects.active=rig;rig.select_set(True)
bpy.ops.object.mode_set(mode='EDIT')
def bone(name,head,tail,parent=None):
    b=arm.edit_bones.new(name);b.head=head;b.tail=tail
    if parent:b.parent=arm.edit_bones[parent]
bone('root',(0,0,0),(0,0,0.9))
bone('spine',(0,0,1.05),(0,0,1.65),'root')
bone('head',(0,0,1.72),(0,0,1.98),'spine')
for side,s in [('L',1),('R',-1)]:
    bone('upper_arm.'+side,(s*.12,0,1.64),(s*.48,0,1.62),'spine')
    bone('forearm.'+side,(s*.48,0,1.62),(s*.85,0,1.60),'upper_arm.'+side)
bpy.ops.object.mode_set(mode='OBJECT')
def smooth(a,b,x):
    t=max(0,min(1,(x-a)/(b-a)));return t*t*(3-2*t)
for o in meshes:
    groups={b.name:o.vertex_groups.new(name=b.name) for b in arm.bones}
    for v in o.data.vertices:
        x,y,z=v.co; weights={}
        if 'Cabello' in o.name:
            weights={'head':1}
        else:
            head=smooth(1.69,1.79,z)*(1-smooth(.10,.19,abs(x)))
            arms=smooth(.12,.23,abs(x))*smooth(1.45,1.57,z)
            arms=min(arms,1-head)
            fore=smooth(.43,.52,abs(x)); side='L' if x>=0 else 'R'
            torso=smooth(.98,1.27,z)*(1-head-arms)
            weights={'head':head,'upper_arm.'+side:arms*(1-fore),'forearm.'+side:arms*fore,'spine':torso,'root':max(0,1-head-arms-torso)}
        for name,w in weights.items():
            if w>.00001:groups[name].add([v.index],w,'REPLACE')
    mod=o.modifiers.new('Companion rig','ARMATURE');mod.object=rig;o.parent=rig
rig.show_in_front=True
def rotate(name,axis,angle):
    p=rig.pose.bones[name];p.rotation_mode='QUATERNION'
    q=p.bone.matrix_local.to_quaternion()
    p.rotation_quaternion=q.inverted() @ Quaternion(axis,angle) @ q
states=['idle','hover','listening','thinking','talking','happy','error','sleep','greeting','touched','petting','dragging','settling','waking']
rig.animation_data_create()
for state in states:
    action=bpy.data.actions.new(state);rig.animation_data.action=action
    for frame in range(1,62,5):
        t=(frame-1)/60; wave=math.sin(t*math.tau)
        for p in rig.pose.bones:p.rotation_quaternion=Quaternion();p.location=(0,0,0)
        rotate('upper_arm.L',(0,1,0),1.30)
        rotate('upper_arm.R',(0,1,0),-1.30)
        rotate('spine',(0,1,0),.012*wave)
        rotate('head',(1,0,0),.015*wave)
        rig.pose.bones['root'].location.z=.003*wave
        if state in ['greeting','happy']:
            rotate('upper_arm.L',(0,1,0),-.2)
            rotate('forearm.L',(0,1,0),-1.1+.15*wave)
        if state in ['hover','listening','touched','petting']:
            rotate('head',(0,1,0),(.10 if state=='petting' else .055)*wave)
        if state=='thinking':
            rotate('head',(0,1,0),.09)
            rotate('upper_arm.R',(0,1,0),-.75)
            rotate('forearm.R',(0,0,1),-.5)
        if state=='talking':
            rotate('forearm.L',(0,0,1),.15+.1*wave)
            rotate('head',(1,0,0),.04*wave)
        if state=='error':rotate('head',(0,0,1),.10*wave)
        if state=='sleep':rotate('head',(1,0,0),.18)
        if state=='dragging':
            rotate('upper_arm.L',(0,1,0),.85);rotate('upper_arm.R',(0,1,0),-.85)
        if state in ['settling','waking']:rotate('head',(1,0,0),.10*(1-t))
        for p in rig.pose.bones:
            p.keyframe_insert('rotation_quaternion',frame=frame,group=p.name)
            p.keyframe_insert('location',frame=frame,group=p.name)
    action.use_fake_user=True
rig.animation_data.action=bpy.data.actions['idle']
scene=bpy.context.scene;scene.frame_start=1;scene.frame_end=61;scene.render.fps=30;scene.frame_set(1)
bpy.ops.object.select_all(action='DESELECT')
rig.select_set(True)
for o in meshes:o.select_set(True)
bpy.context.view_layer.objects.active=rig
bpy.ops.export_scene.gltf(filepath=str(out/'reze-companion.glb'),use_selection=True,export_animations=True,export_animation_mode='ACTIONS',export_force_sampling=True,export_image_format='AUTO')
# Studio preview and editable source are kept outside the shipped web assets.
bpy.ops.object.camera_add(location=(0,-4,1.15));camera=bpy.context.object
camera.rotation_euler=(Vector((0,0,1.05))-camera.location).to_track_quat('-Z','Y').to_euler()
camera.data.type='ORTHO';camera.data.ortho_scale=2.25;scene.camera=camera
scene.render.engine='CYCLES';scene.cycles.samples=16
scene.world=bpy.data.worlds.new('Studio');scene.world.color=(.6,.6,.6)
for loc,power,size in [((1,-3,4),250,4),((-2,-1,2),120,3)]:
    bpy.ops.object.light_add(type='AREA',location=loc);l=bpy.context.object;l.data.energy=power;l.data.shape='DISK';l.data.size=size
    l.rotation_euler=(Vector((0,0,1))-l.location).to_track_quat('-Z','Y').to_euler()
scene.render.resolution_x=640;scene.render.resolution_y=640;scene.render.resolution_percentage=100
scene.render.film_transparent=True
bpy.ops.object.select_all(action='DESELECT');rig.select_set(True);bpy.context.view_layer.objects.active=rig
for screen in bpy.data.screens:
    for area in screen.areas:
        if area.type=='VIEW_3D':
            area.spaces.active.region_3d.view_perspective='CAMERA'
            area.spaces.active.shading.type='MATERIAL'
bpy.ops.wm.save_as_mainfile(filepath=str(work/'reze-companion.blend'))
scene.render.filepath=str(work/'preview.png');bpy.ops.render.render(write_still=True)
(work/'report.json').write_text(json.dumps({'vertices':sum(len(o.data.vertices) for o in meshes),'bones':len(arm.bones),'animations':states,'bytes':(out/'reze-companion.glb').stat().st_size},indent=2))
