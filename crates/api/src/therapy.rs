use crate::{ApiError, ApiResult, AppState, authenticate, authorize_patient};
use axum::{
    Json,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
};
use chrono::{DateTime, NaiveDate, NaiveTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, Postgres, Transaction, types::Json as DbJson};
use std::{collections::HashSet, sync::OnceLock};
use synapse_domain::{Role, User};
use uuid::Uuid;

#[derive(Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct DrugSnapshot {
    #[serde(default)]
    pub(crate) code: String,
    pub(crate) brand: String,
    pub(crate) ingredient: String,
    #[serde(default)]
    pub(crate) dosage_form: String,
    #[serde(default)]
    pub(crate) strength: String,
    pub(crate) group: String,
    pub(crate) source_title: String,
    pub(crate) source_date: NaiveDate,
    pub(crate) source_url: String,
    pub(crate) source_sha256: String,
    pub(crate) checked_on: NaiveDate,
    pub(crate) indications: Vec<String>,
    #[serde(default)]
    #[schema(required = true)]
    pub(crate) diagnosis_codes: Vec<String>,
    pub(crate) effects: Vec<DrugEffects>,
    pub(crate) study_note: String,
    pub(crate) transient_note: String,
}

#[derive(Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct DrugEffects {
    pub(crate) frequency: String,
    pub(crate) items: Vec<String>,
}

#[derive(Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct DiagnosisSnapshot {
    id: Uuid,
    code: String,
    title: String,
    catalog_version: String,
    diagnosed_on: NaiveDate,
}

fn drug() -> &'static DrugSnapshot {
    static VALUE: OnceLock<DrugSnapshot> = OnceLock::new();
    VALUE.get_or_init(|| {
        serde_json::from_str(include_str!("../../../clinical/aviandr.json"))
            .expect("Проверенная справка Авиандра")
    })
}
pub(crate) fn static_drug() -> &'static DrugSnapshot {
    drug()
}

#[derive(Clone, Serialize, Deserialize, FromRow, utoipa::ToSchema)]
pub struct Prescription {
    id: Uuid,
    patient_id: Uuid,
    author_name: String,
    updated_by_name: String,
    #[schema(required = true)]
    diagnosis_id: Option<Uuid>,
    #[schema(value_type = Option<DiagnosisSnapshot>, required = true)]
    diagnosis_snapshot: Option<DbJson<DiagnosisSnapshot>>,
    drug_code: String,
    #[schema(value_type = DrugSnapshot)]
    drug_snapshot: DbJson<DrugSnapshot>,
    dosage: String,
    #[schema(value_type = Vec<String>)]
    intake_times: DbJson<Vec<String>>,
    starts_on: NaiveDate,
    #[schema(required = true)]
    ends_on: Option<NaiveDate>,
    instructions: String,
    status: String,
    version: i32,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    #[schema(required = true)]
    published_at: Option<DateTime<Utc>>,
    #[schema(required = true)]
    cancelled_at: Option<DateTime<Utc>>,
}

const PRESCRIPTION_SELECT: &str = "SELECT p.*,a.name AS author_name,u.name AS updated_by_name FROM prescriptions p JOIN users a ON a.id=p.author_id JOIN users u ON u.id=p.updated_by";

fn conflict(code: &'static str, message: &str) -> ApiError {
    ApiError(StatusCode::CONFLICT, code, message.into())
}

async fn require_doctor(state: &AppState, headers: &HeaderMap) -> ApiResult<User> {
    let user = authenticate(state, headers).await?;
    if user.role != Role::Doctor {
        return Err(ApiError::forbidden());
    }
    Ok(user)
}

async fn require_patient(state: &AppState, headers: &HeaderMap, id: Uuid) -> ApiResult<User> {
    let user = authenticate(state, headers).await?;
    if user.role != Role::Patient || user.id != id {
        return Err(ApiError::forbidden());
    }
    authorize_patient(state, &user, id).await?;
    Ok(user)
}

fn clean_text(value: &str, maximum: usize, label: &str, required: bool) -> ApiResult<String> {
    let value = value.trim();
    if (required && value.is_empty())
        || value.chars().count() > maximum
        || value
            .chars()
            .any(|ch| ch.is_control() && ch != '\n' && ch != '\t')
    {
        return Err(ApiError::invalid(format!(
            "Проверьте поле «{label}» (не более {maximum} символов)"
        )));
    }
    Ok(value.to_owned())
}

fn validate_times(values: Vec<String>) -> ApiResult<Vec<String>> {
    if values.is_empty() || values.len() > 6 {
        return Err(ApiError::invalid(
            "Укажите от одного до шести приёмов в сутки",
        ));
    }
    let mut unique = HashSet::new();
    let mut parsed = Vec::with_capacity(values.len());
    for value in values {
        let time = NaiveTime::parse_from_str(value.trim(), "%H:%M")
            .map_err(|_| ApiError::invalid("Время приёма должно иметь формат ЧЧ:ММ"))?;
        let normalized = time.format("%H:%M").to_string();
        if !unique.insert(normalized.clone()) {
            return Err(ApiError::invalid("Время приёма не должно повторяться"));
        }
        parsed.push(normalized);
    }
    parsed.sort();
    Ok(parsed)
}

