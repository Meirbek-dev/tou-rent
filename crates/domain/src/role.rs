use serde::{Deserialize, Serialize};

/// Роли системы (ТЗ § 3). Политика доступа обязана делать исчерпывающий
/// `match` по этому enum без catch-all (INV-POL-01).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    Guest,
    Participant,
    Organizer,
    Secretary,
    Commission,
    Board,
    Finance,
    Admin,
}

impl Role {
    pub const ALL: [Role; 8] = [
        Role::Guest,
        Role::Participant,
        Role::Organizer,
        Role::Secretary,
        Role::Commission,
        Role::Board,
        Role::Finance,
        Role::Admin,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Role::Guest => "guest",
            Role::Participant => "participant",
            Role::Organizer => "organizer",
            Role::Secretary => "secretary",
            Role::Commission => "commission",
            Role::Board => "board",
            Role::Finance => "finance",
            Role::Admin => "admin",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("неизвестная роль: {0}")]
pub struct UnknownRole(pub String);

impl std::str::FromStr for Role {
    type Err = UnknownRole;

    /// Паритет со значениями enum-типа БД `core.role` (`guest` в БД не хранится).
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "guest" => Ok(Role::Guest),
            "participant" => Ok(Role::Participant),
            "organizer" => Ok(Role::Organizer),
            "secretary" => Ok(Role::Secretary),
            "commission" => Ok(Role::Commission),
            "board" => Ok(Role::Board),
            "finance" => Ok(Role::Finance),
            "admin" => Ok(Role::Admin),
            other => Err(UnknownRole(other.to_owned())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serde_uses_snake_case_wire_format() {
        let json = serde_json::to_string(&Role::Organizer).unwrap();
        assert_eq!(json, "\"organizer\"");
    }

    #[test]
    fn all_covers_every_role() {
        assert_eq!(Role::ALL.len(), 8);
    }
}
