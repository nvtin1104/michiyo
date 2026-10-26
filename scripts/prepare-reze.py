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
body_verts=[o.matrix_world @ v.co for o in meshes if 'Cabello' not in o.name for v in o.data.vertices]
knee_band=[v for v in body_verts if .45<v.z<.6]
lx=sum(abs(v.x) for v in knee_band)/len(knee_band)
ly=sum(v.y for v in knee_band)/len(knee_band)
bone('root',(0,0,0),(0,0,0.9))
bone('spine',(0,0,1.05),(0,0,1.65),'root')
bone('head',(0,0,1.72),(0,0,1.98),'spine')
for side,s in [('L',1),('R',-1)]:
    bone('upper_arm.'+side,(s*.12,0,1.64),(s*.48,0,1.62),'spine')
    bone('forearm.'+side,(s*.48,0,1.62),(s*.85,0,1.60),'upper_arm.'+side)
    bone('thigh.'+side,(s*lx,ly,.98),(s*lx,ly,.53),'root')
    bone('shin.'+side,(s*lx,ly,.53),(s*lx,ly,.06),'thigh.'+side)
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
            # Legs fade into the pelvis inside the shorts; the knee blend keeps stockings smooth.
            legs=(1-smooth(.80,1.0,z))*(1-head-arms-torso)
            left=smooth(-.035,.035,x); shin=1-smooth(.45,.61,z)
            weights={'head':head,'upper_arm.'+side:arms*(1-fore),'forearm.'+side:arms*fore,'spine':torso,
                'thigh.L':legs*left*(1-shin),'shin.L':legs*left*shin,'thigh.R':legs*(1-left)*(1-shin),'shin.R':legs*(1-left)*shin,
                'root':max(0,1-head-arms-torso-legs)}
        for name,w in weights.items():
            if w>.00001:groups[name].add([v.index],w,'REPLACE')
    mod=o.modifiers.new('Companion rig','ARMATURE');mod.object=rig;o.parent=rig
rig.show_in_front=True
# Pose authoring works in world space: +X is Reze's left, -Y faces the camera, +Z is up.
P=rig.pose.bones
rest={b.name:b.matrix_local.copy() for b in arm.bones}
for p in P:p.rotation_mode='QUATERNION'
def posed(p):
    basis=Matrix.Translation(p.location) @ p.rotation_quaternion.to_matrix().to_4x4()
    if not p.parent:return rest[p.name] @ basis
    return posed(p.parent) @ rest[p.parent.name].inverted() @ rest[p.name] @ basis
def aim(name,d):
    """Point a bone along a world direction, after its parents are posed."""
    p=P[name];p.rotation_quaternion=Quaternion()
    q=posed(p).to_quaternion();cur=q @ Vector((0,1,0))
    p.rotation_quaternion=q.inverted() @ cur.rotation_difference(Vector(d).normalized()) @ q
def turn(name,axis,angle):
    """Rotate a bone about a world axis through its head, on top of its pose."""
    p=P[name];q=posed(p).to_quaternion()
    p.rotation_quaternion=p.rotation_quaternion @ q.inverted() @ Quaternion(axis,angle) @ q
hip=Vector((0,ly,.98))
def body(q=Quaternion(),pivot=(0,0,0),offset=(0,0,0)):
    """Rotate the whole figure about a world pivot, then shift it."""
    p=P['root'];r=rest['root'].to_quaternion();pivot=Vector(pivot)
    p.rotation_quaternion=r.inverted() @ q @ r
    p.location=r.inverted() @ (pivot-q @ pivot+Vector(offset))
def mix(a,b,k):return Vector(a).lerp(Vector(b),k)
def ease(a,b,t):return smooth(a,b,t)
def mirror(d):return (-d[0],d[1],d[2])
def arms(left_up,left_fore,right_up=None,right_fore=None):
    aim('upper_arm.L',left_up);aim('forearm.L',left_fore)
    aim('upper_arm.R',right_up or mirror(left_up));aim('forearm.R',right_fore or mirror(left_fore))
def legs(left_thigh,left_shin,right_thigh=None,right_shin=None):
    aim('thigh.L',left_thigh);aim('shin.L',left_shin)
    aim('thigh.R',right_thigh or mirror(left_thigh));aim('shin.R',right_shin or mirror(left_shin))
DOWN_UP=(.22,.06,-.97);DOWN_FORE=(.12,-.28,-.95)
def stand(w):
    # Relaxed contrapposto: weight on the left leg, right knee soft, arms loose.
    body(Quaternion((0,1,0),.03+.01*w),pivot=(lx,ly,0),offset=(0,0,.003*w))
    legs((-.03,0,-1),(-.01,.02,-1),(.04,-.1,-1),(.0,.14,-1))
    turn('spine',(0,1,0),-.045-.008*w);turn('spine',(1,0,0),.012*w)
    arms((.22,.06-.02*w,-.97),DOWN_FORE,(-.24,.04+.02*w,-.97),(-.1,-.24,-.96))
    turn('head',(0,1,0),.05)