fn validate_dates(starts_on: NaiveDate, ends_on: Option<NaiveDate>) -> ApiResult<()> {
    if ends_on.is_some_and(|date| date < starts_on) {
        return Err(ApiError::invalid(
            "Дата окончания не может быть раньше даты начала",
        ));
    }
    Ok(())
}

async fn prescription_for_update(
    tx: &mut Transaction<'_, Postgres>,
    patient_id: Uuid,
    prescription_id: Uuid,
) -> ApiResult<Prescription> {
    sqlx::query_as(&format!(
        "{PRESCRIPTION_SELECT} WHERE p.patient_id=$1 AND p.id=$2 FOR UPDATE OF p"
    ))
    .bind(patient_id)
    .bind(prescription_id)
    .fetch_optional(&mut **tx)
    .await?
    .ok_or_else(ApiError::not_found)
}

async fn prescription_audit(
    tx: &mut Transaction<'_, Postgres>,
    value: &Prescription,
    author_id: Uuid,
    action: &str,
) -> ApiResult<()> {
    sqlx::query("INSERT INTO prescription_changes(id,prescription_id,author_id,version,action,snapshot) VALUES($1,$2,$3,$4,$5,$6)")
        .bind(Uuid::new_v4())
        .bind(value.id)
        .bind(author_id)
        .bind(value.version)
        .bind(action)
        .bind(DbJson(value))
        .execute(&mut **tx)
        .await?;
    Ok(())
}

