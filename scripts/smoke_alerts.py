"""Сквозная проверка критического алерта, прав, состояний и аудита."""
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
        'name': 'Пациент проверки алертов',
        'email': f'alert-api-{identity}@synapse.demo',
        'birth_date': '1990-01-01',
    }, expected=201)
    fixture = Path('tmp/alert-api-fixtures.json')
    fixture.parent.mkdir(exist_ok=True)
    previous = json.loads(fixture.read_text(encoding='utf-8')) if fixture.exists() else []
    fixture.write_text(json.dumps([*previous, identity]), encoding='utf-8')
    invitation = doctor.request(f'/api/v1/patients/{identity}/invitation', 'POST')
    patient.request('/api/v1/auth/activate', 'POST', {
        'code': invitation['code'],
        'password': 'Алерт2026!',
    })
    rules = doctor.request('/api/v1/clinical/alert-rules')
    assert rules['ruleset_version'] == 'championship-mvp-2026-10-27-r3'
    assert {rule['code'] for rule in rules['rules']} == {'gad7_severe_result_review', 'phq9_positive_item_9_review'}
    assert all(rule['priority'] == 'critical' for rule in rules['rules'])
    patient.request('/api/v1/clinical/alert-rules', expected=403)

    events = f'/api/v1/patients/{identity}/adverse-events'
    alerts = f'/api/v1/patients/{identity}/alerts'
    now = dt.datetime.now(dt.timezone.utc).isoformat()
    forbidden_report = {
        'id': str(uuid4()),
        'prescription_id': str(uuid4()),
        'occurred_at': now,
        'severity': 'moderate',
        'effect_items': ['Синтетическая реакция'],
        'note': '',
    }
    patient.request(events, 'POST', forbidden_report, expected=403)
    assert patient.request(alerts)['items'] == []

    definition = doctor.request('/api/v1/questionnaires/gad7')
    assignment = {'id': str(uuid4()), 'scale_version': definition['version'], 'note': 'Синтетическая проверка алерта'}
    questionnaire = f'/api/v1/patients/{identity}/questionnaires'
    doctor.request(questionnaire, 'POST', assignment, expected=201)
    patient.request(questionnaire + f"/{assignment['id']}/submit", 'POST', {
        'version': 1,
        'scale_version': definition['version'],
        'answers': [3, 3, 3, 3, 3, 0, 0],
    }, expected=200)
    patient_page = patient.request(alerts)
    assert len(patient_page['items']) == 1
    alert = patient_page['items'][0]
    assert alert['status'] == 'new' and alert['priority'] == 'critical'
    assert alert['rule_code'] == 'gad7_severe_result_review' and alert['detection_latency_ms'] >= 0
    assert 'не ставит диагноз' in alert['rule_snapshot']['patient_guidance']
    target = alerts + '/' + alert['id']
    patient.request(target + '/acknowledge', 'POST', {'version': 1, 'comment': ''}, expected=403)
    patient.request(target + '/changes', expected=403)
    outsider.request('/api/v1/alerts', expected=200)
    outsider.request(alerts, expected=403)
    doctor_page = doctor.request('/api/v1/alerts?status=new&limit=1')
    assert len(doctor_page['items']) == 1 and doctor_page['items'][0]['patient_id'] == identity
    notifications = doctor.request('/api/v1/notifications?limit=50')['items']
    assert any(item['kind'] == 'critical_alert' and item['related_id'] == alert['id'] for item in notifications)

    doctor.request(target + '/resolve', 'POST', {'version': 1, 'comment': 'Рано'}, expected=409)
    accepted = doctor.request(target + '/acknowledge', 'POST', {'version': 1, 'comment': ''})
    assert accepted['status'] == 'acknowledged' and accepted['version'] == 2
    assert doctor.request(target + '/acknowledge', 'POST', {'version': 1, 'comment': ''}) == accepted
    routed = doctor.request(target + '/routing', 'POST', {
        'version': 2,
        'category': 'preferred',
        'comment': 'Синтетическое решение о желательной консультации врача.',
    })
    assert routed['routing_category'] == 'preferred' and routed['version'] == 3
    assert patient.request(alerts)['items'][0]['routing_category'] == 'preferred'
    assert doctor.request(target + '/routing', 'POST', {
        'version': 2,
        'category': 'preferred',
        'comment': 'Синтетическое решение о желательной консультации врача.',
    }) == routed
    doctor.request(target + '/resolve', 'POST', {'version': 2, 'comment': 'Устаревшая версия'}, expected=409)
    resolved = doctor.request(target + '/resolve', 'POST', {
        'version': 3,
        'comment': 'Синтетическая обработка критического события.',
    })
    assert resolved['status'] == 'resolved' and resolved['version'] == 4
    assert patient.request(alerts)['items'][0]['resolution_comment'] == 'Синтетическая обработка критического события.'
    changes = doctor.request(target + '/changes')
    assert [item['action'] for item in changes] == ['resolved', 'routing_decided', 'acknowledged', 'detected']
    assert changes[-1]['author_name'] is None
    doctor.request('/api/v1/alerts?status=invalid', expected=422)
    doctor.request('/api/v1/alerts?before_id=' + str(uuid4()), expected=422)
    doctor.request(target + '/routing', 'POST', {'version': 4, 'category': 'unknown', 'comment': 'x'}, expected=422)
    print('Алерты GAD-7: правило, права, уведомление, состояния, повтор и аудит пройдены; самоотчёт реакций запрещён.')


if __name__ == '__main__':
    main()
