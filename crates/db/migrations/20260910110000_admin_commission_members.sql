-- FR-1101: управление составом до начала заседаний, без изменения истории.
CREATE OR REPLACE FUNCTION core.admin_commission_member(
  p_commission uuid, p_user uuid, p_role text
) RETURNS boolean LANGUAGE plpgsql AS $$
BEGIN
  -- Открытие заседания и изменение состава не должны пройти одновременно.
  LOCK TABLE core.sessions_meetings IN SHARE ROW EXCLUSIVE MODE;
  PERFORM 1 FROM core.commissions WHERE id = p_commission FOR UPDATE;
  IF NOT FOUND THEN RETURN false; END IF;
  IF EXISTS (SELECT 1 FROM core.sessions_meetings
             WHERE commission_id = p_commission AND opened_at IS NOT NULL)
     OR EXISTS (SELECT 1 FROM core.commission_members cm
                WHERE cm.commission_id = p_commission AND cm.user_id = p_user
                  AND (EXISTS (SELECT 1 FROM core.meeting_attendance a WHERE a.member_id = cm.id)
                    OR EXISTS (SELECT 1 FROM core.coi_declarations d WHERE d.member_id = cm.id)
                    OR EXISTS (SELECT 1 FROM core.member_recusals r
                               WHERE r.member_id = cm.id OR r.replacement_member_id = cm.id))) THEN
    RAISE EXCEPTION 'COMMISSION-LOCKED: состав использован в открытом заседании; изменение запрещено';
  END IF;
  IF p_role IS NULL THEN
    DELETE FROM core.commission_members
    WHERE commission_id = p_commission AND user_id = p_user;
  ELSE
    IF p_role NOT IN ('chairman', 'deputy', 'member', 'reserve') THEN
      RAISE EXCEPTION 'FR-1101: неизвестная роль в составе';
    END IF;
    IF NOT EXISTS (SELECT 1 FROM core.users u
                   JOIN core.role_grants r ON r.user_id = u.id AND r.role = 'commission'
                   WHERE u.id = p_user AND u.is_active
                     AND (u.email_confirmed_at IS NOT NULL OR u.phone_confirmed_at IS NOT NULL))
       OR EXISTS (SELECT 1 FROM core.role_grants WHERE user_id = p_user AND role = 'secretary') THEN
      RAISE EXCEPTION 'COMMISSION-CANDIDATE: нужен активный подтвержденный пользователь с ролью commission, без роли secretary';
    END IF;
    -- У существующего человека меняется только должность; идентичность не подменяется.
    INSERT INTO core.commission_members (commission_id, user_id, member_role)
    VALUES (p_commission, p_user, p_role::core.commission_member_role)
    ON CONFLICT (commission_id, user_id) DO UPDATE
    SET member_role = EXCLUDED.member_role
    WHERE core.commission_members.member_role IS DISTINCT FROM EXCLUDED.member_role;
  END IF;
  -- FK сохраняют декларации, явку и голоса; аудит и сброс утверждения — триггеры.
  RETURN true;
EXCEPTION WHEN foreign_key_violation THEN
  RAISE EXCEPTION 'COMMISSION-LOCKED: участник состава связан с явкой, декларациями или решениями';
END $$;
