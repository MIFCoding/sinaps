"""Сквозная проверка GAD-7: права, контрольные баллы, версии и конкурентные действия."""
import concurrent.futures
import json
from pathlib import Path
import uuid
from smoke_api import Client


def main():
    doctor, outsider, patient = Client(), Client(), Client()
    doctor.login('doctor@synapse.demo')
    outsider.login('other-doctor@synapse.demo')
    fixture = Path('tmp/questionnaire-api-fixtures.json')
    def create(birth='1990-01-01'):
        identity = str(uuid.uuid4())
        doctor.request('/api/v1/patients', 'POST', {'id':identity,'name':'Опросник API','email':f'questionnaire-api-{identity}@synapse.demo','birth_date':birth},expected=201)
        fixture.parent.mkdir(exist_ok=True)
        previous=json.loads(fixture.read_text(encoding='utf-8')) if fixture.exists() else []
        fixture.write_text(json.dumps([*previous,identity]),encoding='utf-8')
        return identity
    identity, other, child = create(), create(), create('2012-01-01')
    path=f'/api/v1/patients/{identity}/questionnaires'
    token=doctor.request(f'/api/v1/patients/{identity}/invitation','POST')['code']
    patient.request('/api/v1/auth/activate','POST',{'code':token,'password':'Опросник2026!'})
    definition=doctor.request('/api/v1/questionnaires/gad7')
    assert len(definition['questions'])==7 and len(definition['options'])==4
    assert definition['minimum_age']==18 and definition['license'].startswith('CC BY-NC')
    patient.request('/api/v1/questionnaires/gad7',expected=403)
    Client().request('/api/v1/questionnaires/gad7',expected=401)
    def assignment():return {'id':str(uuid.uuid4()),'scale_version':definition['version'],'note':'Синтетическое назначение'}
    body=assignment()
    child_path=f'/api/v1/patients/{child}/questionnaires'
    doctor.request(child_path,'POST',body,expected=422)
    patient.request(path,'POST',body,expected=403)
    outsider.request(path,'POST',body,expected=403)
    created=doctor.request(path,'POST',body,expected=201)
    record_path=path+'/'+body['id']
    assert doctor.request(path,'POST',body)['version']==1
    doctor.request(path,'POST',assignment(),expected=409)
    assert patient.request(record_path)['definition']==definition
    doctor.request(f'/api/v1/patients/{other}/questionnaires/'+body['id'],expected=404)
    outsider.request(record_path,expected=403)
    patient.request(record_path+'/changes',expected=403)
    assert len(doctor.request(record_path+'/changes'))==1
    submission={'version':1,'scale_version':definition['version'],'answers':[2]*7}
    doctor.request(record_path+'/submit','POST',submission,expected=403)
    for answers in [[],[0]*6,[0]*8,[0,0,0,0,0,0,4],[0,0,0,0,0,0,-1],[0,0,0,0,0,0,None],[0,0,0,0,0,0,1.5]]:
        patient.request(record_path+'/submit','POST',{**submission,'answers':answers},expected=422)
    assert patient.request(record_path)['score'] is None
    def cloned(source):
        client=Client()
        for cookie in source.cookies:client.cookies.set_cookie(cookie)
        return client
    # Одинаковый повтор должен вернуть тот же результат и создать лишь одно завершение.
    with concurrent.futures.ThreadPoolExecutor(max_workers=2) as pool:
        values=list(pool.map(lambda _:cloned(patient).request(record_path+'/submit','POST',{**submission,'score':0}),range(2)))
    assert all(row['score']==14 and row['band']=='moderate' and row['version']==2 for row in values)
    assert len(doctor.request(record_path+'/changes'))==2
    assert doctor.request(path,'POST',body)['status']=='completed'
    patient.request(record_path+'/submit','POST',{**submission,'answers':[0]*7},expected=409)
    patient.request(record_path+'/submit','POST',{**submission,'version':2147483647},expected=409)
    doctor.request(record_path+'/cancel','POST',{'version':2},expected=409)
    for total,band in [(0,'minimal'),(4,'minimal'),(5,'mild'),(9,'mild'),(10,'moderate'),(14,'moderate'),(15,'severe'),(21,'severe')]:
        request=assignment();doctor.request(path,'POST',request,expected=201)
        left=total;answers=[]
        for _ in range(7):value=min(3,left);answers.append(value);left-=value
        result=patient.request(path+'/'+request['id']+'/submit','POST',{**submission,'answers':answers})
        assert (result['score'],result['band'])==(total,band)
        if total == 15:
            alerts = patient.request(f'/api/v1/patients/{identity}/alerts')['items']
            matching = [item for item in alerts if item['source_id'] == request['id']]
            assert len(matching) == 1
            assert matching[0]['rule_code'] == 'gad7_severe_result_review'
            assert matching[0]['status'] == 'new'
            assert 'не ставит диагноз' in matching[0]['rule_snapshot']['patient_guidance']
            notifications = doctor.request('/api/v1/notifications?limit=50')['items']
            assert any(
                item['kind'] == 'critical_alert' and item['related_id'] == matching[0]['id']
                for item in notifications
            )
    cursor='';seen=[]
    while True:
        page=patient.request(path+'?limit=3&status=completed'+cursor)
        seen.extend(row['id'] for row in page['items'])
        if not page['next_cursor']:break
        point=page['next_cursor'];cursor=f"&before={point['before']}&before_id={point['before_id']}"
    assert len(seen)==len(set(seen))==9
    for query in ['limit=0','status=invalid','before_id='+str(uuid.uuid4())]:doctor.request(path+'?'+query,expected=422)
    request=assignment();doctor.request(path,'POST',request,expected=201);cancel_path=path+'/'+request['id']
    patient.request(cancel_path+'/cancel','POST',{'version':1},expected=403)
    assert doctor.request(cancel_path+'/cancel','POST',{'version':1})['status']=='cancelled'
    assert doctor.request(cancel_path+'/cancel','POST',{'version':1})['version']==2
    patient.request(cancel_path+'/submit','POST',submission,expected=409)
    assert len(doctor.request(cancel_path+'/changes'))==2
    # Отмена и отправка одной версии сериализуются: одно действие принимается, второе получает конфликт.
    request=assignment();doctor.request(path,'POST',request,expected=201);race_path=path+'/'+request['id']
    def race(action):
        client=cloned(patient if action=='submit' else doctor)
        try:client.request(race_path+'/'+action,'POST',submission if action=='submit' else {'version':1});return 200
        except AssertionError as error:
            if 'got 409' in str(error):return 409
            raise
    with concurrent.futures.ThreadPoolExecutor(max_workers=2) as pool:assert sorted(pool.map(race,['submit','cancel']))==[200,409]
    assert len(doctor.request(race_path+'/changes'))==2
    assert created['definition']['source_sha256']==definition['source_sha256']
    print('Опросники: контрольные баллы, полнота, права, неизменность ответов, отмена, конкуренция, курсор и аудит пройдены.')


if __name__=='__main__':main()
