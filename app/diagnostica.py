"""Read-only portable prerequisites check. Does not install or start services."""
import importlib.metadata
import json
import os
from pathlib import Path
import platform
import subprocess
import sys
import urllib.request

ROOT=Path(__file__).resolve().parents[1]
os.environ['PYTHONDONTWRITEBYTECODE']='1'
sys.stdout.reconfigure(encoding='utf-8',errors='replace')

def main():
    report={'version':None,'platform':platform.platform(),'architecture':platform.machine(),'installation':str(ROOT),'checks':{}}
    try:
        version=subprocess.run([str(ROOT/'app/nl-video-suite.exe'),'--version'],capture_output=True,text=True,check=True,timeout=15).stdout.strip()
        report['version']=version
        report['checks']['applicazione']={'ok':True,'detail':'NL Video Suite '+version}
    except Exception as error:
        report['checks']['applicazione']={'ok':False,'detail':str(error)}
    settings=ROOT/'data/settings.json'
    config=json.loads(settings.read_text(encoding='utf-8-sig')) if settings.exists() else {}
    for tool in ('ffmpeg','ffprobe'):
        try:
            result=subprocess.run([str(ROOT/'app/tools'/f'{tool}.exe'),'-version'],capture_output=True,check=True,timeout=15)
            report['checks'][tool]={'ok':True,'detail':result.stdout.decode(errors='replace').splitlines()[0]}
        except Exception as error: report['checks'][tool]={'ok':False,'detail':str(error)}
    try:
        import torch
        import whisperx
        import requests
        report['checks']['python_ai']={'ok':True,'detail':f'Python {platform.python_version()}, torch {torch.__version__}, WhisperX {importlib.metadata.version("whisperx")}'}
    except Exception as error:
        report['checks']['python_ai']={'ok':False,'detail':str(error)+'; se mancano DLL esegui la verifica del Microsoft Visual C++ Redistributable x64.'}
    asr=ROOT/config.get('asr_model_path','models/asr/faster-whisper-large-v3')
    alignment=ROOT/config.get('alignment_model_path','models/alignment/italian-xlsr53')
    report['checks']['modello_whisper']={'ok':(asr/'model.bin').is_file(),'detail':str(asr)}
    report['checks']['allineatore_italiano']={'ok':(alignment/'pytorch_model.bin').is_file(),'detail':str(alignment)}
    llm=('LLM compatibile','openai_base_url','http://127.0.0.1:8080/v1','/models') if config.get('llm_provider')=='openai_compatible' else ('Ollama','ollama_url','http://127.0.0.1:11434','/api/tags')
    for service,key,default,path in [llm,('ComfyUI','comfy_url','http://127.0.0.1:8188','/system_stats')]:
        try:
            with urllib.request.urlopen(config.get(key,default).rstrip('/')+path,timeout=3) as response: ok=response.status==200
        except Exception: ok=False
        report['checks'][service]={'ok':ok,'detail':'Servizio esterno opzionale per montaggio/trascrizione, necessario per le rispettive generazioni AI.'}
    if '--json' in sys.argv: print(json.dumps(report,ensure_ascii=False,indent=2))
    else:
        print('\nNL VIDEO SUITE | Verifica PC\n'+str(ROOT)+'\n')
        for name,result in report['checks'].items(): print(('OK     ' if result['ok'] else 'MANCA  ')+name+' — '+result['detail'])
        print('\nWindows x64. Nessun programma o modello installato da questa verifica.\nOllama e ComfyUI non sono inclusi nel pacchetto. Vedi LEGGIMI.html.')
    return 0 if all(report['checks'][key]['ok'] for key in ('applicazione','ffmpeg','ffprobe','python_ai','modello_whisper','allineatore_italiano')) else 1

if __name__=='__main__':sys.exit(main())