#[utoipa::path(get,path="/api/v1/patients/{id}/prescriptions",operation_id="list_prescriptions",params(("id"=Uuid,Path)),security(("session_cookie"=[])),responses((status=200,body=[Prescription]),(status=401,body=crate::ErrorEnvelope),(status=403,body=crate::ErrorEnvelope),(status=500,body=crate::ErrorEnvelope)))]
pub async fn list_prescriptions(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<Vec<Prescription>>> {
    let user = authenticate(&state, &headers).await?;
    authorize_patient(&state, &user, id).await?;
    let show_drafts = user.role == Role::Doctor;
    let values = sqlx::query_as(&format!("{PRESCRIPTION_SELECT} WHERE p.patient_id=$1 AND ($2 OR p.status<>'draft') ORDER BY p.created_at DESC,p.id DESC LIMIT 100"))
        .bind(id)
        .bind(show_drafts)
        .fetch_all(&state.db)
        .await?;
    Ok(Json(values))
}

#[derive(Deserialize, utoipa::ToSchema)]
pub struct PrescriptionInput {
    id: Uuid,
    version: i32,
    diagnosis_id: Uuid,
    drug_code: String,
    source_sha256: String,
    dosage: String,
    intake_times: Vec<String>,
    starts_on: NaiveDate,
    #[schema(required = true)]
    ends_on: Option<NaiveDate>,
    instructions: String,
}

impl PrescriptionInput {
    fn normalized(mut self) -> ApiResult<Self> {
        if self.drug_code.len() < 2
            || self.drug_code.len() > 64
            || !self.drug_code.chars().all(|character| {
                character.is_ascii_lowercase()
                    || character.is_ascii_digit()
                    || matches!(character, '-' | '_')
            })
            || self.source_sha256.len() != 64
            || !self
                .source_sha256
                .chars()
                .all(|character| character.is_ascii_hexdigit())
        {
            return Err(conflict(
                "drug_reference_changed",
                "Справка препарата изменилась. Загрузите её заново",
            ));
        }
        self.dosage = clean_text(&self.dosage, 200, "Дозировка", true)?;
        self.instructions = clean_text(&self.instructions, 2000, "Инструкции пациенту", false)?;
        self.intake_times = validate_times(self.intake_times)?;
        validate_dates(self.starts_on, self.ends_on)?;
        Ok(self)
    }
}

#[utoipa::path(post,path="/api/v1/patients/{id}/prescriptions",operation_id="create_prescription",params(("id"=Uuid,Path),("X-Synapse-Request"=String,Header)),request_body=PrescriptionInput,security(("session_cookie"=[])),responses((status=201,body=Prescription),(status=200,body=Prescription),(status=401,body=crate::ErrorEnvelope),(status=403,body=crate::ErrorEnvelope),(status=409,body=crate::ErrorEnvelope),(status=422,body=crate::ErrorEnvelope),(status=500,body=crate::ErrorEnvelope)))]
pub async fn create_prescription(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    Json(input): Json<PrescriptionInput>,
) -> ApiResult<(StatusCode, Json<Prescription>)> {
    let doctor = require_doctor(&state, &headers).await?;
    authorize_patient(&state, &doctor, id).await?;
    let input = input.normalized()?;
    if input.version != 1 {
        return Err(ApiError::invalid("Новая запись должна иметь версию 1"));
    }
    let mut tx = state.db.begin().await?;
    sqlx::query("SELECT id FROM users WHERE id=$1 FOR UPDATE")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    if let Some(existing) =
        sqlx::query_as::<_, Prescription>(&format!("{PRESCRIPTION_SELECT} WHERE p.id=$1"))
            .bind(input.id)
            .fetch_optional(&mut *tx)
            .await?
    {
        let exact = existing.patient_id == id
            && existing.diagnosis_id == Some(input.diagnosis_id)
            && existing.drug_code == input.drug_code
            && existing.drug_snapshot.0.source_sha256 == input.source_sha256
            && existing.dosage == input.dosage
            && existing.intake_times.0 == input.intake_times
            && existing.starts_on == input.starts_on
            && existing.ends_on == input.ends_on
            && existing.instructions == input.instructions;
        if !exact {
            return Err(conflict(
                "prescription_conflict",
                "Идентификатор уже использован для другого назначения",
            ));
        }
        tx.commit().await?;
        return Ok((StatusCode::OK, Json(existing)));
    }
    sqlx::query("LOCK TABLE drug_catalog_versions IN SHARE MODE")
        .execute(&mut *tx)
        .await?;
    let drug_snapshot = crate::drug_catalog::by_code(&state, &input.drug_code).await?;
    if input.source_sha256 != drug_snapshot.source_sha256 {
        return Err(conflict(
            "drug_reference_changed",
            "Справка препарата изменилась. Загрузите её заново",
        ));
    }
    let diagnosis: Option<(String, String, String, NaiveDate, String)> = sqlx::query_as(
        "SELECT code,title,catalog_version,diagnosed_on,status FROM clinical_diagnoses WHERE id=$1 AND patient_id=$2 FOR UPDATE",
    )
    .bind(input.diagnosis_id)
    .bind(id)
    .fetch_optional(&mut *tx)
    .await?;
    let (diagnosis_code, diagnosis_title, catalog_version, diagnosed_on, diagnosis_status) =
        diagnosis.ok_or_else(ApiError::not_found)?;
    if diagnosis_status != "active" {
        return Err(ApiError::invalid(
            "Для нового назначения выберите активный диагноз пациента",
        ));
    }
    if !drug_snapshot.diagnosis_codes.contains(&diagnosis_code) {
        return Err(ApiError::invalid(
            "Препарат не связан с выбранным диагнозом в опубликованном справочнике",
        ));
    }
    let diagnosis_snapshot = DiagnosisSnapshot {
        id: input.diagnosis_id,
        code: diagnosis_code,
        title: diagnosis_title,
        catalog_version,
        diagnosed_on,
    };
    let current_exists: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM prescriptions WHERE patient_id=$1 AND drug_code=$2 AND status IN ('draft','active'))")
        .bind(id).bind(&input.drug_code)
        .fetch_one(&mut *tx)
        .await?;
    if current_exists {
        return Err(conflict(
            "prescription_exists",
            "У пациента уже есть черновик или действующее назначение этого препарата",
        ));
    }
    let inserted = sqlx::query("INSERT INTO prescriptions(id,patient_id,author_id,updated_by,diagnosis_id,diagnosis_snapshot,drug_code,drug_snapshot,dosage,intake_times,starts_on,ends_on,instructions,status,version) VALUES($1,$2,$3,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,'draft',1) ON CONFLICT(id) DO NOTHING")
        .bind(input.id).bind(id).bind(doctor.id).bind(input.diagnosis_id).bind(DbJson(diagnosis_snapshot))
        .bind(&input.drug_code).bind(DbJson(drug_snapshot)).bind(input.dosage)
        .bind(DbJson(input.intake_times)).bind(input.starts_on).bind(input.ends_on).bind(input.instructions)
        .execute(&mut *tx).await?;
    if inserted.rows_affected() != 1 {
        return Err(conflict(
            "prescription_conflict",
            "Не удалось создать черновик с этим идентификатором",
        ));
    }
    let value = prescription_for_update(&mut tx, id, input.id).await?;
    prescription_audit(&mut tx, &value, doctor.id, "created").await?;
    tx.commit().await?;
    Ok((StatusCode::CREATED, Json(value)))
}

