//! Матрица «роль × действие» снапшотом (ТЗ § 3): изменение политики доступа
//! видно в диффе; снапшот - защищенный путь, правки только через инженера.

use tou_domain::policy::{Action, is_allowed};
use tou_domain::role::Role;

#[test]
fn policy_matrix_snapshot() {
    let mut matrix = String::new();
    for action in Action::ALL {
        let allowed: Vec<&str> = Role::ALL
            .into_iter()
            .filter(|role| is_allowed(*role, action))
            .map(Role::as_str)
            .collect();
        matrix.push_str(&format!("{action:?}: {}\n", allowed.join(", ")));
    }
    insta::assert_snapshot!("policy_matrix", matrix);
}
