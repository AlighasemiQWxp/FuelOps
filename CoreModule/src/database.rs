use crate::{
    events::{AppEvent, DomainEvent, EventBus},
    http::*,
    id, now, require,
};
use futures_util::TryStreamExt;
use mongodb::{
    bson::{self, doc, Document},
    options::{Acknowledgment, IndexOptions, ReadConcern, WriteConcern},
    Client, ClientSession, Database, IndexModel,
};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use std::{collections::HashSet, sync::Arc};

#[derive(Clone)]
pub struct AppState {
    pub db: Database,
    pub client: Client,
    pub events: EventBus,
    pub config: Arc<Config>,
}
#[derive(Clone)]
pub struct Config {
    pub jwt_secret: String,
    pub web_origin: String,
    pub cookie_secure: bool,
}
impl Config {
    pub fn from_env() -> ApiResult<Self> {
        let jwt_secret = std::env::var("JWT_SECRET").unwrap_or_default();
        require(
            jwt_secret.len() >= 48 && !jwt_secret.starts_with("replace-"),
            "Set a random JWT_SECRET of at least 48 characters",
        )?;
        let web_origin =
            std::env::var("WEB_ORIGIN").unwrap_or_else(|_| "http://localhost:5173".into());
        require(
            !web_origin.ends_with('/'),
            "WEB_ORIGIN must not have a trailing slash",
        )?;
        Ok(Self {
            jwt_secret,
            web_origin,
            cookie_secure: std::env::var("COOKIE_SECURE").as_deref() == Ok("true"),
        })
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Actor {
    pub user_id: String,
    pub org_id: String,
    pub station_id: String,
    pub station_ids: Vec<String>,
    pub permissions: HashSet<String>,
    pub name: String,
}
impl Actor {
    pub fn require(&self, permission: &str) -> ApiResult<()> {
        if self.permissions.contains(permission) {
            Ok(())
        } else {
            Err(ApiError::forbidden())
        }
    }
    pub fn scope(&self) -> Document {
        doc! { "org_id": &self.org_id, "station_id": &self.station_id }
    }
    pub fn owns(&self, org: &str, station: &str) -> bool {
        self.org_id == org && self.station_ids.iter().any(|s| s == station)
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Record<T> {
    pub id: String,
    pub org_id: String,
    pub station_id: String,
    pub version: i64,
    pub created_at: i64,
    pub updated_at: i64,
    pub data: T,
}
impl<T> Record<T> {
    pub fn new(actor: &Actor, data: T) -> Self {
        Self {
            id: id(),
            org_id: actor.org_id.clone(),
            station_id: actor.station_id.clone(),
            version: 1,
            created_at: now(),
            updated_at: now(),
            data,
        }
    }
}
impl AppState {
    pub async fn connect(config: Config) -> ApiResult<Self> {
        let uri = std::env::var("MONGODB_URI").unwrap_or_else(|_| {
            "mongodb://localhost:27017/?replicaSet=rs0&directConnection=true".into()
        });
        let client = Client::with_uri_str(uri).await?;
        let db = client
            .database(&std::env::var("MONGODB_DATABASE").unwrap_or_else(|_| "fuel_ops".into()));
        db.run_command(doc! {"ping": 1}).await?;
        Ok(Self {
            db,
            client,
            events: EventBus::default(),
            config: Arc::new(config),
        })
    }
    pub async fn index(&self, collection: &str, keys: Document, unique: bool) -> ApiResult<()> {
        self.db
            .collection::<Document>(collection)
            .create_index(
                IndexModel::builder()
                    .keys(keys)
                    .options(IndexOptions::builder().unique(unique).build())
                    .build(),
            )
            .await?;
        Ok(())
    }
    pub async fn record_indexes(&self, collection: &str) -> ApiResult<()> {
        self.index(collection, doc! {"org_id":1, "id":1}, true)
            .await?;
        self.index(
            collection,
            doc! {"org_id":1, "station_id":1, "created_at":-1},
            false,
        )
        .await?;
        Ok(())
    }
    pub async fn get<T: DeserializeOwned>(
        &self,
        actor: &Actor,
        collection: &str,
        id: &str,
    ) -> ApiResult<Record<T>> {
        let mut filter = actor.scope();
        filter.insert("id", id);
        let value = self
            .db
            .collection::<Document>(collection)
            .find_one(filter)
            .await?
            .ok_or_else(ApiError::missing)?;
        Ok(bson::from_document(value)?)
    }
    pub async fn list<T: DeserializeOwned>(
        &self,
        actor: &Actor,
        collection: &str,
        query: PageQuery,
    ) -> ApiResult<Page<Record<T>>> {
        let mut filter = actor.scope();
        if let Some(status) = &query.status {
            filter.insert("data.status", status);
        }
        let col = self.db.collection::<Document>(collection);
        let total = col.count_documents(filter.clone()).await?;
        let documents: Vec<Document> = col
            .find(filter)
            .sort(doc! {"created_at":-1, "id":1})
            .skip(query.skip())
            .limit(query.limit())
            .await?
            .try_collect()
            .await?;
        let items = documents
            .into_iter()
            .map(bson::from_document)
            .collect::<Result<_, _>>()?;
        Ok(Page { items, total })
    }
    pub async fn begin(&self, actor: &Actor) -> ApiResult<Transaction> {
        let mut session = self.client.start_session().await?;
        session
            .start_transaction()
            .read_concern(ReadConcern::snapshot())
            .write_concern(WriteConcern::builder().w(Acknowledgment::Majority).build())
            .await?;
        // Serialize commands within an organization to protect cross-station assignment invariants.
        self.db
            .collection::<Document>("write_guards")
            .update_one(
                doc! {"_id": &actor.org_id},
                doc! {"$inc": {"revision": 1_i64}},
            )
            .upsert(true)
            .session(&mut session)
            .await?;
        Ok(Transaction {
            state: self.clone(),
            actor: actor.clone(),
            session,
            events: Vec::new(),
        })
    }
}

pub struct Transaction {
    state: AppState,
    pub actor: Actor,
    pub session: ClientSession,
    events: Vec<AppEvent>,
}
impl Transaction {
    pub fn db(&self) -> Database {
        self.state.db.clone()
    }
    pub async fn get<T: DeserializeOwned>(
        &mut self,
        collection: &str,
        id: &str,
    ) -> ApiResult<Record<T>> {
        let mut filter = self.actor.scope();
        filter.insert("id", id);
        let value = self
            .db()
            .collection::<Document>(collection)
            .find_one(filter)
            .session(&mut self.session)
            .await?
            .ok_or_else(ApiError::missing)?;
        Ok(bson::from_document(value)?)
    }
    pub async fn find(&mut self, collection: &str, extra: Document) -> ApiResult<Vec<Document>> {
        let filter = doc! {"$and": [self.actor.scope(), extra]};
        let mut cursor = self
            .db()
            .collection::<Document>(collection)
            .find(filter)
            .limit(1000)
            .session(&mut self.session)
            .await?;
        Ok(cursor.stream(&mut self.session).try_collect().await?)
    }
    pub async fn insert<T: Serialize>(
        &mut self,
        collection: &str,
        record: &Record<T>,
    ) -> ApiResult<()> {
        require(
            record.org_id == self.actor.org_id && record.station_id == self.actor.station_id,
            "Invalid record scope",
        )?;
        self.db()
            .collection::<Document>(collection)
            .insert_one(bson::to_document(record)?)
            .session(&mut self.session)
            .await?;
        self.audit(collection, &record.id, "created", 0, record.version)
            .await
    }
    pub async fn replace<T: Serialize>(
        &mut self,
        collection: &str,
        record: &mut Record<T>,
        expected: i64,
    ) -> ApiResult<()> {
        require(
            record.org_id == self.actor.org_id && record.station_id == self.actor.station_id,
            "Invalid record scope",
        )?;
        if record.version != expected {
            return Err(ApiError::conflict());
        }
        let mut filter = self.actor.scope();
        filter.insert("id", &record.id);
        filter.insert("version", expected);
        record.version += 1;
        record.updated_at = now();
        let mut snapshot = self
            .db()
            .collection::<Document>(collection)
            .find_one(filter.clone())
            .session(&mut self.session)
            .await?
            .ok_or_else(ApiError::conflict)?;
        snapshot.remove("_id");
        if collection == "users" {
            if let Ok(data) = snapshot.get_document_mut("data") {
                data.remove("password_hash");
            }
        }
        self.db().collection::<Document>("record_history").insert_one(doc! {"id":id(),"org_id":&self.actor.org_id,"station_id":&self.actor.station_id,"collection":collection,"entity_id":&record.id,"version":expected,"snapshot":snapshot,"created_at":now()}).session(&mut self.session).await?;
        let result = self
            .db()
            .collection::<Document>(collection)
            .replace_one(filter, bson::to_document(record)?)
            .session(&mut self.session)
            .await?;
        if result.matched_count != 1 {
            return Err(ApiError::conflict());
        }
        self.audit(collection, &record.id, "updated", expected, record.version)
            .await
    }
    async fn audit(
        &mut self,
        collection: &str,
        entity: &str,
        action: &str,
        before: i64,
        after: i64,
    ) -> ApiResult<()> {
        self.db().collection::<Document>("audit_logs").insert_one(doc! {
            "id": id(), "org_id": &self.actor.org_id, "station_id": &self.actor.station_id,
            "actor_id": &self.actor.user_id, "actor_name": &self.actor.name, "entity_id": entity,
            "collection": collection, "action": action, "before_version": before, "after_version": after, "created_at": now()
        }).session(&mut self.session).await?;
        Ok(())
    }
    pub async fn emit<E: DomainEvent>(&mut self, entity_id: &str, payload: E) -> ApiResult<()> {
        let event = AppEvent {
            id: id(),
            name: E::NAME.into(),
            org_id: self.actor.org_id.clone(),
            station_id: self.actor.station_id.clone(),
            entity_id: entity_id.into(),
            occurred_at: now(),
            payload: bson::to_document(&payload)?,
        };
        self.db()
            .collection::<AppEvent>("outbox")
            .insert_one(&event)
            .session(&mut self.session)
            .await?;
        self.events.push(event);
        Ok(())
    }
    pub async fn commit(mut self) -> ApiResult<()> {
        for attempt in 0..3 {
            match self.session.commit_transaction().await {
                Ok(()) => {
                    for event in self.events {
                        self.state.events.publish(event);
                    }
                    return Ok(());
                }
                Err(error)
                    if error.contains_label("UnknownTransactionCommitResult") && attempt < 2 =>
                {
                    continue
                }
                Err(error) => return Err(error.into()),
            }
        }
        Err(ApiError::conflict())
    }
}
