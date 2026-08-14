//! Матрица «роль × действие» снапшотом (ТЗ § 3): изменение политики доступа
//! видно в диффе; снапшот - защищенный путь, правки только через инженера.
//!
//! В снимке два раздела. Первый - одиночные действия. Второй - составные
//! права «любое из» (`Compound`): без него матрица не описывала проверки,
//! собранные дизъюнкцией, и доказывала меньше, чем казалось.

use tou_domain::policy::{Action, Compound, is_allowed, is_compound_allowed};
use tou_domain::role::Role;

fn roles_where(allowed: impl Fn(Role) -> bool) -> String {
    Role::ALL
        .into_iter()
        .filter(|role| allowed(*role))
        .map(Role::as_str)
        .collect::<Vec<_>>()
        .join(", ")
}

#[test]
fn policy_matrix_snapshot() {
    let mut matrix = String::new();
    for action in Action::ALL {
        matrix.push_str(&format!(
            "{action:?}: {}\n",
            roles_where(|role| is_allowed(role, action))
        ));
    }

    matrix.push_str("\n# составные права (любое из перечисленных действий)\n");
    for compound in Compound::ALL {
        let any_of: Vec<String> = compound
            .any_of
            .iter()
            .map(|action| format!("{action:?}"))
            .collect();
        matrix.push_str(&format!(
            "{} [{}]: {}\n",
            compound.name,
            any_of.join(" | "),
            roles_where(|role| is_compound_allowed(role, compound))
        ));
    }

    insta::assert_snapshot!("policy_matrix", matrix);
}
