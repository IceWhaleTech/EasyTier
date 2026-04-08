mod meshtier;

use std::sync::Arc;

use easytier::common::scoped_task::ScopedTask;

use crate::client_manager::ClientManager;

pub fn start(client_mgr: Arc<ClientManager>) -> ScopedTask<()> {
    meshtier::start_auto_register_task(client_mgr)
}
