"""Воспроизводимый импорт переданного командой PDF без вычисления диагнозов."""
import argparse
import hashlib
import json
import re
import subprocess
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
HEADER = re.compile(r'^\s*(F[0-9xхХX]{2}(?:\.[0-9xхХX]{1,3})?(?:-F\d{2})?)\*?\s+(.+)')
POINT = re.compile(r'^\s*(\(?\d{1,2}[.)]|[а-яa-z][)])\s+(.+)', re.I)
CRITERION = re.compile(r'^\s*((?:G\d+|[А-ЯA-Z])[.)])\s+(.+)')


def clean(value):
    return re.sub(r'\s+', ' ', value).strip()


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('pdf', type=Path)
    args = parser.parse_args()
    raw = args.pdf.read_bytes()
    digest = hashlib.sha256(raw).hexdigest()
    temporary = ROOT / 'tmp/clinical-source/import.txt'
    temporary.parent.mkdir(parents=True, exist_ok=True)
    subprocess.run(['pdftotext', '-layout', str(args.pdf), str(temporary)], check=True)
    pages = temporary.read_text(encoding='utf-8').split('\f')
    assert len([p for p in pages if p.strip()]) == 96, 'Проверьте версию и структуру PDF'
    lines = [{'text':line,'page':page_index+1} for page_index,page in enumerate(pages) for line in page.splitlines()]
    beginnings = [i for i,line in enumerate(lines) if (match := HEADER.match(line['text'])) and match[1]=='F00-F09']
    assert len(beginnings) >= 2, 'Не найдены границы перечня и основного текста'
    start, body_start = [i for i in beginnings if lines[i]['page'] >= 4][:2]
    diagnoses = {}
    current = None
    for row in lines[start:body_start]:
            line, page_index = row['text'], row['page']-1
            if match := HEADER.match(line):
                code, title = match.groups()
                current = code
                diagnoses[code] = {'code': code, 'title': clean(title).rstrip('.'), 'source_page': page_index+1, 'recordable': bool(re.fullmatch(r'F\d{2}(?:\.\d{1,3})?',code))}
            elif current and (short := re.match(r'^\s*\.(\d{1,3})\s+(.+)',line)):
                code = current.split('.')[0]+'.'+short[1]
                diagnoses[code] = {'code':code,'title':clean(short[2]),'source_page':page_index+1,'recordable':bool(re.fullmatch(r'F\d{2}(?:\.\d{1,3})?',code))}
                current = code
            elif current and line.strip() and not re.fullmatch(r'\s*\d+\.\s*',line):
                diagnoses[current]['title'] += ' ' + clean(line)
    assert 'F41.1' in diagnoses and 'F99' in diagnoses
    for code, item in diagnoses.items():
        if '.' in code and len(code.split('.')[1]) > 1:
            parent = code[:-1]
            if parent in diagnoses and not item['title'].lower().startswith(diagnoses[parent]['title'].lower()):
                item['title'] = diagnoses[parent]['title'] + ' — ' + item['title']
    sections = {}
    current = None
    for row in lines[body_start:]:
            line, page_index = row['text'], row['page']-1
            if re.fullmatch(r'\s*\d+\.\s*',line) or not line.strip():
                continue
            match = HEADER.match(line)
            if match and match[1] in diagnoses:
                # Ссылки внутри предложения не принимаются за заголовки разделов.
                rest = match[2].lower()
                if not re.match(r'(или\b|и\b|но\b|в\b|как\b|при\b|соответ|не\b|долж)',rest):
                    current = match[1]
                    sections.setdefault(current, {'code':current, 'source_page':page_index+1, 'lines':[]})
                    continue
            if current:
                sections[current]['lines'].append({'text':line.strip(), 'page':page_index+1})
    findings = {}
    for code, section in sections.items():
        active = None
        context = ''
        def flush():
            if active and len(clean(active['label'])) >= 5:
                label = clean(active['label']).rstrip(';')
                key = hashlib.sha256((code+'|'+context+'|'+label).encode()).hexdigest()[:20]
                findings[key] = {'id':key, 'label':label, 'context':context, 'source_code':code, 'source_page':active['page']}
        for line in section['lines']:
            text = line['text']
            if criterion := CRITERION.match(text):
                flush()
                active = None
                context = clean(text)
            elif point := POINT.match(text):
                flush()
                active = {'label':point[2], 'page':line['page']}
            elif active:
                # Подзаголовок после завершённого пункта относится к следующей группе.
                if active['label'].endswith((';','.')) or text.startswith(('Примечание','Симптомы','Общие симптомы','Другие неспецифические')):
                    flush()
                    active = None
                    context = clean(text)
                else:
                    active['label'] += ' ' + text
            elif context:
                context += ' ' + text
        flush()
        section['text'] = '\n'.join(line['text'] for line in section.pop('lines'))
    assert len(findings) > 300, 'Проверьте разбор пунктов справочника'
    gad = [x for x in findings.values() if x['source_code']=='F41.1']
    assert len(gad) == 22, len(gad)
    payload = {'version':'icd10ps-'+digest[:12]+'-r2', 'source_title':'МКБ-10. Исследовательские диагностические критерии. Материал команды', 'source_sha256':digest, 'source_pages':96, 'reference_url':'https://www.who.int/publications/i/item/9241544554', 'diagnoses':list(diagnoses.values()), 'findings':list(findings.values()), 'sections':list(sections.values())}
    destination = ROOT / 'clinical/catalog.json'
    destination.parent.mkdir(exist_ok=True)
    destination.write_text(json.dumps(payload,ensure_ascii=False,indent=2)+'\n',encoding='utf-8')
    print(json.dumps({'diagnoses':len(diagnoses),'recordable':sum(x['recordable'] for x in diagnoses.values()),'findings':len(findings),'sections':len(sections),'gad_findings':len(gad)},ensure_ascii=False))


if __name__ == '__main__':
    main()
