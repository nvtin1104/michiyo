import bpy, json
from mathutils import Vector
from pathlib import Path
root = Path(__file__).resolve().parents[1]
bpy.ops.wm.read_factory_settings(use_empty=True)
bpy.ops.import_scene.gltf(filepath=str(root/'assets-src/reze/reze_de_chainsaw_man.glb'))
result=[]
for o in bpy.data.objects:
    if o.type == 'MESH':
        pts=[o.matrix_world @ Vector(c) for c in o.bound_box]
        result.append({'name':o.name,'vertices':len(o.data.vertices),'min':[min(p[i] for p in pts) for i in range(3)],'max':[max(p[i] for p in pts) for i in range(3)]})
(root/'artifacts/reze-inspect.json').write_text(json.dumps(result,indent=2))
