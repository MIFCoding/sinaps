"""Клиническая выборка, приватность справочника, версии, аудит и идемпотентность."""
import concurrent.futures
import datetime as dt
import json
from pathlib import Path
from urllib.parse import urlencode
from uuid import uuid4
from smoke_api import Client


def main():
    doctor, outsider, patient = Client(), Client(), Client()
    doctor.login('doctor@synapse.demo')
    outsider.login('other-doctor@synapse.demo')
    identity=str(uuid4())
    doctor.request('/api/v1/patients','POST',{'id':identity,'name':'Пациент клинической проверки','email':f'clinical-api-{identity}@synapse.demo','birth_date':'1990-01-01'},expected=201)
    fixture=Path('tmp/clinical-api-fixtures.json')
    fixture.parent.mkdir(exist_ok=True)
    previous=json.loads(fixture.read_text()) if fixture.exists() else []
    fixture.write_text(json.dumps([*previous,identity]))
    invitation=doctor.request(f'/api/v1/patients/{identity}/invitation','POST')
    patient.request('/api/v1/auth/activate','POST',{'code':invitation['code'],'password':'Клиника2026!'})
    for suffix in ['/clinical/findings','/clinical/diagnosis-catalog','/clinical/source/F41.1','/clinical/drugs']:
        patient.request('/api/v1'+suffix,expected=403)
        Client().request('/api/v1'+suffix,expected=401)
    all_findings=[]
    offset=0
    while True:
        page=doctor.request('/api/v1/clinical/findings?'+urlencode({'limit':50,'offset':offset}))
        all_findings.extend(page['items'])
        if page['next_offset'] is None: break
        offset=page['next_offset']
    assert len(all_findings)==len({row['id'] for row in all_findings})==721
    gad=[row for row in all_findings if row['source_code']=='F41.1']
    assert len(gad)==22 and all(row['source_page']==54 for row in gad)
    source=doctor.request('/api/v1/clinical/source/F41.1')
    assert 'шесть месяцев' in source['text']
    drugs=doctor.request('/api/v1/clinical/drugs')
    drug=next(item['spec'] for item in drugs if item['drug_code']=='aviandr')
    assert drug['source_date']=='2026-07-16' and 'Головная боль' in drug['effects'][0]['items']
    base=f'/api/v1/patients/{identity}/diagnoses'
    assert patient.request(base)['items']==[]
    input={'id':str(uuid4()),'version':1,'catalog_version':page['version'],'code':'F41.1','finding_ids':[gad[0]['id'],gad[1]['id']],'diagnosed_on':dt.date.today().isoformat(),'status':'active','note':'Синтетическая клиническая запись'}
    row=doctor.request(base,'POST',input,expected=201)
    repeated=doctor.request(base,'POST',input)
    assert row==repeated and len(row['findings'])==2
    target=base+'/'+row['id']
    snapshot=patient.request(target)
    assert len(snapshot['findings'])==2 and all(set(item)=={'id','label','source_code','source_page'} for item in snapshot['findings'])
    assert not any(item['label'] in json.dumps(snapshot,ensure_ascii=False) for item in gad[2:])
    assert 'context' not in json.dumps(snapshot) and 'source_sha256' not in json.dumps(snapshot)
    outsider.request(base,expected=403)
    patient.request(target,'PATCH',input,expected=403)
    patient.request(target+'/changes',expected=403)
    outsider.request(target+'/changes',expected=403)
    wrong='/api/v1/patients/00000000-0000-0000-0000-000000000002/diagnoses/'+row['id']
    doctor.request(wrong,expected=404)
    doctor.request(wrong+'/changes',expected=404)
    # Неизменённые данные не добавляют версию и аудит.
    assert doctor.request(target,'PATCH',input)['version']==1
    changed=doctor.request(target,'PATCH',input|{'note':'Новая версия','finding_ids':[gad[0]['id']]})
    assert changed['version']==2
    doctor.request(target,'PATCH',input,expected=409)
    assert len(patient.request(target)['findings'])==1
    audit=doctor.request(target+'/changes')
    assert [item['version'] for item in audit['items']]==[2,1]
    assert len(audit['items'][1]['snapshot']['findings'])==2
    for bad in [{'finding_ids':['неизвестно']},{'finding_ids':[]},{'finding_ids':[gad[0]['id']]*2},{'code':'F1x.0'},{'code':'BAD'},{'catalog_version':'old'},{'diagnosed_on':'1980-01-01'},{'diagnosed_on':(dt.date.today()+dt.timedelta(days=1)).isoformat()}]:
        doctor.request(base,'POST',input|{'id':str(uuid4())}|bad,expected=409 if 'catalog_version' in bad else 422)
    # Одновременный повтор UUID создаёт одну запись и одну версию аудита.
    simultaneous=input|{'id':str(uuid4())}
    def create(_):
        try:return doctor.request(base,'POST',simultaneous,expected=201)
        except AssertionError:return doctor.request(base,'POST',simultaneous)
    with concurrent.futures.ThreadPoolExecutor(max_workers=2) as pool:
        results=list(pool.map(create,range(2)))
    assert len({item['id'] for item in results})==1
    assert len(doctor.request(base+'/'+simultaneous['id']+'/changes')['items'])==1
    # Все страницы доступны без повторов, независимо от изменения статуса.
    first=doctor.request(base+'?limit=1')
    second=doctor.request(base+'?'+urlencode({'limit':1,**first['next_cursor']}))
    assert first['items'][0]['id']!=second['items'][0]['id'] and second['next_cursor'] is None
    for query in ['limit=0','limit=51','before_id='+str(uuid4())]:doctor.request(base+'?'+query,expected=422)
    print('ПРОЙДЕНО: полный каталог, 22 пункта F41.1, выбранные сведения пациента, права, даты, версии, аудит и конкурентный повтор')


if __name__=='__main__':main()
