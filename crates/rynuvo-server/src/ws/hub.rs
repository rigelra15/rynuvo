use rynuvo_proto::WsServerMsg;

/// Realtime fan-out hub. Stubbed until Task 8.
#[derive(Clone, Default)]
pub struct Hub {}

impl Hub {
    pub fn broadcast(&self, _msg: WsServerMsg) {}
    pub fn send_to_admins(&self, _msg: WsServerMsg) {}
    pub fn kick_user(&self, _user_id: i64) {}
    pub fn online_ids(&self) -> Vec<i64> {
        Vec::new()
    }
}
