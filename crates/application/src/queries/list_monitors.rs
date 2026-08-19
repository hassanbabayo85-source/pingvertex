/// Query for listing every monitor visible to the caller.
///
/// No `user_id` field anymore — tenant-wide sharing means the use case
/// lists every monitor belonging to the caller's tenant
/// (`MonitorRepository::find_all_for_tenant`), not just the ones the
/// caller personally created.
#[derive(Debug, Clone, Copy, Default)]
pub struct ListMonitors;
