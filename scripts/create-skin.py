"""Original Moon Gardener sprite. Run with Python + Pillow to regenerate assets."""
from PIL import Image, ImageDraw
import json
from pathlib import Path
root=Path(__file__).resolve().parents[1]
out=root/'public/skins/default'
out.mkdir(parents=True,exist_ok=True)
sheet=Image.new('RGBA',(32*22,32))
for frame in range(22):
    im=Image.new('RGBA',(32,32));d=ImageDraw.Draw(im)
    outline='#354d50'; hair='#e9e3c5'; shadow='#c8cba9'; green='#6d987c'
    # Round hood, leaf antenna, original tiny gardener with a moon pin.
    d.rectangle((10,5,21,6),fill=outline);d.rectangle((8,7,23,19),fill=outline)
    d.rectangle((9,7,22,18),fill=hair);d.rectangle((10,8,21,18),fill='#f4dfbf')
    d.rectangle((9,7,22,10),fill=shadow);d.rectangle((9,10,11,14),fill=hair)
    d.rectangle((20,10,22,16),fill=hair);d.rectangle((12,9,14,11),fill=hair)
    blink=frame in (2,7,14,15);sleep=frame==10
    for x in (13,18):
        d.rectangle((x,14,x+1,14 if blink or sleep else 15),fill=outline)
    if frame in (1,11):
        for x in (13,18):
            d.rectangle((x,14,x+1,15),fill='#f4dfbf')
            offset=-1 if frame==1 else 1
            d.rectangle((x+offset,14,x+1+offset,15),fill=outline)
    if frame in (20,21):
        for x in (13,18):
            d.rectangle((x,14,x+1,15),fill='#f4dfbf')
            offset=-1 if frame==20 else 1
            d.rectangle((x,14+offset,x+1,15+offset),fill=outline)
    d.point((16,17),fill='#a46965');d.point((11,16),fill='#deaaa0');d.point((21,16),fill='#deaaa0')
    d.rectangle((11,19,20,25),fill=outline);d.rectangle((12,19,19,24),fill=green)
    d.rectangle((10,21,11,24),fill='#f4dfbf');d.rectangle((20,21,21,24),fill='#f4dfbf')
    d.rectangle((12,26,14,27),fill=outline);d.rectangle((18,26,20,27),fill=outline)
    d.rectangle((16,3,17,6),fill=outline);d.rectangle((18,2,21,3),fill=green);d.rectangle((17,3,19,4),fill=green)
    d.point((18,20),fill='#f9db88');d.point((18,21),fill='#f9db88')
    if frame==4:d.rectangle((15,17,17,18),fill='#a46965')
    if frame==5:d.point((25,7),fill='#caa155');d.rectangle((24,8,26,8),fill='#caa155');d.point((25,9),fill='#caa155')
    if frame==6:d.rectangle((15,16,17,17),fill='#a46965')
    if frame==8:d.rectangle((24,12,25,14),fill='#b66058');d.point((24,16),fill='#b66058')
    if frame==9:d.rectangle((12,19,19,20),fill='#84a68a');d.rectangle((10,20,11,23),fill='#f4dfbf');d.rectangle((20,20,21,23),fill='#f4dfbf')
    if frame==10:d.text((24,5),'z',fill=outline)
    if frame in (12,13): # Wave with a raised hand.
        d.rectangle((20,21,21,24),fill=(0,0,0,0))
        d.line((21,21,24,18,24 if frame==12 else 26,15),fill=outline,width=2)
        d.rectangle((23 if frame==12 else 25,13,25 if frame==12 else 27,16),fill='#f4dfbf')
    if frame in (14,15): # Contented touch / petting, heart above the hood.
        d.rectangle((14,16,18,17),fill='#f4dfbf');d.line((15,16,16,17,18,16),fill='#a46965')
        d.rectangle((10,16,12,17),fill='#e7a7a1');d.rectangle((20,16,22,17),fill='#e7a7a1')
        hx=25 if frame==14 else 26
        d.line((hx-2,4,hx-1,3,hx,4,hx+1,3,hx+2,4,hx,7,hx-2,4),fill='#c87583',width=1)
    if frame==16: # Picked up: open mouth and arms outward.
        d.rectangle((15,17,17,18),fill='#a46965');d.rectangle((7,20,11,21),fill='#f4dfbf');d.rectangle((21,20,25,21),fill='#f4dfbf')
    if frame==17: # Landing / settling.
        d.rectangle((12,25,20,25),fill=green)
    if frame in (18,19): # Wake and stretch.
        d.line((10,22,7,18),fill='#f4dfbf',width=2);d.line((21,22,24,18),fill='#f4dfbf',width=2)
    sheet.paste(im,(frame*32,0))
sheet.save(out/'spritesheet.png')
states={'idle':[0,0,0,0,9,9,9,9,0,0,0,0],'hover':[1,1,2,1],'listening':[3,2,3,3],'thinking':[5,0,5,0],'talking':[4,0,6,0],'happy':[5,6,5,6],'error':[8,0,8,0],'sleep':[10,10,10,2]}
states.update({'greeting':[12,13,12,13],'touched':[14,14,15,0],'petting':[14,15,14,15],'dragging':[16],'settling':[17,17,0],'waking':[18,19,18,0]})
(out/'manifest.json').write_text(json.dumps({'name':'Moon gardener','frameWidth':32,'frameHeight':32,'states':{k:{'frames':v,'fps':8} for k,v in states.items()}},indent=2))
icons=root/'src-tauri/icons';icons.mkdir(exist_ok=True)
sheet.crop((0,0,32,32)).resize((128,128),Image.Resampling.NEAREST).save(icons/'icon.png')
sheet.crop((0,0,32,32)).resize((256,256),Image.Resampling.NEAREST).save(icons/'icon.ico',sizes=[(16,16),(32,32),(48,48),(64,64),(128,128),(256,256)])