#[utoipa::path(patch,path="/api/v1/patients/{id}/prescriptions/{prescription_id}",operation_id="update_prescription",params(("id"=Uuid,Path),("prescription_id"=Uuid,Path),("X-Synapse-Request"=String,Header)),request_body=PrescriptionInput,security(("session_cookie"=[])),responses((status=200,body=Prescription),(status=401,body=crate::ErrorEnvelope),(status=403,body=crate::ErrorEnvelope),(status=404,body=crate::ErrorEnvelope),(status=409,body=crate::ErrorEnvelope),(status=422,body=crate::ErrorEnvelope),(status=500,body=crate::ErrorEnvelope)))]
pub async fn update_prescription(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((id, prescription_id)): Path<(Uuid, Uuid)>,
    Json(input): Json<PrescriptionInput>,
) -> ApiResult<Json<Prescription>> {
    let doctor = require_doctor(&state, &headers).await?;
    authorize_patient(&state, &doctor, id).await?;
    let input = input.normalized()?;
    if input.id != prescription_id {
        return Err(ApiError::invalid(
            "Идентификатор в адресе и теле должен совпадать",
        ));
    }
    let mut tx = state.db.begin().await?;
    let before = prescription_for_update(&mut tx, id, prescription_id).await?;
    if before.drug_code != input.drug_code
        || before.diagnosis_id != Some(input.diagnosis_id)
        || before.drug_snapshot.0.source_sha256 != input.source_sha256
    {
        return Err(conflict(
            "drug_reference_changed",
            "Препарат и диагноз уже зафиксированы в назначении. Создайте новую схему для другого решения",
        ));
    }
    if !["draft", "active"].contains(&before.status.as_str()) {
        return Err(conflict(
            "prescription_closed",
            "Завершённое назначение нельзя изменить",
        ));
    }
    if before.version != input.version {
        return Err(conflict(
            "stale_version",
            "Назначение уже изменилось. Загрузите актуальную версию",
        ));
    }
    let schedule_changed = before.intake_times.0 != input.intake_times
        || before.starts_on != input.starts_on
        || before.ends_on != input.ends_on;
    if before.status == "active" && schedule_changed {
        let has_marks: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM adherence_marks WHERE prescription_id=$1)",
        )
        .bind(prescription_id)
        .fetch_one(&mut *tx)
        .await?;
        if has_marks {
            return Err(conflict(
                "adherence_exists",
                "Нельзя изменить расписание после отметок пациента. Отмените схему и создайте новую",
            ));
        }
    }
    let unchanged = before.dosage == input.dosage
        && !schedule_changed
        && before.instructions == input.instructions;
    if unchanged {
        tx.commit().await?;
        return Ok(Json(before));
    }
    sqlx::query("UPDATE prescriptions SET dosage=$1,intake_times=$2,starts_on=$3,ends_on=$4,instructions=$5,updated_by=$6,version=version+1,updated_at=now() WHERE id=$7")
        .bind(input.dosage).bind(DbJson(input.intake_times)).bind(input.starts_on)
        .bind(input.ends_on).bind(input.instructions).bind(doctor.id).bind(prescription_id)
        .execute(&mut *tx).await?;
    let value = prescription_for_update(&mut tx, id, prescription_id).await?;
    prescription_audit(&mut tx, &value, doctor.id, "updated").await?;
    tx.commit().await?;
    Ok(Json(value))
}

#[derive(Deserialize, utoipa::ToSchema)]
pub struct VersionInput {
    version: i32,
}

async fn transition(
    state: AppState,
    headers: HeaderMap,
    patient_id: Uuid,
    prescription_id: Uuid,
    input: VersionInput,
    publish: bool,
) -> ApiResult<Json<Prescription>> {
    let doctor = require_doctor(&state, &headers).await?;
    authorize_patient(&state, &doctor, patient_id).await?;
    let mut tx = state.db.begin().await?;
    let before = prescription_for_update(&mut tx, patient_id, prescription_id).await?;
    let target = if publish { "active" } else { "cancelled" };
    if before.status == target && before.version.checked_sub(1) == Some(input.version) {
        tx.commit().await?;
        return Ok(Json(before));
    }
    if before.version != input.version {
        return Err(conflict(
            "stale_version",
            "Назначение уже изменилось. Загрузите актуальную версию",
        ));
    }
    if publish && before.status != "draft" {
        return Err(conflict(
            "prescription_conflict",
            "Опубликовать можно только черновик",
        ));
    }
    if !publish && !["draft", "active"].contains(&before.status.as_str()) {
        return Err(conflict("prescription_closed", "Назначение уже завершено"));
    }
    if publish {
        sqlx::query("UPDATE prescriptions SET status='active',published_at=now(),updated_by=$1,version=version+1,updated_at=now() WHERE id=$2")
            .bind(doctor.id).bind(prescription_id).execute(&mut *tx).await?;
    } else {
        sqlx::query("UPDATE prescriptions SET status='cancelled',cancelled_at=now(),updated_by=$1,version=version+1,updated_at=now() WHERE id=$2")
            .bind(doctor.id).bind(prescription_id).execute(&mut *tx).await?;
    }
    let value = prescription_for_update(&mut tx, patient_id, prescription_id).await?;
    prescription_audit(
        &mut tx,
        &value,
        doctor.id,
        if publish { "published" } else { "cancelled" },
    )
    .await?;
    if publish {
        sqlx::query("INSERT INTO notifications(id,recipient_id,patient_id,kind,related_id) SELECT $1,$2,$2,'prescription_published',$3 FROM patient_profiles WHERE user_id=$2 AND access_accepted ON CONFLICT(recipient_id,kind,related_id) DO NOTHING")
            .bind(Uuid::new_v4()).bind(patient_id).bind(prescription_id).execute(&mut *tx).await?;
    }
    tx.commit().await?;
    Ok(Json(value))
}

