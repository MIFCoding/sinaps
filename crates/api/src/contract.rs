use crate::*;
use utoipa::openapi::{
    OpenApi,
    security::{ApiKey, ApiKeyValue, SecurityScheme},
};
use utoipa_axum::{router::OpenApiRouter, routes};

#[utoipa::path(get, path = "/health", operation_id = "health", responses((status = 200, description = "Процесс отвечает", body = serde_json::Value)))]
async fn health() -> Json<Value> {
    Json(json!({"status": "ok", "service": "synapse-api"}))
}

pub fn routes() -> (Router<AppState>, OpenApi) {
    // Этот реестр одновременно создаёт маршруты Axum и документ OpenAPI.
    let (router, mut document) = OpenApiRouter::<AppState>::new()
        .routes(routes!(health))
        .routes(routes!(ready))
        .routes(routes!(login))
        .routes(routes!(registration::register))
        .routes(routes!(registration::invitation_preview))
        .routes(routes!(account_security::config))
        .routes(routes!(account_security::verify_email))
        .routes(routes!(account_security::request_verification))
        .routes(routes!(account_security::request_password_reset))
        .routes(routes!(account_security::complete_password_reset))
        .routes(routes!(patient_management::activate))
        .routes(routes!(logout))
        .routes(routes!(me))
        .routes(routes!(admin::list_drafts, admin::create_draft))
        .routes(routes!(admin::update_draft))
        .routes(routes!(admin::import_gad7))
        .routes(routes!(drug_catalog::import_aviandr))
        .routes(routes!(drug_catalog::diagnosis_choices))
        .routes(routes!(drug_catalog::get_draft))
        .routes(routes!(drug_catalog::save_draft))
        .routes(routes!(drug_catalog::list))
        .routes(routes!(drug_catalog::publish))
        .routes(routes!(clinical_catalog::list))
        .routes(routes!(clinical_catalog::get_draft))
        .routes(routes!(clinical_catalog::save_draft))
        .routes(routes!(clinical_catalog::publish))
        .routes(routes!(custom_questionnaires::publish))
        .routes(routes!(custom_questionnaires::admin_catalog))
        .routes(routes!(custom_questionnaires::archive))
        .routes(routes!(custom_questionnaires::catalog))
        .routes(routes!(
            custom_questionnaires::list,
            custom_questionnaires::assign
        ))
        .routes(routes!(custom_questionnaires::submit))
        .routes(routes!(custom_questionnaires::cancel))
        .routes(routes!(custom_questionnaires::changes))
        .routes(routes!(
            admin::get_questionnaire_draft,
            admin::save_questionnaire_draft
        ))
        .routes(routes!(patient_directory::list))
        .routes(routes!(notifications::list))
        .routes(routes!(notifications::read_all))
        .routes(routes!(notifications::read))
        .routes(routes!(patients, patient_management::create_patient))
        .routes(routes!(patient, patient_management::update_profile))
        .routes(routes!(patient_management::create_invitation))
        .routes(routes!(patient_management::profile_changes))
        .routes(routes!(patient_management::profile_changes_page))
        .routes(routes!(diary, save_diary))
        .routes(routes!(history::diary))
        .routes(routes!(diary_entry))
        .routes(routes!(events, save_event))
        .routes(routes!(history::events))
        .routes(routes!(observation_event))
        .routes(routes!(clinical::findings))
        .routes(routes!(clinical::codes))
        .routes(routes!(clinical::source))
        .routes(routes!(clinical_recommendations::list))
        .routes(routes!(clinical_recommendations::decide))
        .routes(routes!(clinical_recommendations::publish))
        .routes(routes!(drug_catalog::clinical_list))
        .routes(routes!(alerts::rules))
        .routes(routes!(clinical::list, clinical::create))
        .routes(routes!(clinical::get, clinical::update))
        .routes(routes!(clinical::changes))
        .routes(routes!(questionnaires::template))
        .routes(routes!(questionnaires::list, questionnaires::assign))
        .routes(routes!(questionnaires::get))
        .routes(routes!(questionnaires::submit))
        .routes(routes!(questionnaires::cancel))
        .routes(routes!(questionnaires::changes))
        .routes(routes!(
            therapy::list_prescriptions,
            therapy::create_prescription
        ))
        .routes(routes!(therapy::update_prescription))
        .routes(routes!(therapy::publish_prescription))
        .routes(routes!(therapy::cancel_prescription))
        .routes(routes!(therapy::prescription_changes))
        .routes(routes!(therapy::record_adherence))
        .routes(routes!(therapy::prescription_adherence))
        .routes(routes!(
            therapy::list_adverse_events,
            therapy::record_adverse_effects
        ))
        .routes(routes!(therapy::review_adverse_event))
        .routes(routes!(alerts::list))
        .routes(routes!(alerts::patient_list))
        .routes(routes!(alerts::acknowledge))
        .routes(routes!(alerts::resolve))
        .routes(routes!(alerts::decide_routing))
        .routes(routes!(alerts::changes))
        .routes(routes!(chat::list))
        .routes(routes!(chat::threads))
        .routes(routes!(chat::send))
        .routes(routes!(chat::read))
        .routes(routes!(appointments::list))
        .routes(routes!(appointments::create))
        .routes(routes!(appointments::book))
        .routes(routes!(appointments::cancel))
        .routes(routes!(appointments::reschedule))
        .split_for_parts();
    document.info.title = "СИНАПС API".to_owned();
    document.info.version = env!("CARGO_PKG_VERSION").to_owned();
    document.info.description = Some("Контракт генерируется из типов и обработчиков сервера. Все изменяющие запросы требуют X-Synapse-Request: 1. Сессия определяется cookie; права на пациента проверяются сервером.".to_owned());
    document
        .components
        .as_mut()
        .expect("Схемы зарегистрированы обработчиками")
        .add_security_scheme(
            "session_cookie",
            SecurityScheme::ApiKey(ApiKey::Cookie(ApiKeyValue::new("synapse_session"))),
        );
    (router, document)
}
