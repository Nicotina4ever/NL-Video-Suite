"""Create a ZIP64 portable package from an explicit allowlist, streaming files.

No source/build/quarantine folders, personal caches or original-app paths are
needed to run the package. Project data is opt-in and must be quiescent.
"""
import argparse
from datetime import datetime, timezone
import hashlib
import json
from pathlib import Path
import os
import re
import sys
import subprocess
import zipfile
from collections import Counter

ROOT=Path(__file__).resolve().parents[1]
DIRECTORIES=('app','runtime','engines','workflows','models','licenses','documentazione')
FILES=('Avvia_NL_Video_Suite.cmd','Ferma_NL_Video_Suite.cmd','Verifica_PC.cmd','Crea_Portable.cmd','LEGGIMI.html','README.md')
EXCLUDED_FILES={'snapshot-manifest.json','python-ready.json','server.pid','server.lock'}

def core_package(candidate, output, inventory_only=False):
    """Experimental executable + source helpers, no private data or third-party binaries.

    Uses the existing ZIP packager, streaming directly; no staging tree/model copy.
    """
    candidate = candidate.resolve()
    if candidate.parent != (ROOT/'sviluppo/target/release').resolve() or not candidate.name.startswith('nl-video-suite') or candidate.suffix != '.exe':
        raise RuntimeError('Il candidato deve essere un EXE di sviluppo in target/release')
    digest=hashlib.sha256(candidate.read_bytes()).hexdigest()
    proofs=[]
    for path in (ROOT/'sviluppo/tests/artifacts/v1-baseline').glob('*/report.json'):
        try: proof=json.loads(path.read_text(encoding='utf-8-sig'))
        except (OSError,ValueError): continue
        if proof.get('passed') is True and proof.get('tested_executable_sha256')==digest and any(r.get('status')=='passed' and 'browser' in r.get('test','') for r in proof.get('results',[])):
            proofs.append(path.parent.name)
    if not proofs: raise RuntimeError('Il candidato non ha una prova GUI riuscita sullo stesso SHA256')
    files=[(candidate,'app/nl-video-suite.exe')]
    for name in ('start.ps1','stop.ps1','configura_pc.ps1'):
        files.append((ROOT/'app'/name,'app/'+name))
    for name in ('README.md','LEGGIMI.html','LICENSE'):
        files.append((ROOT/name,name))
    for directory in ('licenses','documentazione','sviluppo/bridge'):
        for source in sorted((ROOT/directory).rglob('*')):
            if source.is_file() and '__pycache__' not in source.parts and source.suffix not in ('.pyc','.log'):
                files.append((source,source.relative_to(ROOT).as_posix()))
    for source in sorted((ROOT/'engines').rglob('*.py')):
        if 'models' not in source.parts:files.append((source,source.relative_to(ROOT).as_posix()))
    for name in ('V1_MASTER_ROADMAP.md','V1_SESSION_HANDOFF.md','V1_PROMOTION.md'):
        files.append((ROOT/'sviluppo/roadmap'/name,'sviluppo/roadmap/'+name))
    for source,_ in files:
        if not source.is_file() or source.is_symlink() or not source.resolve().is_relative_to(ROOT):raise RuntimeError('File non portabile: '+str(source))
    manifest={'application':'NL Video Suite','channel':'experimental-core','version':'0.7.1','candidate_sha256':digest,'gui_proofs':sorted(proofs),
              'includes_personal_data':False,'includes_models':False,'includes_python_runtime':False,'includes_ffmpeg':False,
              'optional_external_services':['Ollama','ComfyUI','local OpenAI-compatible API'],
              'required_for_media':['FFmpeg and ffprobe, selected by user or already in PATH'],
              'full_1_0_portable_acceptance':False,'files':[]}
    if inventory_only:
        print(json.dumps({'manifest':manifest,'source_bytes':sum(p.stat().st_size for p,_ in files),'files':len(files),'archive_created':False},indent=2));return
    target=output.resolve() if output else ROOT/'distribuzione/NL_Video_Suite-Experimental-2026.11.18-Windows-x64-Core.zip'
    if target.parent != (ROOT/'distribuzione').resolve() or target.suffix!='.zip':raise RuntimeError('Output richiesto dentro distribuzione, estensione .zip')
    target.parent.mkdir(exist_ok=True)
    partial=target.with_suffix('.zip.partial')
    if target.exists() or partial.exists():raise RuntimeError('Archivio gia presente: non sovrascrivo una distribuzione')
    prefix='NL_Video_Suite_Experimental/'
    generated={
        'Avvia_NL_Video_Suite.cmd':'@echo off\r\ncd /d "%~dp0"\r\npowershell.exe -NoProfile -STA -ExecutionPolicy Bypass -File "%~dp0app\\configura_pc.ps1" -Avvia -Desktop\r\nif errorlevel 1 pause\r\n',
        'Configura_PC.cmd':'@echo off\r\npowershell.exe -NoProfile -STA -ExecutionPolicy Bypass -File "%~dp0app\\configura_pc.ps1"\r\npause\r\n',
        'Ferma_NL_Video_Suite.cmd':'@echo off\r\npowershell.exe -NoProfile -ExecutionPolicy Bypass -File "%~dp0app\\stop.ps1"\r\nif errorlevel 1 pause\r\n',
        'runtime/README.txt':'Optional Python runtime not included. Configure an existing compatible interpreter; see documentazione/USO_E_INSTALLAZIONE.md.\n',
        'models/README.txt':'No model weights included or downloaded. Select existing models explicitly in Settings.\n',
        'workflows/README.txt':'Import your ComfyUI API JSON using the application. No private workflow is bundled.\n',
        'data/settings.json':json.dumps({'comfy_url':'http://127.0.0.1:8188','ollama_url':'http://127.0.0.1:11434','llm_provider':'ollama','llm_model':'','ollama_execution':'auto','legacy_sources':{},'workflow_roots':[],'python_executable':'runtime/python/python.exe','engine_root':'engines','model_cache_root':'models','asr_model_path':'models/asr/faster-whisper-large-v3','alignment_model_path':'models/alignment/italian-xlsr53'},indent=2),
    }
    with zipfile.ZipFile(partial,'x',compression=zipfile.ZIP_DEFLATED,compresslevel=6,allowZip64=True) as archive:
        for source,relative in files:
            checksum=hashlib.sha256();size=0
            with source.open('rb') as incoming,archive.open(prefix+relative,'w',force_zip64=True) as outgoing:
                for chunk in iter(lambda:incoming.read(1024*1024),b''):outgoing.write(chunk);checksum.update(chunk);size+=len(chunk)
            manifest['files'].append({'path':relative,'bytes':size,'sha256':checksum.hexdigest()})
        for relative,content in generated.items():
            value=content.encode('utf-8');archive.writestr(prefix+relative,value)
            manifest['files'].append({'path':relative,'bytes':len(value),'sha256':hashlib.sha256(value).hexdigest()})
        archive.writestr(prefix+'PACKAGE-MANIFEST.json',json.dumps(manifest,ensure_ascii=False,indent=2))
    partial.rename(target)
    checksum=hashlib.sha256(target.read_bytes()).hexdigest()
    target.with_suffix('.zip.sha256').write_text(checksum+'  '+target.name+'\n',encoding='ascii')
    print(json.dumps({'archive':str(target),'bytes':target.stat().st_size,'sha256':checksum,'candidate_sha256':digest,'full_1_0_portable_acceptance':False}))