#[utoipa::path(post,path="/api/v1/patients/{id}/prescriptions/{prescription_id}/publish",operation_id="publish_prescription",params(("id"=Uuid,Path),("prescription_id"=Uuid,Path),("X-Synapse-Request"=String,Header)),request_body=VersionInput,security(("session_cookie"=[])),responses((status=200,body=Prescription),(status=401,body=crate::ErrorEnvelope),(status=403,body=crate::ErrorEnvelope),(status=404,body=crate::ErrorEnvelope),(status=409,body=crate::ErrorEnvelope),(status=500,body=crate::ErrorEnvelope)))]
pub async fn publish_prescription(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((id, prescription_id)): Path<(Uuid, Uuid)>,
    Json(input): Json<VersionInput>,
) -> ApiResult<Json<Prescription>> {
    transition(state, headers, id, prescription_id, input, true).await
}

#[utoipa::path(post,path="/api/v1/patients/{id}/prescriptions/{prescription_id}/cancel",operation_id="cancel_prescription",params(("id"=Uuid,Path),("prescription_id"=Uuid,Path),("X-Synapse-Request"=String,Header)),request_body=VersionInput,security(("session_cookie"=[])),responses((status=200,body=Prescription),(status=401,body=crate::ErrorEnvelope),(status=403,body=crate::ErrorEnvelope),(status=404,body=crate::ErrorEnvelope),(status=409,body=crate::ErrorEnvelope),(status=500,body=crate::ErrorEnvelope)))]
pub async fn cancel_prescription(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((id, prescription_id)): Path<(Uuid, Uuid)>,
    Json(input): Json<VersionInput>,
) -> ApiResult<Json<Prescription>> {
    transition(state, headers, id, prescription_id, input, false).await
}

#[derive(Serialize, FromRow, utoipa::ToSchema)]
pub struct PrescriptionChange {
    version: i32,
    action: String,
    author_name: String,
    #[schema(value_type = Prescription)]
    snapshot: DbJson<Prescription>,
    created_at: DateTime<Utc>,
}

#[utoipa::path(get,path="/api/v1/patients/{id}/prescriptions/{prescription_id}/changes",operation_id="prescription_changes",params(("id"=Uuid,Path),("prescription_id"=Uuid,Path)),security(("session_cookie"=[])),responses((status=200,body=[PrescriptionChange]),(status=401,body=crate::ErrorEnvelope),(status=403,body=crate::ErrorEnvelope),(status=404,body=crate::ErrorEnvelope),(status=500,body=crate::ErrorEnvelope)))]
pub async fn prescription_changes(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((id, prescription_id)): Path<(Uuid, Uuid)>,
) -> ApiResult<Json<Vec<PrescriptionChange>>> {
    let doctor = require_doctor(&state, &headers).await?;
    authorize_patient(&state, &doctor, id).await?;
    let exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM prescriptions WHERE patient_id=$1 AND id=$2)",
    )
    .bind(id)
    .bind(prescription_id)
    .fetch_one(&state.db)
    .await?;
    if !exists {
        return Err(ApiError::not_found());
    }
    let values=sqlx::query_as("SELECT c.version,c.action,u.name AS author_name,c.snapshot,c.created_at FROM prescription_changes c JOIN users u ON u.id=c.author_id WHERE c.prescription_id=$1 ORDER BY c.version DESC")
        .bind(prescription_id).fetch_all(&state.db).await?;
    Ok(Json(values))
}

#[derive(Serialize, FromRow, utoipa::ToSchema)]
pub struct AdherenceMark {
    id: Uuid,
    prescription_id: Uuid,
    patient_id: Uuid,
    scheduled_on: NaiveDate,
    scheduled_time: String,
    status: String,
    reason: String,
    created_at: DateTime<Utc>,
}

#[derive(Deserialize, utoipa::ToSchema)]
pub struct MarkInput {
    id: Uuid,
    scheduled_on: NaiveDate,
    scheduled_time: String,
    status: String,
    reason: String,
}