def sitting(t,w,sleepy=False):
    """Hug knees on the floor, turned three-quarters so the knees read; gentle rocking."""
    Y=Quaternion((0,0,1),.6);rock=.04*w
    body(Y @ Quaternion((1,0,0),rock),pivot=hip,offset=(0,.02,.19-hip.z))
    legs(Y@Vector((.12,-.78,.62)),Y@Vector((.02,.32,-.95)),Y@Vector((-.12,-.78,.62)),Y@Vector((-.02,.32,-.95)))
    turn('spine',Y@Vector((1,0,0)),.42 if sleepy else .22)
    arms(Y@Vector((.22,-.62,-.75)),Y@Vector((-.75,-.62,.18)),Y@Vector((-.22,-.62,-.75)),Y@Vector((.75,-.62,.18)))
    turn('head',Y@Vector((1,0,0)),(.55 if sleepy else -.12)+.03*w)
    if not sleepy:turn('head',(0,0,1),-.35+.12*math.sin(t*math.tau))
def sit_relax(t,w):
    """Lean back on the hands with legs out, feet swinging."""
    Y=Quaternion((0,0,1),-.6);kick=.18*w
    body(Y @ Quaternion((1,0,0),-.18),pivot=hip,offset=(0,.04,.2-hip.z))
    legs(Y@Vector((.06,-.96,.2)),Y@Vector((.04,-.75,-.66+kick)),Y@Vector((-.02,-.96,.22)),Y@Vector((.08,-.72,-.69-kick)))
    turn('spine',Y@Vector((1,0,0)),-.08)
    arms(Y@Vector((.42,.55,-.72)),Y@Vector((.08,.18,-.98)),Y@Vector((-.42,.55,-.72)),Y@Vector((-.08,.18,-.98)))
    turn('head',(0,0,1),.4+.14*math.sin(t*math.tau));turn('head',Y@Vector((1,0,0)),-.08)
