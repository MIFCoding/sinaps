use crate::{ApiError, ApiResult, AppState, authenticate, authorize_patient};
use axum::{
    Json,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, Postgres, Transaction, types::Json as DbJson};
use std::sync::OnceLock;
use synapse_domain::{Role, User};
use uuid::Uuid;

#[derive(Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct AlertCondition {
    field: String,
    operator: String,
    value: String,
}

#[derive(Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct AlertRule {
    code: String,
    version: String,
    title: String,
    priority: String,
    source_kind: String,
    condition: AlertCondition,
    reason: String,
    actions: Vec<String>,
    patient_guidance: String,
    limitations: String,
}

#[derive(Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct AlertRuleset {
    ruleset_version: String,
    rules: Vec<AlertRule>,
}

fn ruleset() -> &'static AlertRuleset {
    static VALUE: OnceLock<AlertRuleset> = OnceLock::new();
    VALUE.get_or_init(|| {
        serde_json::from_str(include_str!("../../../clinical/alert-rules.json"))
            .expect("Проверенная структура правил алертинга")
    })
}

fn severe_questionnaire_rule() -> &'static AlertRule {
    ruleset()
        .rules
        .iter()
        .find(|rule| rule.code == "gad7_severe_result_review")
        .expect("Правило высокой оценки GAD-7")
}

