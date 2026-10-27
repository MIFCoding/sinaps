"""Проверка администратора на отдельной временной базе Docker Compose."""

import http.cookiejar
import datetime
import json
import os
from pathlib import Path
import socket
import subprocess
import sys
import tempfile
import time
import urllib.error
import urllib.request
import uuid

ROOT = Path(__file__).resolve().parent.parent
EXE = ROOT / "target" / "debug" / "synapse-api.exe"
database = "synapse_admin_check_" + uuid.uuid4().hex[:12]
docker = ["docker", "compose", "exec", "-T", "db"]
environment = os.environ.copy()
environment.update({
    "DATABASE_URL": f"postgres://synapse:synapse_local@localhost:55432/{database}",
    "SYNAPSE_RUNTIME_MODE": "demo",
    "SYNAPSE_MAIL_TRANSPORT": "disabled",
    "SYNAPSE_PUSH_TRANSPORT": "disabled",
    "SYNAPSE_ADMIN_EMAIL": "admin@example.test",
    "SYNAPSE_ADMIN_PASSWORD": "TemporaryAdmin2026!",
    "SYNAPSE_DEMO_SEED": "true",
    "COOKIE_SECURE": "false",
})
with socket.socket() as sock:
    sock.bind(("127.0.0.1", 0))
    port = sock.getsockname()[1]
environment["BIND_ADDR"] = f"127.0.0.1:{port}"
base = f"http://127.0.0.1:{port}"
process = None
created = False


def run_command(*args, expect_success=True):
    result = subprocess.run(args, cwd=ROOT, env=environment, capture_output=True, text=True)
    if (result.returncode == 0) != expect_success:
        raise AssertionError(f"Команда завершилась неожиданно: {args[1:]}; {result.stderr}")
    return result


def client():
    return urllib.request.build_opener(urllib.request.HTTPCookieProcessor(http.cookiejar.CookieJar()))


def request(opener, method, path, body=None):
    headers = {"X-Synapse-Request": "1"}
    data = None
    if body is not None:
        headers["Content-Type"] = "application/json"
        data = json.dumps(body, ensure_ascii=False).encode("utf-8")
    req = urllib.request.Request(base + path, data=data, headers=headers, method=method)
    try:
        with opener.open(req, timeout=5) as response:
            return response.status, json.loads(response.read() or b"null")
    except urllib.error.HTTPError as error:
        return error.code, json.loads(error.read() or b"null")


