-- Deterministic, fictional actors and tasks; no production credentials.
INSERT INTO reference_actors VALUES ('workspace-a', 'user-a'), ('workspace-a', 'user-b'), ('workspace-b', 'user-a') ON CONFLICT DO NOTHING;
INSERT INTO reference_tasks VALUES
 ('workspace-a','user-a','00000000-0000-4000-8000-000000000001','Sketch the garden'),
 ('workspace-a','user-a','00000000-0000-4000-8000-000000000002','Sort the library'),
 ('workspace-a','user-a','00000000-0000-4000-8000-000000000003','Repair the kite'),
 ('workspace-a','user-b','00000000-0000-4000-8000-000000000004','Paint a lantern'),
 ('workspace-b','user-a','00000000-0000-4000-8000-000000000005','Build a birdhouse') ON CONFLICT DO NOTHING;
-- The extracted query module has database-enforced read-only access.
DO $$ BEGIN IF NOT EXISTS (SELECT 1 FROM pg_roles WHERE rolname='reference_reader') THEN CREATE ROLE reference_reader LOGIN; END IF; END $$;
GRANT SELECT ON reference_actors, reference_tasks, reference_planning TO reference_reader;