def pose(state,t):
    w=math.sin(t*math.tau);w2=math.sin(t*math.tau*2)
    for p in P:p.rotation_quaternion=Quaternion();p.location=(0,0,0)
    stand(w)
    if state=='idle':
        turn('head',(0,0,1),.05*math.sin(t*math.tau+1))
    elif state=='idle_sway':
        # Hands behind the back, rocking side to side.
        body(Quaternion((0,1,0),.045*w),offset=(0,0,.004*abs(w)))
        legs((.03*w,0,-1),(0,.02,-1),(.03*w,0,-1),(0,.02,-1))
        turn('spine',(0,1,0),-.05*w)
        arms((.22,.38,-.9),(-.62,.62,-.3))
        turn('head',(0,1,0),-.09*w)
    elif state=='idle_look':
        # Hand on hip, glancing left then right.
        look=.45*(ease(.08,.25,t)-ease(.4,.6,t)*2+ease(.75,.92,t))
        arms((.68,.08,-.73),(-.45,-.18,-.88),mirror(DOWN_UP),mirror(DOWN_FORE))
        turn('spine',(0,0,1),look*.25)
        turn('head',(0,0,1),look);turn('head',(1,0,0),-.05)
    elif state=='idle_stretch':
        k=ease(0,.3,t)*(1-ease(.72,.95,t))
        body(Quaternion((0,1,0),.06*k*math.sin(t*math.tau*1.5)),offset=(0,0,.02*k))
        turn('spine',(1,0,0),-.1*k)
        arms(mix(DOWN_UP,(.18,.05,1),k),mix(DOWN_FORE,(-.35,.05,1),k))
        turn('head',(1,0,0),-.2*k)
    elif state=='idle_hair':
        # Tucks hair behind her right ear.
        k=ease(0,.25,t)*(1-ease(.72,.95,t))
        aim('upper_arm.R',mix(mirror(DOWN_UP),(-.82,-.35,-.12),k))
        aim('forearm.R',mix(mirror(DOWN_FORE),(.78,.22,.58),k))
        turn('head',(0,1,0),-.12*k);turn('head',(0,0,1),-.08*k)
    elif state=='boom':
        # Hooks the choker pin, then yanks it out with a grin.
        reach=ease(0,.22,t)*(1-ease(.38,.46,t));pull=ease(.38,.46,t)*(1-ease(.85,1,t))
        lean=.08*reach-.06*pull
        body(Quaternion((1,0,0),lean),pivot=hip)
        up=mix(mix(mirror(DOWN_UP),(-.3,-.86,-.4),reach),(-.7,-.55,.2),pull)
        fore=mix(mix(mirror(DOWN_FORE),(.5,.3,.81),reach),(-.6,-.55,.6),pull)
        arms((.68,.08,-.73),(-.45,-.18,-.88),up,fore)
        turn('head',(1,0,0),.18*reach-.1*pull);turn('head',(0,1,0),.16*pull)
    elif state=='sit':sitting(t,w)
    elif state=='sit_relax':sit_relax(t,w)
    elif state=='sleep':sitting(t,.4*w,True)
    elif state=='hover':
        body(Quaternion((1,0,0),.05+.01*w),pivot=hip)
        arms((.22,.38,-.9),(-.62,.62,-.3))
        turn('head',(0,1,0),.12+.03*w)
    elif state=='listening':
        aim('upper_arm.R',(-.82,-.3,-.15));aim('forearm.R',(.75,.15,.64))
        turn('head',(0,1,0),-.16+.03*w)
    elif state=='thinking':
        arms((.2,-.4,-.9),(-.88,-.42,.12),(-.25,-.38,-.89),(.4,.06,.92))
        turn('head',(0,1,0),.12);turn('head',(1,0,0),-.1);turn('head',(0,0,1),.05*w)
    elif state=='talking':
        arms((.3,-.25,-.92),(.12+.18*w,-.85,.3+.15*w),(-.3,-.25,-.92),(-.12+.18*w2,-.85,.25-.15*w))
        turn('head',(1,0,0),.05*w2);turn('head',(0,0,1),.05*w)
    elif state in ['greeting','happy']:
        hop=abs(w2)*.03 if state=='happy' else 0
        body(offset=(0,0,hop))
        aim('upper_arm.L',(.78,-.1,.5));aim('forearm.L',(.35*w2,-.12,1))
        if state=='happy':aim('upper_arm.R',(-.78,-.1,.5));aim('forearm.R',(.35*w2,-.12,1))
        turn('head',(0,1,0),.08*w2)
    elif state=='error':
        arms((.62,-.2,-.75),(.6,-.75,.1))
        turn('head',(0,0,1),.18*w2);turn('head',(1,0,0),.06)
    elif state=='touched':
        # Startled hop back, then a pout.
        k=ease(0,.12,t)*(1-ease(.35,.6,t))
        body(Quaternion((1,0,0),-.12*k),pivot=hip,offset=(0,.04*k,.05*k))
        arms(mix(DOWN_UP,(.55,-.45,-.7),k),mix(DOWN_FORE,(.1,-.6,.8),k))
        turn('head',(1,0,0),-.15*k);turn('head',(0,1,0),.1*(1-k))
    elif state=='petting':
        body(offset=(0,0,-.02-.012*w2))
        legs((0,-.05,-1),(0,.1,-1))
        arms((.15,-.45,-.88),(-.9,-.35,.25))
        turn('head',(0,1,0),.16*w);turn('head',(1,0,0),.1)
    elif state=='dragging':
        # Held up by the collar: pendulum swing, flailing arms, kicking legs.
        q=Quaternion((0,1,0),.14*w)
        body(q,pivot=(0,0,1.8),offset=(0,0,.05))
        turn('spine',(0,1,0),-.05*w)
        aim('upper_arm.L',(.7,-.1,.25+.45*w2));aim('forearm.L',(.3,-.2,.9))
        aim('upper_arm.R',(-.7,-.1,.25-.45*w2));aim('forearm.R',(-.3,-.2,.9))
        legs((.04,-.35*w2,-1),(0,.55+.35*w2,-.9),(-.04,.35*w2,-1),(0,.55-.35*w2,-.9))
        turn('head',(0,1,0),-.1*w);turn('head',(1,0,0),.12)
    elif state=='carried':
        # Slung over a shoulder: folded at the waist, seen side-on.
        swing=.05*w
        q=Quaternion((0,1,0),.5+swing) @ Quaternion((0,0,1),math.pi/2)
        body(q,pivot=hip,offset=(0,0,1.45-hip.z))
        aim('spine',(.5,-.05,-.87))
        turn('head',(.5,-.05,-.87),.9)
        for side in 'LR':aim('upper_arm.'+side,(.15*w,0,-1));aim('forearm.'+side,(.1*w,-.1,-1))
        legs((-.5,0,-.87),(-.3,.1*w2,-.9+.3*w2),(-.46,0,-.89),(-.3,-.1*w2,-.9-.3*w2))
    elif state=='settling':
        k=1-ease(0,.5,t)
        body(offset=(0,0,-.07*k))
        legs((0,-.35*k,-1),(0,.35*k,-1))
        arms(mix(DOWN_UP,(.6,-.2,-.6),k),DOWN_FORE)
        turn('head',(1,0,0),.12*k)
    elif state=='waking':
        # Big yawn-stretch after sleeping.
        k=ease(0,.3,t)*(1-ease(.65,1,t))
        arms(mix(DOWN_UP,(.15,0,1),k),mix(DOWN_FORE,(-.3,0,1),k))
        turn('head',(1,0,0),-.18*k)
clips={'idle':61,'hover':61,'listening':61,'thinking':61,'talking':61,'happy':61,'error':61,'sleep':121,
    'greeting':61,'touched':61,'petting':61,'dragging':41,'carried':61,'settling':31,'waking':61,
    'idle_sway':121,'idle_look':121,'idle_stretch':121,'idle_hair':91,'boom':91,'sit':121,'sit_relax':121}
states=list(clips)
rig.animation_data_create()
for state,frames in clips.items():
    action=bpy.data.actions.new(state);rig.animation_data.action=action
    for frame in list(range(1,frames,3))+[frames]:
        pose(state,(frame-1)/(frames-1))
        for p in P:
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
