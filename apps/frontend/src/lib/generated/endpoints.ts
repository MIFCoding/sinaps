// Создано автоматически из серверного OpenAPI. Обновление: npm run api:generate.
export const endpoints = {
  "acknowledge_alert": {
    "method": "POST",
    "path": "/api/v1/patients/{id}/alerts/{alert_id}/acknowledge"
  },
  "activate": {
    "method": "POST",
    "path": "/api/v1/auth/activate"
  },
  "admin_archive_questionnaire": {
    "method": "POST",
    "path": "/api/v1/admin/questionnaires/{id}/archive"
  },
  "admin_clinical_catalog": {
    "method": "GET",
    "path": "/api/v1/admin/clinical-catalog"
  },
  "admin_create_draft": {
    "method": "POST",
    "path": "/api/v1/admin/catalog-drafts"
  },
  "admin_drug_catalog": {
    "method": "GET",
    "path": "/api/v1/admin/drugs"
  },
  "admin_drug_diagnoses": {
    "method": "GET",
    "path": "/api/v1/admin/drug-diagnoses"
  },
  "admin_get_clinical_entry_draft": {
    "method": "GET",
    "path": "/api/v1/admin/catalog-drafts/{id}/clinical-entry"
  },
  "admin_get_drug_draft": {
    "method": "GET",
    "path": "/api/v1/admin/catalog-drafts/{id}/drug"
  },
  "admin_get_questionnaire_draft": {
    "method": "GET",
    "path": "/api/v1/admin/catalog-drafts/{id}/questionnaire"
  },
  "admin_import_aviandr": {
    "method": "POST",
    "path": "/api/v1/admin/catalog-drafts/import-aviandr"
  },
  "admin_import_gad7": {
    "method": "POST",
    "path": "/api/v1/admin/catalog-drafts/import-gad7"
  },
  "admin_list_drafts": {
    "method": "GET",
    "path": "/api/v1/admin/catalog-drafts"
  },
  "admin_publish_clinical_entry": {
    "method": "POST",
    "path": "/api/v1/admin/catalog-drafts/{id}/publish-clinical-entry"
  },
  "admin_publish_drug": {
    "method": "POST",
    "path": "/api/v1/admin/catalog-drafts/{id}/publish-drug"
  },
  "admin_publish_questionnaire": {
    "method": "POST",
    "path": "/api/v1/admin/catalog-drafts/{id}/publish-questionnaire"
  },
  "admin_questionnaire_catalog": {
    "method": "GET",
    "path": "/api/v1/admin/questionnaires"
  },
  "admin_save_clinical_entry_draft": {
    "method": "PATCH",
    "path": "/api/v1/admin/catalog-drafts/{id}/clinical-entry"
  },
  "admin_save_drug_draft": {
    "method": "PATCH",
    "path": "/api/v1/admin/catalog-drafts/{id}/drug"
  },
  "admin_save_questionnaire_draft": {
    "method": "PATCH",
    "path": "/api/v1/admin/catalog-drafts/{id}/questionnaire"
  },
  "admin_update_draft": {
    "method": "PATCH",
    "path": "/api/v1/admin/catalog-drafts/{id}"
  },
  "alert_changes": {
    "method": "GET",
    "path": "/api/v1/patients/{id}/alerts/{alert_id}/changes"
  },
  "alert_rules": {
    "method": "GET",
    "path": "/api/v1/clinical/alert-rules"
  },
  "assign_custom_questionnaire": {
    "method": "POST",
    "path": "/api/v1/patients/{id}/custom-questionnaires"
  },
  "assign_questionnaire": {
    "method": "POST",
    "path": "/api/v1/patients/{id}/questionnaires"
  },
  "auth_config": {
    "method": "GET",
    "path": "/api/v1/auth/config"
  },
  "book_appointment": {
    "method": "POST",
    "path": "/api/v1/appointments/{id}/book"
  },
  "cancel_appointment": {
    "method": "POST",
    "path": "/api/v1/appointments/{id}/cancel"
  },
  "cancel_custom_questionnaire": {
    "method": "POST",
    "path": "/api/v1/patients/{id}/custom-questionnaires/{assignment_id}/cancel"
  },
  "cancel_prescription": {
    "method": "POST",
    "path": "/api/v1/patients/{id}/prescriptions/{prescription_id}/cancel"
  },
  "cancel_questionnaire": {
    "method": "POST",
    "path": "/api/v1/patients/{id}/questionnaires/{assignment_id}/cancel"
  },
  "clinical_codes": {
    "method": "GET",
    "path": "/api/v1/clinical/diagnosis-catalog"
  },
  "clinical_drugs": {
    "method": "GET",
    "path": "/api/v1/clinical/drugs"
  },
  "clinical_findings": {
    "method": "GET",
    "path": "/api/v1/clinical/findings"
  },
  "clinical_source": {
    "method": "GET",
    "path": "/api/v1/clinical/source/{code}"
  },
  "complete_password_reset": {
    "method": "POST",
    "path": "/api/v1/auth/password-reset/complete"
  },
  "create_appointment": {
    "method": "POST",
    "path": "/api/v1/appointments"
  },
  "create_diagnosis": {
    "method": "POST",
    "path": "/api/v1/patients/{id}/diagnoses"
  },
  "create_invitation": {
    "method": "POST",
    "path": "/api/v1/patients/{id}/invitation"
  },
  "create_patient": {
    "method": "POST",
    "path": "/api/v1/patients"
  },
  "create_prescription": {
    "method": "POST",
    "path": "/api/v1/patients/{id}/prescriptions"
  },
  "custom_questionnaire_catalog": {
    "method": "GET",
    "path": "/api/v1/questionnaires/catalog"
  },
  "custom_questionnaire_changes": {
    "method": "GET",
    "path": "/api/v1/patients/{id}/custom-questionnaires/{assignment_id}/changes"
  },
  "decide_alert_routing": {
    "method": "POST",
    "path": "/api/v1/patients/{id}/alerts/{alert_id}/routing"
  },
  "decide_clinical_recommendation": {
    "method": "POST",
    "path": "/api/v1/patients/{id}/recommendations/{recommendation_id}/decision"
  },
  "diagnosis_changes": {
    "method": "GET",
    "path": "/api/v1/patients/{id}/diagnoses/{diagnosis_id}/changes"
  },
  "get_diary_entry": {
    "method": "GET",
    "path": "/api/v1/patients/{id}/diary/{entry_id}"
  },
  "get_event": {
    "method": "GET",
    "path": "/api/v1/patients/{id}/events/{event_id}"
  },
  "get_patient": {
    "method": "GET",
    "path": "/api/v1/patients/{id}"
  },
  "get_questionnaire": {
    "method": "GET",
    "path": "/api/v1/patients/{id}/questionnaires/{assignment_id}"
  },
  "health": {
    "method": "GET",
    "path": "/health"
  },
  "invitation_preview": {
    "method": "GET",
    "path": "/api/v1/auth/invitation/{code}"
  },
  "list_adverse_events": {
    "method": "GET",
    "path": "/api/v1/patients/{id}/adverse-events"
  },
  "list_alerts": {
    "method": "GET",
    "path": "/api/v1/alerts"
  },
  "list_appointments": {
    "method": "GET",
    "path": "/api/v1/appointments"
  },
  "list_chat_threads": {
    "method": "GET",
    "path": "/api/v1/chat/threads"
  },
  "list_clinical_recommendations": {
    "method": "GET",
    "path": "/api/v1/patients/{id}/recommendations"
  },
  "list_custom_questionnaires": {
    "method": "GET",
    "path": "/api/v1/patients/{id}/custom-questionnaires"
  },
  "list_diary": {
    "method": "GET",
    "path": "/api/v1/patients/{id}/diary"
  },
  "list_diary_page": {
    "method": "GET",
    "path": "/api/v1/patients/{id}/diary/page"
  },
  "list_events": {
    "method": "GET",
    "path": "/api/v1/patients/{id}/events"
  },
  "list_events_page": {
    "method": "GET",
    "path": "/api/v1/patients/{id}/events/page"
  },
  "list_messages": {
    "method": "GET",
    "path": "/api/v1/patients/{id}/messages"
  },
  "list_notifications": {
    "method": "GET",
    "path": "/api/v1/notifications"
  },
  "list_patient_alerts": {
    "method": "GET",
    "path": "/api/v1/patients/{id}/alerts"
  },
  "list_patients": {
    "method": "GET",
    "path": "/api/v1/patients"
  },
  "list_patients_page": {
    "method": "GET",
    "path": "/api/v1/patients/page"
  },
  "list_prescriptions": {
    "method": "GET",
    "path": "/api/v1/patients/{id}/prescriptions"
  },
  "list_profile_changes": {
    "method": "GET",
    "path": "/api/v1/patients/{id}/changes"
  },
  "list_profile_changes_page": {
    "method": "GET",
    "path": "/api/v1/patients/{id}/changes/page"
  },
  "list_questionnaires": {
    "method": "GET",
    "path": "/api/v1/patients/{id}/questionnaires"
  },
  "login": {
    "method": "POST",
    "path": "/api/v1/auth/login"
  },
  "logout": {
    "method": "POST",
    "path": "/api/v1/auth/logout"
  },
  "me": {
    "method": "GET",
    "path": "/api/v1/me"
  },
  "patient_diagnoses": {
    "method": "GET",
    "path": "/api/v1/patients/{id}/diagnoses"
  },
  "patient_diagnosis": {
    "method": "GET",
    "path": "/api/v1/patients/{id}/diagnoses/{diagnosis_id}"
  },
  "prescription_adherence": {
    "method": "GET",
    "path": "/api/v1/patients/{id}/prescriptions/{prescription_id}/adherence"
  },
  "prescription_changes": {
    "method": "GET",
    "path": "/api/v1/patients/{id}/prescriptions/{prescription_id}/changes"
  },
  "publish_clinical_recommendation": {
    "method": "POST",
    "path": "/api/v1/patients/{id}/recommendations/{recommendation_id}/publish"
  },
  "publish_prescription": {
    "method": "POST",
    "path": "/api/v1/patients/{id}/prescriptions/{prescription_id}/publish"
  },
  "questionnaire_changes": {
    "method": "GET",
    "path": "/api/v1/patients/{id}/questionnaires/{assignment_id}/changes"
  },
  "questionnaire_definition": {
    "method": "GET",
    "path": "/api/v1/questionnaires/gad7"
  },
  "read_all_notifications": {
    "method": "POST",
    "path": "/api/v1/notifications/read-all"
  },
  "read_messages": {
    "method": "POST",
    "path": "/api/v1/patients/{id}/messages/read"
  },
  "read_notification": {
    "method": "POST",
    "path": "/api/v1/notifications/{id}/read"
  },
  "ready": {
    "method": "GET",
    "path": "/ready"
  },
  "record_adherence": {
    "method": "POST",
    "path": "/api/v1/patients/{id}/prescriptions/{prescription_id}/marks"
  },
  "record_adverse_effects": {
    "method": "POST",
    "path": "/api/v1/patients/{id}/adverse-events"
  },
  "register": {
    "method": "POST",
    "path": "/api/v1/auth/register"
  },
  "request_email_verification": {
    "method": "POST",
    "path": "/api/v1/auth/verification-email"
  },
  "request_password_reset": {
    "method": "POST",
    "path": "/api/v1/auth/password-reset"
  },
  "reschedule_appointment": {
    "method": "POST",
    "path": "/api/v1/appointments/{id}/reschedule"
  },
  "resolve_alert": {
    "method": "POST",
    "path": "/api/v1/patients/{id}/alerts/{alert_id}/resolve"
  },
  "review_adverse_event": {
    "method": "POST",
    "path": "/api/v1/patients/{id}/adverse-events/{event_id}/review"
  },
  "save_diary": {
    "method": "POST",
    "path": "/api/v1/patients/{id}/diary"
  },
  "save_event": {
    "method": "POST",
    "path": "/api/v1/patients/{id}/events"
  },
  "send_message": {
    "method": "POST",
    "path": "/api/v1/patients/{id}/messages"
  },
  "submit_custom_questionnaire": {
    "method": "POST",
    "path": "/api/v1/patients/{id}/custom-questionnaires/{assignment_id}/submit"
  },
  "submit_questionnaire": {
    "method": "POST",
    "path": "/api/v1/patients/{id}/questionnaires/{assignment_id}/submit"
  },
  "update_diagnosis": {
    "method": "PATCH",
    "path": "/api/v1/patients/{id}/diagnoses/{diagnosis_id}"
  },
  "update_patient": {
    "method": "PATCH",
    "path": "/api/v1/patients/{id}"
  },
  "update_prescription": {
    "method": "PATCH",
    "path": "/api/v1/patients/{id}/prescriptions/{prescription_id}"
  },
  "verify_email": {
    "method": "POST",
    "path": "/api/v1/auth/verify-email"
  }
} as const;
