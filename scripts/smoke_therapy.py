"""Сквозная проверка назначений, приверженности и нежелательных явлений."""
import datetime as dt
import json
from pathlib import Path
from uuid import uuid4

from smoke_api import Client


def main():
    doctor, outsider, patient = Client(), Client(), Client()
    doctor.login('doctor@synapse.demo')
    outsider.login('other-doctor@synapse.demo')
    identity = str(uuid4())
    doctor.request('/api/v1/patients', 'POST', {
        'id': identity,
        'name': 'Пациент проверки терапии',
        'email': f'therapy-api-{identity}@synapse.demo',
        'birth_date': '1990-01-01',
    }, expected=201)
    fixture = Path('tmp/therapy-api-fixtures.json')
    fixture.parent.mkdir(exist_ok=True)
    previous = json.loads(fixture.read_text(encoding='utf-8')) if fixture.exists() else []
    fixture.write_text(json.dumps([*previous, identity]), encoding='utf-8')
    invitation = doctor.request(f'/api/v1/patients/{identity}/invitation', 'POST')
    patient.request('/api/v1/auth/activate', 'POST', {
        'code': invitation['code'],
        'password': 'Терапия2026!',
    })
    diagnosis_catalog = doctor.request('/api/v1/clinical/diagnosis-catalog?search=F41.1')
    findings_catalog = doctor.request('/api/v1/clinical/findings?search=F41.1')
    diagnosis = {
        'id': str(uuid4()),
        'version': 1,
        'catalog_version': diagnosis_catalog['version'],
        'code': 'F41.1',
        'finding_ids': [findings_catalog['items'][0]['id']],
        'diagnosed_on': dt.date.today().isoformat(),
        'status': 'active',
        'note': 'Синтетический активный диагноз для проверки связи назначения.',
    }
    saved_diagnosis = doctor.request(f'/api/v1/patients/{identity}/diagnoses', 'POST', diagnosis, expected=201)
    references = doctor.request('/api/v1/clinical/drugs')
    selected = next((item for item in references if item['drug_code'] != 'aviandr'), None)
    if selected is None:
        selected = next(item for item in references if item['drug_code'] == 'aviandr')
    drug = selected['spec']
    base = f'/api/v1/patients/{identity}/prescriptions'
    today = dt.date.today().isoformat()
    body = {
        'id': str(uuid4()),
        'version': 1,
        'diagnosis_id': saved_diagnosis['id'],
        'drug_code': selected['drug_code'],
        'source_sha256': drug['source_sha256'],
        'dosage': 'Синтетическая схема для проверки',
        'intake_times': ['09:00', '21:00'],
        'starts_on': today,
        'ends_on': None,
        'instructions': 'Только демонстрационные данные.',
    }
    patient.request(base, 'POST', body, expected=403)
    outsider.request(base, expected=403)
    draft = doctor.request(base, 'POST', body, expected=201)
    assert draft['status'] == 'draft' and draft['version'] == 1
    assert doctor.request(base, 'POST', body) == draft
    assert patient.request(base) == []
    doctor.request(base, 'POST', body | {'id': str(uuid4())}, expected=409)
    target = f'{base}/{draft["id"]}'
    changed = doctor.request(target, 'PATCH', body | {
        'dosage': 'Изменённая синтетическая схема',
    })
    assert changed['version'] == 2
    doctor.request(target, 'PATCH', body, expected=409)
    active = doctor.request(target + '/publish', 'POST', {'version': 2})
    assert active['status'] == 'active' and active['published_at']
    assert doctor.request(target + '/publish', 'POST', {'version': 2}) == active
    visible = patient.request(base)
    assert len(visible) == 1 and visible[0]['dosage'] == 'Изменённая синтетическая схема'

    mark_path = target + '/marks'
    first_mark = {
        'id': str(uuid4()),
        'scheduled_on': today,
        'scheduled_time': '09:00',
        'status': 'taken',
        'reason': '',
    }
    doctor.request(mark_path, 'POST', first_mark, expected=403)
    assert patient.request(mark_path, 'POST', first_mark, expected=201)['status'] == 'taken'
    assert patient.request(mark_path, 'POST', first_mark)['id'] == first_mark['id']
    patient.request(mark_path, 'POST', first_mark | {'id': str(uuid4()), 'status': 'skipped'}, expected=409)
    patient.request(mark_path, 'POST', first_mark | {'id': str(uuid4()), 'scheduled_time': '10:00'}, expected=422)
    second_mark = first_mark | {
        'id': str(uuid4()),
        'scheduled_time': '21:00',
        'status': 'skipped',
        'reason': 'Синтетическая причина',
    }
    patient.request(mark_path, 'POST', second_mark, expected=201)
    summary = doctor.request(target + f'/adherence?from={today}&to={today}')
    assert (summary['expected'], summary['taken'], summary['skipped'], summary['unmarked'], summary['percent']) == (2, 1, 1, 0, 50.0)
    doctor.request(target, 'PATCH', body | {
        'version': 3,
        'dosage': 'Изменённая синтетическая схема',
        'intake_times': ['10:00'],
    }, expected=409)

    adverse_base = f'/api/v1/patients/{identity}/adverse-events'
    effect = drug['effects'][0]['items'][0]
    event = {
        'id': str(uuid4()),
        'prescription_id': draft['id'],
        'occurred_at': dt.datetime.now(dt.timezone.utc).isoformat(),
        'severity': 'moderate',
        'effect_items': [effect],
        'note': 'Синтетическая запись врача после опроса.',
    }
    patient.request(adverse_base, 'POST', event, expected=403)
    outsider.request(adverse_base, 'POST', event, expected=403)
    doctor.request(adverse_base, 'POST', event | {'id': str(uuid4()), 'effect_items': ['Неизвестная реакция']}, expected=422)
    identified = doctor.request(adverse_base, 'POST', event, expected=201)
    assert identified['status'] == 'reviewed' and identified['effect_items'] == [effect]
    assert identified['author_name'] == 'Иван Сергеевич'
    assert doctor.request(adverse_base, 'POST', event)['id'] == event['id']
    assert patient.request(adverse_base)[0]['effect_items'] == [effect]
    outsider.request(adverse_base, expected=403)
    notifications = doctor.request('/api/v1/notifications?limit=50')
    assert not any(item['kind'] == 'adverse_event' and item['related_id'] == event['id'] for item in notifications['items'])

    cancelled = doctor.request(target + '/cancel', 'POST', {'version': 3})
    assert cancelled['status'] == 'cancelled' and cancelled['version'] == 4
    patient.request(mark_path, 'POST', first_mark | {
        'id': str(uuid4()),
        'scheduled_time': '21:00',
    }, expected=409)
    changes = doctor.request(target + '/changes')
    assert [item['action'] for item in changes] == ['cancelled', 'published', 'updated', 'created']
    patient.request(target + '/changes', expected=403)
    print('Терапия: черновик, публикация, права, приверженность и врачебная фиксация реакций по справке пройдены.')


if __name__ == '__main__':
    main()