try:
    run_command(*docker, "createdb", "-U", "synapse", database)
    created = True
    run_command(str(EXE), "create-admin")
    run_command(str(EXE), "create-admin", expect_success=False)
    run_command(str(EXE), "seed")
    process = subprocess.Popen([str(EXE)], cwd=ROOT, env=environment, stdout=subprocess.DEVNULL,
                               stderr=subprocess.DEVNULL, creationflags=getattr(subprocess, "CREATE_NO_WINDOW", 0))
    anonymous, admin, doctor, patient, other_doctor, other_patient = client(), client(), client(), client(), client(), client()
    for _ in range(40):
        try:
            if request(anonymous, "GET", "/health")[0] == 200:
                break
        except (OSError, urllib.error.URLError):
            time.sleep(0.25)
    else:
        raise AssertionError("Временный API не запустился")

    def login(opener, email, password, role):
        status, user = request(opener, "POST", "/api/v1/auth/login", {"email": email, "password": password})
        assert status == 200 and user["role"] == role, (status, user)

    login(admin, "admin@example.test", "TemporaryAdmin2026!", "admin")
    login(doctor, "doctor@synapse.demo", "Synapse2026!", "doctor")
    login(patient, "patient@synapse.demo", "Synapse2026!", "patient")
    login(other_doctor, "other-doctor@synapse.demo", "Synapse2026!", "doctor")
    login(other_patient, "other-patient@synapse.demo", "Synapse2026!", "patient")
    assert request(anonymous, "GET", "/api/v1/admin/catalog-drafts")[0] == 401
    assert request(doctor, "GET", "/api/v1/admin/catalog-drafts")[0] == 403
    assert request(patient, "GET", "/api/v1/admin/catalog-drafts")[0] == 403
    assert request(admin, "GET", "/api/v1/patients")[0] == 403
    assert request(admin, "GET", "/api/v1/patients/00000000-0000-0000-0000-000000000002")[0] == 403
    assert request(admin, "POST", "/api/v1/appointments/00000000-0000-0000-0000-000000000002/cancel", {"expected_version": 1})[0] == 403
    assert request(doctor, "GET", "/api/v1/patients")[0] == 200
    draft = {"kind": "disease", "title": "Черновик", "source": "Источник", "details": "Описание"}
    status, saved = request(admin, "POST", "/api/v1/admin/catalog-drafts", draft)
    assert status == 201 and saved["version"] == 1, (status, saved)
    path = "/api/v1/admin/catalog-drafts/" + saved["id"]
    changed = {**draft, "title": "Черновик 2", "expected_version": 1}
    status, saved = request(admin, "PATCH", path, changed)
    assert status == 200 and saved["version"] == 2, (status, saved)
    assert request(admin, "PATCH", path, changed)[0] == 409
    status, rows = request(admin, "GET", "/api/v1/admin/catalog-drafts")
    assert status == 200 and len(rows) == 1 and rows[0]["title"] == "Черновик 2"
    count = run_command(*docker, "psql", "-U", "synapse", "-d", database, "-Atc", "SELECT count(*) FROM catalog_draft_changes").stdout.strip()
    assert count == "2", count
    item = {"kind": "questionnaire", "title": "Тестовая форма", "source": "Синтетический источник", "details": ""}
    status, questionnaire = request(admin, "POST", "/api/v1/admin/catalog-drafts", item)
    assert status == 201
    form_path = f"/api/v1/admin/catalog-drafts/{questionnaire['id']}/questionnaire"
    spec = {"instruction": "Инструкция", "minimum_age": 18, "license": "Для проверки", "limitations": "Тестовая форма", "source_url": None, "scoring_url": None, "source_sha256": None, "attribution": None, "license_url": None, "checked_on": None, "questions": [
        {"id": str(uuid.uuid4()), "text": "Вопрос 1", "options": [{"label": "Нет", "weight": 0}, {"label": "Да", "weight": 20}]},
        {"id": str(uuid.uuid4()), "text": "Вопрос 2", "options": [{"label": "Редко", "weight": 1}, {"label": "Часто", "weight": 3}]}],
        "bands": [{"min": 1, "max": 2, "label": "Первый"}, {"min": 3, "max": 23, "label": "Второй"}]}
    assert request(doctor, "GET", form_path)[0] == 403
    invalid = {**spec, "bands": [{"min": 1, "max": 2, "label": "Неполный"}]}
    assert request(admin, "PATCH", form_path, {"expected_version": 1, "spec": invalid})[0] == 422
    status, response = request(admin, "PATCH", form_path, {"expected_version": 1, "spec": spec})
    assert status == 200 and response["version"] == 2 and response["spec"] == spec, (status, response)
    assert request(admin, "PATCH", form_path, {"expected_version": 1, "spec": spec})[0] == 409
    assert request(admin, "GET", form_path)[1]["spec"] == spec
    assert request(admin, "PATCH", f"/api/v1/admin/catalog-drafts/{questionnaire['id']}", {**item, "kind": "disease", "expected_version": 2})[0] == 422
    count = run_command(*docker, "psql", "-U", "synapse", "-d", database, "-Atc", "SELECT count(*) FROM catalog_draft_changes WHERE snapshot->'questionnaire'->>'instruction' = 'Инструкция'").stdout.strip()
    assert count == "1", count
    publish_path = f"/api/v1/admin/catalog-drafts/{questionnaire['id']}/publish-questionnaire"
    assert request(doctor, "POST", publish_path, {"expected_version": 2, "reviewed": True})[0] == 403
    assert request(admin, "POST", publish_path, {"expected_version": 2, "reviewed": False})[0] == 422
    status, form = request(admin, "POST", publish_path, {"expected_version": 2, "reviewed": True})
    assert status == 201 and form["spec"] == spec and form["active"], (status, form)
    assert request(admin, "POST", publish_path, {"expected_version": 2, "reviewed": True})[0] == 409
    assert request(patient, "GET", "/api/v1/questionnaires/catalog")[0] == 403
    initial_catalog = request(doctor, "GET", "/api/v1/questionnaires/catalog")[1]
    assert any(row["id"] == form["id"] for row in initial_catalog)
    builtin_phq9 = next(row for row in initial_catalog if row["title"].endswith("PHQ‑9"))
    assert len(builtin_phq9["spec"]["questions"]) == 9
    patient_id = "00000000-0000-0000-0000-000000000002"
    assignments = f"/api/v1/patients/{patient_id}/custom-questionnaires"
    new_assignment = {"id": str(uuid.uuid4()), "form_id": form["id"], "note": "Проверка"}
    assert request(patient, "POST", assignments, new_assignment)[0] == 403
    assert request(other_doctor, "POST", assignments, new_assignment)[0] == 403
    assert request(other_patient, "GET", assignments)[0] == 403
    run_command(*docker, "psql", "-U", "synapse", "-d", database, "-c", "UPDATE patient_profiles SET birth_date=current_date - interval '10 years' WHERE user_id='00000000-0000-0000-0000-000000000003'")
    assert request(doctor, "POST", "/api/v1/patients/00000000-0000-0000-0000-000000000003/custom-questionnaires", {**new_assignment, "id": str(uuid.uuid4())})[0] == 422
    status, assigned = request(doctor, "POST", assignments, new_assignment)
    assert status == 201 and assigned["spec"] == spec and assigned["status"] == "assigned", (status, assigned)
    assert request(doctor, "POST", assignments, new_assignment)[0] == 200
    assert request(doctor, "POST", assignments, {**new_assignment, "id": str(uuid.uuid4())})[0] == 409
    altered = {**spec, "instruction": "Новая инструкция"}
    status, updated = request(admin, "PATCH", form_path, {"expected_version": 2, "spec": altered})
    assert status == 200 and updated["version"] == 3
    status, latest = request(admin, "POST", publish_path, {"expected_version": 3, "reviewed": True})
    assert status == 201 and latest["id"] != form["id"]
    assert request(patient, "GET", assignments)[1]["items"][0]["spec"] == spec
    submit_path = f"{assignments}/{assigned['id']}/submit"
    assert request(doctor, "POST", submit_path, {"version": 1, "answers": [1, 1]})[0] == 403
    assert request(patient, "POST", submit_path, {"version": 1, "answers": [9, 1]})[0] == 422
    assert request(patient, "POST", submit_path, {"version": 1, "answers": [1]})[0] == 422
    status, result = request(patient, "POST", submit_path, {"version": 1, "answers": [1, 1]})
    assert status == 200 and result["score"] == 23 and result["band"] == "Второй" and result["spec"] == spec, (status, result)
    assert request(patient, "POST", submit_path, {"version": 1, "answers": [1, 1]})[0] == 200
    assert request(patient, "POST", submit_path, {"version": 1, "answers": [0, 1]})[0] == 409
    assert request(patient, "GET", f"{assignments}/{assigned['id']}/changes")[0] == 403
    assert len(request(doctor, "GET", f"{assignments}/{assigned['id']}/changes")[1]) == 2
    assert request(doctor, "POST", assignments, {**new_assignment, "id": str(uuid.uuid4())})[0] == 409
    status, cancelled = request(doctor, "POST", assignments, {**new_assignment, "id": str(uuid.uuid4()), "form_id": latest["id"]})
    assert status == 201
    cancel_path = f"{assignments}/{cancelled['id']}/cancel"
    assert request(patient, "POST", cancel_path, {"version": 1})[0] == 403
    assert request(doctor, "POST", cancel_path, {"version": 1})[1]["status"] == "cancelled"
    assert request(patient, "POST", f"{assignments}/{cancelled['id']}/submit", {"version": 1, "answers": [1, 1]})[0] == 409
    assert request(admin, "POST", f"/api/v1/admin/questionnaires/{latest['id']}/archive")[0] == 200
    assert [row["id"] for row in request(doctor, "GET", "/api/v1/questionnaires/catalog")[1]] == [builtin_phq9["id"]]
    assert any(row["score"] == 23 for row in request(patient, "GET", assignments)[1]["items"])
    audit_count = run_command(*docker, "psql", "-U", "synapse", "-d", database, "-Atc", "SELECT count(*) FROM custom_questionnaire_changes").stdout.strip()
    assert audit_count == "4", audit_count
    alert_count = run_command(*docker, "psql", "-U", "synapse", "-d", database, "-Atc", "SELECT count(*) FROM alerts").stdout.strip()
    assert alert_count == "0", alert_count
    phq_assignment = {"id": str(uuid.uuid4()), "form_id": builtin_phq9["id"], "note": "Проверка PHQ‑9"}
    status, phq_assigned = request(doctor, "POST", assignments, phq_assignment)
    assert status == 201 and phq_assigned["status"] == "assigned"
    phq_submit = f"{assignments}/{phq_assigned['id']}/submit"
    status, phq_result = request(patient, "POST", phq_submit, {"version": 1, "answers": [0, 0, 0, 0, 0, 0, 0, 0, 1]})
    assert status == 200 and phq_result["score"] == 1 and phq_result["band"] == "Минимальная выраженность симптомов"
    phq_alerts = request(doctor, "GET", "/api/v1/alerts?status=new")[1]["items"]
    assert any(row["source_id"] == phq_assigned["id"] and row["rule_code"] == "phq9_positive_item_9_review" for row in phq_alerts)
    assert request(patient, "GET", assignments)[1]["items"]
    alert_count = run_command(*docker, "psql", "-U", "synapse", "-d", database, "-Atc", "SELECT count(*) FROM alerts").stdout.strip()
    assert alert_count == "1", alert_count
    scratch = ROOT / "tmp"
    scratch.mkdir(exist_ok=True)
    clinical_env = {**environment, "SYNAPSE_TEST_API": base, "PYTHONIOENCODING": "utf-8"}
    with tempfile.TemporaryDirectory(prefix="clinical_check_", dir=scratch) as isolated_dir:
        clinical = subprocess.run([sys.executable, str(ROOT / "scripts" / "smoke_clinical.py")], cwd=isolated_dir, env=clinical_env, capture_output=True, text=True, encoding="utf-8")
        assert clinical.returncode == 0, clinical.stderr
    disease = {"kind": "disease", "title": "Проверочное заболевание", "source": "https://example.test/disease", "details": "Синтетическая запись для проверки"}
    status, disease_draft = request(admin, "POST", "/api/v1/admin/catalog-drafts", disease)
    assert status == 201
    disease_path = f"/api/v1/admin/catalog-drafts/{disease_draft['id']}/clinical-entry"
    disease_spec = {"code": "ZZ-MVP-01", "source_code": "", "context": "", "source_page": 0}
    status, disease_saved = request(admin, "PATCH", disease_path, {"expected_version": 1, "spec": disease_spec})
    assert status == 200 and disease_saved["version"] == 2
    disease_publish = f"/api/v1/admin/catalog-drafts/{disease_draft['id']}/publish-clinical-entry"
    assert request(doctor, "POST", disease_publish, {"expected_version": 2, "reviewed": True})[0] == 403
    assert request(admin, "POST", disease_publish, {"expected_version": 2, "reviewed": False})[0] == 422
    status, disease_entry = request(admin, "POST", disease_publish, {"expected_version": 2, "reviewed": True})
    assert status == 201 and disease_entry["code"] == "ZZ-MVP-01", (status, disease_entry)
    finding = {"kind": "finding", "title": "Проверочный критерий", "source": "https://example.test/finding", "details": "Синтетическая запись для проверки"}
    status, finding_draft = request(admin, "POST", "/api/v1/admin/catalog-drafts", finding)
    assert status == 201
    finding_path = f"/api/v1/admin/catalog-drafts/{finding_draft['id']}/clinical-entry"
    finding_spec = {"code": "", "source_code": "ZZ-MVP-01", "context": "Условия и контекст тестового пункта.", "source_page": 1}
    status, finding_saved = request(admin, "PATCH", finding_path, {"expected_version": 1, "spec": finding_spec})
    assert status == 200 and finding_saved["version"] == 2
    finding_publish = f"/api/v1/admin/catalog-drafts/{finding_draft['id']}/publish-clinical-entry"
    status, finding_entry = request(admin, "POST", finding_publish, {"expected_version": 2, "reviewed": True})
    assert status == 201 and finding_entry["label"] == "Проверочный критерий", (status, finding_entry)
    code_page = request(doctor, "GET", "/api/v1/clinical/diagnosis-catalog?search=ZZ-MVP-01")[1]
    finding_page = request(doctor, "GET", "/api/v1/clinical/findings?search=ZZ-MVP-01")[1]
    assert any(item["code"] == "ZZ-MVP-01" for item in code_page["items"])
    managed_finding = next(item for item in finding_page["items"] if item["id"] == f"managed:{finding_draft['id']}")
    assert code_page["version"] == finding_page["version"]
    assert "https://example.test/finding" in request(doctor, "GET", "/api/v1/clinical/source/ZZ-MVP-01")[1]["text"]
    assert "Условия и контекст" in request(doctor, "GET", f"/api/v1/clinical/source/{managed_finding['id']}")[1]["text"]
    clinical_patient = "00000000-0000-0000-0000-000000000002"
    diagnosis = {"id": str(uuid.uuid4()), "version": 1, "catalog_version": code_page["version"], "code": "ZZ-MVP-01", "finding_ids": [managed_finding["id"]], "diagnosed_on": datetime.date.today().isoformat(), "status": "active", "note": "Синтетическая проверка управляемого диагноза"}
    status, saved_diagnosis = request(doctor, "POST", f"/api/v1/patients/{clinical_patient}/diagnoses", diagnosis)
    assert status == 201 and saved_diagnosis["title"] == "Проверочное заболевание", (status, saved_diagnosis)
    patient_snapshot = request(patient, "GET", f"/api/v1/patients/{clinical_patient}/diagnoses/{saved_diagnosis['id']}")[1]
    assert [item["label"] for item in patient_snapshot["findings"]] == ["Проверочный критерий"]
    assert request(patient, "GET", "/api/v1/clinical/findings")[0] == 403
    assert request(doctor, "GET", "/api/v1/questionnaires/gad7")[0] == 200
    status, gad7_draft = request(admin, "POST", "/api/v1/admin/catalog-drafts/import-gad7")
    assert status == 201 and gad7_draft["kind"] == "questionnaire", (status, gad7_draft)
    assert request(admin, "POST", "/api/v1/admin/catalog-drafts/import-gad7")[0] == 200
    gad7_form_path = f"/api/v1/admin/catalog-drafts/{gad7_draft['id']}/questionnaire"
    gad7_spec = request(admin, "GET", gad7_form_path)[1]["spec"]
    assert len(gad7_spec["questions"]) == 7 and gad7_spec["minimum_age"] == 18
    assert [option["weight"] for option in gad7_spec["questions"][0]["options"]] == [0, 1, 2, 3]
    assert request(doctor, "GET", "/api/v1/questionnaires/gad7")[0] == 200
    status, gad7_form = request(admin, "POST", f"/api/v1/admin/catalog-drafts/{gad7_draft['id']}/publish-questionnaire", {"expected_version": 1, "reviewed": True})
    assert status == 201 and gad7_form["active"], (status, gad7_form)
    assert any(row["id"] == builtin_phq9["id"] for row in request(doctor, "GET", "/api/v1/questionnaires/catalog")[1])
    assert request(doctor, "GET", "/api/v1/questionnaires/gad7")[1]["questions"] == [question["text"] for question in gad7_spec["questions"]]
    assert gad7_spec["source_sha256"] and gad7_spec["source_url"].startswith("https://")
    altered_gad7 = {**gad7_spec, "questions": [{**gad7_spec["questions"][0], "options": [{"label": "Изменено", "weight": 0}, *gad7_spec["questions"][0]["options"][1:]]}, *gad7_spec["questions"][1:]]}
    status, changed_gad7 = request(admin, "PATCH", gad7_form_path, {"expected_version": 1, "spec": altered_gad7})
    assert status == 200 and changed_gad7["version"] == 2
    assert request(admin, "POST", f"/api/v1/admin/catalog-drafts/{gad7_draft['id']}/publish-questionnaire", {"expected_version": 2, "reviewed": True})[0] == 422
    assert request(doctor, "GET", "/api/v1/questionnaires/gad7")[1]["questions"] == [question["text"] for question in gad7_spec["questions"]]
    scratch = ROOT / "tmp"
    scratch.mkdir(exist_ok=True)
    gad_env = {**environment, "SYNAPSE_TEST_API": base, "PYTHONIOENCODING": "utf-8"}
    with tempfile.TemporaryDirectory(prefix="gad7_check_", dir=scratch) as isolated_dir:
        gad = subprocess.run([sys.executable, str(ROOT / "scripts" / "smoke_questionnaires.py")], cwd=isolated_dir, env=gad_env, capture_output=True, text=True, encoding="utf-8")
        assert gad.returncode == 0, gad.stderr
    status, drug_draft = request(admin, "POST", "/api/v1/admin/catalog-drafts/import-aviandr")
    assert status == 201 and drug_draft["kind"] == "drug", (status, drug_draft)
    assert request(admin, "POST", "/api/v1/admin/catalog-drafts/import-aviandr")[0] == 200
    drug_path = f"/api/v1/admin/catalog-drafts/{drug_draft['id']}/drug"
    drug_spec = request(admin, "GET", drug_path)[1]["spec"]
    assert drug_spec["brand"] == "Авиандр" and drug_spec["effects"]
    assert request(doctor, "GET", drug_path)[0] == 403
    publish_drug_path = f"/api/v1/admin/catalog-drafts/{drug_draft['id']}/publish-drug"
    assert request(admin, "POST", publish_drug_path, {"expected_version": 1, "reviewed": False})[0] == 422
    status, published_drug = request(admin, "POST", publish_drug_path, {"expected_version": 1, "reviewed": True})
    assert status == 201 and published_drug["active"] and published_drug["spec"] == drug_spec, (status, published_drug)
    published_for_doctor = request(doctor, "GET", "/api/v1/clinical/drugs")[1]
    assert next(item["spec"] for item in published_for_doctor if item["drug_code"] == "aviandr")["source_sha256"] == drug_spec["source_sha256"]
    assert request(patient, "GET", "/api/v1/clinical/drugs")[0] == 403
    assert request(admin, "GET", "/api/v1/admin/drugs")[1][0]["id"] == published_drug["id"]
    custom_drug = request(admin, "POST", "/api/v1/admin/catalog-drafts", {
        "kind": "drug", "title": "Демо-препарат", "source": "https://example.org/spec",
        "details": "Синтетическая запись для проверки расширяемого каталога.",
    })[1]
    custom_path = f"/api/v1/admin/catalog-drafts/{custom_drug['id']}/drug"
    custom_spec = {
        **drug_spec,
        "code": "demo_medicine",
        "brand": "Демо-препарат",
        "ingredient": "Демо-вещество",
        "dosage_form": "Таблетки",
        "strength": "10 мг",
        "source_title": "Синтетическая инструкция для проверки",
        "source_url": "https://example.org/spec",
        "source_sha256": "b" * 64,
        "diagnosis_codes": ["F41.1"],
    }
    assert request(admin, "GET", custom_path)[1]["spec"] is None
    saved_custom = request(admin, "PATCH", custom_path, {"expected_version": 1, "spec": custom_spec})
    assert saved_custom[0] == 200 and saved_custom[1]["spec"]["code"] == "demo_medicine"
    custom_publish = request(admin, "POST", f"/api/v1/admin/catalog-drafts/{custom_drug['id']}/publish-drug", {
        "expected_version": 2, "reviewed": True,
    })
    assert custom_publish[0] == 201 and custom_publish[1]["drug_code"] == "demo_medicine"
    active_drugs = request(doctor, "GET", "/api/v1/clinical/drugs")[1]
    assert any(entry["drug_code"] == "demo_medicine" and entry["spec"]["strength"] == "10 мг" for entry in active_drugs)
    therapy_env = {**environment, "SYNAPSE_TEST_API": base, "PYTHONIOENCODING": "utf-8"}
    with tempfile.TemporaryDirectory(prefix="therapy_check_", dir=scratch) as isolated_dir:
        therapy = subprocess.run([sys.executable, str(ROOT / "scripts" / "smoke_therapy.py")], cwd=isolated_dir, env=therapy_env, capture_output=True, text=True, encoding="utf-8")
        assert therapy.returncode == 0, therapy.stderr
        therapy_ids = json.loads((Path(isolated_dir) / "tmp" / "therapy-api-fixtures.json").read_text(encoding="utf-8"))
    with tempfile.TemporaryDirectory(prefix="alerts_check_", dir=scratch) as isolated_dir:
        alerts = subprocess.run([sys.executable, str(ROOT / "scripts" / "smoke_alerts.py")], cwd=isolated_dir, env=therapy_env, capture_output=True, text=True, encoding="utf-8")
        assert alerts.returncode == 0, alerts.stderr
    snapshot_sha = run_command(*docker, "psql", "-U", "synapse", "-d", database, "-Atc", f"SELECT drug_snapshot->>'source_sha256' FROM prescriptions WHERE patient_id='{therapy_ids[-1]}'").stdout.strip()
    assert snapshot_sha == custom_spec["source_sha256"], snapshot_sha
    revised_spec = {**drug_spec, "source_sha256": "a" * 64, "study_note": drug_spec["study_note"] + "\nПроверка новой версии."}
    status, saved_drug = request(admin, "PATCH", drug_path, {"expected_version": 1, "spec": revised_spec})
    assert status == 200 and saved_drug["version"] == 2, (status, saved_drug)
    status, revised_drug = request(admin, "POST", publish_drug_path, {"expected_version": 2, "reviewed": True})
    assert status == 201 and revised_drug["id"] != published_drug["id"]
    published_for_doctor = request(doctor, "GET", "/api/v1/clinical/drugs")[1]
    assert next(item["spec"] for item in published_for_doctor if item["drug_code"] == "aviandr")["source_sha256"] == "a" * 64
    preserved_sha = run_command(*docker, "psql", "-U", "synapse", "-d", database, "-Atc", f"SELECT drug_snapshot->>'source_sha256' FROM prescriptions WHERE patient_id='{therapy_ids[-1]}'").stdout.strip()
    assert preserved_sha == snapshot_sha, preserved_sha
    print("Проверены роли, управляемый клинический каталог, публикация опросников, диагнозы и аудит на отдельной базе.")
finally:
    if process is not None:
        process.terminate()
        process.wait(timeout=10)
    if created:
        subprocess.run([*docker, "dropdb", "-U", "synapse", "--if-exists", database], cwd=ROOT, check=True)
