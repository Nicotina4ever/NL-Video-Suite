"""Standard-library dossier writer, embedded by Rust; no models or rendering."""
import csv
import hashlib
import html
import io
import json
from pathlib import Path
import sys
import zipfile


def timecode(ms):
    ms = max(0, int(ms))
    seconds, millis = divmod(ms, 1000)
    minutes, seconds = divmod(seconds, 60)
    hours, minutes = divmod(minutes, 60)
    return f"{hours:02}:{minutes:02}:{seconds:02}.{millis:03}"


def write_package(data, output):
    output = Path(output)
    partial = output.with_suffix('.zip.partial')
    if output.exists() or partial.exists():
        raise ValueError('Output already exists; no overwrite')
    root = Path(data['project_root']).resolve()
    include = data.get('include_media', False)
    rows = []
    covered = set()
    for index,row in enumerate(data['album_script']):
        if row['timing_source'] == 'deleted_from_timeline':
            continue
        if row.get('timeline_clip_id'):
            covered.add(row['timeline_clip_id'])
        rows.append(dict(id=row['item_id'], asset_id=row['asset_id'], chapter=row['chapter'],
                         start_ms=row['start_ms'], duration_ms=row['duration_ms'],
                         title=row.get('title') or f'Fotografia {index+1:02}', text=row['cue']['text'], direction=row['cue']['direction'],
                         timing_source=row['timing_source'], stale=row['stale'], origin=row['cue']['origin']))
    # Native scene IDs are already linked through the common assembler's stable UUIDs.
    import uuid
    clips = {c['id']: c for c in data['project']['timeline']['clips']}
    for scene in data['narrative']['scenes']:
        # Prefer the explicit mapping supplied by the native manifest; never invent timing.
        clip = next((c for c in clips.values() if c['id'] == scene.get('timeline_clip_id')), None)
        if clip:
            covered.add(clip['id'])
        rows.append(dict(id=scene['id'], asset_id=scene.get('visual_asset_id'), chapter=scene.get('direction', {}).get('act', ''),
                         start_ms=clip['start_ms'] if clip else None,
                         duration_ms=clip['duration_ms'] if clip else scene['duration_ms'], title=scene['title'],
                         text=scene['narration'], direction='\n'.join(f'{k}: {v}' for k,v in scene.get('direction',{}).items() if v) or 'Indicazioni di regia da completare.',
                         timing_source='timeline' if clip else 'unassembled_scene', stale=not bool(clip), origin='saved_narrative_draft'))
    assets = {a['id']: a for a in data['assets']}
    for clip in clips.values():
        asset=assets.get(clip['asset_id'], {})
        if clip['id'] in covered or asset.get('kind') not in ('image','video'):
            continue
        rows.append(dict(id=clip['id'],asset_id=clip['asset_id'],chapter='',start_ms=clip['start_ms'],duration_ms=clip['duration_ms'],trim_in_ms=clip['trim_in_ms'],title=clip.get('label') or asset.get('name') or 'Inquadratura manuale',text='',direction='Clip manuale: conserva la composizione salvata. Trasformazioni complete in project.json → timeline.clips['+clip['id']+'].transform.',timing_source='timeline',stale=False,origin='manual_timeline'))
    rows.sort(key=lambda r: (r['start_ms'] is None, r['start_ms'] or 0, r['id']))
    data['shot_cards'] = rows
    try:
        with zipfile.ZipFile(partial, 'w', allowZip64=True, compression=zipfile.ZIP_DEFLATED) as z:
            for i, asset in enumerate(data['assets']):
                asset['included'] = False
                if include and asset.get('pertinent',True) and asset.get('available',True):
                    source = (root / asset['project_relative_path']).resolve()
                    if not source.is_relative_to(root / 'assets') or not source.is_file():
                        raise ValueError(f"Missing or non-owned project media: {asset['id']}")
                    before = source.stat()
                    digest = hashlib.sha256()
                    info = zipfile.ZipInfo(asset['package_path'])
                    info.compress_type = zipfile.ZIP_STORED
                    with source.open('rb') as f, z.open(info, 'w', force_zip64=True) as dst:
                        while block := f.read(1024 * 1024):
                            dst.write(block)
                            digest.update(block)
                    after = source.stat()
                    if (before.st_size, before.st_mtime_ns) != (after.st_size, after.st_mtime_ns):
                        raise ValueError('Source changed while packaging: ' + asset['id'])
                    asset.update(included=True, sha256=digest.hexdigest())
                print(json.dumps(dict(done=i + 1, total=len(data['assets']))), flush=True)
            for name in ('project', 'narrative'):
                z.writestr(name + '.json', json.dumps(data[name], ensure_ascii=False, indent=2))
            sheet = io.StringIO(newline='')
            writer = csv.writer(sheet)
            writer.writerow(['shot_id', 'asset_id', 'chapter', 'start_HH_MM_SS_mmm', 'duration_ms', 'title', 'narration', 'direction', 'timing_source', 'needs_review'])
            text, cards = [], []
            assets = {a['id']: a for a in data['assets']}
            for row in rows:
                start = timecode(row['start_ms']) if row['start_ms'] is not None else 'NON MONTATO'
                values = [row[k] if k != 'start_ms' else start for k in ('id', 'asset_id', 'chapter', 'start_ms', 'duration_ms', 'title', 'text', 'direction', 'timing_source', 'stale')]
                writer.writerow(["'" + v if isinstance(v, str) and v.lstrip().startswith(('=', '+', '-', '@')) else v for v in values])
                text.append(f"[{row['chapter']} | {start} | {row['duration_ms']} ms | {row['id']}]\n{row['title']}\n{row['text']}\nRegia: {row['direction']}\nStato: {row['timing_source']}, revisione={row['stale']}\n")
                asset = assets.get(row['asset_id'], {})
                picture = f'<img alt="Materiale dello shot" loading="lazy" src="{html.escape(asset["package_path"], quote=True)}">' if asset.get('included') and asset.get('kind') == 'image' else ''
                if asset.get('included') and asset.get('kind')=='video':
                    picture=f'<video controls preload="metadata" src="{html.escape(asset["package_path"],quote=True)}#t={row.get("trim_in_ms",0)/1000:.3f},{(row.get("trim_in_ms",0)+row["duration_ms"])/1000:.3f}"></video>'
                cards.append('<article>' + picture + '<h2>' + (html.escape(str(row['chapter']))+' · ' if row['chapter'] else '') + html.escape(row['title']) + '</h2><p>' + html.escape(start) + f" · {row['duration_ms']} ms</p><p>" + html.escape(row['text']) + '</p><pre>' + html.escape(row['direction']) + '</pre><small>' + html.escape(str(row['asset_id']) if row['asset_id'] else 'Asset non assegnato') + ' · ' + html.escape({'timeline':'Tempi dalla timeline','unassembled_scene':'Shot non montato','planned_sequence':'Tempi pianificati'}.get(row['timing_source'],row['timing_source'])) + '</small></article>')
            z.writestr('shotlist.csv', '\ufeff' + sheet.getvalue())
            z.writestr('copione.txt', '\n'.join(text))
            orientation = data['project'].get('production_preset')
            z.writestr('orientamento.txt', ('Indicazioni artistiche revisionabili, non fatti osservati.\n' + '\n\n'.join(k + ': ' + str(v) for k, v in orientation.items())) if orientation else 'Nessun orientamento imposto. Progetto libero.')
            titles = io.StringIO(newline='')
            tw = csv.writer(titles)
            tw.writerow(['motion_id','name','chapters','start_HH_MM_SS_mmm','end_HH_MM_SS_mmm','text','approved'])
            for title in data.get('motion_titles',[]):
                span = title.get('effective_range')
                chapters = sorted({r['chapter'] for r in rows if span and r['start_ms'] is not None and r['start_ms'] < span['end_ms'] and r['start_ms']+r['duration_ms'] > span['start_ms']})
                value = '\n'.join(l['text'] for l in title['text_layers'])
                cells=[title['id'],title['name'],' / '.join(chapters),timecode(span['start_ms']) if span else 'NON VISIBILE',timecode(span['end_ms']) if span else '',value,title['approved']]
                tw.writerow(["'"+v if isinstance(v,str) and v.lstrip().startswith(('=','+','-','@')) else v for v in cells])
            z.writestr('titoli.csv','\ufeff'+titles.getvalue())
            sound=io.StringIO(newline='');sw=csv.writer(sound);sw.writerow(['clip_id','asset_id','name','start_HH_MM_SS_mmm','source_in_ms','duration_ms','gain','bus','routing_source'])
            routing=data['project']['audio'].get('routing',{});tracks={t['id']:t for t in data['project']['timeline']['tracks']};sound_cues=[]
            for clip in clips.values():
                asset=assets.get(clip['asset_id'],{})
                if not (asset.get('media') or {}).get('has_audio') or clip['volume']<=0:
                    continue
                track=tracks.get(clip['track_id'],{});bus=routing.get(clip['track_id'],'music' if track.get('kind')=='audio' else 'voice');mode='explicit' if clip['track_id'] in routing else 'legacy routing default; content not classified'
                cells=[clip['id'],clip['asset_id'],asset.get('name',''),timecode(clip['start_ms']),clip['trim_in_ms'],clip['duration_ms'],clip['volume'],bus,mode]
                sw.writerow(["'"+v if isinstance(v,str) and v.lstrip().startswith(('=','+','-','@')) else v for v in cells]);sound_cues.append(dict(clip=clip,bus=bus,routing_source=mode))
            data['audio_cues']=sound_cues
            z.writestr('audio-cues.csv','\ufeff'+sound.getvalue())
            z.writestr('manifest.json', json.dumps(data, ensure_ascii=False, indent=2))
            instructions = '''AFTER EFFECTS — PASSAGGIO MANUALE, NON AEP
1. Crea la composizione leggendo aspect_ratio e social/project settings in project.json. Mantieni il frame rate razionale del progetto, se impostato; i timecode nel CSV sono HH:MM:SS.mmm, non frame SMPTE.
2. Importa media/ se incluso; altrimenti ricollega le copie del progetto indicate nel manifest. Controlla SHA-256 quando presente. Non alterare gli originali.
3. Usa shotlist.csv e copione.txt: tempi dalla timeline effettiva solo per gli shot marcati timeline. NON MONTATO e needs_review richiedono regia manuale.
4. Timeline e transform in project.json descrivono posizioni, trim, adattamento, fade e pan/zoom della suite. Sono dati nativi documentali, non keyframe AE già convertiti: ricostruisci e confronta l'anteprima.
5. I testi/livelli Motion rimangono editabili in project.motion.graphics; titoli.csv riporta testi e intervalli effettivi con i capitoli sovrapposti. Ricrea gerarchia, font disponibili e safe area. Non convertire automaticamente un effetto nativo in un effetto AE di nome simile.
6. Adatta la struttura al progetto: capitoli, atti, sezioni o montaggio libero. I trattamenti sono facoltativi e modificabili; non incorporare il testo nelle fotografie. Copione salvato e proposte IA non equivalgono ad approvazione artistica.
7. Audio originale, bus, automazioni e finalizzazione sono documentati in project.audio e timeline. Non applicare normalizzazioni aggiuntive senza scelta editoriale; controlla sincronismo ascoltando.
8. Prompt, reference, continuità e binding workflow sono in narrative.json/manifest.workflows. I grafi forniti sono quelli locali salvati, non un'attestazione di preflight corrente. Verifica nodi, modelli già disponibili e compatibilità degli ingressi prima dell'invio. Nessun checkpoint incluso e nessuna generazione eseguita dal dossier. Gli asset_ranges indicano finestre nel file originale di conditioning, non tempi del video generato o sostituzioni del master originale.
'''
            z.writestr('AFTER_EFFECTS.txt', instructions)
            for w in data['workflows']:
                if w.get('api_graph') is not None:
                    z.writestr('workflows/' + str(uuid.UUID(w['id'])) + '.json', json.dumps(w['api_graph'], ensure_ascii=False, indent=2))
            attention=data.get('needs_attention',[])
            labels={'shot_visual':'visual dello shot','shot_voice':'voce dello shot','reference':'reference','not_generated_or_unassigned':'non generato o non assegnato','not_generated':'non generato','needs_review':'da revisionare','missing_media':'media','missing_file':'file mancante','missing_graph_file':'file workflow mancante','missing_from_catalog':'assente dal catalogo','unassigned_manual_audio':'audio manuale non assegnato'}
            missing='<section><h2>Da completare o revisionare</h2><ul>'+''.join('<li>'+html.escape(str(r.get('name') or r.get('id')))+' · '+html.escape(labels.get(r['kind'],str(r['kind'])))+' · '+html.escape(labels.get(r['status'],str(r['status'])))+'</li>' for r in attention)+'</ul></section>' if attention else '<p>Nessun elemento mancante o da revisionare rilevato dal contratto tecnico. La qualità artistica richiede revisione umana.</p>'
            audio_cards=[]
            for segment in data['narrative'].get('audio_segments', []):
                a=assets.get(segment['asset_id'], {})
                label=html.escape(segment.get('label') or a.get('name') or 'Master originale')
                start=segment['source']['start_ms']/1000
                end=segment['source']['end_ms']/1000
                if a.get('included'):
                    url=html.escape(a['package_path'], quote=True)
                    audio_cards.append(f'<section><h2>{label}</h2><p>Finestra sorgente {start:.3f}–{end:.3f}s · posizione master {segment["start_ms"]/1000:.3f}s. File originale, nessuna normalizzazione.</p><audio controls preload="metadata" data-start="{start}" data-end="{end}" src="{url}"></audio><p><a href="{url}">Apri / scarica audio originale</a></p></section>')
                else:
                    audio_cards.append(f'<section><h2>{label}</h2><p>Audio non incluso: usa il dossier con i media oppure ricollega il file dal manifest. Finestra sorgente {start:.3f}–{end:.3f}s.</p></section>')
            audio_html=''.join(audio_cards)+"<script>for(const a of document.querySelectorAll('audio')){a.addEventListener('loadedmetadata',()=>{if(a.dataset.start)a.currentTime=Number(a.dataset.start);});a.addEventListener('play',()=>{if(a.currentTime>=Number(a.dataset.end)||a.currentTime<Number(a.dataset.start))a.currentTime=Number(a.dataset.start);});a.addEventListener('timeupdate',()=>{if(a.currentTime>=Number(a.dataset.end))a.pause();});a.addEventListener('error',()=>{const p=document.createElement('p');p.textContent='Audio non riproducibile dal browser: controlla il file originale con il collegamento qui sopra.';a.after(p);},{once:true});}</script>"
            z.writestr('LEGGIMI.html', '<!doctype html><meta charset="utf-8"><title>Dossier di produzione</title><style>body{font:16px system-ui;background:#17121e;color:#eee;max-width:1000px;margin:32px auto;padding:16px}a{color:#d9b6ff}article,section{padding:20px;margin:16px 0;border:1px solid #7531ff;border-radius:12px}img,video{max-width:100%;max-height:280px}pre{white-space:pre-wrap;overflow-wrap:anywhere}small{overflow-wrap:anywhere}</style><h1>' + html.escape(data['project_title']) + '</h1><p>Stato salvato del progetto. Nessuna nuova inferenza, registrazione o approvazione. Nessun export AEP. I timecode sono millisecondi, non frame SMPTE.</p><p>' + ('Media pertinenti disponibili inclusi con SHA-256; consulta i mancanti qui sotto.' if include else 'Solo riferimenti: i media vanno ricollegati al progetto originale.') + '</p><p><a href="orientamento.txt">Orientamento</a> · <a href="copione.txt">Copione</a> · <a href="shotlist.csv">Shot list</a> · <a href="titoli.csv">Titoli</a> · <a href="audio-cues.csv">Cue audio</a> · <a href="manifest.json">Manifest e provenienza</a> · <a href="AFTER_EFFECTS.txt">Indicazioni After Effects</a></p>' + missing + audio_html + ''.join(cards))
        partial.replace(output)
    except BaseException:
        if partial.exists():
            partial.unlink()  # Only the unique archive created by this invocation.
        raise


if __name__ == '__main__':
    write_package(json.loads(Path(sys.argv[1]).read_text(encoding='utf-8')), sys.argv[2])
