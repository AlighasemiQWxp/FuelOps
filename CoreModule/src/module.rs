use crate::{ApiError, ApiResult, AppState};
use async_trait::async_trait;
use axum::Router;
use std::collections::HashSet;

#[async_trait]
pub trait Module: Send + Sync {
    fn name(&self) -> &'static str;
    fn dependencies(&self) -> &'static [&'static str] {
        &[]
    }
    fn routes(&self) -> Router<AppState>;
    async fn initialize(&self, _: &AppState) -> ApiResult<()> {
        Ok(())
    }
}
#[derive(Default)]
pub struct CoreModule {
    modules: Vec<Box<dyn Module>>,
}
impl CoreModule {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn register(mut self, module: impl Module + 'static) -> Self {
        self.modules.push(Box::new(module));
        self
    }
    pub fn order(&self) -> ApiResult<Vec<usize>> {
        let mut names = HashSet::new();
        for module in &self.modules {
            if !names.insert(module.name()) {
                return Err(ApiError::validation("Duplicate module"));
            }
        }
        let mut ready = HashSet::new();
        let mut ordered = Vec::new();
        while ordered.len() < self.modules.len() {
            let previous = ordered.len();
            for (index, module) in self.modules.iter().enumerate() {
                if !ready.contains(module.name())
                    && module
                        .dependencies()
                        .iter()
                        .all(|name| ready.contains(name))
                {
                    ready.insert(module.name());
                    ordered.push(index);
                }
            }
            if previous == ordered.len() {
                return Err(ApiError::validation(
                    "Missing or circular module dependency",
                ));
            }
        }
        Ok(ordered)
    }
    pub async fn initialize(&self, state: &AppState) -> ApiResult<Router<AppState>> {
        let mut routes = Router::new();
        for index in self.order()? {
            let module = &self.modules[index];
            module.initialize(state).await?;
            state
                .db
                .collection::<crate::bson::Document>("schema_migrations")
                .update_one(
                    crate::bson::doc! {"_id":module.name()},
                    crate::bson::doc! {"$setOnInsert":{"version":1_i64,"applied_at":crate::now()}},
                )
                .upsert(true)
                .await?;
            routes = routes.merge(module.routes());
        }
        state.record_indexes("outbox").await?;
        state.record_indexes("record_history").await?;
        state
            .index(
                "record_history",
                crate::bson::doc! {"org_id":1,"collection":1,"entity_id":1,"version":1},
                true,
            )
            .await?;
        Ok(routes)
    }
    pub async fn shutdown(state: AppState) {
        state.client.shutdown().await;
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    struct TestModule(&'static str, &'static [&'static str]);
    #[async_trait]
    impl Module for TestModule {
        fn name(&self) -> &'static str {
            self.0
        }
        fn dependencies(&self) -> &'static [&'static str] {
            self.1
        }
        fn routes(&self) -> Router<AppState> {
            Router::new()
        }
    }
    #[test]
    fn orders_dependencies() {
        assert_eq!(
            CoreModule::new()
                .register(TestModule("b", &["a"]))
                .register(TestModule("a", &[]))
                .order()
                .unwrap(),
            vec![1, 0]
        );
    }
    #[test]
    fn rejects_missing_and_cycles() {
        assert!(CoreModule::new()
            .register(TestModule("b", &["a"]))
            .order()
            .is_err());
        assert!(CoreModule::new()
            .register(TestModule("a", &["b"]))
            .register(TestModule("b", &["a"]))
            .order()
            .is_err());
    }
    #[test]
    fn rejects_duplicate_names() {
        assert!(CoreModule::new()
            .register(TestModule("a", &[]))
            .register(TestModule("a", &[]))
            .order()
            .is_err());
    }
}