#[utoipa::path(get,path="/api/v1/clinical/alert-rules",operation_id="alert_rules",security(("session_cookie"=[])),responses((status=200,body=AlertRuleset),(status=401,body=crate::ErrorEnvelope),(status=403,body=crate::ErrorEnvelope),(status=500,body=crate::ErrorEnvelope)))]
pub async fn rules(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> ApiResult<Json<AlertRuleset>> {
    doctor(&state, &headers).await?;
    Ok(Json(ruleset().clone()))
}

#[derive(Clone, Serialize, Deserialize, FromRow, utoipa::ToSchema)]
pub struct Alert {
    id: Uuid,
    patient_id: Uuid,
    patient_name: String,
    source_kind: String,
    source_id: Uuid,
    source_occurred_at: DateTime<Utc>,
    source_created_at: DateTime<Utc>,
    rule_code: String,
    rule_version: String,
    #[schema(value_type = AlertRule)]
    rule_snapshot: DbJson<AlertRule>,
    priority: String,
    reason: String,
    status: String,
    version: i32,
    detected_at: DateTime<Utc>,
    detection_latency_ms: i64,
    #[schema(required = true)]
    acknowledged_at: Option<DateTime<Utc>>,
    #[schema(required = true)]
    acknowledged_by_name: Option<String>,
    #[schema(required = true)]
    resolved_at: Option<DateTime<Utc>>,
    #[schema(required = true)]
    resolved_by_name: Option<String>,
    resolution_comment: String,
    #[schema(required = true)]
    routing_category: Option<String>,
    routing_comment: String,
    #[schema(required = true)]
    routing_decided_at: Option<DateTime<Utc>>,
    #[schema(required = true)]
    routing_decided_by_name: Option<String>,
}

const SELECT: &str = "SELECT a.id,a.patient_id,p.name AS patient_name,a.source_kind,a.source_id,a.source_occurred_at,a.source_created_at,a.rule_code,a.rule_version,a.rule_snapshot,a.priority,a.reason,a.status,a.version,a.detected_at,a.detection_latency_ms,a.acknowledged_at,ack.name AS acknowledged_by_name,a.resolved_at,res.name AS resolved_by_name,a.resolution_comment,a.routing_category,a.routing_comment,a.routing_decided_at,route.name AS routing_decided_by_name FROM alerts a JOIN users p ON p.id=a.patient_id LEFT JOIN users ack ON ack.id=a.acknowledged_by LEFT JOIN users res ON res.id=a.resolved_by LEFT JOIN users route ON route.id=a.routing_decided_by";

fn conflict(message: &str) -> ApiError {
    ApiError(StatusCode::CONFLICT, "alert_conflict", message.into())
}

async fn doctor(state: &AppState, headers: &HeaderMap) -> ApiResult<User> {
    let user = authenticate(state, headers).await?;
    if user.role != Role::Doctor {
        return Err(ApiError::forbidden());
    }
    Ok(user)
}

async fn audit(
    tx: &mut Transaction<'_, Postgres>,
    value: &Alert,
    author_id: Option<Uuid>,
    action: &str,
) -> ApiResult<()> {
    sqlx::query("INSERT INTO alert_changes(id,alert_id,author_id,version,action,snapshot) VALUES($1,$2,$3,$4,$5,$6)")
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

pub async fn evaluate_questionnaire(
    tx: &mut Transaction<'_, Postgres>,
    patient_id: Uuid,
    source_id: Uuid,
    score: i16,
    completed_at: DateTime<Utc>,
) -> ApiResult<Option<Uuid>> {
    let rule = severe_questionnaire_rule();
    if score < 15 {
        return Ok(None);
    }
    persist_questionnaire_alert(tx, patient_id, source_id, completed_at, rule)
        .await
        .map(Some)
}

pub async fn evaluate_phq9_item(
    tx: &mut Transaction<'_, Postgres>,
    patient_id: Uuid,
    source_id: Uuid,
    completed_at: DateTime<Utc>,
) -> ApiResult<Uuid> {
    let rule = ruleset()
        .rules
        .iter()
        .find(|rule| rule.code == "phq9_positive_item_9_review")
        .expect("Правило оценки пункта 9 PHQ-9");
    persist_questionnaire_alert(tx, patient_id, source_id, completed_at, rule).await
}

async fn persist_questionnaire_alert(
    tx: &mut Transaction<'_, Postgres>,
    patient_id: Uuid,
    source_id: Uuid,
    completed_at: DateTime<Utc>,
    rule: &AlertRule,
) -> ApiResult<Uuid> {
    let detected_at = Utc::now();
    let latency = (detected_at - completed_at).num_milliseconds().max(0);
    let alert_id = Uuid::new_v4();
    let inserted = sqlx::query("INSERT INTO alerts(id,patient_id,source_kind,source_id,source_occurred_at,source_created_at,rule_code,rule_version,rule_snapshot,priority,reason,status,version,detected_at,detection_latency_ms,resolution_comment) VALUES($1,$2,'questionnaire',$3,$4,$4,$5,$6,$7,'critical',$8,'new',1,$9,$10,'') ON CONFLICT(source_kind,source_id,rule_code) DO NOTHING")
        .bind(alert_id)
        .bind(patient_id)
        .bind(source_id)
        .bind(completed_at)
        .bind(&rule.code)
        .bind(&rule.version)
        .bind(DbJson(rule))
        .bind(&rule.reason)
        .bind(detected_at)
        .bind(latency)
        .execute(&mut **tx)
        .await?;
    let id = if inserted.rows_affected() == 1 {
        alert_id
    } else {
        sqlx::query_scalar(
            "SELECT id FROM alerts WHERE source_kind='questionnaire' AND source_id=$1 AND rule_code=$2",
        )
        .bind(source_id)
        .bind(&rule.code)
        .fetch_one(&mut **tx)
        .await?
    };
    if inserted.rows_affected() == 1 {
        let value = sqlx::query_as::<_, Alert>(&format!("{SELECT} WHERE a.id=$1"))
            .bind(id)
            .fetch_one(&mut **tx)
            .await?;
        audit(tx, &value, None, "detected").await?;
        sqlx::query("INSERT INTO notifications(id,recipient_id,patient_id,kind,related_id) SELECT $1,doctor_id,user_id,'critical_alert',$3 FROM patient_profiles WHERE user_id=$2 ON CONFLICT (recipient_id,kind,related_id) DO NOTHING")
            .bind(Uuid::new_v4())
            .bind(patient_id)
            .bind(id)
            .execute(&mut **tx)
            .await?;
    }
    Ok(id)
}
#[derive(Deserialize, utoipa::IntoParams)]
#[into_params(parameter_in = Query)]
pub struct AlertQuery {
    limit: Option<i64>,
    before: Option<DateTime<Utc>>,
    before_id: Option<Uuid>,
    status: Option<String>,
}

impl AlertQuery {
    fn values(&self) -> ApiResult<(i64, Option<&str>)> {
        let limit = self.limit.unwrap_or(20);
        let status = self.status.as_deref();
        if !(1..=50).contains(&limit)
            || self.before.is_some() != self.before_id.is_some()
            || status.is_some_and(|value| {
                !["new", "acknowledged", "resolved", "active"].contains(&value)
            })
        {
            return Err(ApiError::invalid(
                "Проверьте статус, размер страницы и полный курсор",
            ));
        }
        Ok((limit, status))
    }
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct AlertCursor {
    before: DateTime<Utc>,
    before_id: Uuid,
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct AlertPage {
    items: Vec<Alert>,
    #[schema(required = true)]
    next_cursor: Option<AlertCursor>,
}

fn page(mut items: Vec<Alert>, limit: i64) -> AlertPage {
    let more = items.len() > limit as usize;
    items.truncate(limit as usize);
    let next_cursor = more.then(|| {
        let last = items.last().expect("Непустая страница алертов");
        AlertCursor {
            before: last.detected_at,
            before_id: last.id,
        }
    });
    AlertPage { items, next_cursor }
}

#[utoipa::path(get,path="/api/v1/alerts",operation_id="list_alerts",params(AlertQuery),security(("session_cookie"=[])),responses((status=200,body=AlertPage),(status=400,body=String,content_type="text/plain"),(status=401,body=crate::ErrorEnvelope),(status=403,body=crate::ErrorEnvelope),(status=422,body=crate::ErrorEnvelope),(status=500,body=crate::ErrorEnvelope)))]
pub async fn list(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<AlertQuery>,
) -> ApiResult<Json<AlertPage>> {
    let user = doctor(&state, &headers).await?;
    let (limit, status) = query.values()?;
    let items=sqlx::query_as::<_,Alert>(&format!("{SELECT} JOIN patient_profiles pp ON pp.user_id=a.patient_id WHERE pp.doctor_id=$1 AND ($2::timestamptz IS NULL OR (a.detected_at,a.id)<($2,$3)) AND ($4::text IS NULL OR ($4='active' AND a.status IN ('new','acknowledged')) OR ($4<>'active' AND a.status=$4)) ORDER BY a.detected_at DESC,a.id DESC LIMIT $5"))
        .bind(user.id).bind(query.before).bind(query.before_id).bind(status).bind(limit+1).fetch_all(&state.db).await?;
    Ok(Json(page(items, limit)))
}

#[utoipa::path(get,path="/api/v1/patients/{id}/alerts",operation_id="list_patient_alerts",params(("id"=Uuid,Path),AlertQuery),security(("session_cookie"=[])),responses((status=200,body=AlertPage),(status=400,body=String,content_type="text/plain"),(status=401,body=crate::ErrorEnvelope),(status=403,body=crate::ErrorEnvelope),(status=422,body=crate::ErrorEnvelope),(status=500,body=crate::ErrorEnvelope)))]
pub async fn patient_list(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    Query(query): Query<AlertQuery>,
) -> ApiResult<Json<AlertPage>> {
    let user = authenticate(&state, &headers).await?;
    authorize_patient(&state, &user, id).await?;
    let (limit, status) = query.values()?;
    let items=sqlx::query_as::<_,Alert>(&format!("{SELECT} WHERE a.patient_id=$1 AND ($2::timestamptz IS NULL OR (a.detected_at,a.id)<($2,$3)) AND ($4::text IS NULL OR ($4='active' AND a.status IN ('new','acknowledged')) OR ($4<>'active' AND a.status=$4)) ORDER BY a.detected_at DESC,a.id DESC LIMIT $5"))
        .bind(id).bind(query.before).bind(query.before_id).bind(status).bind(limit+1).fetch_all(&state.db).await?;
    Ok(Json(page(items, limit)))
}

#[derive(Deserialize, utoipa::ToSchema)]
pub struct AlertActionInput {
    version: i32,
    comment: String,
}

#[derive(Deserialize, utoipa::ToSchema)]
pub struct RoutingDecisionInput {
    version: i32,
    category: String,
    comment: String,
}

fn routing_category(value: &str) -> ApiResult<&'static str> {
    match value {
        "mandatory" => Ok("mandatory"),
        "preferred" => Ok("preferred"),
        "not_indicated" => Ok("not_indicated"),
        _ => Err(ApiError::invalid(
            "Выберите категорию: обязательно, желательно или не направлять",
        )),
    }
}

async fn locked(
    tx: &mut Transaction<'_, Postgres>,
    patient_id: Uuid,
    alert_id: Uuid,
) -> ApiResult<Alert> {
    sqlx::query_as::<_, Alert>(&format!(
        "{SELECT} WHERE a.patient_id=$1 AND a.id=$2 FOR UPDATE OF a"
    ))
    .bind(patient_id)
    .bind(alert_id)
    .fetch_optional(&mut **tx)
    .await?
    .ok_or_else(ApiError::not_found)
}

#[utoipa::path(post,path="/api/v1/patients/{id}/alerts/{alert_id}/acknowledge",operation_id="acknowledge_alert",params(("id"=Uuid,Path),("alert_id"=Uuid,Path),("X-Synapse-Request"=String,Header)),request_body=AlertActionInput,security(("session_cookie"=[])),responses((status=200,body=Alert),(status=401,body=crate::ErrorEnvelope),(status=403,body=crate::ErrorEnvelope),(status=404,body=crate::ErrorEnvelope),(status=409,body=crate::ErrorEnvelope),(status=422,body=crate::ErrorEnvelope),(status=500,body=crate::ErrorEnvelope)))]
pub async fn acknowledge(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((id, alert_id)): Path<(Uuid, Uuid)>,
    Json(input): Json<AlertActionInput>,
) -> ApiResult<Json<Alert>> {
    let user = doctor(&state, &headers).await?;
    authorize_patient(&state, &user, id).await?;
    if !input.comment.trim().is_empty() {
        return Err(ApiError::invalid(
            "Комментарий заполняется при завершении обработки",
        ));
    }
    let mut tx = state.db.begin().await?;
    let before = locked(&mut tx, id, alert_id).await?;
    if before.status == "acknowledged" && before.version.checked_sub(1) == Some(input.version) {
        tx.commit().await?;
        return Ok(Json(before));
    }
    if before.version != input.version || before.status != "new" {
        return Err(conflict(
            "Алерт уже изменился. Загрузите актуальное состояние",
        ));
    }
    sqlx::query("UPDATE alerts SET status='acknowledged',version=version+1,acknowledged_at=now(),acknowledged_by=$1 WHERE id=$2")
        .bind(user.id).bind(alert_id).execute(&mut *tx).await?;
    let value = locked(&mut tx, id, alert_id).await?;
    audit(&mut tx, &value, Some(user.id), "acknowledged").await?;
    tx.commit().await?;
    Ok(Json(value))
}

fn clean_comment(value: &str) -> ApiResult<String> {
    let value = value.trim();
    if value.is_empty()
        || value.chars().count() > 4000
        || value
            .chars()
            .any(|ch| ch.is_control() && ch != '\n' && ch != '\t')
    {
        return Err(ApiError::invalid(
            "Комментарий об обработке обязателен и не должен превышать 4000 символов",
        ));
    }
    Ok(value.to_owned())
}

#[utoipa::path(post,path="/api/v1/patients/{id}/alerts/{alert_id}/resolve",operation_id="resolve_alert",params(("id"=Uuid,Path),("alert_id"=Uuid,Path),("X-Synapse-Request"=String,Header)),request_body=AlertActionInput,security(("session_cookie"=[])),responses((status=200,body=Alert),(status=401,body=crate::ErrorEnvelope),(status=403,body=crate::ErrorEnvelope),(status=404,body=crate::ErrorEnvelope),(status=409,body=crate::ErrorEnvelope),(status=422,body=crate::ErrorEnvelope),(status=500,body=crate::ErrorEnvelope)))]
pub async fn resolve(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((id, alert_id)): Path<(Uuid, Uuid)>,
    Json(input): Json<AlertActionInput>,
) -> ApiResult<Json<Alert>> {
    let user = doctor(&state, &headers).await?;
    authorize_patient(&state, &user, id).await?;
    let comment = clean_comment(&input.comment)?;
    let mut tx = state.db.begin().await?;
    let before = locked(&mut tx, id, alert_id).await?;
    if before.status == "resolved"
        && before.version.checked_sub(1) == Some(input.version)
        && before.resolution_comment == comment
    {
        tx.commit().await?;
        return Ok(Json(before));
    }

    if before.version != input.version || before.status != "acknowledged" {
        return Err(conflict(
            "Сначала примите актуальный алерт в работу, затем завершите обработку",
        ));
    }
    sqlx::query("UPDATE alerts SET status='resolved',version=version+1,resolved_at=now(),resolved_by=$1,resolution_comment=$2 WHERE id=$3")
        .bind(user.id).bind(comment).bind(alert_id).execute(&mut *tx).await?;
    let value = locked(&mut tx, id, alert_id).await?;
    audit(&mut tx, &value, Some(user.id), "resolved").await?;
    tx.commit().await?;
    Ok(Json(value))
}

#[utoipa::path(post,path="/api/v1/patients/{id}/alerts/{alert_id}/routing",operation_id="decide_alert_routing",params(("id"=Uuid,Path),("alert_id"=Uuid,Path),("X-Synapse-Request"=String,Header)),request_body=RoutingDecisionInput,security(("session_cookie"=[])),responses((status=200,body=Alert),(status=401,body=crate::ErrorEnvelope),(status=403,body=crate::ErrorEnvelope),(status=404,body=crate::ErrorEnvelope),(status=409,body=crate::ErrorEnvelope),(status=422,body=crate::ErrorEnvelope),(status=500,body=crate::ErrorEnvelope)))]
pub async fn decide_routing(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((id, alert_id)): Path<(Uuid, Uuid)>,
    Json(input): Json<RoutingDecisionInput>,
) -> ApiResult<Json<Alert>> {
    let user = doctor(&state, &headers).await?;
    authorize_patient(&state, &user, id).await?;
    let category = routing_category(input.category.trim())?;
    let comment = clean_comment(&input.comment)?;
    let mut tx = state.db.begin().await?;
    let before = locked(&mut tx, id, alert_id).await?;
    if before.routing_category.as_deref() == Some(category)
        && before.routing_comment == comment
        && before.routing_decided_at.is_some()
        && before.version.checked_sub(1) == Some(input.version)
    {
        tx.commit().await?;
        return Ok(Json(before));
    }
    if before.version != input.version {
        return Err(conflict(
            "Алерт уже изменился. Загрузите актуальное состояние",
        ));
    }
    sqlx::query("UPDATE alerts SET version=version+1,routing_category=$1,routing_comment=$2,routing_decided_at=now(),routing_decided_by=$3 WHERE id=$4")
        .bind(category)
        .bind(comment)
        .bind(user.id)
        .bind(alert_id)
        .execute(&mut *tx)
        .await?;
    let value = locked(&mut tx, id, alert_id).await?;
    audit(&mut tx, &value, Some(user.id), "routing_decided").await?;
    tx.commit().await?;
    Ok(Json(value))
}

#[derive(Serialize, FromRow, utoipa::ToSchema)]
pub struct AlertChange {
    version: i32,
    action: String,
    #[schema(required = true)]
    author_name: Option<String>,
    #[schema(value_type = Alert)]
    snapshot: DbJson<Alert>,
    created_at: DateTime<Utc>,
}

#[utoipa::path(get,path="/api/v1/patients/{id}/alerts/{alert_id}/changes",operation_id="alert_changes",params(("id"=Uuid,Path),("alert_id"=Uuid,Path)),security(("session_cookie"=[])),responses((status=200,body=[AlertChange]),(status=401,body=crate::ErrorEnvelope),(status=403,body=crate::ErrorEnvelope),(status=404,body=crate::ErrorEnvelope),(status=500,body=crate::ErrorEnvelope)))]
pub async fn changes(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((id, alert_id)): Path<(Uuid, Uuid)>,
) -> ApiResult<Json<Vec<AlertChange>>> {
    let user = doctor(&state, &headers).await?;
    authorize_patient(&state, &user, id).await?;
    let exists: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM alerts WHERE patient_id=$1 AND id=$2)")
            .bind(id)
            .bind(alert_id)
            .fetch_one(&state.db)
            .await?;
    if !exists {
        return Err(ApiError::not_found());
    }
    let values=sqlx::query_as("SELECT c.version,c.action,u.name AS author_name,c.snapshot,c.created_at FROM alert_changes c LEFT JOIN users u ON u.id=c.author_id WHERE c.alert_id=$1 ORDER BY c.version DESC")
        .bind(alert_id).fetch_all(&state.db).await?;
    Ok(Json(values))
}
