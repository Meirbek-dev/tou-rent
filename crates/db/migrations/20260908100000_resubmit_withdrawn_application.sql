-- FR-401, FR-404: отозванная заявка сохраняется в истории, но не мешает
-- новой подаче до дедлайна. Одновременно разрешена только одна
-- неотозванная заявка участника на лот, в том числе при конкурентной подаче.
CREATE UNIQUE INDEX IF NOT EXISTS applications_active_lot_participant_idx
  ON core.applications (lot_id, participant_id)
  WHERE status <> 'withdrawn';

ALTER TABLE core.applications
  DROP CONSTRAINT IF EXISTS applications_lot_id_participant_id_key;
