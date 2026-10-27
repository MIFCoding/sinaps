ALTER TABLE patient_profiles ADD COLUMN medical_history jsonb NOT NULL DEFAULT
'{"complaints":"","present_illness":"","life_history":"","comorbidities_status":"unknown","comorbidities_details":""}'::jsonb;
