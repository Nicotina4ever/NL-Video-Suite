"""Read-only license evidence. NOT a legal opinion/compliance certification."""
from email.parser import Parser
import csv
import json
from pathlib import Path
import subprocess
from datetime import datetime, timezone

ROOT=Path(__file__).resolve().parents[2]


def main():
    out=ROOT/'sviluppo/tests/artifacts/license-audit'/datetime.now(timezone.utc).strftime('%Y%m%dT%H%M%SZ')
    out.mkdir(parents=True)
    report=dict(technical_inventory_completed=True,public_distribution_approved=False,downloads=False,packages=[],open_gates=[])
    for info in sorted((ROOT/'runtime/python/Lib/site-packages').glob('*.dist-info')):
        metadata=info/'METADATA'
        if not metadata.is_file():
            report['open_gates'].append('Missing METADATA: '+info.name)
            continue
        m=Parser().parsestr(metadata.read_text(encoding='utf-8',errors='replace'))
        texts=[str(p.relative_to(ROOT)).replace('\\','/') for p in info.rglob('*') if p.is_file() and any(s in p.name.lower() for s in ('license','licence','copying','notice'))]
        record=info/'RECORD'
        if record.is_file():
            for row in csv.reader(record.read_text(encoding='utf-8').splitlines()):
                if row and any(s in Path(row[0]).name.lower() for s in ('license','licence','copying','notice')):
                    candidate=(info.parent/row[0]).resolve()
                    if candidate.is_relative_to(ROOT) and candidate.is_file():
                        texts.append(candidate.relative_to(ROOT).as_posix())
        texts=sorted(set(texts))
        expression=m.get('License-Expression') or m.get('License')
        classifiers=[v for v in m.get_all('Classifier',[]) if v.startswith('License ::')]
        item=dict(name=m.get('Name'),version=m.get('Version'),license_declared=expression,license_classifiers=classifiers,license_text_files=texts,project_urls=m.get_all('Project-URL',[]))
        report['packages'].append(item)
        if not texts:
            report['open_gates'].append('Review license text elsewhere in package or metadata: '+str(item['name']))
    for name in ('ffmpeg','ffprobe'):
        binary=ROOT/'app/tools'/f'{name}.exe'
        for flag in ('-version','-buildconf','-L'):
            r=subprocess.run([str(binary),flag],capture_output=True,text=True,timeout=20)
            (out/f'{name}-{flag[1:]}.txt').write_text(r.stdout+'\n'+r.stderr,encoding='utf-8')
            assert r.returncode==0
    report['engine_license_files']=[str(p.relative_to(ROOT)).replace('\\','/') for p in (ROOT/'engines').rglob('*') if p.is_file() and p.name.lower() in ('license','license.txt','license.md','copying','notice')]
    report['open_gates'] += ['FFmpeg GPLv3 build: verify complete corresponding source/build/dependency notices, not only upstream commit URL.',
                             'Confirm ownership/redistribution terms for internal legacy-derived engines; no license texts discovered is not permission.',
                             'Fonts are referenced from the OS, not bundled; no Lemon Milk/font redistribution authorized by this audit.']
    report['primary_sources_consulted']=['https://ffmpeg.org/legal.html','https://www.gyan.dev/ffmpeg/builds/','https://github.com/FFmpeg/FFmpeg/commit/946fcce07b']
    report['package_count']=len(report['packages'])
    (out/'report.json').write_text(json.dumps(report,ensure_ascii=False,indent=2),encoding='utf-8')
    print('LICENSE EVIDENCE '+str(out)+'; packages='+str(report['package_count'])+'; public distribution still NOT approved.')


if __name__=='__main__':
    main()
