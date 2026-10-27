ALTER TABLE patient_profiles ADD COLUMN physical_info jsonb NOT NULL DEFAULT
'{"height_cm":null,"weight_kg":null,"blood_group":null}'::jsonb;