def selected_files(include_data):
    result=[]
    for directory in DIRECTORIES+(('data',) if include_data else ()):
        base=ROOT/directory
        if not base.is_dir():raise RuntimeError('Cartella richiesta assente: '+directory)
        for parent,dirs,files in os.walk(base,followlinks=False):
            dirs[:]=sorted(d for d in dirs if d not in ('__pycache__','.locks','.cache','logs') and not (Path(parent)/d).is_symlink())
            for name in sorted(files):
                source=Path(parent)/name
                if name in EXCLUDED_FILES or source.suffix in ('.pyc','.log','.partial'):continue
                if source.is_symlink():raise RuntimeError('Link non portabile: '+str(source))
                if not source.resolve().is_relative_to(ROOT):raise RuntimeError('File fuori dalla suite: '+str(source))
                result.append(source)
    result.extend(ROOT/name for name in FILES)
    return sorted(result)

def preflight(include_data=False, expected_version=None):
    """Read-only inventory: no hashing/repacking of models and no service/inference calls."""
    required=['app/nl-video-suite.exe','app/tools/ffmpeg.exe','app/tools/ffprobe.exe','runtime/python/python.exe','models/asr/faster-whisper-large-v3/model.bin','models/alignment/italian-xlsr53/pytorch_model.bin']
    for name in required:
        if not (ROOT/name).is_file():raise RuntimeError('Pacchetto incompleto: '+name)
    version=subprocess.run([str(ROOT/'app/nl-video-suite.exe'),'--version'],capture_output=True,text=True,check=True,timeout=15).stdout.strip()
    if not re.fullmatch(r'\d+\.\d+\.\d+(?:-[A-Za-z0-9.-]+)?',version):raise RuntimeError('Versione eseguibile non valida: '+version)
    if expected_version is not None and version!=expected_version:raise RuntimeError('Versione attesa non presente nell eseguibile: '+version+' / '+expected_version)
    files=selected_files(include_data)
    missing=[str(p.relative_to(ROOT)) for p in files if not p.is_file()]
    if missing:raise RuntimeError('File richiesti assenti: '+', '.join(missing))
    components=Counter();packages=Counter();largest=[]
    for source in files:
        rel=source.relative_to(ROOT);size=source.stat().st_size
        components['/'.join(rel.parts[:2])]+=size
        if len(rel.parts)>4 and rel.parts[:4]==('runtime','python','Lib','site-packages'):packages[rel.parts[4]]+=size
        if size>=50_000_000:largest.append({'path':rel.as_posix(),'bytes':size})
    report={'version':version,'includes_personal_data':include_data,'file_count':len(files),'source_bytes':sum(components.values()),'archive_created':False,'models_hashed_or_copied':False,'real_ai_inference':False,'scope':'Read-only packaging preflight, NOT a portable/release acceptance test',
            'components_bytes':dict(components.most_common()),'largest_python_packages_bytes':dict(packages.most_common(20)),
            'largest_files':sorted(largest,key=lambda f:f['bytes'],reverse=True)[:20],
            'capability_notes':{
                'app_and_ffmpeg':'Required for manual editing, native motion, subtitles, stills and MP4. Not a certified standalone lightweight package.',
                'runtime_python_and_asr_models':'Required by bundled WhisperX/Italian alignment. External Qwen is a referenced alternative, not an embedded replacement.',
                'torch_cuda_libraries':'Part of the installed Torch build; DO NOT trim DLLs individually or assume CPU import remains functional.',
                'clipsai_ast_model':'Optional audio-event classifier; retained in this complete package. No deletion performed.',
                'external_services':'Ollama/ComfyUI/Qwen checkpoints not duplicated by packaging.'}}
    return report,files

