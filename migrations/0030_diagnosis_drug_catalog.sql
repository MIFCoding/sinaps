ALTER TABLE drug_catalog_versions
    DROP CONSTRAINT drug_catalog_versions_drug_code_check;
ALTER TABLE prescriptions
    DROP CONSTRAINT prescriptions_drug_code_check;
ALTER TABLE prescriptions
    ADD CONSTRAINT prescriptions_drug_code_check
    CHECK (char_length(drug_code) BETWEEN 2 AND 64);

ALTER TABLE prescriptions
    ADD COLUMN diagnosis_id uuid REFERENCES clinical_diagnoses(id),
    ADD COLUMN diagnosis_snapshot jsonb;

CREATE INDEX prescriptions_diagnosis_idx ON prescriptions(diagnosis_id);
