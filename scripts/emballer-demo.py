#!/usr/bin/env python3
"""Archive par liste blanche : aucune base/configuration locale n'est copiée."""
from pathlib import Path
import json, shutil, zipfile, hashlib
root=Path(__file__).resolve().parents[1]
out=root/'dist-rust';out.mkdir(exist_ok=True)
stage=out/'EO-Suivi-demo-2.2.44'
if stage.exists():
    raise SystemExit('Le dossier de livraison existe déjà ; choisir un nouveau dossier avant de reconstruire.')
stage.mkdir()
for dirname in ['src','templates','resources','migrations','static','tests']:
    shutil.copytree(root/dirname,stage/dirname)
for filename in ['Cargo.toml','Cargo.lock','DEMONSTRATION.md','Dockerfile.demo','README.md','Etat-projet-eo-suivi-rust.md','NOTICE-UTILISATEUR.md']:
    shutil.copy2(root/filename,stage/filename)
(stage/'.github/workflows').mkdir(parents=True)
shutil.copy2(root/'.github/workflows/rust.yml',stage/'.github/workflows/rust.yml')
(stage/'scripts').mkdir()
for name in ['lancer-demo.sh','mettre-a-jour-debian13.sh','build-linux-musl.sh','lancer.sh']:
    shutil.copy2(root/'scripts'/name,stage/'scripts'/name)
shutil.copy2(root/'target/release/eo-suivi-elevage',stage/'eo-suivi-elevage')
meta=json.loads(Path('/tmp/eo-metadata.json').read_text())
licdir=stage/'LICENCES-TIERS';licdir.mkdir()
manifest=[]
for pkg in meta['packages']:
    if not pkg.get('source'):continue
    folder=Path(pkg['manifest_path']).parent
    target=licdir/(pkg['name']+'-'+pkg['version']);target.mkdir()
    for f in folder.rglob('*'):
        if f.is_file() and f.name.lower().startswith(('license','licence','copying','notice','copyright')):
            dest=target/f.relative_to(folder);dest.parent.mkdir(parents=True,exist_ok=True);shutil.copy2(f,dest)
    manifest.append({'name':pkg['name'],'version':pkg['version'],'license':pkg.get('license'),'source':pkg['source'],'repository':pkg.get('repository')})
(stage/'LICENCES-TIERS.json').write_text(json.dumps(manifest,ensure_ascii=False,indent=2))
files=[f for f in stage.rglob('*') if f.is_file()]
assert not any(f.suffix in ['.db','.sqlite','.pdf','.log'] or f.name.startswith('.env') for f in files)
assert not (stage/'data').exists() and not (stage/'donnees-demo').exists()
checks=''.join(hashlib.sha256(f.read_bytes()).hexdigest()+'  '+str(f.relative_to(stage))+'\n' for f in sorted(files))
(stage/'SHA256SUMS').write_text(checks)
archive=out/(stage.name+'-linux-x86_64.zip')
with zipfile.ZipFile(archive,'w',zipfile.ZIP_DEFLATED,compresslevel=6,strict_timestamps=False) as z:
    for f in stage.rglob('*'):
        if f.is_file():z.write(f,f.relative_to(out))
with zipfile.ZipFile(archive) as z:
    assert z.testzip() is None
print(archive)
print('Fichiers :',len(files),'Taille ZIP :',archive.stat().st_size)
print('SHA256 :',hashlib.sha256(archive.read_bytes()).hexdigest())