#[utoipa::path(post,path="/api/v1/patients/{id}/prescriptions/{prescription_id}/marks",operation_id="record_adherence",params(("id"=Uuid,Path),("prescription_id"=Uuid,Path),("X-Synapse-Request"=String,Header)),request_body=MarkInput,security(("session_cookie"=[])),responses((status=201,body=AdherenceMark),(status=200,body=AdherenceMark),(status=401,body=crate::ErrorEnvelope),(status=403,body=crate::ErrorEnvelope),(status=404,body=crate::ErrorEnvelope),(status=409,body=crate::ErrorEnvelope),(status=422,body=crate::ErrorEnvelope),(status=500,body=crate::ErrorEnvelope)))]
pub async fn record_adherence(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((id, prescription_id)): Path<(Uuid, Uuid)>,
    Json(mut input): Json<MarkInput>,
) -> ApiResult<(StatusCode, Json<AdherenceMark>)> {
    let patient = require_patient(&state, &headers, id).await?;
    input.reason = clean_text(&input.reason, 1000, "Причина пропуска", false)?;
    if !["taken", "skipped"].contains(&input.status.as_str()) {
        return Err(ApiError::invalid(
            "Статус должен быть «принято» или «пропущено»",
        ));
    }
    if input.status == "taken" && !input.reason.is_empty() {
        return Err(ApiError::invalid(
            "Причина указывается только для пропущенного приёма",
        ));
    }
    let time = NaiveTime::parse_from_str(input.scheduled_time.trim(), "%H:%M")
        .map_err(|_| ApiError::invalid("Время приёма должно иметь формат ЧЧ:ММ"))?;
    let normalized = time.format("%H:%M").to_string();
    let mut tx = state.db.begin().await?;
    let prescription = prescription_for_update(&mut tx, id, prescription_id).await?;
    if prescription.status != "active" {
        return Err(conflict(
            "prescription_inactive",
            "Отметить можно только действующее назначение",
        ));
    }
    if input.scheduled_on < prescription.starts_on
        || prescription
            .ends_on
            .is_some_and(|end| input.scheduled_on > end)
        || input.scheduled_on > Utc::now().date_naive()
        || !prescription.intake_times.contains(&normalized)
    {
        return Err(ApiError::invalid(
            "Отметка не соответствует дате и времени действующей схемы",
        ));
    }
    if let Some(existing)=sqlx::query_as::<_,AdherenceMark>("SELECT id,prescription_id,patient_id,scheduled_on,to_char(scheduled_time,'HH24:MI') AS scheduled_time,status,reason,created_at FROM adherence_marks WHERE id=$1 OR (prescription_id=$2 AND scheduled_on=$3 AND scheduled_time=$4)")
        .bind(input.id).bind(prescription_id).bind(input.scheduled_on).bind(time).fetch_optional(&mut *tx).await? {
        if existing.id!=input.id || existing.patient_id!=patient.id || existing.status!=input.status || existing.reason!=input.reason { return Err(conflict("adherence_conflict","Этот приём уже отмечен иначе")); }
        tx.commit().await?;
        return Ok((StatusCode::OK,Json(existing)));
    }
    sqlx::query("INSERT INTO adherence_marks(id,prescription_id,patient_id,scheduled_on,scheduled_time,status,reason) VALUES($1,$2,$3,$4,$5,$6,$7)")
        .bind(input.id).bind(prescription_id).bind(id).bind(input.scheduled_on).bind(time).bind(input.status).bind(input.reason).execute(&mut *tx).await?;
    let value=sqlx::query_as("SELECT id,prescription_id,patient_id,scheduled_on,to_char(scheduled_time,'HH24:MI') AS scheduled_time,status,reason,created_at FROM adherence_marks WHERE id=$1")
        .bind(input.id).fetch_one(&mut *tx).await?;
    tx.commit().await?;
    Ok((StatusCode::CREATED, Json(value)))
}