def main():
    parser=argparse.ArgumentParser(description='Crea ZIP64 portable; per default SENZA i tuoi progetti.')
    parser.add_argument('--con-dati',action='store_true',help='Include progetti e dati personali; richiede suite ferma.')
    parser.add_argument('--verifica',action='store_true',help='Solo inventario read-only JSON, senza ZIP/hash/copie/inferenze.')
    parser.add_argument('--version-attesa',help='Rifiuta una build diversa dalla versione richiesta, es.1.0.0 per la release finale.')
    parser.add_argument('--core',action='store_true',help='Core sperimentale leggero: nessun runtime, modello o binario di terzi.')
    parser.add_argument('--candidate',type=Path,help='Candidato di sviluppo con prova GUI verificata; solo --core.')
    parser.add_argument('--output',type=Path,help='Archivio Core in distribuzione; nessuno staging o copia massiva.')
    args=parser.parse_args()
    if args.core:
        if args.con_dati or not args.candidate:parser.error('--core richiede --candidate e non accetta dati personali')
        return core_package(args.candidate,args.output,args.verifica)
    if args.candidate or args.output:parser.error('--candidate/--output richiedono --core')
    if args.verifica:
        report,_=preflight(args.con_dati,args.version_attesa)
        print(json.dumps(report,ensure_ascii=False,indent=2));return
    lock=None
    if args.con_dati:
        import msvcrt
        lock=(ROOT/'data/server.lock').open('a+b')
        try:msvcrt.locking(lock.fileno(),msvcrt.LK_NBLCK,1)
        except OSError:lock.close();raise RuntimeError('Ferma la suite prima di creare un archivio con i dati personali.')
    try:
        report,files=preflight(args.con_dati,args.version_attesa)
        version=report['version']
        directory=ROOT/'distribuzione';directory.mkdir(exist_ok=True)
        stamp=datetime.now().strftime('%Y%m%d-%H%M%S')
        target=directory/f'NL_Video_Suite-{version}-Windows-x64-Portable-{stamp}.zip'
        temporary=target.with_suffix('.zip.partial')
        print(f'Creazione {target.name}\nProgetti personali: {"INCLUSI" if args.con_dati else "ESCLUSI"}',flush=True)
        manifest={'application':'NL Video Suite','version':version,'created_at':datetime.now(timezone.utc).isoformat(),'platform':'Windows-x64','includes_personal_data':args.con_dati,'external_services':['Ollama','ComfyUI'],'files':[]}
        total=0;last_report=0
        with zipfile.ZipFile(temporary,'x',compression=zipfile.ZIP_DEFLATED,compresslevel=1,allowZip64=True) as archive:
            for source in files:
                relative=source.relative_to(ROOT).as_posix();name='NL_Video_Suite/'+relative
                info=zipfile.ZipInfo.from_file(source,name);info.compress_type=zipfile.ZIP_DEFLATED;info._compresslevel=1
                digest=hashlib.sha256();size=0
                with source.open('rb') as incoming,archive.open(info,'w',force_zip64=True) as outgoing:
                    for chunk in iter(lambda:incoming.read(4*1024*1024),b''):
                        outgoing.write(chunk);digest.update(chunk);size+=len(chunk);total+=len(chunk)
                        if total-last_report>=512*1024*1024:
                            print(f'Elaborati {total/(1024**3):.1f} GiB: {relative}',flush=True);last_report=total
                manifest['files'].append({'path':relative,'bytes':size,'sha256':digest.hexdigest()})
            if not args.con_dati:
                settings={'comfy_url':'http://127.0.0.1:8188','ollama_url':'http://127.0.0.1:11434','legacy_sources':{},'llm_model':'qwen2.5:14b-instruct','python_executable':'runtime/python/python.exe','engine_root':'engines','workflow_roots':[],'asr_model_path':'models/asr/faster-whisper-large-v3','model_cache_root':'models','alignment_model_path':'models/alignment/italian-xlsr53'}
                clean_settings=json.dumps(settings,indent=2).encode('utf-8')
                archive.writestr('NL_Video_Suite/data/settings.json',clean_settings)
                manifest['files'].append({'path':'data/settings.json','bytes':len(clean_settings),'sha256':hashlib.sha256(clean_settings).hexdigest()})
            archive.writestr('NL_Video_Suite/PACKAGE-MANIFEST.json',json.dumps(manifest,ensure_ascii=False,indent=2))
        temporary.rename(target)
        digest=hashlib.sha256()
        with target.open('rb') as incoming:
            for chunk in iter(lambda:incoming.read(4*1024*1024),b''):digest.update(chunk)
        target.with_suffix('.zip.sha256').write_text(digest.hexdigest()+'  '+target.name+'\n',encoding='ascii')
        print(f'Pronto: {target}\nDimensione: {target.stat().st_size/(1024**3):.2f} GiB\nSHA-256 salvato accanto allo ZIP. Nessun file originale spostato o cancellato.',flush=True)
    finally:
        if lock:lock.close()

if __name__=='__main__':
    try:main()
    except Exception as error:print('ERRORE: '+str(error),file=sys.stderr);sys.exit(1)
