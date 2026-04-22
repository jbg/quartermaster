#![allow(dead_code)]

use uuid::Uuid;

pub struct MockOffServer {
    session_id: String,
}

impl MockOffServer {
    pub async fn start() -> Self {
        let session_id = Uuid::now_v7().simple().to_string();
        qm_api::openfoodfacts::register_mock_session(&session_id).await;
        Self { session_id }
    }

    pub fn base_url(&self) -> String {
        format!("mock://off/{}", self.session_id)
    }

    pub async fn hit_count(&self, barcode: &str) -> usize {
        qm_api::openfoodfacts::mock_session_hit_count(&self.session_id, barcode).await
    }
}

impl Drop for MockOffServer {
    fn drop(&mut self) {
        if let Ok(handle) = tokio::runtime::Handle::try_current() {
            let session_id = self.session_id.clone();
            handle.spawn(async move {
                qm_api::openfoodfacts::unregister_mock_session(&session_id).await;
            });
        }
    }
}
