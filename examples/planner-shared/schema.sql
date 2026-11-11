-- Synthetic reference data only. All writes serialize on one actor row;
-- the partial unique index independently enforces one active focus.
CREATE TABLE IF NOT EXISTS reference_actors (
  tenant text NOT NULL, user_id text NOT NULL, PRIMARY KEY (tenant, user_id)
);
CREATE TABLE IF NOT EXISTS reference_tasks (
  tenant text NOT NULL, user_id text NOT NULL, task_id text NOT NULL, title text NOT NULL,
  PRIMARY KEY (tenant, user_id, task_id),
  FOREIGN KEY (tenant, user_id) REFERENCES reference_actors
);
CREATE TABLE IF NOT EXISTS reference_planning (
  tenant text NOT NULL, user_id text NOT NULL, planning_id text NOT NULL,
  task_id text NOT NULL, planned_for date NOT NULL, position integer NOT NULL CHECK (position > 0),
  version integer NOT NULL DEFAULT 1 CHECK (version > 0), focused boolean NOT NULL DEFAULT false,
  paused_at text,
  PRIMARY KEY (tenant, user_id, planning_id), UNIQUE (tenant, user_id, task_id, planned_for),
  FOREIGN KEY (tenant, user_id, task_id) REFERENCES reference_tasks
);
CREATE UNIQUE INDEX IF NOT EXISTS reference_one_focus ON reference_planning (tenant, user_id) WHERE focused;
CREATE TABLE IF NOT EXISTS reference_receipts (
  tenant text NOT NULL, user_id text NOT NULL, key text NOT NULL, digest text NOT NULL, response jsonb NOT NULL,
  PRIMARY KEY (tenant, user_id, key), FOREIGN KEY (tenant, user_id) REFERENCES reference_actors
);
