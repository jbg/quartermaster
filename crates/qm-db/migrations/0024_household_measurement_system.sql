ALTER TABLE household ADD COLUMN measurement_system TEXT NOT NULL DEFAULT 'metric';

UPDATE household
SET measurement_system = 'metric'
WHERE measurement_system = '';