#[derive(Deserialize, utoipa::IntoParams)]
#[into_params(parameter_in=Query)]
pub struct AdherenceQuery {
    from: NaiveDate,
    to: NaiveDate,
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct AdherenceSummary {
    from: NaiveDate,
    to: NaiveDate,
    expected: i64,
    taken: i64,
    skipped: i64,
    unmarked: i64,
    #[schema(required = true)]
    percent: Option<f64>,
    marks: Vec<AdherenceMark>,
}

#[utoipa::path(get,path="/api/v1/patients/{id}/prescriptions/{prescription_id}/adherence",operation_id="prescription_adherence",params(("id"=Uuid,Path),("prescription_id"=Uuid,Path),AdherenceQuery),security(("session_cookie"=[])),responses((status=200,body=AdherenceSummary),(status=401,body=crate::ErrorEnvelope),(status=403,body=crate::ErrorEnvelope),(status=404,body=crate::ErrorEnvelope),(status=422,body=crate::ErrorEnvelope),(status=500,body=crate::ErrorEnvelope)))]
pub async fn prescription_adherence(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((id, prescription_id)): Path<(Uuid, Uuid)>,
    Query(query): Query<AdherenceQuery>,
) -> ApiResult<Json<AdherenceSummary>> {
    let user = authenticate(&state, &headers).await?;
    authorize_patient(&state, &user, id).await?;
    if query.to < query.from || (query.to - query.from).num_days() > 366 {
        return Err(ApiError::invalid(
            "Период должен составлять от 1 до 367 дней",
        ));
    }
    let prescription = sqlx::query_as::<_, Prescription>(&format!(
        "{PRESCRIPTION_SELECT} WHERE p.patient_id=$1 AND p.id=$2 AND ($3 OR p.status<>'draft')"
    ))
    .bind(id)
    .bind(prescription_id)
    .bind(user.role == Role::Doctor)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(ApiError::not_found)?;
    let last = [
        query.to,
        Utc::now().date_naive(),
        prescription.ends_on.unwrap_or(query.to),
        prescription
            .cancelled_at
            .map(|value| value.date_naive())
            .unwrap_or(query.to),
    ]
    .into_iter()
    .min()
    .expect("Четыре даты");
    let first = query.from.max(prescription.starts_on);
    let expected = if last < first {
        0
    } else {
        (last - first).num_days().saturating_add(1) * (prescription.intake_times.len() as i64)
    };
    let marks=sqlx::query_as::<_,AdherenceMark>("SELECT id,prescription_id,patient_id,scheduled_on,to_char(scheduled_time,'HH24:MI') AS scheduled_time,status,reason,created_at FROM adherence_marks WHERE prescription_id=$1 AND scheduled_on BETWEEN $2 AND $3 ORDER BY scheduled_on DESC,scheduled_time DESC")
        .bind(prescription_id).bind(query.from).bind(query.to).fetch_all(&state.db).await?;
    let taken = marks.iter().filter(|mark| mark.status == "taken").count() as i64;
    let skipped = marks.iter().filter(|mark| mark.status == "skipped").count() as i64;
    let unmarked = expected.saturating_sub(taken + skipped);
    let percent =
        (expected > 0).then(|| ((taken as f64 / expected as f64) * 1000.0).round() / 10.0);
    Ok(Json(AdherenceSummary {
        from: query.from,
        to: query.to,
        expected,
        taken,
        skipped,
        unmarked,
        percent,
        marks,
    }))
}

#[derive(Clone, Serialize, FromRow, utoipa::ToSchema)]
pub struct AdverseEvent {
    id: Uuid,
    patient_id: Uuid,
    #[schema(required = true)]
    prescription_id: Option<Uuid>,
    author_name: String,
    occurred_at: DateTime<Utc>,
    severity: String,
    description: String,
    effect_items: Vec<String>,
    status: String,
    review_note: String,
    #[schema(required = true)]
    reviewer_name: Option<String>,
    version: i32,
    created_at: DateTime<Utc>,
    #[schema(required = true)]
    reviewed_at: Option<DateTime<Utc>>,
}

const ADVERSE_SELECT: &str = "SELECT e.id,e.patient_id,e.prescription_id,a.name AS author_name,e.occurred_at,e.severity,e.description,e.effect_items,e.status,e.review_note,r.name AS reviewer_name,e.version,e.created_at,e.reviewed_at FROM adverse_events e JOIN users a ON a.id=e.author_id LEFT JOIN users r ON r.id=e.reviewed_by";

async fn adverse_audit(
    tx: &mut Transaction<'_, Postgres>,
    value: &AdverseEvent,
    author_id: Uuid,
    action: &str,
) -> ApiResult<()> {
    sqlx::query("INSERT INTO adverse_event_changes(id,adverse_event_id,author_id,version,action,snapshot) VALUES($1,$2,$3,$4,$5,$6)")
        .bind(Uuid::new_v4()).bind(value.id).bind(author_id).bind(value.version).bind(action).bind(DbJson(value)).execute(&mut **tx).await?;
    Ok(())
}

#[utoipa::path(get,path="/api/v1/patients/{id}/adverse-events",operation_id="list_adverse_events",params(("id"=Uuid,Path)),security(("session_cookie"=[])),responses((status=200,body=[AdverseEvent]),(status=401,body=crate::ErrorEnvelope),(status=403,body=crate::ErrorEnvelope),(status=500,body=crate::ErrorEnvelope)))]
pub async fn list_adverse_events(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<Vec<AdverseEvent>>> {
    let user = authenticate(&state, &headers).await?;
    authorize_patient(&state, &user, id).await?;
    let values = sqlx::query_as::<_, AdverseEvent>(&format!(
        "{ADVERSE_SELECT} WHERE e.patient_id=$1 ORDER BY e.occurred_at DESC,e.id DESC LIMIT 100"
    ))
    .bind(id)
    .fetch_all(&state.db)
    .await?;
    Ok(Json(values))
}

#[derive(Deserialize, utoipa::ToSchema)]
pub struct AdverseInput {
    id: Uuid,
    prescription_id: Uuid,
    occurred_at: DateTime<Utc>,
    severity: String,
    effect_items: Vec<String>,
    note: String,
}

#[utoipa::path(post,path="/api/v1/patients/{id}/adverse-events",operation_id="record_adverse_effects",params(("id"=Uuid,Path),("X-Synapse-Request"=String,Header)),request_body=AdverseInput,security(("session_cookie"=[])),responses((status=201,body=AdverseEvent),(status=200,body=AdverseEvent),(status=401,body=crate::ErrorEnvelope),(status=403,body=crate::ErrorEnvelope),(status=409,body=crate::ErrorEnvelope),(status=422,body=crate::ErrorEnvelope),(status=500,body=crate::ErrorEnvelope)))]
pub async fn record_adverse_effects(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    Json(mut input): Json<AdverseInput>,
) -> ApiResult<(StatusCode, Json<AdverseEvent>)> {
    let doctor = require_doctor(&state, &headers).await?;
    authorize_patient(&state, &doctor, id).await?;
    input.note = clean_text(&input.note, 4000, "Комментарий врача", false)?;
    if !["mild", "moderate", "severe"].contains(&input.severity.as_str()) {
        return Err(ApiError::invalid(
            "Выберите выраженность реакции по итогам беседы с пациентом",
        ));
    }
    if input.occurred_at > Utc::now() + chrono::Duration::minutes(5) {
        return Err(ApiError::invalid("Дата выявления не может быть в будущем"));
    }
    input.effect_items.sort();
    input.effect_items.dedup();
    if input.effect_items.is_empty() || input.effect_items.len() > 100 {
        return Err(ApiError::invalid(
            "Выберите выявленные реакции из справки назначенного препарата",
        ));
    }
    let mut tx = state.db.begin().await?;
    let prescription = prescription_for_update(&mut tx, id, input.prescription_id).await?;
    if prescription.status == "draft" {
        return Err(ApiError::not_found());
    }
    let available: HashSet<_> = prescription
        .drug_snapshot
        .0
        .effects
        .iter()
        .flat_map(|group| group.items.iter().map(String::as_str))
        .collect();
    if input
        .effect_items
        .iter()
        .any(|effect| !available.contains(effect.as_str()))
    {
        return Err(ApiError::invalid(
            "Реакции должны входить в сохранённую справку назначенного препарата",
        ));
    }
    let description = if input.note.is_empty() {
        input.effect_items.join(", ")
    } else {
        format!("{}. {}", input.effect_items.join(", "), input.note)
    };
    if let Some(existing) =
        sqlx::query_as::<_, AdverseEvent>(&format!("{ADVERSE_SELECT} WHERE e.id=$1"))
            .bind(input.id)
            .fetch_optional(&mut *tx)
            .await?
    {
        let exact = existing.patient_id == id
            && existing.prescription_id == Some(input.prescription_id)
            && existing.occurred_at == input.occurred_at
            && existing.severity == input.severity
            && existing.effect_items == input.effect_items
            && existing.description == description;
        if !exact {
            return Err(conflict(
                "adverse_event_conflict",
                "Идентификатор уже использован для другой записи врача",
            ));
        }
        tx.commit().await?;
        return Ok((StatusCode::OK, Json(existing)));
    }
    sqlx::query("INSERT INTO adverse_events(id,patient_id,prescription_id,author_id,occurred_at,severity,description,effect_items,status,review_note,reviewed_by,reviewed_at,version) VALUES($1,$2,$3,$4,$5,$6,$7,$8,'reviewed',$9,$4,now(),1)")
        .bind(input.id).bind(id).bind(input.prescription_id).bind(doctor.id).bind(input.occurred_at).bind(input.severity).bind(description).bind(&input.effect_items).bind(&input.note).execute(&mut *tx).await?;
    let value = sqlx::query_as::<_, AdverseEvent>(&format!("{ADVERSE_SELECT} WHERE e.id=$1"))
        .bind(input.id)
        .fetch_one(&mut *tx)
        .await?;
    adverse_audit(&mut tx, &value, doctor.id, "identified").await?;
    tx.commit().await?;
    Ok((StatusCode::CREATED, Json(value)))
}

#[derive(Deserialize, utoipa::ToSchema)]
pub struct ReviewInput {
    version: i32,
    review_note: String,
}

#[utoipa::path(post,path="/api/v1/patients/{id}/adverse-events/{event_id}/review",operation_id="review_adverse_event",params(("id"=Uuid,Path),("event_id"=Uuid,Path),("X-Synapse-Request"=String,Header)),request_body=ReviewInput,security(("session_cookie"=[])),responses((status=200,body=AdverseEvent),(status=401,body=crate::ErrorEnvelope),(status=403,body=crate::ErrorEnvelope),(status=404,body=crate::ErrorEnvelope),(status=409,body=crate::ErrorEnvelope),(status=422,body=crate::ErrorEnvelope),(status=500,body=crate::ErrorEnvelope)))]
pub async fn review_adverse_event(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((id, event_id)): Path<(Uuid, Uuid)>,
    Json(mut input): Json<ReviewInput>,
) -> ApiResult<Json<AdverseEvent>> {
    let doctor = require_doctor(&state, &headers).await?;
    authorize_patient(&state, &doctor, id).await?;
    input.review_note = clean_text(&input.review_note, 4000, "Комментарий врача", true)?;
    let mut tx = state.db.begin().await?;
    let before = sqlx::query_as::<_, AdverseEvent>(&format!(
        "{ADVERSE_SELECT} WHERE e.patient_id=$1 AND e.id=$2 FOR UPDATE OF e"
    ))
    .bind(id)
    .bind(event_id)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or_else(ApiError::not_found)?;
    if before.status == "reviewed"
        && before.version.checked_sub(1) == Some(input.version)
        && before.review_note == input.review_note
    {
        tx.commit().await?;
        return Ok(Json(before));
    }
    if before.version != input.version {
        return Err(conflict(
            "stale_version",
            "Сообщение уже обработано или изменилось",
        ));
    }
    if before.status != "reported" {
        return Err(conflict(
            "adverse_event_reviewed",
            "Сообщение уже обработано",
        ));
    }
    sqlx::query("UPDATE adverse_events SET status='reviewed',review_note=$1,reviewed_by=$2,reviewed_at=now(),version=version+1 WHERE id=$3")
        .bind(input.review_note).bind(doctor.id).bind(event_id).execute(&mut *tx).await?;
    let value = sqlx::query_as::<_, AdverseEvent>(&format!("{ADVERSE_SELECT} WHERE e.id=$1"))
        .bind(event_id)
        .fetch_one(&mut *tx)
        .await?;
    adverse_audit(&mut tx, &value, doctor.id, "reviewed").await?;
    tx.commit().await?;
    Ok(Json(value))
}
