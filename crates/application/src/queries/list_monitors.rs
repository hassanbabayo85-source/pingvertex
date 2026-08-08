use domain::shared::ids::UserId;

/// Query for listing all monitors for a specific user.
#[derive(Debug, Clone, Copy)]
pub struct ListMonitors {
    pub user_id: UserId,
}
