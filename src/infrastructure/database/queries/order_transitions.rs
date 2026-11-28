use sqlx::{Sqlite, Transaction};

use crate::domain::commerce::order::{Order, TransitionOrder};
use crate::domain::commerce::state_machine::Actor;
use crate::error::AppResult;

macro_rules! member_role_exists {
    ($account:literal, $establishment:literal, $role:literal) => {
        concat!(
            "EXISTS(
                SELECT 1 FROM establishment_members
                WHERE account_id = ",
            $account,
            " AND establishment_id = ",
            $establishment,
            " AND member_role = ",
            $role,
            "
                UNION ALL
                SELECT 1 FROM establishment_member_roles
                WHERE account_id = ",
            $account,
            " AND establishment_id = ",
            $establishment,
            " AND member_role = ",
            $role,
            ")"
        )
    };
}
pub(super) use member_role_exists;

pub(crate) async fn has_member_role(
    tx: &mut Transaction<'_, Sqlite>,
    establishment_id: &str,
    account_id: &str,
    actor: Actor,
) -> AppResult<bool> {
    Ok(
        sqlx::query_scalar(concat!("SELECT ", member_role_exists!("?1", "?2", "?3")))
            .bind(account_id)
            .bind(establishment_id)
            .bind(actor.as_str())
            .fetch_one(&mut **tx)
            .await?,
    )
}

pub(crate) async fn append(
    tx: &mut Transaction<'_, Sqlite>,
    changed: &Order,
    command: TransitionOrder<'_>,
) -> AppResult<()> {
    sqlx::query(
        "INSERT INTO order_transitions (
            establishment_id, order_id, version, from_state, to_state,
            actor_kind, actor_account_id, reason_code, at
         ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&changed.establishment_id)
    .bind(&changed.id)
    .bind(changed.version)
    .bind(command.expected_state.as_str())
    .bind(changed.state.as_str())
    .bind(command.actor.as_str())
    .bind(command.actor_account_id)
    .bind(command.action.reason_code())
    .bind(changed.updated_at)
    .execute(&mut **tx)
    .await?;
    Ok(())
}
