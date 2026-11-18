"""Review an existing authentic trailer, without inference or media rendering.

One persistent isolated project; reruns reuse imported originals and saved review.
This is assistant-authored editorial work, not a model or human approval test.
"""
import hashlib
import html
import json
import socket
import subprocess
import time
import urllib.request
import uuid
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
SOURCE = Path(r'K:\AI\Regista_AI\Materiale Trailer')
OUT = ROOT / 'sviluppo/tests/artifacts/audio-first-real/20261106-trailer-review'
DATA = OUT / 'data'


def main():
    DATA.mkdir(parents=True, exist_ok=True)
    settings = DATA / 'settings.json'
    if not settings.exists():
        settings.write_text(json.dumps({'comfy_url': 'http://127.0.0.1:8188',
            'ollama_url': 'http://127.0.0.1:9', 'legacy_sources': {},
            'python_executable': str(ROOT / 'runtime/python/python.exe')}), encoding='utf-8')
    sock = socket.socket(); sock.bind(('127.0.0.1', 0)); port = sock.getsockname()[1]; sock.close()
    base = 'http://127.0.0.1:' + str(port)
    def api(path, method='GET', body=None):
        req = urllib.request.Request(base + path, method=method,
            data=json.dumps(body).encode() if body is not None else None,
            headers={'Content-Type': 'application/json'})
        with urllib.request.urlopen(req, timeout=40) as response:
            return json.load(response)
    def done(job):
        for _ in range(400):
            value = api('/api/jobs/' + job['id'])
            if value['status'] in ('failed', 'succeeded'):
                assert value['status'] == 'succeeded', value['message']
                return value
            time.sleep(.1)
        raise RuntimeError('Job timeout; existing work preserved')
    log = (OUT / 'server.log').open('ab')
    process = subprocess.Popen([str(ROOT / 'sviluppo/target/release/nl-video-suite-preproduction.exe'),
        '--port', str(port), '--data-dir', str(DATA)], cwd=str(ROOT), stdout=log, stderr=log,
        creationflags=getattr(subprocess, 'CREATE_NO_WINDOW', 0))
    try:
        for _ in range(100):
            try:
                projects = api('/api/projects'); break
            except OSError: time.sleep(.1)
        else: raise RuntimeError('Isolated app not ready')
        p = projects[0] if projects else api('/api/projects', 'POST', {
            'title': 'Voice Shield · regia reale da revisionare', 'aspect_ratio': '16:9'})
        prefix = '/api/projects/' + p['id']
        def normalized(value):
            value=str(value)
            return (value[4:] if value.startswith('\\\\?\\') else value).casefold()
        files = [('01/intro.mp3','01/Qwen_Next_00035_.png'),
                 ('02/Test Scena1 - Audio.wav','02/asset_00010_.png'),
                 ('03/ScenaHorrorAsset.wav','03/Qwen_Next_00118_.png'),
                 ('04/ScenaBattagliaAsset.wav','04/Qwen_Next_00074_.png')]
        for pair in files:
            for name in pair:
                source = str(SOURCE / name)
                if not any(normalized(a['source_path']) == normalized(source) for a in p['assets']):
                    done(api(prefix+'/assets', 'POST', {'path': source}))
                    p = api(prefix)
        catalog = api('/api/production-presets')
        if not p.get('production_preset'):
            preset = next(v for v in catalog if v['source'] == 'trailer')
            p = api(prefix+'/production-preset', 'PUT', {'expected_updated_at': p['updated_at'], 'selection': preset})
        prod = api(prefix+'/narrative')
        observations = [
            'Osservazione visiva dell’assistente: sala con pubblico e schermo SITV; non è una reference identificativa del protagonista.',
            'Osservazione visiva dell’assistente: uomo con capelli lunghi chiari, occhiali, presso una finestra. Identità non confermata; utilizzare come immagine illustrativa, non come stesso protagonista degli altri frame.',
            'Osservazione visiva dell’assistente: uomo con capelli scuri e giacca in strada notturna. Identità non confermata; non stabilire equivalenza con la persona della seconda reference.',
            'Osservazione visiva dell’assistente: robot scuro con occhi rossi e scritta luminosa in città danneggiata. Elemento di finzione, non evidenza di eventi reali.'
        ]
        acts = ['Annuncio / promessa', 'Dubbio / mestiere', 'Minaccia / risposta', 'Contrattacco / dedica']
        # Editable staging durations, not inferred word alignment or musical beats.
        preferred = [[4000,4000], [7000,6000,4000,5000,5000,7000,4000,6000,8000,5000,5000],
                     [4000,5000,3000,6000], [4000,5000,4000,6000,4000,5000,6000]]
        checks, workflow_reports = [], []
        asset_for = lambda name: next(a for a in p['assets'] if normalized(a['source_path']) == normalized(SOURCE/name))
        selected_workflows = []
        for group, (audio_name, image_name) in enumerate(files):
            graph_path = SOURCE / ('workflows_modificati/c%02d_vA_modified.json' % [1,3,5,7][group])
            imported = api('/api/workflows/import', 'POST', {'name': graph_path.name,
                'graph': json.loads(graph_path.read_text(encoding='utf-8'))})['workflow']
            check_path=OUT/('workflow-check-'+str(group+1)+'.json')
            cached=json.loads(check_path.read_text(encoding='utf-8')) if check_path.exists() else None
            bindings = {'151.audio':asset_for(audio_name)['id'],
                        '152.image':asset_for(image_name)['id'], '153.image':asset_for(image_name)['id']}
            reuse=bool(cached and cached['workflow_id']==imported['id'] and not cached['controls']['offline'])
            controls=cached['controls'] if reuse else api('/api/workflows/'+imported['id']+'/narrative-controls?project_id='+p['id'])
            report=cached['preflight'] if reuse else api('/api/workflows/diagnose','POST',{'workflow_id':imported['id'],'asset_inputs':bindings})
            refs = {c['address']: c for c in controls['reference_inputs']}
            workflow_reports.append({'source':str(graph_path),'workflow_id':imported['id'],
                'controls':controls,'preflight':report,'semantic_acceptance':False,'live_check_reused':reuse})
            (OUT/('workflow-check-'+str(group+1)+'.json')).write_text(json.dumps(workflow_reports[-1],ensure_ascii=False,indent=2),encoding='utf-8')
            assert any(c['address']=='138.value' for c in controls['text_inputs'])
            if not (refs['151.audio']['media_kind']=='audio' and refs['151.audio']['upload_supported']):
                raise RuntimeError('Live audio loader unverified; see workflow-check-'+str(group+1)+'.json')
            assert refs['152.image']['media_kind']=='image' and refs['153.image']['media_kind']=='image'
            assert json.loads(graph_path.read_text(encoding='utf-8'))['92']['class_type']=='SaveVideo'
            # output_roles are optional author tags, not inferred capabilities.
            (OUT/('workflow-check-'+str(group+1)+'.json')).write_text(json.dumps(workflow_reports[-1],ensure_ascii=False,indent=2),encoding='utf-8')
            selected_workflows.append(imported)
        if not prod['scenes']:
            prod.update(title=p['title'], premise='Trailer di finzione Voice Shield: annuncio, dubbio, minaccia, risposta. Arco tratto dalla bozza locale, non da un nuovo ascolto o ASR.',
                creative_brief='Il master originale è il riferimento temporale. Nessuna promessa di lip-sync o identità. I frame disponibili sono reference provvisorie; la continuità del protagonista richiede scelta umana.',
                script_approved=False, audio_master_approved=False)
            prod['continuity'].update(style='Fotografia cinematografica, contrasti controllati; raccordi sobri.',
                established_facts='Solo universo di finzione tratto da Bozza Testo Trailer.txt. Nessuna identità reale o equivalenza fra i due volti confermata.',
                camera_grammar='Aprire il contesto, stringere nei dubbi, allargare nel confronto. Non animare ogni fotografia allo stesso modo.')
            clock = 0
            for group, pair in enumerate(files):
                audio, image = map(asset_for, pair); duration = audio['media']['duration_ms']; segment = str(uuid.uuid4()); reference = str(uuid.uuid4())
                prod['audio_segments'].append({'id':segment,'asset_id':audio['id'],'source':{'start_ms':0,'end_ms':duration},
                    'start_ms':clock,'gain':1,'label':acts[group],
                    'notes':'Origine e intenzione dalla bozza locale, non trascrizione verificata. Ascoltare e rivedere gli stacchi. Nessuna classificazione automatica Voce/Musica/FX.'})
                prod['references'].append({'id':reference,'kind':'source','name':acts[group]+' · reference provvisoria',
                    'source_asset_id':image['id'],'notes':observations[group],'approved':False,'use_for_story':True})
                boundaries = preferred[group] + [duration-sum(preferred[group])]
                local = 0
                for n, length in enumerate(boundaries):
                    assert length > 0
                    span = {'start_ms':local,'end_ms':local+length}
                    prod['scenes'].append({'id':str(uuid.uuid4()),'title':acts[group]+' · shot '+str(n+1),
                        'narration':'', 'visual_prompt': 'One cinematic shot using the bound image as an illustrative visual reference. Preserve its visible setting and composition. '+
                            ['Establish the event audience; do not invent a close-up speaking hero.',
                             'Reflective insert by the window. Do not claim this is the same identity as other references.',
                             'Build tension in the night street, restrained camera; identity remains unconfirmed.',
                             'Reveal the fictional robot in the damaged street; preserve its design.'][group]+
                            ' No lip-sync guarantee, no new titles or written words. Original soundtrack is retained separately; do not replace it.',
                        'duration_ms':length,'reference_ids':[reference], 'visual_kind':'video',
                        'voice_mode':'none','visual_asset_id':image['id'], 'visual_approved':False,
                        'audio_anchor':{'segment_id':segment,'source':span},
                        'direction':{'act':acts[group], 'camera':['Inquadratura ferma','Avvicinamento lento','Dettaglio / raccordo'][n%3],
                            'state_before':'Proposta temporale da ascoltare', 'state_after':'Non approvata: verificare frase e continuità'},
                        'visual_workflow':{'workflow_id':selected_workflows[group]['id'],'text_input':'138.value',
                            'inputs':{'132.value':length/1000},
                            'asset_inputs':{'151.audio':audio['id'],'152.image':image['id'],'153.image':image['id']},
                            'asset_ranges':{'151.audio':span}}})
                    local += length
                assert local == duration
                clock += duration
            prod = api(prefix+'/narrative','PUT',{'production':prod})
            p = api(prefix)
            api(prefix+'/edit-versions','POST',{'name':'QA reale · piano non approvato, master integro',
                'expected_updated_at':p['updated_at'],'expected_narrative_revision':prod['revision']})
        for segment in prod['audio_segments']:
            shots=sorted((s for s in prod['scenes'] if s['audio_anchor']['segment_id']==segment['id']),key=lambda s:s['audio_anchor']['source']['start_ms'])
            assert shots[0]['audio_anchor']['source']['start_ms']==segment['source']['start_ms']
            assert shots[-1]['audio_anchor']['source']['end_ms']==segment['source']['end_ms']
            assert all(a['audio_anchor']['source']['end_ms']==b['audio_anchor']['source']['start_ms'] for a,b in zip(shots,shots[1:]))
        dossier = api(prefix+'/production-package')
        assert len(dossier['narrative']['scenes'])==28
        assert any(v['status']=='needs_review' for v in dossier['needs_attention'])
        checks.append('Four genuine complete sources, 28 editable shots; exact per-source coverage without overlaps/gaps, independent original master, no fabricated ASR times. Proposed shot times require listening.')
        hashes=[]
        for asset in p['assets']:
            original=Path(asset['source_path']); copy=DATA/'projects'/p['id']/'assets'/asset['file_name']
            def digest(f):
                h=hashlib.sha256()
                with f.open('rb') as stream:
                    for chunk in iter(lambda:stream.read(1024*1024),b''): h.update(chunk)
                return h.hexdigest()
            assert digest(original)==digest(copy)
            hashes.append({'source':str(original),'sha256':digest(original)})
        report={'technical_pass':True,'simulated_media':False,'new_inference':False,'new_render':False,
            'human_acceptance':False,'human_listening':False,'media_provenance':'Audio locali originali; reference visive già generate in Regista AI, non nuove riprese o nuove inferenze.', 'project_id':p['id'],'shots':len(prod['scenes']),
            'duration_ms':sum(s['source']['end_ms'] for s in prod['audio_segments']),
            'checks':checks,'source_integrity':hashes,'workflow_reports':workflow_reports,
            'creative_findings':[
                'Il vecchio c01 richiede un protagonista parlante ma collega due viste della folla: identità e lip-sync non dimostrati.',
                'I vecchi prompt citano Eroe.png, robot.png e Audio N: nomi simbolici, non prova del collegamento effettivo.',
                'I due ritratti osservati differiscono nell’aspetto. Nessuna identità comune confermata: le nuove bozze li trattano come inserti illustrativi, con continuità da approvare.',
                'I vecchi grafi configurano 7s ma i prompt ne descrivono 5; le sorgenti misurano 13,248/67,469/24,684/41,007s. Il piano copre il master misurato con intervalli modificabili, non ancora verificati sulla frase.',
                'Il ritaglio LoadAudioUI deve usare il clock locale dopo l’estrazione: correzione nella copia inviata e ricevuta esplicita; grafo originale intatto.',
                'I due ingressi immagine condividono deliberatamente una reference provvisoria per shot: non rappresentano due identità confermate. Scegliere il character sheet prima della produzione.',
                'Non sono stati analizzati beat, eventi o fonemi. Stacchi, progressione emotiva, identità e ascolto restano gate umani; nessun risultato generativo o successo artistico dichiarato.']}
        (OUT/'report.json').write_text(json.dumps(report,ensure_ascii=False,indent=2),encoding='utf-8')
        (OUT/'dossier.json').write_text(json.dumps(dossier,ensure_ascii=False,indent=2),encoding='utf-8')
        preflight_html='<section><h2>Compatibilità ComfyUI reale · sola lettura</h2>'+''.join('<p>Workflow '+str(i+1)+': '+('PASS tecnico' if w['preflight']['ready'] else 'BLOCCATO')+'</p><ul>'+''.join('<li>'+html.escape(x['code']+': '+x['message'])+'</li>' for x in w['preflight']['issues'])+'</ul>' for i,w in enumerate(workflow_reports))+'</section>'
        cards=[]
        for i,segment in enumerate(prod['audio_segments']):
            audio=next(a for a in p['assets'] if a['id']==segment['asset_id']);image=asset_for(files[i][1]);rel=lambda a:'data/projects/'+p['id']+'/assets/'+a['file_name']
            cards.append('<section><h2>'+html.escape(segment['label'])+'</h2><p>Master '+str(segment['source']['end_ms']/1000)+'s · non approvato</p><audio controls preload="metadata" src="'+html.escape(rel(audio))+'"></audio><br><img src="'+html.escape(rel(image))+'"><p>'+html.escape(observations[i])+'</p><ol>'+''.join('<li>'+html.escape(s['title'])+' · '+str(s['audio_anchor']['source'])+'<p>'+html.escape(s['visual_prompt'])+'</p></li>' for s in prod['scenes'] if s['audio_anchor']['segment_id']==segment['id'])+'</ol></section>')
        (OUT/'VALUTA_REGIA.html').write_text('<!doctype html><meta charset="utf-8"><title>Regia trailer · QA reale</title><style>body{font:16px system-ui;background:#16121d;color:#eee;max-width:1000px;margin:30px auto;padding:20px}section{border:1px solid #7531fe;padding:20px;margin:20px 0}img{width:100%;max-width:640px}p,li{line-height:1.5}a{color:#e4469b}</style><h1>Voice Shield · proposta da revisionare</h1><p>Materiali autentici riusati. Piano scritto dall’assistente, nessuna nuova inferenza. Nessun nuovo MP4, ascolto umano o accettazione artistica certificati. I confini sono proposte di regia, non allineamento delle parole.</p><ul>'+''.join('<li>'+html.escape(v)+'</li>' for v in report['creative_findings'])+'</ul>'+preflight_html+''.join(cards),encoding='utf-8')
        print(json.dumps({'project_id':p['id'],'shots':report['shots'],'duration_ms':report['duration_ms'],
            'preflight_ready':[w['preflight']['ready'] for w in workflow_reports],
            'review':str(OUT/'VALUTA_REGIA.html')},ensure_ascii=False))
    finally:
        process.terminate();process.wait(timeout=10);log.close()


if __name__ == '__main__':
    main()
